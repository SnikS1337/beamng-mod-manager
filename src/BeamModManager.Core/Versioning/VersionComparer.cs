using System.Text.RegularExpressions;

namespace BeamModManager.Core.Versioning;

public enum VersionOrder { Unknown, Older, Same, Newer }

/// <summary>
/// Сравнение версий, записанных как попало («v1.3.2», «1.3», «Update 2!»).
/// Понимает только числа с точками или с префиксом v; всё остальное считается неразобранным.
/// </summary>
public static class VersionComparer
{
    private static readonly Regex Dotted = new(@"(?<![\p{L}\p{Nd}.])v?(\d+(?:\.\d+)+)(?![\p{Nd}])", RegexOptions.IgnoreCase | RegexOptions.Compiled);
    private static readonly Regex VPrefixed = new(@"(?<![\p{L}\p{Nd}])v(\d+)(?![\p{Nd}.])", RegexOptions.IgnoreCase | RegexOptions.Compiled);

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

        var len = Math.Max(a.Length, b.Length);
        for (var i = 0; i < len; i++)
        {
            var x = i < a.Length ? a[i] : 0;
            var y = i < b.Length ? b[i] : 0;
            if (y > x) return VersionOrder.Newer;
            if (y < x) return VersionOrder.Older;
        }
        return VersionOrder.Same;
    }
}
