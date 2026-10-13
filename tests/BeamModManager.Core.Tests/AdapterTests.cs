using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Net;
using BeamModManager.Core.Search;
using BeamModManager.Core.Sources;

namespace BeamModManager.Core.Tests;

public static class AdapterTests
{
    private static string Fx(string path)
    {
        var dir = AppContext.BaseDirectory;
        while (dir != null && !Directory.Exists(Path.Combine(dir, "tests", "fixtures"))) dir = Path.GetDirectoryName(dir);
        return File.ReadAllText(Path.Combine(dir!, "tests", "fixtures", path));
    }

    private sealed class Pages : IPageFetcher
    {
        public readonly List<Uri> Requested = new();
        private readonly Dictionary<string, (int Status, string Body)> _map;
        public Pages(Dictionary<string, (int, string)> map) { _map = map; }
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct)
        {
            Requested.Add(url);
            return Task.FromResult(_map.TryGetValue(url.AbsoluteUri, out var r) ? new RawResponse(r.Status, r.Body) : new RawResponse(404, ""));
        }
    }

    private static PoliteFetcher Polite(IPageFetcher p) =>
        new(p, new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero });

    private const string Challenge = "<html><head><title>Один момент…</title></head><body>Проверяем, человек ли вы</body></html>";

    public static void Register(TestRunner t)
    {
        t.Group("Адаптеры сайтов");

        t.Run("STMods: поиск строит адрес с экранированием и разбирает страницу результатов", async () =>
        {
            var pages = new Pages(new() { ["https://stmods.org/search.php?search_request=toyota%20sienna"] = (200, Fx("stmods/search-real.html")) });
            var src = new StModsSource(Polite(pages));
            var r = await src.SearchAsync("toyota sienna");
            Assert.Eq(3, r.Count);
            Assert.True(r.All(l => l.Source == "stmods"), "источник");
        });

        t.Run("Modland: поиск и лента идут по адресам, которые мы видели", async () =>
        {
            var list = Fx("modland/list-real.html");
            var pages = new Pages(new()
            {
                ["https://www.modland.net/search/sienna"] = (200, list),
                ["https://www.modland.net/beamng.drive-mods"] = (200, list),
                ["https://www.modland.net/beamng.drive-mods/page/3"] = (200, list),
            });
            var src = new ModlandSource(Polite(pages));
            Assert.Eq(7, (await src.SearchAsync("sienna")).Count);
            Assert.Eq(7, (await src.FeedPageAsync(1)).Count);
            Assert.Eq(7, (await src.FeedPageAsync(3)).Count);
        });

        t.Run("ModsGaming: поиск отключён и в общий поиск не попадает, лента работает", async () =>
        {
            var pages = new Pages(new() { ["https://modsgaming.ru/load/beamng_drive/mashiny/38-2"] = (200, Fx("modsgaming/list-real.html")) });
            var src = new ModsGamingSource(Polite(pages));
            Assert.False(src.SupportsSearch);
            Assert.Eq(10, (await src.FeedPageAsync(2)).Count);
            var resp = await new SearchService(new IModSource[] { src }).SearchAsync("bmw");
            Assert.Eq(0, pages.Requested.Count - 1);
            Assert.True(resp.Sources.Single().Note!.Contains("не поддерживается"), "пометка в отчёте");
        });

        t.Run("карточка STMods: заголовок из h1, размер и хост", async () =>
        {
            var url = new Uri("https://stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng/");
            var pages = new Pages(new() { [url.ToString()] = (200, Fx("stmods/mod-real.html")) });
            var l = await new StModsSource(Polite(pages)).GetListingAsync(url);
            Assert.True(l is not null, "найдено");
            Assert.Eq("3.3", l!.ModVersion);
            Assert.Eq("matissator", l.Author);
            Assert.Eq("modsfire", l.Offers.Single().Host);
        });

        t.Run("карточка Modland платная: Paid и Patreon", async () =>
        {
            var url = new Uri("https://www.modland.net/beamng.drive-mods/maps/paid-duskport-realistic-city-map.html");
            var pages = new Pages(new() { [url.ToString()] = (200, Fx("modland/mod-paid-real.html")) });
            var l = await new ModlandSource(Polite(pages)).GetListingAsync(url);
            Assert.Eq(PriceStatus.Paid, l!.Price);
        });

        t.Run("несуществующая карточка: null, не исключение", async () =>
        {
            var l = await new StModsSource(Polite(new Pages(new()))).GetListingAsync(new Uri("https://stmods.org/beamng_drive/mods/нет/"));
            Assert.True(l is null, "null");
        });

        t.Run("проверка «вы человек?» превращается в статус источника в общем поиске", async () =>
        {
            var pages = new Pages(new() { ["https://www.modland.net/search/car"] = (403, Challenge) });
            var src = new ModlandSource(Polite(pages));
            var resp = await new SearchService(new IModSource[] { src }).SearchAsync("car");
            Assert.Eq(SourceStatus.NeedsHumanCheck, resp.Sources.Single().Status);
            Assert.Eq(0, resp.Cards.Count);
        });

        t.Group("Привязка установленных модов");

        t.Run("имя файла: автор и название из слага STMods", () =>
        {
            var g = BindingSuggester.FromFileName("bmw_e34_by_matissator_for_beamng.zip");
            Assert.Eq("bmw e34", g.Name);
            Assert.Eq("matissator", g.Author);
        });

        t.Run("имя файла: метка сайта и версия через подчёркивание", () =>
        {
            var g = BindingSuggester.FromFileName("ibishu-miramar-dirt-race_ModLandNet.zip");
            Assert.Eq("ibishu miramar dirt race", g.Name);
            var g2 = BindingSuggester.FromFileName("mercedes_g_wagon-1_0.zip");
            Assert.Eq("mercedes g wagon", g2.Name);
            Assert.Eq("1.0", g2.Version);
        });

        t.Run("имя с цифрой модели не принимается за версию", () =>
            Assert.Eq("bmw e34", BindingSuggester.FromFileName("bmw_e34.zip").Name));

        var mod = new InstalledMod("e34", "BMW E34", "bmw_e34_by_matissator_for_beamng.zip", "3.3", 223_053_086, DateTimeOffset.UnixEpoch);

        t.Run("подбор: совпали название, автор и размер: можно подтвердить автоматически", () =>
        {
            var c = new Listing("stmods", new Uri("https://stmods.org/x/"), "x", "BMW E34 версия 3.3 для BeamNG.drive (v0.39.x)")
            { Author = "matissator", SizeBytes = 223_053_086 };
            var s = BindingSuggester.Suggest(mod, new[] { c }).Single();
            Assert.True(s.AutoConfirmable, $"оценка {s.Match.Score}");
        });

        t.Run("подбор: только название: кандидат есть, автоподтверждения нет", () =>
        {
            var c = new Listing("modland", new Uri("https://m/x.html"), "x", "BMW E34");
            var s = BindingSuggester.Suggest(mod with { SizeBytes = null }, new[] { c }).Single();
            Assert.False(s.AutoConfirmable);
        });

        t.Run("подбор: другой автор с тем же названием отсеивается", () =>
        {
            var c = new Listing("stmods", new Uri("https://stmods.org/y/"), "y", "BMW E34 версия 1.0 для BeamNG.drive") { Author = "bobrex", SizeBytes = 223_053_086 };
            Assert.Eq(0, BindingSuggester.Suggest(mod, new[] { c }).Count);
        });

        t.Run("подбор: лучшие кандидаты первыми, не больше заданного числа", () =>
        {
            var good = new Listing("stmods", new Uri("https://stmods.org/a/"), "a", "BMW E34") { Author = "matissator", SizeBytes = 223_000_000 };
            var weak = new Listing("modland", new Uri("https://m/b.html"), "b", "BMW E34");
            var r = BindingSuggester.Suggest(mod, new[] { weak, good }, max: 1);
            Assert.Eq("stmods", r.Single().Listing.Source);
        });
    }
}
