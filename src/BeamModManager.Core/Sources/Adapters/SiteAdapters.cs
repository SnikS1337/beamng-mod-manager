using System.Net;
using System.Text.RegularExpressions;
using BeamModManager.Core.Models;
using BeamModManager.Core.Net;
using BeamModManager.Core.Sources.Modland;
using BeamModManager.Core.Sources.ModsGaming;
using BeamModManager.Core.Sources.BeamNg;
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
    /// <summary>Страница результатов поиска; по умолчанию устроена как лента.</summary>
    protected virtual IReadOnlyList<Listing> ParseSearch(string html) => ParseList(html);
    protected abstract Listing ParseDetail(string html, Listing basis);
    protected abstract Uri FeedUrl(int page);
    protected abstract Uri? SearchUrl(string query);
    /// <summary>Форма, которую поиск отправляет на <see cref="SearchUrl"/> (null — обычный GET).</summary>
    protected virtual string? SearchForm(string query) => null;
    /// <summary>Сколько страниц выдачи читать: сайт без сортировки и с 10 результатами на странице — больше одной.</summary>
    protected virtual int SearchPages => 1;
    /// <summary>Адрес следующей страницы выдачи (со второй); null — страниц больше нет.</summary>
    protected virtual Uri? SearchPageUrl(string query, int page) => null;
    protected abstract string SlugOf(Uri url);
    /// <summary>Главная страница сайта: по ней проверяем, что адрес карточки действительно принадлежит этому сайту.</summary>
    protected abstract Uri SiteRoot { get; }

    /// <summary>Главная страница сайта (кнопка «Открыть сайт», сброс паузы после проверки «вы человек?»).</summary>
    public Uri Home => SiteRoot;

    /// <summary>Адрес страницы этого сайта (а не чужого): по нему окно «Указать страницу» выбирает адаптер.</summary>
    public bool Owns(Uri url) => BelongsToSite(url);

    /// <summary>
    /// Адрес похож на страницу одного мода, а не на раздел, поиск или профиль. Нужен окну «Указать страницу»: раздел
    /// тоже открывается, но следить на нём не за чем (размер первого мода в списке менялся бы каждый день).
    /// </summary>
    public bool IsModPage(Uri url) => BelongsToSite(url) && IsModPath(url.AbsolutePath.Split('/', StringSplitOptions.RemoveEmptyEntries));

    /// <summary>Части пути адреса («beamng_drive», «mods», «bmw_e34»): страница ли это одного мода.</summary>
    protected abstract bool IsModPath(string[] segments);

    /// <summary>
    /// На странице есть хоть что-то, по чему видно обновление: версия, размер или файл. Нет ничего — это не страница мода
    /// или сайт поменял вёрстку, и разбор её больше не понимает.
    /// </summary>
    public static bool Readable(Listing l) => l.ModVersion is not null || l.SizeBytes is not null || l.Offers.Count > 0;

    /// <summary>Адрес привязки вводит пользователь; чужой сайт, другая схема или «file://» этим адаптером не открываются.</summary>
    protected bool BelongsToSite(Uri url)
    {
        if (url.Scheme != Uri.UriSchemeHttps && url.Scheme != Uri.UriSchemeHttp) return false;
        static string Bare(string h) => h.ToLowerInvariant().StartsWith("www.") ? h.ToLowerInvariant()[4..] : h.ToLowerInvariant();
        var site = Bare(SiteRoot.Host);
        var host = Bare(url.Host);
        return host == site || host.EndsWith("." + site);
    }

    /// <summary>
    /// Разбор страницы с защитой от неожиданностей: слишком сложная страница (таймаут регулярного выражения)
    /// или любая ошибка разбора означает отказ только этого сайта, а не падение программы.
    /// </summary>
    protected T Safe<T>(Func<T> parse, string what)
    {
        try { return parse(); }
        catch (RegexMatchTimeoutException) { throw new SourceException(SourceStatus.Unavailable, $"{what}: страница слишком сложная для разбора"); }
        catch (Exception e) when (e is FormatException or ArgumentException or InvalidOperationException or IndexOutOfRangeException or OverflowException or NullReferenceException)
        { throw new SourceException(SourceStatus.Unavailable, $"{what}: не удалось разобрать страницу ({e.GetType().Name})"); }
    }

    /// <summary>Страница или null, если её нет (404). Остальные неудачи — SourceException.</summary>
    protected async Task<string?> LoadAsync(Uri url, CancellationToken ct, string? form = null)
    {
        var r = form is null ? await Fetcher.GetAsync(url, ct).ConfigureAwait(false) : await Fetcher.PostFormAsync(url, form, ct).ConfigureAwait(false);
        if (r.IsSuccess) return r.Body;
        if (r.IsNotFound) return null;
        throw new SourceException(r.Source == SourceStatus.Ok ? SourceStatus.Unavailable : r.Source, r.Note ?? "сайт не ответил");
    }

    public async Task<IReadOnlyList<Listing>> SearchAsync(string query, CancellationToken ct = default)
    {
        var url = SearchUrl(query);
        if (url is null) return Array.Empty<Listing>();
        var html = await LoadAsync(url, ct, SearchForm(query)).ConfigureAwait(false);
        if (html is null) return Array.Empty<Listing>();
        var found = Safe(() => ParseSearch(html), "поиск").ToList();
        for (var p = 2; p <= SearchPages && found.Count > 0; p++)
        {
            if (SearchPageUrl(query, p) is not { } next) break;
            string? more;
            try { more = await LoadAsync(next, ct).ConfigureAwait(false); }
            catch (SourceException) { break; } // первая страница уже есть: сбой следующей не отменяет найденное
            if (more is null) break;
            var page = Safe(() => ParseSearch(more), "поиск");
            var added = page.Where(l => found.All(f => f.SourceId != l.SourceId)).ToList();
            if (added.Count == 0) break;
            found.AddRange(added);
        }
        return found;
    }

    public async Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default)
    {
        var html = await LoadAsync(FeedUrl(Math.Max(1, page)), ct).ConfigureAwait(false);
        return html is null ? Array.Empty<Listing>() : Safe(() => ParseList(html), "лента");
    }

    public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? since, CancellationToken ct = default) => FeedPageAsync(1, ct);

    public async Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default)
    {
        if (!BelongsToSite(url)) return null; // страницы «нет»: сообщим как ненайденную, чужой сайт не опрашиваем
        var html = await LoadAsync(url, ct).ConfigureAwait(false);
        if (html is null) return null;
        var basis = Safe(() => new Listing(Id, url, SlugOf(url), PageTitle(html)), "карточка");
        var parsed = Text.TitleParser.Parse(basis.Title);
        return Safe(() => ParseDetail(html, basis with { ModVersion = parsed.ModVersion, GameVersion = parsed.GameVersion }), "карточка");
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
    protected override Uri SiteRoot => Root;
    protected override Uri FeedUrl(int page) => new(Root, page <= 1 ? "beamng.drive-mods" : $"beamng.drive-mods/page/{page}");
    // /beamng.drive-mods/cars/toyota-sienna-4.html (бывает и без раздела); разделы, страницы ленты и моды других игр — нет
    protected override bool IsModPath(string[] p) =>
        p.Length >= 2 && p[0].Equals("beamng.drive-mods", StringComparison.OrdinalIgnoreCase) && p[^1].EndsWith(".html", StringComparison.OrdinalIgnoreCase) && p[^1].Length > ".html".Length;
    // Запрос из одних точек («..») превратился бы в переход вверх по адресу.
    protected override Uri? SearchUrl(string q) => q.Trim('.', ' ').Length == 0 ? null : new(Root, "search/" + Uri.EscapeDataString(q));
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
    protected override Uri SiteRoot => Root;
    protected override Uri FeedUrl(int page) => new(Root, page <= 1 ? "beamng_drive/" : $"beamng_drive/?page={page}");
    // Только раздел BeamNG.drive (без него ищет по всем играм: из первых 10 найденных для BeamNG 4–5, остальное — City Car
    // Driving и SpinTires); 50 самых свежих за раз — сортировку и число на странице сайт берёт только из формы
    // (проверено на живом сайте 2026-10-18: без формы 10 штук по «релевантности», первыми моды 2017 года).
    protected override Uri? SearchUrl(string q) => new(Root, "search.php?search_request=" + Uri.EscapeDataString(q) + "&partition=beamng");
    protected override string? SearchForm(string q) => "per_page_search=50&search_order_by=" + Uri.EscapeDataString("`date_public` DESC");
    // /beamng_drive/mods/bmw_e34_by_matissator_for_beamng/; /beamng_drive/ и /beamng_drive/mods/ — разделы
    protected override bool IsModPath(string[] p) => p.Length >= 3 && p[0].Equals("beamng_drive", StringComparison.OrdinalIgnoreCase);
}

