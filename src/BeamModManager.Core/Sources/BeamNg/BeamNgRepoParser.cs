using System.Globalization;
using System.Net;
using System.Text.RegularExpressions;
using BeamModManager.Core.Models;

namespace BeamModManager.Core.Sources.BeamNg;

/// <summary>
/// Разбор официального репозитория beamng.com/resources (XenForo Resource Manager).
/// Лента по умолчанию отсортирована по «Last Update». Адреса на странице относительные (base — корень сайта).
/// У мода есть «Unique ID» (например MOKU3EJNG), тот самый, который использует игра в beamng:v1/showMod/{id}.
/// </summary>
public static class BeamNgRepoParser
{
    public const string SourceId = "beamng";
    public static readonly Uri Base = new("https://beamng.com/");

    private static readonly Regex Item = new(@"<li[^>]*class=""[^""]*\bresourceListItem\b[^""]*""[^>]*\bid=""resource-(?<id>\d+)""[^>]*>(?<b>.*?)(?=<li[^>]*class=""[^""]*\bresourceListItem\b|</ol>|\z)", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex TitleBlock = new(@"<h3 class=""title"">(?<t>.*?)</h3>", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex TitleLink = new(@"<a href=""(?<h>resources/[^""?]+/)""[^>]*>(?<n>[^<]+)</a>", RegexOptions.Compiled);
    private static readonly Regex VersionSpan = new(@"<span class=""(?:version|muted)"">(?<v>[^<]*)</span>", RegexOptions.Compiled);
    private static readonly Regex Author = new(@"<a href=""resources/authors/[^""]*"">(?<a>[^<]+)</a>", RegexOptions.Compiled);
    private static readonly Regex Category = new(@"<a href=""resources/categories/[^""]*"">(?<c>[^<]+)</a>", RegexOptions.Compiled);
    private static readonly Regex Updated = new(@"<dl class=""resourceUpdated"">.*?<(?:abbr|span)(?<attrs>[^>]*)>(?<txt>[^<]*)</(?:abbr|span)>", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex DataTime = new(@"data-time=""(?<t>\d+)""", RegexOptions.Compiled);
    private static readonly Regex TitleAttr = new(@"title=""(?<t>[^""]+)""", RegexOptions.Compiled);
    private static readonly Regex UniqueId = new(@"Unique ID:</dt>\s*<dd[^>]*>\s*(?<id>[A-Za-z0-9]+)\s*</dd>", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex DownloadBtn = new(@"<a[^>]*href=""(?<h>resources/[^""]+/download\?version=\d+)""[^>]*>\s*Download Now\s*(?:<small[^>]*>\s*(?<sz>[^<]*?)\s*</small>)?", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex SizeRx = new(@"(?<n>\d+(?:[.,]\d+)?)\s*(?<u>KB|MB|GB)", RegexOptions.IgnoreCase | RegexOptions.Compiled);
    private static readonly Regex Tags = new("<[^>]+>", RegexOptions.Compiled);

    public static IReadOnlyList<Listing> ParseList(string html)
    {
        var result = new List<Listing>();
        foreach (Match m in Item.Matches(html))
        {
            var b = m.Groups["b"].Value;
            var tb = TitleBlock.Match(b);
            if (!tb.Success) continue;
            var link = TitleLink.Match(tb.Groups["t"].Value);
            if (!link.Success || !Uri.TryCreate(Base, WebUtility.HtmlDecode(link.Groups["h"].Value), out var url)) continue;
            var name = WebUtility.HtmlDecode(link.Groups["n"].Value).Trim();
            var ver = VersionSpan.Match(tb.Groups["t"].Value);
            var upd = Updated.Match(b);
            var author = Author.Match(b);
            var cat = Category.Match(b);
            result.Add(new Listing(SourceId, url, m.Groups["id"].Value, name)
            {
                ModVersion = ver.Success && ver.Groups["v"].Value.Trim().Length > 0 ? WebUtility.HtmlDecode(ver.Groups["v"].Value).Trim() : null,
                Author = author.Success ? WebUtility.HtmlDecode(author.Groups["a"].Value).Trim() : null,
                Category = cat.Success ? WebUtility.HtmlDecode(cat.Groups["c"].Value).Trim() : null,
                UpdatedAt = upd.Success ? ParseDate(upd.Groups["attrs"].Value, upd.Groups["txt"].Value) : null,
            });
        }
        return result;
    }

    /// <summary>Страница мода: Unique ID, размер и прямая ссылка «Download Now» (файл раздаётся самим сайтом).</summary>
    public static Listing ParseDetail(string html, Listing basis)
    {
        var uid = UniqueId.Match(html);
        var btn = DownloadBtn.Match(html);
        var offers = new List<Offer>();
        long? size = basis.SizeBytes;
        if (btn.Success && Uri.TryCreate(Base, WebUtility.HtmlDecode(btn.Groups["h"].Value), out var u))
        {
            if (btn.Groups["sz"].Success) size = ParseSize(btn.Groups["sz"].Value) ?? size;
            offers.Add(new Offer(OfferKind.Direct, "beamng.com", u, SizeBytes: size));
        }
        var cat = Category.Match(html);
        var author = Author.Match(html);
        var h1 = Regex.Match(html, @"<h1>.*?<span class=""muted"">(?<v>[^<]+)</span>", RegexOptions.Singleline);
        return basis with
        {
            ExternalId = uid.Success ? uid.Groups["id"].Value : basis.ExternalId,
            SizeBytes = size,
            Author = basis.Author ?? (author.Success ? WebUtility.HtmlDecode(author.Groups["a"].Value).Trim() : null),
            Category = basis.Category ?? (cat.Success ? WebUtility.HtmlDecode(cat.Groups["c"].Value).Trim() : null),
            ModVersion = h1.Success ? WebUtility.HtmlDecode(h1.Groups["v"].Value).Trim() : basis.ModVersion,
            Offers = offers,
            // Файл отдаёт сам сайт, без оплаты: положительный признак бесплатности.
            Price = offers.Count > 0 ? PriceStatus.Free : basis.Price,
        };
    }

    public static string SlugOf(Uri url)
    {
        var last = url.AbsolutePath.TrimEnd('/').Split('/').Last();
        var m = Regex.Match(last, @"\.(\d+)$");
        return m.Success ? m.Groups[1].Value : last;
    }

    private static DateTimeOffset? ParseDate(string attrs, string text)
    {
        // Точнее всего epoch в data-time; иначе «Oct 13, 2026 at 3:17 PM» (часовой пояс страницы неизвестен, считаем UTC).
        var dt = DataTime.Match(attrs);
        if (dt.Success && long.TryParse(dt.Groups["t"].Value, out var s)) return DateTimeOffset.FromUnixTimeSeconds(s);
        var cand = new[] { TitleAttr.Match(attrs) is { Success: true } t ? t.Groups["t"].Value : "", text };
        foreach (var c in cand)
            if (DateTimeOffset.TryParseExact(c.Replace(" at ", " "), new[] { "MMM d, yyyy h:mm tt", "MMM d, yyyy" },
                    CultureInfo.InvariantCulture, DateTimeStyles.AssumeUniversal, out var d)) return d;
        return null;
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
