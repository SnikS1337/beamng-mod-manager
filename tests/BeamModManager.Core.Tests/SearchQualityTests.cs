using BeamModManager.Core.Models;
using BeamModManager.Core.Net;
using BeamModManager.Core.Search;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Sources.ModsGaming;
using BeamModManager.Core.Text;

namespace BeamModManager.Core.Tests;

/// <summary>Поиск после обкатки 0.3.6 (2026-10-18): опечатка на ModsGaming, заголовки «BeamNG . drive скачать мод …».</summary>
public static class SearchQualityTests
{
    private static string Fx(string path)
    {
        var dir = AppContext.BaseDirectory;
        while (dir != null && !Directory.Exists(Path.Combine(dir, "tests", "fixtures"))) dir = Path.GetDirectoryName(dir);
        return File.ReadAllText(Path.Combine(dir!, "tests", "fixtures", path));
    }

    private sealed class Page : IPageFetcher
    {
        private readonly Func<Uri, string> _html;
        public Page(string html) => _html = _ => html;
        public Page(Func<Uri, string> html) => _html = html;
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct) => Task.FromResult(new RawResponse(200, _html(url)));
    }

    private static bool Wide(Uri u) => Uri.UnescapeDataString(u.Query).Contains("beamng.drive", StringComparison.Ordinal);

    private static Listing L(string title, string path) => new("modsgaming", new Uri("https://modsgaming.ru" + path), path, title);

    public static void Register(TestRunner t)
    {
        t.Group("Поиск: качество выдачи");

        t.Run("«BeamNG . drive скачать мод Карта …» — без игры и «скачать мод»; обычные названия с BeamNG не трогаем", () =>
        {
            Assert.Eq("Карта Город N", TitleParser.Parse("BeamNG . drive скачать мод Карта Город N").Name);
            Assert.Eq("Пак машин", TitleParser.Parse("BeamNG.drive скачать моды Пак машин").Name);
            Assert.Eq("BeamNG Drive Utility Mod", TitleParser.Parse("BeamNG Drive Utility Mod").Name);
            Assert.Eq("BeamUI REWORK", TitleParser.Parse("BeamUI REWORK").Name);
            Assert.Eq("BeamNG . drive скачать мод", TitleParser.Parse("BeamNG . drive скачать мод").Name.Trim());
        });

        t.Run("подходит, если слово человека есть в названии или адресе; русское — и в транслите; служебные слова не в счёт", () =>
        {
            var found = new[]
            {
                L("Robert BMV 33 v", "/load/beamng_drive/mashiny/robert_bmv_33/38-1-0-1"),
                L("BMW M5 G90 Update", "/load/beamng_drive/mashiny/bmw_m5_g90/38-1-0-2"),
                L("Ford Engines", "/load/beamng_drive/drugoe/ford_engines/38-1-0-3"),
                L("Human Dummy", "/load/beamng_drive/drugoe/human_dummy/38-1-0-4"),
            };
            Assert.Eq("Robert BMV 33 v", string.Join("|", SearchRelevance.Filter("bmv", found).Select(l => l.Title)));
            Assert.Eq("BMW M5 G90 Update", string.Join("|", SearchRelevance.Filter("BMW", found).Select(l => l.Title)));
            Assert.Eq("Robert BMV 33 v", string.Join("|", SearchRelevance.Filter("бмв", found).Select(l => l.Title)));
            Assert.Eq(4, SearchRelevance.Filter("beamng.drive", found, new[] { "beamng", "drive", "beamng.drive" }).Count);
            Assert.Eq(4, SearchRelevance.Filter("!", found).Count);
        });

        t.Run("ModsGaming: лента — только «Машины»: мод из другого раздела по ленте «не менялся» не считается, его карточку открываем", () =>
        {
            var now = new DateTimeOffset(2026, 10, 18, 12, 0, 0, TimeSpan.Zero);
            var car = new Updates.TrackedBinding("car", "modsgaming", "1", now.AddDays(-2), null, "1.0");
            var taa = new Updates.TrackedBinding("taa", "modsgaming", "2", now.AddDays(-2), null, "1.0");
            var feed = new Dictionary<string, IReadOnlyDictionary<string, Updates.FeedEntry>> { ["modsgaming"] = new Dictionary<string, Updates.FeedEntry>() };
            var cov = new Dictionary<string, DateTimeOffset> { ["modsgaming"] = now.AddDays(-10) }; // лента дошла глубже последней проверки
            var src = new ModsGamingSource(new PoliteFetcher(new Page("")));
            var urls = new Dictionary<string, Uri>
            {
                ["car"] = new("https://modsgaming.ru/load/beamng_drive/mashiny/bmw/38-1-0-1"),
                ["taa"] = new("https://modsgaming.ru/load/beamng_drive/drugoe/taa_fix/39-1-0-2"),
            };
            var plan = Updates.UpdateCheckPlanner.PlanCards(new[] { car, taa }, feed, now, new Updates.UpdateCheckOptions(), cov,
                inFeed: b => src.FeedCovers(urls[b.ModId]));
            Assert.True(plan.ConfirmedUnchanged.Any(b => b.ModId == "car"), "машина: нет в ленте «Машины» — не менялась");
            Assert.False(plan.ConfirmedUnchanged.Any(b => b.ModId == "taa"), "улучшалка: лента о ней ничего не говорит");
            Assert.True(new ModlandSource(new PoliteFetcher(new Page(""))).FeedCovers(new Uri("https://www.modland.net/beamng.drive-mods/other/x.html")), "у других сайтов лента — все моды");
        });

        t.Run("ModsGaming: по настоящей выдаче — «sienna» всё оставляет, опечатка без совпадений — пусто, а не всё подряд", async () =>
        {
            var html = Fx("modsgaming/search-real.html");
            var all = ModsGamingParser.ParseSearch(html).Count;
            Assert.True(all > 0, "выдача разобралась");
            static PoliteFetcher F(Func<Uri, string> page) => new(new Page(page), new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero },
                delay: (_, _) => Task.CompletedTask);
            Assert.Eq(all, (await new ModsGamingSource(F(_ => html)).SearchAsync("sienna")).Count);
            // опечатка: как есть сайт ничего не нашёл, с «beamng.drive» — что угодно по «beamng»: из этого ничего не берём
            Assert.Eq(0, (await new ModsGamingSource(F(u => Wide(u) ? html : "<html></html>")).SearchAsync("bmv")).Count);
        });

        t.Run("один мод в нескольких версиях на сайте — одна запись, самая новая, на месте первой; разные авторы и сайты — отдельно", () =>
        {
            Listing V(string src, string id, string title, string? author = null) =>
                new(src, new Uri($"https://{src}.test/{id}"), id, title) { Author = author, ModVersion = TitleParser.Parse(title).ModVersion };
            var r = SearchService.NewestOnly(new[]
            {
                V("modsgaming", "a", "Temporal Anti Aliasing v1.0"), V("modsgaming", "b", "Toyota Sienna v2.0"),
                V("modsgaming", "c", "Temporal Anti Aliasing v1.7"), V("modsgaming", "d", "Temporal Anti Aliasing v1.3"),
                V("stmods", "e", "Temporal Anti Aliasing v1.2"), V("modsgaming", "f", "Toyota Sienna v1.0", "другой"),
                V("modsgaming", "g", "Карта v1.0 (0.36)"), V("modsgaming", "h", "Карта v1.0 (0.38)"), V("modsgaming", "i", "X v1"), V("modsgaming", "j", "X v2"),
                V("modsgaming", "k", "Ford Focus v1.0"), V("modsgaming", "l", "Ford Focus v1.0"), V("modsgaming", "m", "Ford Focus"),
            });
            // та же версия или без версии — возможно, разные моды с одним названием: обе остаются
            Assert.Eq("c|b|e|f|h|i|j|k|l|m", string.Join("|", r.Select(l => l.SourceId)));
        });

        t.Run("ModsGaming: картинка мода — со страницы мода (detimg), не из «похожих модов» ниже", () =>
        {
            var html = Fx("modsgaming/mod-real.html");
            Assert.Eq("https://modsgaming.ru/_ld/1604/53750859.jpg", ModsGamingParser.ImageOf(html)?.AbsoluteUri);
            var related = "<tr><td class=\"poh-td\" style=\"background:url(/_ld/1588/59283584.jpg);\"></td></tr>";
            Assert.True(ModsGamingParser.ImageOf(related) is null, "чужая картинка");
            Assert.True(ModsGamingParser.ImageOf("<td class=\"detimg\" style=\"background:url(http://evil.example/a.jpg);\">") is null, "только свой сайт");
        });

        t.Run("поиск: картинку со страницы мода открывает только её сайт; чужой адрес — без запросов", async () =>
        {
            var html = Fx("modsgaming/mod-real.html");
            var asked = new List<string>();
            var f = new PoliteFetcher(new Page(u => { lock (asked) asked.Add(u.AbsoluteUri); return html; }),
                new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero }, delay: (_, _) => Task.CompletedTask);
            var backend = new BeamModManager.Ui.Search.LibrarySearchBackend(
                new IModSource[] { new StModsSource(f), new ModsGamingSource(f) }, () => new BeamModManager.Core.Settings.AppSettings(),
                new BeamModManager.Core.Matching.MatchDecisionStore());
            var page = new Uri("https://modsgaming.ru/load/beamng_drive/mashiny/x/38-1-0-1");
            Assert.Eq("https://modsgaming.ru/_ld/1604/53750859.jpg", (await backend.PreviewOfAsync(page, default))?.AbsoluteUri);
            Assert.Eq(page.AbsoluteUri, asked.Single());
            Assert.True(await backend.PreviewOfAsync(new Uri("https://example.com/x"), default) is null, "чужой сайт");
            Assert.Eq(1, asked.Count);
        });

        t.Run("ModsGaming: слово только в описании (TAA) — запрос как есть находит, такие моды остаются", async () =>
        {
            var html = Fx("modsgaming/search-real.html");
            var all = ModsGamingParser.ParseSearch(html).Count;
            static PoliteFetcher F(Func<Uri, string> page) => new(new Page(page), new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero },
                delay: (_, _) => Task.CompletedTask);
            // как есть сайт нашёл моды, в названиях которых «taa» нет (слово в описании); с «beamng.drive» — ничего
            var found = await new ModsGamingSource(F(u => Wide(u) ? "<html></html>" : html)).SearchAsync("TAA");
            Assert.Eq(all, found.Count);
        });
    }
}