/// <summary>ModsGaming: лента раздела «BeamNG Машины», карточки и поиск (результаты — таблицы, а не карточки).</summary>
public sealed class ModsGamingSource : SiteAdapter
{
    private static readonly Uri Root = new("https://modsgaming.ru/");
    public ModsGamingSource(PoliteFetcher f) : base(f) { }
    public override string Id => ModsGamingParser.SourceId;
    public override string DisplayName => "ModsGaming";
    public override bool SupportsSearch => true;
    protected override IReadOnlyList<Listing> ParseList(string html) => ModsGamingParser.ParseList(html);
    protected override IReadOnlyList<Listing> ParseSearch(string html) => ModsGamingParser.ParseSearch(html);
    protected override Listing ParseDetail(string html, Listing basis) => ModsGamingParser.ParseDetail(html, basis);
    protected override string SlugOf(Uri url) => Slug(url);
    protected override Uri SiteRoot => Root;
    protected override Uri FeedUrl(int page) => new(Root, page <= 1 ? "load/beamng_drive/mashiny/38" : $"load/beamng_drive/mashiny/38-{page}");
    // Поиск сайта идёт по всем играм, 10 на странице, без сортировки по дате: по «bmw» на первой странице — City Car
    // Driving, GTA 5, Farming Simulator, ни одного BeamNG. С «beamng.drive» в запросе — только BeamNG (проверено 2026-10-18).
    protected override Uri? SearchUrl(string q) => new(Root, "search/?q=" + Uri.EscapeDataString(ForBeamNg(q)) + "&t=0");
    protected override int SearchPages => 2;
    protected override Uri? SearchPageUrl(string q, int page) => new(Root, "search/?q=" + Uri.EscapeDataString(ForBeamNg(q)) + "&t=0&p=" + page);
    private static string ForBeamNg(string q) => q.Contains("beamng", StringComparison.OrdinalIgnoreCase) ? q : q.Trim() + " beamng.drive";
    // /load/beamng_drive/mashiny/38-1-0-160435 или …/mod_vaz_2106_…; раздел — /load/beamng_drive/mashiny/38 и 38-2 (страницы)
    protected override bool IsModPath(string[] p) =>
        p.Length >= 4 && p[0].Equals("load", StringComparison.OrdinalIgnoreCase) && p[1].Equals("beamng_drive", StringComparison.OrdinalIgnoreCase) && !Regex.IsMatch(p[^1], @"^\d+(?:-\d+)?$");

