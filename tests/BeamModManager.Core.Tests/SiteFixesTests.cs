using BeamModManager.Core.Net;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Sources.BeamNg;
using BeamModManager.Core.Sources.StMods;

namespace BeamModManager.Core.Tests;

/// <summary>После выпуска 0.3 (2026-10-18), по живым сайтам: репозиторий «изменился», поиск STMods и ModsGaming.</summary>
public static class SiteFixesTests
{
    private static string Fx(string path)
    {
        var dir = AppContext.BaseDirectory;
        while (dir != null && !Directory.Exists(Path.Combine(dir, "tests", "fixtures"))) dir = Path.GetDirectoryName(dir);
        return File.ReadAllText(Path.Combine(dir!, "tests", "fixtures", path));
    }

    /// <summary>Адрес (и форма) → страница; запоминает, что спросили.</summary>
    private sealed class Site : IPageFetcher
    {
        public readonly Dictionary<string, string> Get = new(), Post = new();
        public readonly List<string> Log = new();
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct)
        {
            Log.Add("GET " + url.AbsoluteUri);
            return Task.FromResult(Get.TryGetValue(url.AbsoluteUri, out var b) ? new RawResponse(200, b) : new RawResponse(404, ""));
        }
        public Task<RawResponse> PostFormAsync(Uri url, string form, CancellationToken ct)
        {
            Log.Add("POST " + url.AbsoluteUri + " " + form);
            return Task.FromResult(Post.TryGetValue(url.AbsoluteUri + " " + form, out var b) ? new RawResponse(200, b) : new RawResponse(404, ""));
        }
    }

    private static PoliteFetcher Polite(IPageFetcher p) => new(p, new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero, MaxRetries = 0 });

    private const string StForm = "per_page_search=50&search_order_by=%60date_public%60%20DESC";

    public static void Register(TestRunner t)
    {
        t.Group("Сайты после 0.3: живые страницы 2026-10-18");

        t.Run("репозиторий BeamNG: перенос строки между «<a» и «href» — лента читается (раньше «пусто», и программа писала «сайт изменился»)", () =>
        {
            var l = BeamNgRepoParser.ParseList(Fx("beamng/list-2026-10-18.html"));
            Assert.Eq("Autodrom Most|LICHIGODS DICE|Racing Line mod", string.Join("|", l.Select(x => x.Title)));
            Assert.Eq("39785|39786|39806", string.Join("|", l.Select(x => x.SourceId)));
            Assert.Eq("0.0.1|7.0|v1.0", string.Join("|", l.Select(x => x.ModVersion)));
            Assert.Eq("Dogin", l[0].Author);
            Assert.Eq("Terrains, Levels, Maps", l[0].Category);
            Assert.Eq("https://beamng.com/resources/autodrom-most.39785/", l[0].Url.AbsoluteUri);
        });

        t.Run("репозиторий BeamNG: лента с переносами — не «сайт изменился»", async () =>
        {
            var site = new Site();
            site.Get["https://beamng.com/resources/"] = Fx("beamng/list-2026-10-18.html");
            var page = await new BeamNgRepoSource(Polite(site)).FeedPageAsync(1);
            Assert.Eq(3, page.Count);
        });

        t.Run("STMods: ищем только в разделе BeamNG, 50 самых свежих формой; у карточек есть картинка", async () =>
        {
            var site = new Site();
            site.Post["https://stmods.org/search.php?search_request=bmw&partition=beamng " + StForm] = Fx("stmods/search-beamng-2026-10-18.html");
            var r = await new StModsSource(Polite(site)).SearchAsync("bmw");
            Assert.Eq("POST https://stmods.org/search.php?search_request=bmw&partition=beamng " + StForm, site.Log.Single());
            Assert.Eq(2, r.Count);
            Assert.Eq("BMW E34 версия 3.3 для BeamNG.drive (v0.39.x)", r[0].Title);
            Assert.Eq("3.3", r[0].ModVersion);
            Assert.Eq("https://stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng/bmw_e34_by_matissator_for_beamng_img1pre.jpg?6acc95e59b8c2",
                r[0].PreviewUrl?.AbsoluteUri);
            Assert.Eq("matissator", r[0].Author);
        });

        t.Run("STMods: картинка из стиля карточки — только https и только своя; без стиля — без картинки", () =>
        {
            var card = "<div class=\"row index-card\"><a class=\"col-sm-4 index-card-img\" href=\"/beamng_drive/mods/x/\" style=\"background-image: url({0})\"></a>"
                       + "<h3 class=\"text-left\"><b>X</b><small>версия 1.0 для BeamNG.drive</small></h3></div>";
            Assert.Eq("https://stmods.org/beamng_drive/mods/x/a.jpg", StModsParser.ParseList(string.Format(card, "'/beamng_drive/mods/x/a.jpg'")).Single().PreviewUrl?.AbsoluteUri);
            Assert.Eq("https://stmods.org/b.jpg", StModsParser.ParseList(string.Format(card, "\"/b.jpg\"")).Single().PreviewUrl?.AbsoluteUri);
            Assert.True(StModsParser.ParseList(string.Format(card, "'http://evil.example/a.jpg'")).Single().PreviewUrl is null, "не https");
            Assert.True(StModsParser.ParseList(card.Replace(" style=\"background-image: url({0})\"", "")).Single().PreviewUrl is null, "нет стиля");
        });

        t.Run("ModsGaming: к запросу «beamng.drive» (сайт ищет по всем играм), две страницы, повторы не дублируются", async () =>
        {
            var site = new Site();
            var html = Fx("modsgaming/search-real.html");
            site.Get["https://modsgaming.ru/search/?q=bmw%20beamng.drive&t=0"] = html;
            site.Get["https://modsgaming.ru/search/?q=bmw%20beamng.drive&t=0&p=2"] = html; // та же выдача — ничего нового
            var src = new ModsGamingSource(Polite(site));
            var one = await src.SearchAsync("bmw");
            Assert.True(one.Count > 0 && one.Select(l => l.SourceId).Distinct().Count() == one.Count, "без повторов");
            Assert.Eq(2, site.Log.Count);
            site.Log.Clear();
            await new ModsGamingSource(Polite(site)).SearchAsync("bmw BeamNG");
            Assert.Eq("GET https://modsgaming.ru/search/?q=bmw%20BeamNG&t=0", site.Log[0], "«beamng» уже есть — не добавляем");
        });

        t.Run("ModsGaming: вторая страница не открылась — найденное на первой остаётся", async () =>
        {
            var site = new Site();
            site.Get["https://modsgaming.ru/search/?q=bmw%20beamng.drive&t=0"] = Fx("modsgaming/search-real.html");
            var r = await new ModsGamingSource(Polite(site)).SearchAsync("bmw");
            Assert.True(r.Count > 0, "первая страница");
        });

        t.Run("вежливая загрузка: форма кэшируется отдельно от GET того же адреса и проходит ту же проверку «вы человек?»", async () =>
        {
            var site = new Site();
            var u = new Uri("https://stmods.org/search.php?search_request=a");
            site.Get[u.AbsoluteUri] = "<p>get</p>";
            site.Post[u.AbsoluteUri + " x=1"] = "<p>post</p>";
            site.Post[u.AbsoluteUri + " x=2"] = "<html><head><title>Один момент…</title></head><body>Проверяем, человек ли вы</body></html>";
            var f = Polite(site);
            Assert.Eq("<p>get</p>", (await f.GetAsync(u)).Body);
            Assert.Eq("<p>post</p>", (await f.PostFormAsync(u, "x=1")).Body);
            Assert.True((await f.PostFormAsync(u, "x=1")).FromCache, "повтор формы — из кэша");
            Assert.Eq(SourceStatus.NeedsHumanCheck, (await f.PostFormAsync(u, "x=2")).Source);
        });
    }
}
