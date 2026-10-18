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
        private readonly string _html;
        public Page(string html) => _html = html;
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct) => Task.FromResult(new RawResponse(200, _html));
    }

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

        t.Run("ModsGaming: по настоящей выдаче — «sienna» всё оставляет, опечатка без совпадений — пусто, а не всё подряд", async () =>
        {
            var html = Fx("modsgaming/search-real.html");
            var f = new PoliteFetcher(new Page(html), new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero },
                delay: (_, _) => Task.CompletedTask);
            var all = ModsGamingParser.ParseSearch(html).Count;
            Assert.True(all > 0, "выдача разобралась");
            Assert.Eq(all, (await new ModsGamingSource(f).SearchAsync("sienna")).Count);
            f.ClearCache();
            Assert.Eq(0, (await new ModsGamingSource(f).SearchAsync("bmv")).Count);
        });
    }
}
