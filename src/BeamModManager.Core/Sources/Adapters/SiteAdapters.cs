using System.Net;
using System.Text.RegularExpressions;
using BeamModManager.Core.Models;
using BeamModManager.Core.Net;
using BeamModManager.Core.Sources.Modland;
using BeamModManager.Core.Sources.ModsGaming;
using BeamModManager.Core.Sources.StMods;

namespace BeamModManager.Core.Sources;

/// <summary>Сайт ответил так, что результата нет: проверка «вы человек?», сбой, перегрузка.</summary>
public sealed class SourceException : Exception
{
    public SourceStatus Status { get; }
    public SourceException(SourceStatus status, string message) : base(message) { Status = status; }
}

/// <summary>Общая часть адаптеров: загрузка через вежливый слой и перевод ответа в страницу или исключение.</summary>
public abstract class SiteAdapter : IModSource
{
    protected readonly PoliteFetcher Fetcher;
    protected SiteAdapter(PoliteFetcher fetcher) { Fetcher = fetcher; }

    public abstract string Id { get; }
    public abstract string DisplayName { get; }
    public abstract bool SupportsSearch { get; }

    protected abstract IReadOnlyList<Listing> ParseList(string html);
    protected abstract Listing ParseDetail(string html, Listing basis);
    protected abstract Uri FeedUrl(int page);
    protected abstract Uri? SearchUrl(string query);
    protected abstract string SlugOf(Uri url);

    /// <summary>Страница или null, если её нет (404). Остальные неудачи — SourceException.</summary>
    protected async Task<string?> LoadAsync(Uri url, CancellationToken ct)
    {
        var r = await Fetcher.GetAsync(url, ct).ConfigureAwait(false);
        if (r.IsSuccess) return r.Body;
        if (r.IsNotFound) return null;
        throw new SourceException(r.Source == SourceStatus.Ok ? SourceStatus.Unavailable : r.Source, r.Note ?? "сайт не ответил");
    }

    public async Task<IReadOnlyList<Listing>> SearchAsync(string query, CancellationToken ct = default)
    {
        var url = SearchUrl(query);
        if (url is null) return Array.Empty<Listing>();
        var html = await LoadAsync(url, ct).ConfigureAwait(false);
        return html is null ? Array.Empty<Listing>() : ParseList(html);
    }

    public async Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default)
    {
        var html = await LoadAsync(FeedUrl(Math.Max(1, page)), ct).ConfigureAwait(false);
        return html is null ? Array.Empty<Listing>() : ParseList(html);
    }

    public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? since, CancellationToken ct = default) => FeedPageAsync(1, ct);

    public async Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default)
    {
        var html = await LoadAsync(url, ct).ConfigureAwait(false);
        if (html is null) return null;
        var basis = new Listing(Id, url, SlugOf(url), PageTitle(html));
        var parsed = Text.TitleParser.Parse(basis.Title);
        return ParseDetail(html, basis with { ModVersion = parsed.ModVersion, GameVersion = parsed.GameVersion });
    }

    public async Task<IReadOnlyList<Offer>> GetOffersAsync(Listing listing, CancellationToken ct = default)
    {
        if (listing.Offers.Count > 0) return listing.Offers;
        return (await GetListingAsync(listing.Url, ct).ConfigureAwait(false))?.Offers ?? Array.Empty<Offer>();
    }

    private static readonly Regex H1 = new(@"<h1[^>]*>(?<t>.*?)</h1>", RegexOptions.Singleline | RegexOptions.IgnoreCase | RegexOptions.Compiled);
    private static readonly Regex TitleTag = new(@"<title[^>]*>(?<t>.*?)</title>", RegexOptions.Singleline | RegexOptions.IgnoreCase | RegexOptions.Compiled);
    private static readonly Regex AnyTag = new("<[^>]+>", RegexOptions.Compiled);
    private static readonly Regex Comment = new("<!--.*?-->", RegexOptions.Singleline | RegexOptions.Compiled);

    protected static string PageTitle(string html)
    {
        var m = H1.Match(html);
        if (!m.Success) m = TitleTag.Match(html);
        var raw = m.Success ? m.Groups["t"].Value : "";
        var t = Regex.Replace(WebUtility.HtmlDecode(AnyTag.Replace(Comment.Replace(raw, ""), " ")), @"\s+", " ").Trim();
        // «Скачать мод …» и «Мод …» на STMods, « - BeamNG.drive» на Modland
        t = Regex.Replace(t, @"^(?:Скачать\s+)?[Мм]од\s+", "");
        t = Regex.Replace(t, @"\s+-\s+BeamNG\.drive$", "", RegexOptions.IgnoreCase);
        return t;
    }
}