    private static string Slug(Uri url)
    {
        // Идентификатор записи — число в конце адреса: …/38-1-0-160435
        var last = url.AbsolutePath.TrimEnd('/').Split('/').Last();
        var m = Regex.Match(last, @"-(\d+)$");
        return m.Success ? m.Groups[1].Value : last;
    }
}

/// <summary>
/// Официальный репозиторий beamng.com/resources. Лента по умолчанию — «Last Update», 100 модов на странице.
/// Поиск не включён: форма поиска XenForo отправляется POST-запросом с токеном, образца результатов нет.
/// </summary>
public sealed class BeamNgRepoSource : SiteAdapter
{
    private static readonly Uri Root = new("https://beamng.com/");
    public BeamNgRepoSource(PoliteFetcher f) : base(f) { }
    public override string Id => BeamNgRepoParser.SourceId;
    public override string DisplayName => "BeamNG (официальный)";
    public override bool SupportsSearch => false;
    protected override IReadOnlyList<Listing> ParseList(string html) => BeamNgRepoParser.ParseList(html);
    protected override Listing ParseDetail(string html, Listing basis) => BeamNgRepoParser.ParseDetail(html, basis);
    protected override string SlugOf(Uri url) => BeamNgRepoParser.SlugOf(url);
    protected override Uri SiteRoot => Root;
    protected override Uri FeedUrl(int page) => new(Root, page <= 1 ? "resources/" : $"resources/?page={page}");
    protected override Uri? SearchUrl(string q) => null;
    // /resources/acng-tire-heat.34567/ (и вкладки записи: …/updates); лента /resources/ и разделы /resources/categories/… — нет
    protected override bool IsModPath(string[] p) => p.Length >= 2 && p[0].Equals("resources", StringComparison.OrdinalIgnoreCase) && Regex.IsMatch(p[1], @"\.\d+$");
}
