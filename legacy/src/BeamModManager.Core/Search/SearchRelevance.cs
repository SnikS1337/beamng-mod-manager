using System.Text;
using BeamModManager.Core.Models;

namespace BeamModManager.Core.Search;

/// <summary>
/// Подходит ли найденное сайтом к запросу человека: хоть одно его слово есть в названии или в адресе страницы (2026-10-18).
/// Нужно там, где программа дописывает к запросу свои слова (ModsGaming: «beamng.drive»), а сайт ищет через «или».
/// Русское слово сверяется с адресом и в латинице: адреса у сайтов транслитом («бмв» — «bmv»).
/// </summary>
public static class SearchRelevance
{
    public static IReadOnlyList<Listing> Filter(string query, IReadOnlyList<Listing> found, IReadOnlyCollection<string>? ignore = null)
    {
        var words = Words(query).Where(w => w.Length >= 2 && (ignore is null || !ignore.Contains(w, StringComparer.OrdinalIgnoreCase))).ToList();
        if (words.Count == 0) return found; // не из чего судить — как ответил сайт
        var latin = words.Select(Translit).ToList();
        return found.Where(l =>
        {
            var title = Fold(l.Title);
            var path = Fold(l.Url.AbsolutePath);
            return words.Any(w => title.Contains(w, StringComparison.Ordinal) || path.Contains(w, StringComparison.Ordinal))
                || latin.Any(w => w.Length >= 2 && (path.Contains(w, StringComparison.Ordinal) || title.Contains(w, StringComparison.Ordinal)));
        }).ToList();
    }

    private static IEnumerable<string> Words(string s)
    {
        var cur = new StringBuilder();
        foreach (var ch in s)
        {
            if (char.IsLetterOrDigit(ch)) { cur.Append(ch); continue; }
            if (cur.Length > 0) { yield return Fold(cur.ToString()); cur.Clear(); }
        }
        if (cur.Length > 0) yield return Fold(cur.ToString());
    }

    private static string Fold(string s) => s.ToLowerInvariant().Replace('ё', 'е');

    private static readonly Dictionary<char, string> Table = new()
    {
        ['а'] = "a", ['б'] = "b", ['в'] = "v", ['г'] = "g", ['д'] = "d", ['е'] = "e", ['ж'] = "zh", ['з'] = "z", ['и'] = "i",
        ['й'] = "j", ['к'] = "k", ['л'] = "l", ['м'] = "m", ['н'] = "n", ['о'] = "o", ['п'] = "p", ['р'] = "r", ['с'] = "s",
        ['т'] = "t", ['у'] = "u", ['ф'] = "f", ['х'] = "h", ['ц'] = "c", ['ч'] = "ch", ['ш'] = "sh", ['щ'] = "sch", ['ъ'] = "",
        ['ы'] = "y", ['ь'] = "", ['э'] = "e", ['ю'] = "ju", ['я'] = "ja",
    };

    private static string Translit(string w)
    {
        var sb = new StringBuilder(w.Length);
        foreach (var ch in w) sb.Append(Table.TryGetValue(ch, out var t) ? t : ch.ToString());
        return sb.ToString();
    }
}