public sealed class ModlandSource : SiteAdapter
{
    private static readonly Uri Root = new("https://www.modland.net/");
    public ModlandSource(PoliteFetcher f) : base(f) { }
    public override string Id => ModlandParser.SourceId;
    public override string DisplayName => "Modland";
    public override bool SupportsSearch => true;
    protected override IReadOnlyList<Listing> ParseList(string html) => ModlandParser.ParseList(html);
    protected override Listing ParseDetail(string html, Listing basis) => ModlandParser.ParseDetail(html, basis);
    protected override string SlugOf(Uri url) => ModlandParser.SlugOf(url);
    protected override Uri FeedUrl(int page) => new(Root, page <= 1 ? "beamng.drive-mods" : $"beamng.drive-mods/page/{page}");
    protected override Uri? SearchUrl(string q) => new(Root, "search/" + Uri.EscapeDataString(q));
}

public sealed class StModsSource : SiteAdapter
{
    private static readonly Uri Root = new("https://stmods.org/");
    public StModsSource(PoliteFetcher f) : base(f) { }
    public override string Id => StModsParser.SourceId;
    public override string DisplayName => "STMods";
    public override bool SupportsSearch => true;
    protected override IReadOnlyList<Listing> ParseList(string html) => StModsParser.ParseList(html);
    protected override Listing ParseDetail(string html, Listing basis) => StModsParser.ParseDetail(html, basis);
    protected override string SlugOf(Uri url) => StModsParser.SlugOf(url);
    protected override Uri FeedUrl(int page) => new(Root, page <= 1 ? "beamng_drive/" : $"beamng_drive/?page={page}");
    protected override Uri? SearchUrl(string q) => new(Root, "search.php?search_request=" + Uri.EscapeDataString(q));
}

/// <summary>Поиск ModsGaming не включён: образца страницы с результатами у нас нет. Лента и карточки работают.</summary>
public sealed class ModsGamingSource : SiteAdapter
{
    private static readonly Uri Root = new("https://modsgaming.ru/");
    public ModsGamingSource(PoliteFetcher f) : base(f) { }
    public override string Id => ModsGamingParser.SourceId;
    public override string DisplayName => "ModsGaming";
    public override bool SupportsSearch => false;
    protected override IReadOnlyList<Listing> ParseList(string html) => ModsGamingParser.ParseList(html);
    protected override Listing ParseDetail(string html, Listing basis) => ModsGamingParser.ParseDetail(html, basis);
    protected override string SlugOf(Uri url) => Slug(url);
    protected override Uri FeedUrl(int page) => new(Root, page <= 1 ? "load/beamng_drive/mashiny/38" : $"load/beamng_drive/mashiny/38-{page}");
    protected override Uri? SearchUrl(string q) => null;

    private static string Slug(Uri url)
    {
        // Идентификатор записи — число в конце адреса: …/38-1-0-160435
        var last = url.AbsolutePath.TrimEnd('/').Split('/').Last();
        var m = Regex.Match(last, @"-(\d+)$");
        return m.Success ? m.Groups[1].Value : last;
    }
}
