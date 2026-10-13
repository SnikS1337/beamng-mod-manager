using BeamModManager.Core.Models;
using BeamModManager.Core.Updates;
using BeamModManager.Core.Versioning;

namespace BeamModManager.Core.Library;

/// <summary>Найденное обновление для одного установленного мода.</summary>
public sealed record UpdateFinding(
    string ModId,
    string ModName,
    string Source,
    UpdateStatus Status,
    string? InstalledVersion,
    string? LatestVersion,
    Uri Url);

/// <summary>
/// Склеивает хранилище, планировщик и решение «есть обновление». Сеть сюда не входит:
/// слой выше получает ленты и карточки и передаёт сюда результат.
/// </summary>
public static class UpdateCheckService
{
    /// <summary>Привязки для проверки: только подтверждённые, с тем, что видели раньше.</summary>
    public static IReadOnlyList<TrackedBinding> Tracked(ILibraryStore store) =>
        store.Bindings().Where(b => b.Confirmed).Select(b =>
        {
            var c = store.GetCheck(b.ModId, b.Source);
            return new TrackedBinding(b.ModId, b.Source, b.SourceId, c?.LastCheckedUtc, c?.LastChangedUtc, c?.LastVersion, c?.LastSize);
        }).ToList();

    public static FeedPlan PlanFeeds(ILibraryStore store, DateTimeOffset now, UpdateCheckOptions? options = null) =>
        UpdateCheckPlanner.PlanFeeds(Tracked(store), store.SourceStatuses(), now, options);

    /// <summary>Лента подтвердила, что запись не менялась: обновляем только время проверки.</summary>
    public static void ApplyFeedUnchanged(ILibraryStore store, TrackedBinding b, DateTimeOffset now)
    {
        var c = store.GetCheck(b.ModId, b.Source);
        if (c is null) return; // без прошлых данных планировщик такие случаи не пропускает
        store.SaveCheck(c with { LastCheckedUtc = now });
    }

    /// <summary>Результат открытия карточки: запоминаем, что сайт показывает сейчас.</summary>
    public static void ApplyCard(ILibraryStore store, TrackedBinding b, Listing current, DateTimeOffset now)
    {
        var prev = store.GetCheck(b.ModId, b.Source);
        var changed = prev is null || prev.LastVersion != current.ModVersion
                      || (prev.LastSize is not null && current.SizeBytes is not null && prev.LastSize != current.SizeBytes);
        store.SaveCheck(new CheckState(
            b.ModId, b.Source, now,
            changed ? now : prev!.LastChangedUtc,
            current.ModVersion,
            current.SizeBytes ?? prev?.LastSize));
    }

    /// <summary>Список обновлений. Считается при чтении, поэтому установка новой версии убирает пункт сама.</summary>
    public static IReadOnlyList<UpdateFinding> Pending(ILibraryStore store)
    {
        var result = new List<UpdateFinding>();
        foreach (var mod in store.Mods())
        {
            UpdateFinding? best = null;
            foreach (var b in store.Bindings().Where(x => x.ModId == mod.Id && x.Confirmed))
            {
                var c = store.GetCheck(mod.Id, b.Source);
                if (c is null) continue;
                var status = UpdateDecider.Decide(
                    new VersionSnapshot(mod.Version, mod.SizeBytes),
                    new VersionSnapshot(c.LastVersion, c.LastSize));
                if (status == UpdateStatus.None) continue;
                if (best is null || status > best.Status)
                    best = new UpdateFinding(mod.Id, mod.Name, b.Source, status, mod.Version, c.LastVersion, b.Url);
            }
            if (best is not null) result.Add(best);
        }
        return result;
    }
}
