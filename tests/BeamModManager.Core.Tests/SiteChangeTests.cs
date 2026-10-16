using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Net;
using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Sources.StMods;
using BeamModManager.Ui.Updates;

namespace BeamModManager.Core.Tests;

/// <summary>
/// Сайт поменял страницы. Главное — не молчать: страница, которую разбор больше не понимает, — это «не прочитали»,
/// а не «обновлений нет». И ссылка на раздел сайта вместо мода не принимается.
/// </summary>
public static class SiteChangeTests
{
    private static readonly DateTimeOffset Now = new(2026, 10, 16, 12, 0, 0, TimeSpan.Zero);
    private static readonly DateTimeOffset Old = Now.AddDays(-2);

    private const string StUrl = "https://stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng/";
    private const string StFeed = "https://stmods.org/beamng_drive/";

    private static string Fx(string path)
    {
        var dir = AppContext.BaseDirectory;
        while (dir != null && !Directory.Exists(Path.Combine(dir, "tests", "fixtures"))) dir = Path.GetDirectoryName(dir);
        return File.ReadAllText(Path.Combine(dir!, "tests", "fixtures", path));
    }

    /// <summary>Сайт из папки образцов: адрес → (код, страница); остальное — 404. Страницы можно менять между проверками.</summary>
    private sealed class Pages : IPageFetcher
    {
        public readonly Dictionary<string, (int Status, string Body)> Map = new();
        public readonly List<string> Requested = new();
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct)
        {
            lock (Requested) Requested.Add(url.AbsoluteUri);
            return Task.FromResult(Map.TryGetValue(url.AbsoluteUri, out var r) ? new RawResponse(r.Status, r.Body) : new RawResponse(404, ""));
        }
    }

    private static PoliteFetcher Polite(Pages p) =>
        new(p, new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero, MaxRetries = 0 });

    private static SiteAdapter[] All(PoliteFetcher f) =>
        new SiteAdapter[] { new ModlandSource(f), new StModsSource(f), new ModsGamingSource(f), new BeamNgRepoSource(f) };

    private static async Task<Listing?> Card(Func<PoliteFetcher, SiteAdapter> make, string url, string html)
    {
        var p = new Pages();
        p.Map[url] = (200, html);
        return await make(Polite(p)).GetListingAsync(new Uri(url));
    }

    /// <summary>Мод в библиотеке, который следят на STMods; прошлая проверка видела версию <paramref name="lastVer"/>.</summary>
    private static InMemoryLibraryStore Library(string installed, string? lastVer, long? lastSize = null, string url = StUrl, string id = "e34")
    {
        var s = new InMemoryLibraryStore();
        Add(s, id, installed, url, lastVer, lastSize);
        return s;
    }

    private static void Add(InMemoryLibraryStore s, string id, string installed, string url, string? lastVer, long? lastSize = null)
    {
        s.UpsertMod(new InstalledMod(id, "BMW " + id, id + ".zip", installed, null, Now.AddDays(-60)));
        var u = new Uri(url);
        s.UpsertBinding(new ModBinding(id, "stmods", StModsParser.SlugOf(u), u, true));
        if (lastVer is not null || lastSize is not null) s.SaveCheck(new CheckState(id, "stmods", Old, Old, lastVer, lastSize));
    }

    // Переделки настоящей карточки STMods, как бывает при смене вёрстки
    private static string StNoVersion(string html) => html.Replace("Мод BMW E34 версия 3.3 для BeamNG.drive (v0.39.x)", "BMW E34");
    private static string StNewButtons(string html) => html.Replace("Скачать мод (", "Download (").Replace("download_file.php", "get.php");
    private static string StRedesign(string html) => StNewButtons(StNoVersion(html));

    private static string Piece(string s, Random rnd)
    {
        if (s.Length == 0) return "";
        var from = rnd.Next(s.Length);
        return s.Substring(from, Math.Min(200, s.Length - from));
    }

    public static void Register(TestRunner t)
    {
        t.Group("Сайт изменился: какие ссылки — страницы модов");

        t.Run("страницы модов узнаются на всех четырёх сайтах", () =>
        {
            var f = Polite(new Pages());
            var a = All(f);
            string[] mods =
            {
                "https://www.modland.net/beamng.drive-mods/cars/toyota-sienna-4.html",
                "https://modland.net/beamng.drive-mods/maps/paid-duskport-realistic-city-map.html",
                "https://www.modland.net/beamng.drive-mods/2011-2017-toyota-sienna.html",
                "http://www.modland.net/BeamNG.drive-mods/cars/x.html",
                StUrl,
                "https://stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng",
                "https://modsgaming.ru/load/beamng_drive/mashiny/38-1-0-160435",
                "https://modsgaming.ru/load/beamng_drive/mashiny/mod_vaz_2106_v6_0_0_38_x_dlja_beamng_drive",
                "https://modsgaming.ru/load/beamng_drive/karty/mod_karta_domik_v_derevne_v1_0_0_38_x_dlja_beamng_drive",
                "https://beamng.com/resources/acng-tire-heat-pressure-wear.34567/",
                "https://www.beamng.com/resources/acng.34567/updates",
            };
            foreach (var m in mods)
                Assert.True(a.Count(x => x.IsModPage(new Uri(m))) == 1, "мод: " + m);
        });

        t.Run("разделы, ленты, поиск, профили, главные и моды других игр — не страницы модов", () =>
        {
            var f = Polite(new Pages());
            var a = All(f);
            string[] sections =
            {
                "https://www.modland.net/", "https://www.modland.net/beamng.drive-mods", "https://www.modland.net/beamng.drive-mods/cars",
                "https://www.modland.net/beamng.drive-mods/page/2", "https://www.modland.net/beamng.drive-mods/version/039x",
                "https://www.modland.net/search/sienna", "https://www.modland.net/user/rudd", "https://www.modland.net/news",
                "https://www.modland.net/gta-5-mods/vehicles/cars/2017-toyota-sienna-xle-premium-10-replaceadd-on.html",
                "https://www.modland.net/beamng.drive-mods/.html",
                "https://stmods.org/", StFeed, "https://stmods.org/beamng_drive/mods/", "https://stmods.org/search.php?search_request=x",
                "https://stmods.org/x/",
                "https://modsgaming.ru/", "https://modsgaming.ru/load/beamng_drive/mashiny/38", "https://modsgaming.ru/load/beamng_drive/mashiny/38-2",
                "https://modsgaming.ru/load/beamng_drive/mashiny", "https://modsgaming.ru/search/?q=sienna&t=0",
                "https://modsgaming.ru/load/farming_simulator/traktory/mod_x",
                "https://beamng.com/", "https://beamng.com/resources/", "https://beamng.com/resources/categories/vehicles.2/",
                "https://beamng.com/forums/x.12/",
                "https://evil.example/beamng_drive/mods/x/", "ftp://stmods.org/beamng_drive/mods/x/",
            };
            foreach (var s in sections)
                Assert.False(a.Any(x => x.IsModPage(new Uri(s))), "не мод: " + s);
        });

        t.Group("Сайт изменился: разбор настоящих страниц");

        t.Run("настоящие страницы модов всех сайтов читаются: есть версия, размер или файл", async () =>
        {
            (Func<PoliteFetcher, SiteAdapter> Make, string Url, string Fx)[] pages =
            {
                (f => new StModsSource(f), StUrl, "stmods/mod-real.html"),
                (f => new ModlandSource(f), "https://www.modland.net/beamng.drive-mods/maps/paid-duskport-realistic-city-map.html", "modland/mod-paid-real.html"),
                (f => new ModlandSource(f), "https://www.modland.net/beamng.drive-mods/cars/x.html", "modland/pack-golf-kofi-real.html"),
                (f => new ModlandSource(f), "https://www.modland.net/beamng.drive-mods/cars/y.html", "modland/mod-free.html"),
                (f => new ModsGamingSource(f), "https://modsgaming.ru/load/beamng_drive/mashiny/38-1-0-160435", "modsgaming/mod-real.html"),
                (f => new BeamNgRepoSource(f), "https://beamng.com/resources/acng.34567/", "beamng/mod-real.html"),
            };
            foreach (var (make, url, fx) in pages)
            {
                var l = await Card(make, url, Fx(fx));
                Assert.True(l is not null && SiteAdapter.Readable(l), "читается: " + fx);
            }
        });

        t.Run("ленты и поиск, открытые как карточка, не читаются (кроме Modland, его отсекает адрес)", async () =>
        {
            Assert.False(SiteAdapter.Readable((await Card(f => new StModsSource(f), StUrl, Fx("stmods/list-real.html")))!), "STMods лента");
            Assert.False(SiteAdapter.Readable((await Card(f => new StModsSource(f), StUrl, Fx("stmods/search-real.html")))!), "STMods поиск");
            Assert.False(SiteAdapter.Readable((await Card(f => new ModsGamingSource(f), "https://modsgaming.ru/load/beamng_drive/mashiny/38-1-0-1", Fx("modsgaming/list-real.html")))!), "ModsGaming лента");
            Assert.False(SiteAdapter.Readable((await Card(f => new BeamNgRepoSource(f), "https://beamng.com/resources/a.1/", Fx("beamng/list-real.html")))!), "BeamNG лента");
            // лента Modland даёт размер первого мода — поэтому ссылки на разделы не принимаются по адресу
            Assert.True(SiteAdapter.Readable((await Card(f => new ModlandSource(f), "https://www.modland.net/beamng.drive-mods/cars/z.html", Fx("modland/list-real.html")))!), "Modland лента «читается»");
        });

        t.Run("STMods сменил вёрстку: без версии в заголовке — размер из кнопки; без кнопки — версия; без обоих — не читается", async () =>
        {
            var html = Fx("stmods/mod-real.html");
            var noVer = (await Card(f => new StModsSource(f), StUrl, StNoVersion(html)))!;
            Assert.True(noVer.ModVersion is null && noVer.SizeBytes is not null, "размер остался");
            var noBtn = (await Card(f => new StModsSource(f), StUrl, StNewButtons(html)))!;
            Assert.Eq("3.3", noBtn.ModVersion);
            Assert.True(noBtn.Offers.Count == 0, "кнопки не узнаём");
            var both = (await Card(f => new StModsSource(f), StUrl, StRedesign(html)))!;
            Assert.False(SiteAdapter.Readable(both), "ничего не читается");
            Assert.Eq("BMW E34", both.Title);
        });

        t.Run("заглушки вместо страницы (пусто, «на обслуживании», главная) не роняют разбор и не читаются", async () =>
        {
            string[] stubs =
            {
                "",
                "<html><body></body></html>",
                "<html><head><title>Сайт на обслуживании</title></head><body><h1>Скоро вернёмся</h1></body></html>",
                "<h1>404 — страница не найдена</h1>",
                Fx("stmods/list-real.html"),
            };
            foreach (var make in new Func<PoliteFetcher, SiteAdapter>[] { f => new StModsSource(f), f => new ModsGamingSource(f), f => new BeamNgRepoSource(f) })
                foreach (var stub in stubs)
                {
                    var src = make(Polite(new Pages()));
                    var url = src switch
                    {
                        StModsSource => StUrl,
                        ModsGamingSource => "https://modsgaming.ru/load/beamng_drive/mashiny/38-1-0-1",
                        _ => "https://beamng.com/resources/a.1/",
                    };
                    var l = await Card(make, url, stub);
                    Assert.True(l is null || !SiteAdapter.Readable(l), $"{src.Id}: заглушка не читается ({stub.Length} символов)");
                }
        });

        t.Run("испорченные куски настоящих страниц: разбор не падает ничем, кроме понятного «сайт не ответил»", async () =>
        {
            var rnd = new Random(20261016);
            (Func<PoliteFetcher, SiteAdapter> Make, string Url, string Fx)[] pages =
            {
                (f => new StModsSource(f), StUrl, "stmods/mod-real.html"),
                (f => new ModlandSource(f), "https://www.modland.net/beamng.drive-mods/cars/x.html", "modland/pack-golf-kofi-real.html"),
                (f => new ModsGamingSource(f), "https://modsgaming.ru/load/beamng_drive/mashiny/38-1-0-160435", "modsgaming/mod-real.html"),
                (f => new BeamNgRepoSource(f), "https://beamng.com/resources/acng.34567/", "beamng/mod-real.html"),
            };
            var iterations = Environment.GetEnvironmentVariable("BMM_SKIP_FUZZ") == "1" ? 15 : 60;
            foreach (var (make, url, fx) in pages)
            {
                var html = Fx(fx);
                for (var i = 0; i < iterations; i++)
                {
                    var cut = html;
                    for (var k = 0; k < 1 + rnd.Next(6); k++)
                    {
                        var at = rnd.Next(cut.Length);
                        var len = Math.Min(cut.Length - at, rnd.Next(1, 4000));
                        cut = rnd.Next(3) switch
                        {
                            0 => cut.Remove(at, len),                                         // кусок пропал
                            1 => cut.Insert(at, Piece(cut, rnd)),                              // повтор куска
                            _ => cut.Remove(at, len).Insert(at, new string('<', rnd.Next(1, 50))), // мусор вместо разметки
                        };
                    }
                    try { await Card(make, url, cut); }
                    catch (SourceException) { }
                }
            }
        });

        t.Group("Сайт изменился: проверка обновлений не молчит");

        t.Run("страница мода больше не читается: не «обновлений нет», а «не прочитали»; прошлое и срок не тронуты", async () =>
        {
            var p = new Pages();
            p.Map[StFeed] = (200, Fx("stmods/list-real.html"));
            p.Map[StUrl] = (200, StRedesign(Fx("stmods/mod-real.html")));
            var store = Library("3.2", "3.2", url: StUrl);
            // лента не знает этого мода под таким адресом: открывается карточка
            store.UpsertBinding(new ModBinding("e34", "stmods", "e34-renamed", new Uri(StUrl), true));
            var r = await UpdateRunner.RunAsync(store, new IModSource[] { new StModsSource(Polite(p)) }, Now);
            var rep = r.Sources.Single();
            Assert.True(rep.LayoutChanged, "сайт, похоже, изменился");
            Assert.True(rep.Note!.Contains("не читаются"), rep.Note);
            Assert.Eq("BMW e34", rep.UnreadableMods.Single());
            Assert.False(r.Complete, "проверка неполная");
            Assert.Eq(0, r.Findings.Count);
            var c = store.GetCheck("e34", "stmods")!;
            Assert.Eq(Old, c.LastCheckedUtc);
            Assert.Eq("3.2", c.LastVersion);
        });

        t.Run("в следующий раз мод проверяется снова; сайт починили — находится 3.3, отметка уходит", async () =>
        {
            var p = new Pages();
            p.Map[StFeed] = (200, Fx("stmods/list-real.html"));
            p.Map[StUrl] = (200, StRedesign(Fx("stmods/mod-real.html")));
            var store = Library("3.2", "3.2");
            store.UpsertBinding(new ModBinding("e34", "stmods", "e34-renamed", new Uri(StUrl), true));
            var src = new StModsSource(Polite(p));
            await UpdateRunner.RunAsync(store, new IModSource[] { src }, Now);
            p.Map[StUrl] = (200, Fx("stmods/mod-real.html"));
            src = new StModsSource(Polite(p)); // без кэша прошлого ответа
            var r = await UpdateRunner.RunAsync(store, new IModSource[] { src }, Now.AddMinutes(10));
            Assert.Eq(2, p.Requested.Count(u => u == StUrl));
            Assert.Eq("3.3", r.Findings.Single().LatestVersion);
            Assert.False(r.Sources.Single().LayoutChanged, "отметки нет");
            Assert.True(r.Sources.Single().Note is null, r.Sources.Single().Note ?? "");
        });

        t.Run("вместо мода открывается главная (200): не читается, ложного обновления по размеру нет", async () =>
        {
            var p = new Pages();
            p.Map[StFeed] = (200, Fx("stmods/list-real.html"));
            p.Map[StUrl] = (200, Fx("stmods/list-real.html"));
            var store = Library("3.3", "3.3", 223053086);
            store.UpsertBinding(new ModBinding("e34", "stmods", "e34-renamed", new Uri(StUrl), true));
            var r = await UpdateRunner.RunAsync(store, new IModSource[] { new StModsSource(Polite(p)) }, Now);
            Assert.Eq(0, r.Findings.Count);
            Assert.True(r.Sources.Single().LayoutChanged, "заметили");
            Assert.Eq(223053086L, store.GetCheck("e34", "stmods")!.LastSize);
        });

        t.Run("из трёх модов не прочитался один: остальные проверены, в отчёте «1 из 3»", async () =>
        {
            var p = new Pages();
            p.Map[StFeed] = (200, "<html></html>"); // лента пуста — все три идут через карточки
            var urls = new[] { "a", "b", "c" }.Select(x => $"https://stmods.org/beamng_drive/mods/{x}/").ToArray();
            var html = Fx("stmods/mod-real.html");
            p.Map[urls[0]] = (200, html);
            p.Map[urls[1]] = (200, html.Replace("версия 3.3", "версия 3.2"));
            p.Map[urls[2]] = (200, StRedesign(html));
            var store = new InMemoryLibraryStore();
            Add(store, "a", "3.2", urls[0], "3.2");
            Add(store, "b", "3.2", urls[1], "3.2");
            Add(store, "c", "3.2", urls[2], "3.2");
            var r = await UpdateRunner.RunAsync(store, new IModSource[] { new StModsSource(Polite(p)) }, Now);
            var rep = r.Sources.Single();
            Assert.Eq("a", r.Findings.Single().ModId);
            Assert.Eq("BMW c", rep.UnreadableMods.Single());
            Assert.True(rep.LayoutChanged, "заметили");
            // пустая лента тоже говорит о вёрстке; про карточки — отдельной строкой не повторяем
            Assert.True(rep.Note!.Contains("вёрстк"), rep.Note);
            Assert.Eq(Now, store.GetCheck("b", "stmods")!.LastCheckedUtc);
            Assert.Eq(Old, store.GetCheck("c", "stmods")!.LastCheckedUtc);
        });

        t.Run("частичный сбой без пустой ленты: «не прочитали 1 из 2 страниц»", async () =>
        {
            var p = new Pages();
            p.Map[StFeed] = (200, Fx("stmods/list-real.html"));
            var urls = new[] { "a", "b" }.Select(x => $"https://stmods.org/beamng_drive/mods/{x}/").ToArray();
            var html = Fx("stmods/mod-real.html");
            p.Map[urls[0]] = (200, html);
            p.Map[urls[1]] = (200, StRedesign(html));
            var store = new InMemoryLibraryStore();
            Add(store, "a", "3.3", urls[0], "3.3");
            Add(store, "b", "3.3", urls[1], "3.3");
            var r = await UpdateRunner.RunAsync(store, new IModSource[] { new StModsSource(Polite(p)) }, Now);
            Assert.Eq("не прочитали 1 из 2 страниц: возможно, изменилась вёрстка сайта", r.Sources.Single().Note);
        });

        t.Run("мод, о котором раньше ничего не знали, без версии на странице — как раньше: проверен, без отметки", async () =>
        {
            var p = new Pages();
            p.Map[StFeed] = (200, Fx("stmods/list-real.html"));
            p.Map[StUrl] = (200, StRedesign(Fx("stmods/mod-real.html")));
            var store = Library("1.0", null);
            store.UpsertBinding(new ModBinding("e34", "stmods", "e34-renamed", new Uri(StUrl), true));
            var r = await UpdateRunner.RunAsync(store, new IModSource[] { new StModsSource(Polite(p)) }, Now);
            Assert.False(r.Sources.Single().LayoutChanged, "не о чем судить");
            Assert.Eq(Now, store.GetCheck("e34", "stmods")!.LastCheckedUtc);
        });

        t.Run("пустая лента и пропавшие разом все карточки — тоже «возможно, сайт изменился»", async () =>
        {
            var p = new Pages();
            p.Map[StFeed] = (200, "<html><body>Новый дизайн</body></html>");
            var store = new InMemoryLibraryStore();
            foreach (var x in new[] { "a", "b", "c" }) Add(store, x, "1.0", $"https://stmods.org/beamng_drive/mods/{x}/", "1.0");
            var r = await UpdateRunner.RunAsync(store, new IModSource[] { new StModsSource(Polite(p)) }, Now);
            var rep = r.Sources.Single();
            Assert.True(rep.LayoutChanged, "заметили");
            Assert.True(rep.Note!.Contains("карточки не найдены"), rep.Note);
            // пропавшие все разом — виноват сайт: моды не помечаются «страница пропала»
            Assert.True(new[] { "a", "b", "c" }.All(x => store.GetCheck(x, "stmods")!.MissCount == 0), "промахи не засчитаны");
        });

        t.Group("Сайт изменился: окно «Указать страницу»");

        t.Run("ссылка на раздел, ленту, поиск — понятный отказ, сайт не опрашивается", async () =>
        {
            var p = new Pages();
            var f = Polite(p);
            var s = new AppSettings();
            var be = new LibraryUpdatesBackend(new InMemoryLibraryStore(), All(f), () => s, () => Now);
            foreach (var link in new[] { StFeed, "https://www.modland.net/beamng.drive-mods/cars", "modsgaming.ru/load/beamng_drive/mashiny/38", "https://beamng.com/resources/" })
                Assert.Eq("Это раздел сайта, а не страница мода. Откройте сам мод и скопируйте ссылку оттуда", (await be.CheckPageAsync(link)).Problem);
            Assert.Eq(0, p.Requested.Count);
        });

        t.Run("страница мода открылась, но не читается: следить так не получится, причина названа", async () =>
        {
            var p = new Pages();
            p.Map[StUrl] = (200, StRedesign(Fx("stmods/mod-real.html")));
            var s = new AppSettings();
            var be = new LibraryUpdatesBackend(new InMemoryLibraryStore(), All(Polite(p)), () => s, () => Now);
            var r = await be.CheckPageAsync(StUrl);
            Assert.True(r.Listing is null, "не принята");
            Assert.True(r.Problem!.Contains("ни версии, ни файла") && r.Problem.Contains("STMods"), r.Problem);
            p.Map[StUrl] = (200, Fx("stmods/mod-real.html"));
            var be2 = new LibraryUpdatesBackend(new InMemoryLibraryStore(), All(Polite(p)), () => s, () => Now);
            Assert.Eq("3.3", (await be2.CheckPageAsync(StUrl)).Listing!.ModVersion);
        });
    }
}
