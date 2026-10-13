using System.Globalization;
using System.Net;
using System.Text.RegularExpressions;
using BeamModManager.Core.Models;
using BeamModManager.Core.Pricing;
using BeamModManager.Core.Text;

namespace BeamModManager.Core.Sources.Modland;

/// <summary>Разбор страниц Modland (список и карточка мода). Только разбор текста, без сети.</summary>
public static class ModlandParser
{
    public const string SourceId = "modland";
    public static readonly Uri Base = new("https://www.modland.net/");

    private static readonly Regex PostBlock = new(@"<div[^>]*class=""[^""]*\bpost-medium\b[^""]*""[^>]*>(?<b>.*?)(?=<div[^>]*class=""[^""]*\bpost-medium\b|\z)", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex TitleLink = new(@"post-title[^>]*>\s*<a[^>]*href=""(?<h>[^""]+)""[^>]*>(?<t>.*?)</a>", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex Tags = new("<[^>]+>", RegexOptions.Compiled);
    private static readonly Regex Size = new(@"(?<n>\d+(?:[.,]\d+)?)\s*(?<u>KB|MB|GB)\b", RegexOptions.IgnoreCase | RegexOptions.Compiled);
    private static readonly Regex Anchor = new(@"<a[^>]*href=""(?<h>[^""]+)""[^>]*>(?<t>.*?)</a>", RegexOptions.Singleline | RegexOptions.Compiled);

    public static IReadOnlyList<Listing> ParseList(string html)
    {
        var result = new List<Listing>();
        foreach (Match m in PostBlock.Matches(html))
        {
            var body = m.Groups["b"].Value;
            var tl = TitleLink.Match(body);
            if (!tl.Success || !Uri.TryCreate(Base, WebUtility.HtmlDecode(tl.Groups["h"].Value), out var url)) continue;

            var title = Clean(tl.Groups["t"].Value);
            if (title.Length == 0) continue;

            string? author = null, category = null;
            foreach (Match a in Anchor.Matches(body))
            {
                var href = a.Groups["h"].Value;
                if (author is null && href.Contains("/user/")) author = Clean(a.Groups["t"].Value);
                else if (category is null && Uri.TryCreate(Base, WebUtility.HtmlDecode(href), out var au)
                         && au.Host.EndsWith("modland.net")
                         && au.AbsolutePath.TrimEnd('/').Split('/', StringSplitOptions.RemoveEmptyEntries) is { Length: 2 } seg
                         && seg[0] == "beamng.drive-mods" && !seg[1].EndsWith(".html") && seg[1] != "page")
                    category = Clean(a.Groups["t"].Value);
            }

            var parsed = TitleParser.Parse(title);
            result.Add(new Listing(SourceId, url, SlugOf(url), title)
            {
                Author = author,
                Category = category,
                SizeBytes = ParseSize(Tags.Replace(body, " ")),
                ModVersion = parsed.ModVersion,
                GameVersion = parsed.GameVersion,
                // Сайт сам ставит значок «$» (Paid content) рядом с названием: это главный признак, метка в тексте не нужна.
                Price = HasPaidBadge(tl.Groups["t"].Value) ? PriceStatus.Paid : PaidDetector.Detect(title, null, null),
            });
        }
        return result;
    }

    private static readonly Regex H1Title = new(@"<h1[^>]*class=""[^""]*post-title[^""]*""[^>]*>(?<b>.*?)</h1>", RegexOptions.Singleline | RegexOptions.Compiled);

    /// <summary>Значок платного контента рядом с названием (в ленте и на странице мода).</summary>
    public static bool HasPaidBadge(string titleHtml) =>
        titleHtml.Contains("title=\"Paid content\"", StringComparison.OrdinalIgnoreCase);

    private static string TitleBlock(string html) => H1Title.Match(html) is { Success: true } m ? m.Groups["b"].Value : "";

    private static readonly Regex ExternalTitle = new("title=\"External Download - (?<u>https?://[^\"\\s]+)\"", RegexOptions.Compiled);

    /// <summary>Дополняет запись данными со страницы мода: версия, цена, ссылки.</summary>
    public static Listing ParseDetail(string html, Listing basis)
    {
        var offers = new List<Offer>();
        var downloadUrls = new List<Uri>();
        var sawExternalBlock = html.Contains("External Download Links", StringComparison.OrdinalIgnoreCase);
        // Внешние ссылки Modland открывает через окно-подтверждение: адрес лежит в title="External Download - URL",
        // а href у такой ссылки нет. Берём их вместе с обычными ссылками.
        var hrefs = Anchor.Matches(html).Select(a => WebUtility.HtmlDecode(a.Groups["h"].Value))
            .Concat(sawExternalBlock
                ? ExternalTitle.Matches(html).Select(m => WebUtility.HtmlDecode(m.Groups["u"].Value))
                : Enumerable.Empty<string>())
            .Distinct();
        foreach (var href in hrefs)
        {
            if (!Uri.TryCreate(href, UriKind.Absolute, out var u)) continue;
            var host = u.Host.StartsWith("www.") ? u.Host[4..] : u.Host;
            if (sawExternalBlock && Hosts.HostLinkParser.PaidHosts.Any(p => host.EndsWith(p, StringComparison.OrdinalIgnoreCase)))
            {
                if (offers.All(o => o.Url != u)) offers.Add(new Offer(OfferKind.PaidLink, host, u));
                downloadUrls.Add(u);
            }
            else if (Hosts.HostLinkParser.TryParse(href) is { } link)
            {
                offers.Add(new Offer(OfferKind.ExternalHost, link.Host, u, link.FileId));
                downloadUrls.Add(u);
            }
        }

        var text = WebUtility.HtmlDecode(Tags.Replace(html, " "));
        var latest = Regex.Match(text, @"Latest Version\s+(?<v>\d+(?:\.\d+)*)");
        var gv = Regex.Match(text, @"Game Version\s*:?\s*(?<v>\d+(?:\.\d+)*(?:\.x)?)");
        var price = HasPaidBadge(TitleBlock(html)) ? PriceStatus.Paid : PaidDetector.Detect(basis.Title, null, downloadUrls);

        return basis with
        {
            ModVersion = latest.Success ? latest.Groups["v"].Value : basis.ModVersion,
            GameVersion = gv.Success ? gv.Groups["v"].Value : basis.GameVersion,
            SizeBytes = basis.SizeBytes ?? ParseSize(text),
            Price = price == PriceStatus.Unknown ? basis.Price : price,
            Offers = offers,
        };
    }

    public static string SlugOf(Uri url) =>
        Path.GetFileNameWithoutExtension(url.AbsolutePath.TrimEnd('/'));

    public static long? ParseSize(string text)
    {
        var m = Size.Match(text);
        if (!m.Success) return null;
        var n = double.Parse(m.Groups["n"].Value.Replace(',', '.'), CultureInfo.InvariantCulture);
        var mult = m.Groups["u"].Value.ToUpperInvariant() switch { "KB" => 1024d, "MB" => 1024d * 1024, _ => 1024d * 1024 * 1024 };
        return (long)(n * mult);
    }

    private static string Clean(string s) =>
        Regex.Replace(WebUtility.HtmlDecode(Tags.Replace(s, " ")), @"\s+", " ").Trim();
}
