namespace BeamModManager.Core.Net;

/// <summary>
/// Страницы одних сайтов — через встроенный браузер, остальных — обычным запросом (2026-10-18, Modland). Сайт за защитой
/// Cloudflare не пускает простой запрос программы, а браузер с пройденной человеком проверкой пускает. Вежливость (паузы,
/// кэш, «вы человек?») — снаружи, в <see cref="PoliteFetcher"/>, одна на всех.
/// </summary>
public sealed class RoutingPageFetcher : IPageFetcher
{
    private readonly IPageFetcher _plain, _browser;
    private readonly HashSet<string> _browserHosts;

    public RoutingPageFetcher(IPageFetcher plain, IPageFetcher browser, IEnumerable<string> browserHosts)
    {
        _plain = plain;
        _browser = browser;
        _browserHosts = new HashSet<string>(browserHosts, StringComparer.OrdinalIgnoreCase);
    }

    public bool UsesBrowser(Uri url) => _browserHosts.Contains(url.Host);

    public Task<RawResponse> GetAsync(Uri url, CancellationToken ct) =>
        (UsesBrowser(url) ? _browser : _plain).GetAsync(url, ct);

    public Task<RawResponse> PostFormAsync(Uri url, string form, CancellationToken ct) =>
        (UsesBrowser(url) ? _browser : _plain).PostFormAsync(url, form, ct);
}
