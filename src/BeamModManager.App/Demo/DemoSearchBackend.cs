using System.IO;
using BeamModManager.Core.Matching;
using BeamModManager.Core.Models;
using BeamModManager.Core.Search;
using BeamModManager.Core.Sources;
using BeamModManager.Ui.Search;

namespace BeamModManager.App.Demo;

/// <summary>
/// «Поиск» для снимков на сборочной машине: сайты не опрашиваются, на любой запрос — один и тот же набор, как на доске
/// «Поиск» (бесплатные, платный, без цены, установленный, «возможно, тот же мод»). Сайты отвечают по очереди.
/// Картинки — образцы из design/demo (адреса file://, окно берёт их как есть).
/// </summary>
public sealed class DemoSearchBackend : ISearchBackend
{
    private readonly SearchService _service;

    public DemoSearchBackend(string photos)
    {
        Uri? P(string n) { var p = Path.Combine(photos, n + ".jpg"); return File.Exists(p) ? new Uri(Path.GetFullPath(p)) : null; }
        Listing L(string site, string id, string title, string? ver, string? author, PriceStatus price, string? photo, string host) =>
            new(site, new Uri($"https://{host}/{id}"), id, title) { ModVersion = ver, Author = author, Price = price, PreviewUrl = photo is null ? null : P(photo) };
        var modland = new Site("modland", 0,
            L("modland", "1", "Lamborghini Huracán STO 1.0", "1.0", "garage47", PriceStatus.Free, "lambo", "modland.net"),
            L("modland", "2", "Lamborghini Huracán EVO 1.6", "1.6", "volt_lab", PriceStatus.Free, "coupe", "modland.net"),
            L("modland", "3", "Lamborghini Aventador SVJ 2.0", "2.0", "pro_mods", PriceStatus.Paid, "mustang", "modland.net"),
            L("modland", "4", "Lamborghini Diablo GT 0.9", "0.9", "retro_gt", PriceStatus.Free, "dacia", "modland.net"),
            L("modland", "5", "Lamborghini Countach LPI 800-4 1.2 (Replace)", "1.2", "kuro_garage", PriceStatus.Free, "truck", "modland.net"));
        var gaming = new Site("modsgaming", 150,
            L("modsgaming", "a", "МОД Lamborghini Huracán STO v1.0 ДЛЯ BEAMNG.DRIVE", "1.0", null, PriceStatus.Unknown, null, "modsgaming.ru"),
            L("modsgaming", "b", "Lamborghini Urus Performante v1.3", "1.3", "andrey_s", PriceStatus.Unknown, null, "modsgaming.ru"),
            L("modsgaming", "c", "Lamborghini Countach LPI v1.2", "1.2", null, PriceStatus.Unknown, null, "modsgaming.ru"));
        var st = new Site("stmods", 300,
            L("stmods", "x", "Lamborghini Gallardo (2008) версия 1.4 для BeamNG.drive", "1.4", "st_team", PriceStatus.Free, null, "stmods.ru"),
            L("stmods", "y", "Lamborghini Huracán STO версия 1.0", "1.0", null, PriceStatus.Free, null, "stmods.ru"));
        _service = new SearchService(new IModSource[] { modland, gaming, st }, decisions: Decisions);
    }

    public MatchDecisionStore Decisions { get; } = new();

    public IReadOnlyList<SearchSite> Sites() => new[]
    {
        new SearchSite("modland", "Modland", true, true),
        new SearchSite("modsgaming", "ModsGaming", true, true),
        new SearchSite("stmods", "STMods", true, true),
        new SearchSite("beamng", "Репозиторий BeamNG", true, false),
    };

    public Task<SearchResponse> SearchAsync(string query, Action<SearchResponse>? progress, CancellationToken ct) => _service.SearchAsync(query, progress, ct);
    public SearchResponse Regroup(SearchResponse previous, string query) => _service.Regroup(previous, query);
    public void Decide(Listing a, Listing b, ManualMatch decision) => Decisions.Set(a, b, decision);

    private sealed class Site(string id, int delayMs, params Listing[] items) : IModSource
    {
        public string Id => id;
        public string DisplayName => id;
        public bool SupportsSearch => true;
        public async Task<IReadOnlyList<Listing>> SearchAsync(string query, CancellationToken ct = default)
        {
            if (delayMs > 0) await Task.Delay(delayMs, ct).ConfigureAwait(false);
            return items;
        }
        public Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default) => Task.FromResult<Listing?>(null);
        public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? since, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<IReadOnlyList<Offer>> GetOffersAsync(Listing listing, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Offer>>(Array.Empty<Offer>());
    }
}
