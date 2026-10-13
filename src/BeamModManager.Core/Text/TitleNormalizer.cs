using System.Text.RegularExpressions;

namespace BeamModManager.Core.Text;

/// <summary>
/// Приводит название мода к виду для сравнения: без служебных слов сайтов, версий и скобочного шума.
/// Ограничение: числа вида «5.0» считаются версией и удаляются (например, объём двигателя в названии).
/// </summary>
public static class TitleNormalizer
{
    private const RegexOptions Opt = RegexOptions.IgnoreCase | RegexOptions.CultureInvariant | RegexOptions.Compiled;

    // Квадратные скобки целиком считаем шумом: [PACK], [PAID], [0.39].
    private static readonly Regex Square = new(@"\[[^\]]*\]", Opt);

    // Круглые скобки удаляем только если внутри версия или служебное слово; «(1972)» остаётся.
    private static readonly Regex Round = new(@"\(([^)]*)\)", Opt);
    private static readonly Regex RoundNoise = new(@"\d\.\d|\b(?:update|fix|fixed|pack|beta|wip|paid|premium)\b", Opt);

    private static readonly Regex LeadingMod = new(@"^\s*(?:мод|mod)\b", Opt);
    private static readonly Regex ForBeam = new(@"\b(?:для|for)\s+beamng(?:\s*\.?\s*drive)?\b", Opt);
    private static readonly Regex BareBeam = new(@"\bbeamng(?:\s*\.?\s*drive)?\b", Opt);
    private static readonly Regex UpdateWord = new(@"\bupdate\s*\d*\b|\bfix(?:ed)?\b", Opt);
    private static readonly Regex DottedVersion = new(@"\bv?\d+(?:\.\d+)+(?:\.x)?\b", Opt);
    private static readonly Regex ShortVersion = new(@"\bv\d+\b", Opt);
    private static readonly Regex NonWord = new(@"[^\p{L}\p{Nd}]+", Opt);

    public static string Normalize(string title) => string.Join(' ', Tokens(title));

    public static IReadOnlyList<string> Tokens(string title)
    {
        if (string.IsNullOrWhiteSpace(title)) return Array.Empty<string>();

        var s = Square.Replace(title, " ");
        s = Round.Replace(s, m => RoundNoise.IsMatch(m.Groups[1].Value) ? " " : " " + m.Groups[1].Value + " ");
        s = LeadingMod.Replace(s, " ");
        s = ForBeam.Replace(s, " ");
        s = BareBeam.Replace(s, " ");
        s = UpdateWord.Replace(s, " ");
        s = DottedVersion.Replace(s, " ");
        s = ShortVersion.Replace(s, " ");
        s = NonWord.Replace(s, " ").ToLowerInvariant();

        return s.Split(' ', StringSplitOptions.RemoveEmptyEntries);
    }

    /// <summary>Сходство названий по множествам токенов (Жаккар), 0..1.</summary>
    public static double Similarity(string a, string b)
    {
        var x = new HashSet<string>(Tokens(a));
        var y = new HashSet<string>(Tokens(b));
        if (x.Count == 0 || y.Count == 0) return 0;

        var inter = x.Count(y.Contains);
        var union = x.Count + y.Count - inter;
        return (double)inter / union;
    }
}
