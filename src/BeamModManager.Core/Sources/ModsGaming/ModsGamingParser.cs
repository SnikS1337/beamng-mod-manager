using System.Globalization;
using System.Net;
using System.Text.RegularExpressions;
using BeamModManager.Core.Models;
using BeamModManager.Core.Text;

namespace BeamModManager.Core.Sources.ModsGaming;

/// <summary>
/// Разбор ModsGaming (uCoz). Ссылка «СКАЧАТЬ» ведёт на внутренний адрес /load/0-0-1-{id}-20,
/// который сайт сам перенаправляет на файлообменник; лаунчер её не открывает и не обходит,
/// она нужна только как признак «скачивание существует».
/// </summary>
public static class ModsGamingParser
{
    public const string SourceId = "modsgaming";
    public static readonly Uri Base = new("https://modsgaming.ru/");

    private static readonly Regex Entry = new(@"<div[^>]*\bid=""entryID(?<id>\d+)""[^>]*>(?<b>.*?)(?=<div[^>]*\bid=""entryID\d+""|\z)", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex TitleLink = new(@"<a(?<attrs>[^>]*\bclass=""[^""]*\bvidmattit\b[^""]*""[^>]*)>(?<t>.*?)</a>", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex HrefAttr = new(@"\bhref=""(?<h>[^""]+)""", RegexOptions.Compiled);
    private static readonly Regex Tags = new("<[^>]+>", RegexOptions.Compiled);
    private static readonly Regex Comment = new("<!--.*?-->", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex Author = new(@"АВТОР\s+МОДА\s*:\s*(?<a>[^\r\n<]+?)\s*(?:<|\r|\n|$)", RegexOptions.IgnoreCase | RegexOptions.Compiled);
    private static readonly Regex DownloadLink = new(@"<a(?=[^>]*\bclass=""[^""]*filedowl-a)(?=[^>]*\bhref=""(?<h>[^""]+)"")[^>]*>[^<]*</a>\s*(?:<span[^>]*>)?\s*(?<size>[\d.,]+\s*(?:KB|MB|GB))?", RegexOptions.IgnoreCase | RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex SizeRx = new(@"(?<n>\d+(?:[.,]\d+)?)\s*(?<u>KB|MB|GB)\b", RegexOptions.IgnoreCase | RegexOptions.Compiled);

    public static IReadOnlyList<Listing> ParseList(string html)
    {
        var result = new List<Listing>();
        foreach (Match m in Entry.Matches(html))
        {
            var tl = TitleLink.Match(m.Groups["b"].Value);
            if (!tl.Success) continue;
            var href = HrefAttr.Match(tl.Groups["attrs"].Value);
            if (!href.Success || !Uri.TryCreate(Base, WebUtility.HtmlDecode(href.Groups["h"].Value), out var url)) continue;
            var title = Clean(tl.Groups["t"].Value);
            if (title.Length == 0) continue;
            var p = TitleParser.Parse(title);
            result.Add(new Listing(SourceId, url, m.Groups["id"].Value, title)
            {
                ModVersion = p.ModVersion,
                GameVersion = p.GameVersion,
                Category = CategoryOf(url),
                // Платный контент в этом разделе не наблюдался, но «бесплатно» без доказательств не ставим.
                Price = p.HasPaidMarker ? PriceStatus.Paid : PriceStatus.Unknown,
            });
        }
        return result;
    }

    private static readonly Regex SearchLink = new(@"<td[^>]*class=""search-td-1""[^>]*>\s*<a[^>]*\bhref=""(?<h>[^""]+)""[^>]*>(?<t>.*?)</a>", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex SearchDate = new(@"class=""search-span-2""[^>]*>.*?(?<d>\d{4}-\d{2}-\d{2}(?:\s+\d{2}:\d{2}:\d{2})?)", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex EntryId = new(@"-(?<id>\d+)$", RegexOptions.Compiled);

    /// <summary>
    /// Страница поиска: каждый результат — своя таблица со ссылкой, датой и фрагментом описания
    /// (размера и автора там нет, их даёт карточка). Берём только раздел BeamNG; подсветка найденных слов (&lt;b&gt;) снимается.
    /// </summary>
    public static IReadOnlyList<Listing> ParseSearch(string html)
    {
        var result = new List<Listing>();
        var seen = new HashSet<string>();
        var links = SearchLink.Matches(html);
        for (var i = 0; i < links.Count; i++)
        {
            var m = links[i];
            if (!Uri.TryCreate(Base, WebUtility.HtmlDecode(m.Groups["h"].Value), out var url)) continue;
            if (!url.AbsolutePath.StartsWith("/load/beamng_drive/", StringComparison.Ordinal)) continue;
            var idm = EntryId.Match(url.AbsolutePath.TrimEnd('/'));
            if (!idm.Success || !seen.Add(idm.Groups["id"].Value)) continue;
            var title = Clean(m.Groups["t"].Value);
            if (title.Length == 0) continue;
            // Дата лежит в той же таблице, до следующего результата.
            var end = i + 1 < links.Count ? links[i + 1].Index : html.Length;
            var d = SearchDate.Match(html.Substring(m.Index, end - m.Index));
            var p = TitleParser.Parse(title);
            result.Add(new Listing(SourceId, url, idm.Groups["id"].Value, title)
            {
                ModVersion = p.ModVersion,
                GameVersion = p.GameVersion,
                Category = CategoryOf(url),
                PublishedAt = d.Success && DateTimeOffset.TryParse(d.Groups["d"].Value, CultureInfo.InvariantCulture, DateTimeStyles.AssumeUniversal, out var when) ? when : null,
                Price = p.HasPaidMarker ? PriceStatus.Paid : PriceStatus.Unknown,
            });
        }
        return result;
    }

    public static Listing ParseDetail(string html, Listing basis)
    {
        var withoutComments = Comment.Replace(html, "");
        var a = Author.Match(Tags.Replace(withoutComments, "\n"));
        var d = DownloadLink.Match(withoutComments);
        var offers = new List<Offer>();
        long? size = basis.SizeBytes;
        if (d.Success)
        {
            if (d.Groups["size"].Success) size = ParseSize(d.Groups["size"].Value);
            if (Uri.TryCreate(Base, WebUtility.HtmlDecode(d.Groups["h"].Value), out var u))
                offers.Add(new Offer(OfferKind.Direct, "modsgaming", u, SizeBytes: size));
        }
        return basis with
        {
            Author = a.Success ? WebUtility.HtmlDecode(a.Groups["a"].Value).Trim() : basis.Author,
            SizeBytes = size,
            Offers = offers,
            PreviewUrl = ImageOf(withoutComments) ?? basis.PreviewUrl,
        };
    }

    // главная картинка мода: <td class="detimg" style="background:url(/_ld/1604/53750859.jpg);">; запасная — первый снимок
    // (screendiv). Картинки «Похожих модов» (poh-td) — чужие, их не берём
    private static readonly Regex DetailImage = new(@"class=""(?:detimg|screendiv-1)""[^>]*background:\s*url\(\s*['""]?(?<u>/_ld/[^'""\)]+)", RegexOptions.Compiled);

    /// <summary>Картинка мода на его странице (в выдаче поиска картинок нет — 2026-10-18: карточки ModsGaming были без фото).</summary>
    public static Uri? ImageOf(string html) =>
        DetailImage.Match(html) is { Success: true } m && Uri.TryCreate(Base, WebUtility.HtmlDecode(m.Groups["u"].Value.Trim()), out var u)
        && u.Scheme == Uri.UriSchemeHttps ? u : null;

    public static long? ParseSize(string text)
    {
        var m = SizeRx.Match(text);
        if (!m.Success) return null;
        var n = double.Parse(m.Groups["n"].Value.Replace(',', '.'), CultureInfo.InvariantCulture);
        var mult = m.Groups["u"].Value.ToUpperInvariant() switch { "KB" => 1024d, "MB" => 1024d * 1024, _ => 1024d * 1024 * 1024 };
        return (long)(n * mult);
    }

    private static string? CategoryOf(Uri url)
    {
        var parts = url.AbsolutePath.Split('/', StringSplitOptions.RemoveEmptyEntries);
        return parts.Length >= 3 && parts[0] == "load" ? parts[2] : null;
    }

    private static string Clean(string s) =>
        Regex.Replace(WebUtility.HtmlDecode(Tags.Replace(Comment.Replace(s, ""), " ")), @"\s+", " ").Trim();
}
