using System.Globalization;
using System.Net;
using System.Text.RegularExpressions;
using BeamModManager.Core.Hosts;

namespace BeamModManager.Core.Sources.ModsFire;

/// <summary>Метаданные файла с первой страницы ModsFire.</summary>
/// <summary>Какая страница ModsFire перед нами.</summary>
public enum ModsFireStep { Unknown, First, Second }

public sealed record ModsFireFile(string FileName, string Extension, long? SizeBytes, DateTimeOffset? UploadedAt, string? FileId);

/// <summary>
/// Читает только то, что видно на первой странице файла (имя, размер, дата).
/// Кнопки «Generate Link» и «Download» нажимает пользователь в окне загрузки;
/// ссылку и токен со страницы разбор не использует.
/// </summary>
public static class ModsFireParser
{
    private static readonly Regex Name = new(@"<h1>\s*Download File\s*</h1>.*?<h3>\s*(?<n>[^<]+?)\s*</h3>", RegexOptions.Singleline | RegexOptions.IgnoreCase | RegexOptions.Compiled);
    private static readonly Regex Ext = new(@"class=""file-ext"">\s*(?<e>[^<\s]+)", RegexOptions.Compiled);
    private static readonly Regex SizeP = new(@"data-bs-original-title=""File size""[^>]*>.*?<p>\s*(?<s>[^<]+?)\s*</p>", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex DateP = new(@"data-bs-original-title=""File upload date""[^>]*>.*?<p>\s*(?<d>\d{4}-\d{2}-\d{2}(?:\s+\d{2}:\d{2})?)\s*</p>", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex Id = new(@"(?:report\?u=|/download/|/d/|/install_mod/)(?<id>[A-Za-z0-9]{6,32})", RegexOptions.Compiled);
    private static readonly Regex SizeRx = new(@"(?<n>\d+(?:[.,]\d+)?)\s*(?<u>KB|MB|GB)", RegexOptions.IgnoreCase | RegexOptions.Compiled);

    /// <summary>
    /// Первый шаг: кнопка «Generate Link» на /download/{id}/{токен}. Второй: кнопка с id="downloadnow" на /d/{id}.
    /// Определяем только для подсказки в окне загрузки; ни одну из ссылок лаунчер не открывает сам.
    /// </summary>
    public static ModsFireStep DetectStep(string html)
    {
        if (html.Contains("id=\"downloadnow\"", StringComparison.Ordinal)) return ModsFireStep.Second;
        if (html.Contains("Generate Link", StringComparison.OrdinalIgnoreCase)) return ModsFireStep.First;
        return ModsFireStep.Unknown;
    }

    /// <summary>Метаданные файла на любом из двух шагов (имя, размер и дата показаны на обоих).</summary>
    public static ModsFireFile? ParseFile(string html) => ParseFirstStep(html);

    public static ModsFireFile? ParseFirstStep(string html)
    {
        var n = Name.Match(html);
        if (!n.Success) return null;
        var id = Id.Match(html);
        var size = SizeP.Match(html);
        var date = DateP.Match(html);
        var ext = Ext.Match(html);

        DateTimeOffset? when = null;
        if (date.Success && DateTimeOffset.TryParse(date.Groups["d"].Value, CultureInfo.InvariantCulture,
                DateTimeStyles.AssumeUniversal, out var parsed))
            when = parsed;

        return new ModsFireFile(
            WebUtility.HtmlDecode(n.Groups["n"].Value),
            ext.Success ? ext.Groups["e"].Value.ToLowerInvariant() : "",
            size.Success ? ParseSize(size.Groups["s"].Value) : null,
            when,
            id.Success && HostLinkParser.TryParse($"https://modsfire.com/{id.Groups["id"].Value}") is { } l ? l.FileId : null);
    }

    public static long? ParseSize(string text)
    {
        var m = SizeRx.Match(text);
        if (!m.Success) return null;
        var n = double.Parse(m.Groups["n"].Value.Replace(',', '.'), CultureInfo.InvariantCulture);
        var mult = m.Groups["u"].Value.ToUpperInvariant() switch { "KB" => 1024d, "MB" => 1024d * 1024, _ => 1024d * 1024 * 1024 };
        return (long)(n * mult);
    }
}
