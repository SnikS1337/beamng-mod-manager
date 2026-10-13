using BeamModManager.Core.Models;
using BeamModManager.Core.Search;

namespace BeamModManager.Core.Library;

/// <summary>Привязка, страница которой несколько проверок подряд не находится (удалена автором или сменила адрес).</summary>
public sealed record DeadBinding(InstalledMod Mod, ModBinding Binding, int Misses);

public sealed record RepairProposal(DeadBinding Dead, IReadOnlyList<BindingSuggestion> Candidates, IReadOnlyList<SourceReport> Sources);

/// <summary>
/// Починка привязок: если страницы мода больше нет, предлагаем похожие записи на сайтах, а пользователь выбирает.
/// Ничего не привязывается само: найденное по названию всегда нужно подтвердить.
/// </summary>
public static class BindingRepair
{
    /// <summary>Одна неудача может быть сбоем сайта; потерянной привязку считаем после двух проверок подряд.</summary>
    public const int DeadAfterMisses = Updates.UpdateCheckPlanner.DeadAfterMisses;

    public static IReadOnlyList<DeadBinding> Dead(ILibraryStore store)
    {
        var mods = store.Mods().ToDictionary(m => m.Id);
        var byKey = store.Bindings().Where(b => b.Confirmed).ToDictionary(b => (b.ModId, b.Source));
        var result = new List<DeadBinding>();
        foreach (var c in store.Checks())
        {
            if (c.MissCount < DeadAfterMisses) continue;
            if (mods.TryGetValue(c.ModId, out var mod) && byKey.TryGetValue((c.ModId, c.Source), out var b))
                result.Add(new DeadBinding(mod, b, c.MissCount));
        }
        return result.OrderBy(d => d.Mod.Name, StringComparer.OrdinalIgnoreCase).ThenBy(d => d.Binding.Source, StringComparer.Ordinal).ToList();
    }

    /// <summary>Ищет замену по названию. Исключает саму потерянную страницу и сайты, где у мода уже есть другая живая привязка.</summary>
    public static async Task<RepairProposal> ProposeAsync(ILibraryStore store, DeadBinding dead, SearchService search, CancellationToken ct = default)
    {
        var response = await search.SearchAsync(BindingSuggester.QueryFor(dead.Mod), ct).ConfigureAwait(false);
        var otherSources = OtherBoundSources(store, dead);
        var candidates = BindingSuggester.Suggest(dead.Mod, response.Cards.SelectMany(c => c.Listings))
            .Where(s => !IsSamePage(s.Listing, dead.Binding) && !otherSources.Contains(s.Listing.Source))
            .ToList();
        return new RepairProposal(dead, candidates, response.Sources);
    }

    /// <summary>
    /// Заменяет потерянную привязку выбранной записью. Возвращает false, если за время выбора что-то изменилось
    /// (мод удалён, привязку уже починили или у мода появилась другая привязка на этом сайте): тогда ничего не трогаем.
    /// </summary>
    public static bool Apply(ILibraryStore store, DeadBinding dead, Listing chosen)
    {
        lock (ApplyGate) return ApplyCore(store, dead, chosen);
    }

    // Проверки и замена идут несколькими обращениями к хранилищу; одновременные «починить» не должны перемешаться.
    private static readonly object ApplyGate = new();

    private static bool ApplyCore(ILibraryStore store, DeadBinding dead, Listing chosen)
    {
        if (store.GetMod(dead.Mod.Id) is null) return false;
        var current = store.Bindings().FirstOrDefault(b => b.ModId == dead.Mod.Id && b.Source == dead.Binding.Source);
        if (current is null || current.SourceId != dead.Binding.SourceId) return false;           // уже изменена или убрана
        if (IsSamePage(chosen, dead.Binding)) return false;                                         // ту же мёртвую страницу выбирать незачем
        if (OtherBoundSources(store, dead).Contains(chosen.Source)) return false;                  // перезаписали бы живую привязку

        // Сначала убираем старую вместе с её состоянием проверок (иначе при замене на том же сайте остались бы старые промахи).
        store.RemoveBinding(dead.Mod.Id, dead.Binding.Source);
        store.UpsertBinding(new ModBinding(dead.Mod.Id, chosen.Source, chosen.SourceId, chosen.Url, Confirmed: true));
        return true;
    }

    private static HashSet<string> OtherBoundSources(ILibraryStore store, DeadBinding dead) =>
        store.Bindings().Where(b => b.ModId == dead.Mod.Id && b.Source != dead.Binding.Source).Select(b => b.Source).ToHashSet();

    private static bool IsSamePage(Listing l, ModBinding b) =>
        l.Source == b.Source && (l.SourceId == b.SourceId || l.Url == b.Url);
}
