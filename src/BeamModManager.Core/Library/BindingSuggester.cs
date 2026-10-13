using System.Text.RegularExpressions;
using BeamModManager.Core.Matching;
using BeamModManager.Core.Models;
using BeamModManager.Core.Search;

namespace BeamModManager.Core.Library;

public sealed record GuessedName(string Name, string? Author, string? Version);

public sealed record BindingSuggestion(Listing Listing, MatchResult Match)
{
    /// <summary>Только при уверенном совпадении (Same); остальное подтверждает пользователь.</summary>
    public bool AutoConfirmable => Match.Verdict == MatchVerdict.Same;
}

/// <summary>Подбор привязок: к какой записи на сайте относится установленный файл.</summary>
public static class BindingSuggester
{
    private static readonly Regex ByFor = new(@"[_\-\s]by[_\-\s](?<a>.+?)[_\-\s]for[_\-\s]beamng.*$", RegexOptions.IgnoreCase | RegexOptions.Compiled);
    private static readonly Regex SiteTag = new(@"(?<![\p{L}\p{Nd}])[_\-\s]?(?:modland[_\-\s]?net|modsgaming|stmods)(?![\p{L}\p{Nd}])", RegexOptions.IgnoreCase | RegexOptions.Compiled);
    private static readonly Regex TrailingVersion = new(@"\s(?:v\s*)?(?<v>\d{1,3}(?:\s\d{1,3})+)$", RegexOptions.IgnoreCase | RegexOptions.Compiled);
    // Версия через точку в конце: «bmw_m3_v1.2», «car 2.0b».
    private static readonly Regex DottedTrailing = new(@"\s(?:v\s*)?(?<v>\d{1,3}(?:\.\d{1,3})+[a-z]?)$", RegexOptions.IgnoreCase | RegexOptions.Compiled);
    private static readonly Regex VPrefixed = new(@"\sv(?<v>\d{1,3}(?:\s\d{1,3})*)$", RegexOptions.IgnoreCase | RegexOptions.Compiled);

    private static readonly Regex EngineLike = new(@"^\sv\s*(?:4|6|8|10|12|16)$", RegexOptions.IgnoreCase | RegexOptions.Compiled);

    /// <summary>Достаёт читаемое название (и, если есть, автора и версию) из имени файла.</summary>
    public static GuessedName FromFileName(string fileName)
    {
        var s = Path.GetFileNameWithoutExtension(fileName.Trim());
        s = SiteTag.Replace(s, "");

        string? author = null;
        var m = ByFor.Match(s);
        if (m.Success)
        {
            author = Regex.Replace(m.Groups["a"].Value, @"[_\-]+", " ").Trim();
            s = s[..m.Index];
        }

        s = Regex.Replace(s, @"[_\-]+", " ").Trim();

        string? version = null;
        var v = DottedTrailing.Match(s);
        if (!v.Success) v = TrailingVersion.Match(s);
        if (!v.Success) v = VPrefixed.Match(s);
        // «V8», «V6», «V12» — это двигатель, а не версия: оставляем в названии.
        if (v.Success && EngineLike.IsMatch(v.Value)) v = Match.Empty;
        if (v.Success)
        {
            version = Regex.Replace(v.Groups["v"].Value, @"\s+", ".");
            s = s[..v.Index].Trim();
        }
        return new GuessedName(s, author, version);
    }

    public static IReadOnlyList<BindingSuggestion> Suggest(InstalledMod mod, IEnumerable<Listing> candidates, int max = 5)
    {
        var guess = FromFileName(mod.FileName);
        var probe = new Listing("installed", new Uri("https://installed.invalid/" + Uri.EscapeDataString(mod.Id)), mod.Id,
            string.IsNullOrWhiteSpace(mod.Name) ? guess.Name : mod.Name)
        {
            Author = guess.Author,
            SizeBytes = mod.SizeBytes,
            ModVersion = mod.Version ?? guess.Version,
        };

        return candidates
            .Select(c => new BindingSuggestion(c, DuplicateMatcher.Compare(probe, c)))
            .Where(s => s.Match.Verdict != MatchVerdict.Different)
            .OrderByDescending(s => s.Match.Score)
            .Take(max)
            .ToList();
    }

    /// <summary>Ищет кандидатов через общий поиск и ранжирует. Поиск идёт по названию, угаданному из имени файла.</summary>
    public static async Task<IReadOnlyList<BindingSuggestion>> FindAsync(
        InstalledMod mod, SearchService search, CancellationToken ct = default)
    {
        var query = FromFileName(mod.FileName).Name;
        if (string.IsNullOrWhiteSpace(query)) query = mod.Name;
        var response = await search.SearchAsync(query, ct).ConfigureAwait(false);
        return Suggest(mod, response.Cards.SelectMany(c => c.Listings));
    }
}
