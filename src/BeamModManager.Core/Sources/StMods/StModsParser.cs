using System.Globalization;
using System.Net;
using System.Text.RegularExpressions;
using BeamModManager.Core.Models;
using BeamModManager.Core.Text;

namespace BeamModManager.Core.Sources.StMods;

/// <summary>
/// Разбор STMods (список, поиск, карточка мода). Кнопка скачивания ведёт на внутренний
/// адрес /download_file.php?update_id=…; лаунчер его не открывает и не раскрывает,
/// он нужен только как признак «файл есть, хост такой-то, размер такой-то».
/// </summary>
public static class StModsParser
{
    public const string SourceId = "stmods";
    public static readonly Uri Base = new("https://stmods.org/");

    private static readonly Regex Card = new(@"<div class=""row index-card""[^>]*>(?<b>.*?)(?=<div class=""row index-card""|\z)", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex CardHref = new(@"<a[^>]*class=""[^""]*index-card-img[^""]*""[^>]*href=""(?<h>[^""]+)""", RegexOptions.Singleline | RegexOptions.Compiled);
    // картинка карточки — фон ссылки: style="background-image: url('/beamng_drive/mods/…/…_img1pre.jpg?')"
    private static readonly Regex CardImage = new(@"<a[^>]*class=""[^""]*index-card-img[^""]*""[^>]*background-image:\s*url\(\s*['""]?(?<u>[^'""\)]+)", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex CardTitle = new(@"<h3 class=""text-left"">\s*<b>(?<n>.*?)</b>\s*(?:<small>(?<s>.*?)</small>)?", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex AuthorLink = new(@"Автор:\s*<a[^>]*>(?<a>.*?)</a>", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex DownloadBtn = new(@"<a[^>]*href=""(?<h>/download_file\.php\?[^""]+)""[^>]*>\s*Скачать\s+мод\s*(?:\((?<sz>[^)]*)\))?\s*(?:<br>\s*<small>\s*с\s+(?<host>[^<]+?)\s*</small>)?", RegexOptions.Singleline | RegexOptions.Compiled);
    private static readonly Regex Tags = new("<[^>]+>", RegexOptions.Compiled);
    private static readonly Regex SizeRx = new(@"(?<n>\d+(?:[\s ]?\d{3})*(?:[.,]\d+)?)\s*(?<u>КБ|МБ|ГБ|KB|MB|GB)", RegexOptions.IgnoreCase | RegexOptions.Compiled);

    public static IReadOnlyList<Listing> ParseList(string html)
    {
        var result = new List<Listing>();
        foreach (Match m in Card.Matches(html))
        {
            var b = m.Groups["b"].Value;
            var href = CardHref.Match(b);
            var title = CardTitle.Match(b);
            if (!href.Success || !title.Success) continue;
            if (!Uri.TryCreate(Base, WebUtility.HtmlDecode(href.Groups["h"].Value), out var url)) continue;
            // В поиске попадаются моды других игр.
            if (!url.AbsolutePath.StartsWith("/beamng_drive/", StringComparison.Ordinal)) continue;

            var name = Clean(title.Groups["n"].Value);
            var tail = Clean(title.Groups["s"].Value);
            var full = tail.Length > 0 ? $"{name} {tail}" : name;
            var parsed = TitleParser.Parse(full);
            var author = AuthorLink.Match(b);
            var image = CardImage.Match(b);
            Uri? preview = null;
            if (image.Success && Uri.TryCreate(Base, WebUtility.HtmlDecode(image.Groups["u"].Value.Trim()), out var p) && p.Scheme == Uri.UriSchemeHttps)
                preview = p;

            result.Add(new Listing(SourceId, url, SlugOf(url), full)
            {
                ModVersion = parsed.ModVersion,
                GameVersion = parsed.GameVersion,
                Author = author.Success ? Clean(author.Groups["a"].Value) : null,
                Price = parsed.HasPaidMarker ? PriceStatus.Paid : PriceStatus.Unknown,
                PreviewUrl = preview,
            });
        }
        return result;
    }

    /// <summary>Отмечена ли запись в списке как обновление уже существующего мода.</summary>
    public static bool IsUpdateMarked(string cardHtml) => cardHtml.Contains(">ОБНОВЛЕНИЕ<", StringComparison.Ordinal);

    public static Listing ParseDetail(string html, Listing basis)
    {
        var offers = new List<Offer>();
        long? size = basis.SizeBytes;
        // Первая кнопка на странице относится к актуальной версии; ниже идут прошлые версии.
        var d = DownloadBtn.Match(html);
        if (d.Success && Uri.TryCreate(Base, WebUtility.HtmlDecode(d.Groups["h"].Value), out var u))
        {
            if (d.Groups["sz"].Success) size = ParseSize(d.Groups["sz"].Value) ?? size;
            var host = d.Groups["host"].Success ? Clean(d.Groups["host"].Value).ToLowerInvariant() : "stmods";
            offers.Add(new Offer(OfferKind.ExternalHost, host, u, SizeBytes: size));
        }

        var a = AuthorLink.Match(html);
        return basis with
        {
            Author = a.Success ? Clean(a.Groups["a"].Value) : basis.Author,
            SizeBytes = size,
            Offers = offers,
        };
    }

    public static long? ParseSize(string text)
    {
        var m = SizeRx.Match(text);
        if (!m.Success) return null;
        var num = Regex.Replace(m.Groups["n"].Value, @"[\s ]", "").Replace(',', '.');
        var n = double.Parse(num, CultureInfo.InvariantCulture);
        var mult = m.Groups["u"].Value.ToUpperInvariant() switch
        {
            "КБ" or "KB" => 1024d,
            "МБ" or "MB" => 1024d * 1024,
            _ => 1024d * 1024 * 1024,
        };
        return (long)(n * mult);
    }

    public static string SlugOf(Uri url) =>
        url.AbsolutePath.TrimEnd('/').Split('/').Last();

    private static string Clean(string s) =>
        Regex.Replace(WebUtility.HtmlDecode(Tags.Replace(s, " ")), @"\s+", " ").Trim();
}
