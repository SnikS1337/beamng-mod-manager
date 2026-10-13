using System.Text.RegularExpressions;

namespace BeamModManager.Core.Versioning;

public enum VersionOrder { Unknown, Older, Same, Newer }

/// <summary>
/// Сравнение версий, записанных как попало («v1.3.2», «1.3», «Update 2!»).
/// Понимает только числа с точками или с префиксом v; всё остальное считается неразобранным.
/// </summary>
public static class VersionComparer
{
    // После цифр: пред-релизная метка («1.0-beta2», «1.0b2», «rc1») ЛИБО буква-ревизия вплотную («1.2b» — правка после 1.2).
    // Одиночные a/b считаются пред-релизом только с номером: «1.2b» чаще значит «исправленная 1.2», а не «бета».
    private static readonly Regex Dotted = new(@"(?<![\p{L}\p{Nd}.])v?(\d+(?:\.\d+)+)(?![\p{Nd}])(?:(?:[-_. ]?(?<pre>alpha|beta|rc|preview|pre|dev|wip|test|[ab](?=\d))(?![\p{L}])\.?(?<pn>\d+)?)|(?<suf>[a-z])(?![\p{L}\p{Nd}]))?", RegexOptions.IgnoreCase | RegexOptions.Compiled);
    private static readonly Regex VPrefixed = new(@"(?<![\p{L}\p{Nd}])v(\d+)(?![\p{Nd}.])", RegexOptions.IgnoreCase | RegexOptions.Compiled);

    /// <summary>Ранг стабильности: пред-релиз раньше релиза той же версии (0.3.1-beta &lt; 0.3.1), alpha &lt; beta &lt; rc.</summary>
    private static int PreRank(string? tag) => tag?.ToLowerInvariant() switch
    {
        null or "" => 100,
        "rc" => 3,
        "beta" or "b" => 2,
        "alpha" or "a" => 1,
        _ => 0, // dev, wip, test, pre, preview
    };

    public static int[]? TryParse(string? text)
    {
        if (string.IsNullOrWhiteSpace(text)) return null;

        var m = Dotted.Match(text);
        var digits = m.Success ? m.Groups[1].Value : VPrefixed.Match(text) is { Success: true } v ? v.Groups[1].Value : null;
        if (digits is null) return null;

        var parts = digits.Split('.');
        var result = new int[parts.Length];
        for (var i = 0; i < parts.Length; i++)
            if (!int.TryParse(parts[i], out result[i])) return null;
        return result;
    }

    /// <summary>Как «current» соотносится с «installed»: Newer значит, что на сайте версия свежее.</summary>
    public static VersionOrder Compare(string? installed, string? current)
    {
        var a = TryParse(installed);
        var b = TryParse(current);
        if (a is null || b is null) return VersionOrder.Unknown;
        var (rankA, numA, sufA) = Pre(installed);
        var (rankB, numB, sufB) = Pre(current);

        var len = Math.Max(a.Length, b.Length);
        for (var i = 0; i < len; i++)
        {
            var x = i < a.Length ? a[i] : 0;
            var y = i < b.Length ? b[i] : 0;
            if (y > x) return VersionOrder.Newer;
            if (y < x) return VersionOrder.Older;
        }
        // Числа равны: решает пред-релизная метка (beta → релиз = обновление, rc1 → rc2 = обновление).
        if (rankB != rankA) return rankB > rankA ? VersionOrder.Newer : VersionOrder.Older;
        if (numB != numA) return numB > numA ? VersionOrder.Newer : VersionOrder.Older;
        if (sufB != sufA) return sufB > sufA ? VersionOrder.Newer : VersionOrder.Older; // 1.2 < 1.2a < 1.2b
        return VersionOrder.Same;
    }

    private static (int Rank, int Num, char Suffix) Pre(string? text)
    {
        if (string.IsNullOrWhiteSpace(text)) return (100, 0, '\0');
        var m = Dotted.Match(text);
        if (!m.Success) return (100, 0, '\0');
        var tag = m.Groups["pre"].Success ? m.Groups["pre"].Value : null;
        var n = m.Groups["pn"].Success && int.TryParse(m.Groups["pn"].Value, out var v) ? v : 0;
        var suf = m.Groups["suf"].Success ? char.ToLowerInvariant(m.Groups["suf"].Value[0]) : '\0';
        return (PreRank(tag), tag is null ? 0 : Math.Max(n, 1), suf); // «beta» без номера = «beta1»
    }
}
