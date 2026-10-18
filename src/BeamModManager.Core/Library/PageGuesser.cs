using BeamModManager.Core.Models;
using BeamModManager.Core.Text;
using BeamModManager.Core.Versioning;

namespace BeamModManager.Core.Library;

/// <summary>Предложенная страница: запись на сайте и насколько она похожа (для выбора лучшей).</summary>
public sealed record PageGuess(Listing Listing, double Score);

/// <summary>
/// Подбор страницы для мода, который положили в папку сами (2026-10-18: «какой мод ни положи — не проверяется»). Мод
/// ищется на сайтах по названию; предлагается одна страница, только если она явно та: название совпадает почти целиком,
/// автор и размер не спорят, версия на сайте не старше установленной, и на том же сайте нет второй такой же похожей.
/// Привязка ставится неподтверждённой — следить начинаем, только когда человек нажмёт «Следить».
/// </summary>
public static class PageGuesser
{
    /// <summary>Ничего не нашли — следующий поиск для этого мода не раньше.</summary>
    public static readonly TimeSpan RetryAfter = TimeSpan.FromDays(14);

    /// <summary>Сколько модов искать за одну проверку: каждый — несколько запросов к каждому сайту.</summary>
    public const int PerRun = 20;

    /// <summary>Доля совпавших слов названия (Жаккар), с которой страница считается «тем же модом».</summary>
    public const double MinName = 0.8;

    /// <summary>Вторая страница того же сайта ближе этого к лучшей — непонятно, какая из них: не предлагаем ничего.</summary>
    public const double Ambiguous = 0.05;

    /// <summary>Моды, для которых пора искать страницу: без привязок, без отказа «Не проверять», не искали недавно; новые первыми.</summary>
    public static IReadOnlyList<InstalledMod> Due(ILibraryStore store, DateTimeOffset now)
    {
        var bound = store.Bindings().Select(b => b.ModId).ToHashSet();
        return store.Mods()
            .Where(m => !bound.Contains(m.Id) && !store.IsCheckingDeclined(m.Id))
            .Where(m => store.GetPageSearch(m.Id) is not { } p || (!p.Rejected && (now - p.At >= RetryAfter || p.At > now)))
            .OrderByDescending(m => m.InstalledAtUtc)
            .Take(PerRun)
            .ToList();
    }

    /// <summary>Чем искать мод на сайтах: лучшее известное название, иначе угаданное из имени файла.</summary>
    public static string QueryFor(InstalledMod mod, string? title) =>
        !string.IsNullOrWhiteSpace(title) ? title.Trim() : BindingSuggester.QueryFor(mod);

    /// <param name="title">Название мода из окна (репозиторий, info.json, сайт); null — только имя файла и запись библиотеки.</param>
    public static PageGuess? Pick(InstalledMod mod, string? title, IEnumerable<Listing> found)
    {
        var guess = BindingSuggester.FromFileName(mod.FileName);
        var names = new[] { title, mod.Name, guess.Name }
            .Where(n => !string.IsNullOrWhiteSpace(n) && TitleNormalizer.Tokens(n!).Count > 0)
            .Select(n => n!).Distinct(StringComparer.OrdinalIgnoreCase).ToList();
        if (names.Count == 0) return null;
        var installed = mod.Version ?? guess.Version;
        var author = Key(guess.Author);

        var seen = new HashSet<(string, string)>();
        var fits = new List<PageGuess>();
        foreach (var l in found)
        {
            if (!seen.Add((l.Source, l.SourceId)) || l.Price == PriceStatus.Paid) continue; // платный — у автора, следить не за чем
            var (sim, tokens) = names.Select(n => (TitleNormalizer.Similarity(n, l.Title), TitleNormalizer.Tokens(n).Count)).MaxBy(x => x.Item1);
            // одно слово («police») совпадает со слишком многим: только целиком
            if (sim < MinName || (tokens < 2 && sim < 1)) continue;
            if (author.Length > 0 && Key(l.Author) is { Length: > 0 } a && a != author) continue;
            if (mod.SizeBytes is > 0 && l.SizeBytes is > 0
                && (double)Math.Min(mod.SizeBytes.Value, l.SizeBytes.Value) / Math.Max(mod.SizeBytes.Value, l.SizeBytes.Value) < 0.85) continue;
            var score = sim;
            switch (VersionComparer.Compare(installed, l.ModVersion ?? TitleParser.Parse(l.Title).ModVersion))
            {
                case VersionOrder.Older: continue; // на сайте старее установленного — это не его страница
                case VersionOrder.Same: score += 0.1; break;
                case VersionOrder.Newer: score += 0.02; break;
            }
            fits.Add(new PageGuess(l, score));
        }
        if (fits.Count == 0) return null;
        var best = fits.OrderByDescending(f => f.Score).First();
        var rival = fits.Any(f => !ReferenceEquals(f, best) && f.Listing.Source == best.Listing.Source && f.Listing.SourceId != best.Listing.SourceId
            && best.Score - f.Score < Ambiguous);
        return rival ? null : best;
    }

    private static string Key(string? s) =>
        string.IsNullOrWhiteSpace(s) ? "" : new string(s.Where(char.IsLetterOrDigit).ToArray()).ToLowerInvariant();
}
