using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Sources.ModsGaming;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Updates;

namespace BeamModManager.Core.Tests;

/// <summary>Подбор страниц модам, которые положили в папку сами (2026-10-18: «какой мод ни положи — не проверяется»).</summary>
public static class PageGuessTests
{
    private static readonly DateTimeOffset Now = new(2026, 10, 18, 12, 0, 0, TimeSpan.Zero);

    private static string Fx(string path)
    {
        var dir = AppContext.BaseDirectory;
        while (dir != null && !Directory.Exists(Path.Combine(dir, "tests", "fixtures"))) dir = Path.GetDirectoryName(dir);
        return File.ReadAllText(Path.Combine(dir!, "tests", "fixtures", path));
    }

    private static InstalledMod Mod(string id, string file, string? version = null, long? size = null) =>
        new(id, BindingSuggester.FromFileName(file).Name, file, version, size, Now.AddDays(-1));

    private static Listing L(string src, string id, string title, string? author = null, long? size = null, PriceStatus price = PriceStatus.Unknown) =>
        new(src, new Uri($"https://{src}.test/{id}"), id, title) { Author = author, SizeBytes = size, Price = price };

    /// <summary>Сайт с готовой выдачей; считает запросы.</summary>
    private sealed class Site(string id, Func<string, IReadOnlyList<Listing>> answer) : IModSource
    {
        public int Calls;
        public string Id => id;
        public string DisplayName => id;
        public bool SupportsSearch => true;
        public Task<IReadOnlyList<Listing>> SearchAsync(string query, CancellationToken ct = default) { Interlocked.Increment(ref Calls); return Task.FromResult(answer(query)); }
        public Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default) => Task.FromResult<Listing?>(null);
        public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? since, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<IReadOnlyList<Offer>> GetOffersAsync(Listing listing, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Offer>>(Array.Empty<Offer>());
    }

    public static void Register(TestRunner t)
    {
        t.Group("Подбор страниц модам без них");

        t.Run("настоящая выдача ModsGaming: «toyota_sienna_2017_v1.0» — страница v2.0; два одинаковых «2023 Toyota Sienna» — не гадаем", () =>
        {
            var found = Search.SearchService.NewestOnly(ModsGamingParser.ParseSearch(Fx("modsgaming/search-real.html")));
            var g = PageGuesser.Pick(Mod("a", "toyota_sienna_2017_v1.0.zip", "1.0"), null, found);
            Assert.True(g is not null && g.Listing.Title.Contains("Sienna 2017 v2.0", StringComparison.Ordinal), "нашли: " + g?.Listing.Title);
            Assert.True(PageGuesser.Pick(Mod("b", "2023_toyota_sienna.zip"), null, found) is null, "две похожие страницы на одном сайте — какая, непонятно");
        });

        t.Run("не предлагаем: другое название, другой автор, сильно другой размер, на сайте версия старше, платный, одно общее слово", () =>
        {
            var mod = Mod("a", "bmw_e34_by_matissator_for_beamng_v3.3.zip", "3.3", 100_000_000);
            Assert.Eq("bmw e34", PageGuesser.Pick(mod, null, new[] { L("stmods", "1", "BMW E34 версия 3.3", "matissator", 101_000_000) })?.Listing.Title is { } t1
                ? Text.TitleNormalizer.Normalize(t1) : "нет");
            Assert.True(PageGuesser.Pick(mod, null, new[] { L("stmods", "2", "BMW E30 версия 3.3") }) is null, "другое название");
            Assert.True(PageGuesser.Pick(mod, null, new[] { L("stmods", "3", "BMW E34 версия 3.3", "someone") }) is null, "другой автор");
            Assert.True(PageGuesser.Pick(mod, null, new[] { L("stmods", "4", "BMW E34 версия 3.3", size: 40_000_000) }) is null, "размер");
            Assert.True(PageGuesser.Pick(mod, null, new[] { L("stmods", "5", "BMW E34 версия 2.0") }) is null, "на сайте старее установленного");
            Assert.True(PageGuesser.Pick(mod, null, new[] { L("modland", "6", "BMW E34 v3.3", price: PriceStatus.Paid) }) is null, "платный");
            Assert.True(PageGuesser.Pick(Mod("p", "police.zip"), null, new[] { L("stmods", "7", "Police Pack") }) is null, "одно слово — только целиком");
            Assert.True(PageGuesser.Pick(Mod("p", "police.zip"), null, new[] { L("stmods", "8", "Police") }) is not null, "одно слово целиком — да");
        });

        t.Run("название из окна (info.json) сильнее имени файла; одна страница на двух сайтах — первая из выдачи", () =>
        {
            var mod = Mod("a", "taa_final.zip");
            var found = new[] { L("modsgaming", "1", "Temporal Anti Aliasing v1.7"), L("stmods", "2", "Temporal Anti Aliasing 1.7") };
            Assert.True(PageGuesser.Pick(mod, null, found) is null, "по имени файла не узнать");
            Assert.Eq("modsgaming", PageGuesser.Pick(mod, "Temporal Anti Aliasing", found)?.Listing.Source);
            Assert.Eq("Temporal Anti Aliasing", PageGuesser.QueryFor(mod, "Temporal Anti Aliasing"));
            Assert.Eq("taa final", PageGuesser.QueryFor(mod, null));
        });

        t.Run("кого искать: без привязок, без «Не проверять», не искали 14 дней, от кого не отказались; новые первыми, не больше 20", () =>
        {
            var s = new InMemoryLibraryStore();
            for (var i = 0; i < 25; i++) s.UpsertMod(Mod("m" + i, $"mod_{i}.zip") with { InstalledAtUtc = Now.AddDays(-i) });
            s.UpsertBinding(new ModBinding("m0", "stmods", "1", new Uri("https://stmods.test/1"), true));
            s.UpsertBinding(new ModBinding("m1", "stmods", "2", new Uri("https://stmods.test/2"), false));
            s.SetCheckingDeclined("m2", true);
            s.SetPageSearch("m3", new PageSearch(Now.AddDays(-3)));
            s.SetPageSearch("m4", new PageSearch(Now.AddDays(-15)));
            s.SetPageSearch("m5", new PageSearch(Now.AddDays(-30), Rejected: true));
            var due = PageGuesser.Due(s, Now).Select(m => m.Id).ToList();
            Assert.Eq(PageGuesser.PerRun, due.Count);
            Assert.Eq("m4|m6|m7", string.Join("|", due.Take(3)));
            Assert.True(!due.Intersect(new[] { "m0", "m1", "m2", "m3", "m5" }).Any(), "лишние: " + string.Join(",", due));
        });

        t.Run("библиотека помнит подбор между запусками; файл без него (до 0.3.9) читается", () =>
        {
            var dir = Path.Combine(Path.GetTempPath(), "bmm-pages-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(dir);
            try
            {
                var path = Path.Combine(dir, "library.json");
                var a = new JsonLibraryStore(path);
                a.UpsertMod(Mod("x", "x_car.zip"));
                a.SetPageSearch("x", new PageSearch(Now, Rejected: true));
                Assert.Eq(new PageSearch(Now, true), new JsonLibraryStore(path).GetPageSearch("x"));
                File.WriteAllText(path, File.ReadAllText(path).Replace("\"PageSearches\"", "\"Unused\"", StringComparison.Ordinal));
                var old = new JsonLibraryStore(path);
                Assert.True(old.GetPageSearch("x") is null && old.GetMod("x") is not null && old.LoadWarnings.Count == 0, "старый файл без подбора");
            }
            finally { Directory.Delete(dir, true); }
        });

        t.Run("при проверке: нашли — страница предложена, не подтверждена; повторно не ищем; «Следить», «Это другой мод», «Отменить»", async () =>
        {
            var store = new InMemoryLibraryStore();
            store.UpsertMod(Mod("a", "toyota_sienna_2017_v1.0.zip", "1.0"));
            store.UpsertMod(Mod("b", "lada_niva_4x4.zip"));
            var site = new Site("modsgaming", q => q.Contains("sienna", StringComparison.OrdinalIgnoreCase)
                ? new[] { L("modsgaming", "s", "МОД Toyota Sienna 2017 v2.0 (0.34.x) ДЛЯ BEAMNG.DRIVE") }
                : Array.Empty<Listing>());
            var settings = new AppSettings { DisabledSources = Array.Empty<string>() };
            var be = new LibraryUpdatesBackend(store, new IModSource[] { site }, () => settings, () => Now);
            var steps = new List<string>();
            Assert.Eq(1, await be.FindPagesAsync(_ => null, new Sync(p => steps.Add($"{p.Done}/{p.Total}"))));
            Assert.Eq("0/2|1/2|2/2", string.Join("|", steps));
            var b = store.Bindings().Single();
            Assert.True(b.ModId == "a" && b.SourceId == "s" && !b.Confirmed, "предложена, следить не начали");
            Assert.True(store.GetPageSearch("b") is { Rejected: false }, "у второго искали, не нашли — помним когда");
            Assert.Eq(2, site.Calls);
            Assert.Eq(0, await be.FindPagesAsync(_ => null));
            Assert.Eq(2, site.Calls, "второй раз не ищем");

            var confirmed = be.ConfirmPage("a");
            Assert.True(confirmed is not null && store.Bindings().Single().Confirmed, "«Следить»");
            Assert.Eq(1, UpdateCheckService.Tracked(store).Count);
            be.UndoPage(confirmed!);
            Assert.False(store.Bindings().Single().Confirmed, "«Отменить» — снова только предложена");
            var rejected = be.RejectPage("a");
            Assert.True(rejected is not null && store.Bindings().Count == 0 && store.GetPageSearch("a") is { Rejected: true }, "«Это другой мод»");
            Assert.True(PageGuesser.Due(store, Now.AddDays(60)).All(m => m.Id != "a"), "отклонённую больше не ищем");
            be.UndoPage(rejected!);
            Assert.True(store.Bindings().Single() is { Confirmed: false, ModId: "a" } && store.GetPageSearch("a") is { Rejected: false }, "«Отменить» вернул предложение");
        });

        t.Run("ни один сайт не ответил — не считаем, что искали; «Остановить» прерывает подбор", async () =>
        {
            var store = new InMemoryLibraryStore();
            store.UpsertMod(Mod("a", "toyota_sienna_2017.zip"));
            var down = new Site("modsgaming", _ => throw new SourceException(SourceStatus.Unavailable, "код 503"));
            var settings = new AppSettings { DisabledSources = Array.Empty<string>() };
            var be = new LibraryUpdatesBackend(store, new IModSource[] { down }, () => settings, () => Now);
            Assert.Eq(0, await be.FindPagesAsync(_ => null));
            Assert.True(store.GetPageSearch("a") is null, "попробуем в следующий раз");

            for (var i = 0; i < 5; i++) store.UpsertMod(Mod("m" + i, $"car_{i}_mod.zip"));
            LibraryUpdatesBackend? self = null;
            var calls = 0;
            var slow = new Site("stmods", _ => { if (++calls == 2) self!.Cancel(); return Array.Empty<Listing>(); });
            self = new LibraryUpdatesBackend(store, new IModSource[] { slow }, () => settings, () => Now);
            await self.FindPagesAsync(_ => null);
            Assert.Eq(2, calls, "после «Остановить» дальше не ищем");
        });

        t.Run("гараж: найденная страница — «Следить на ModsGaming», «Это другой мод», внизу её «Открыть страницу»; карточка «нашли страницу»", () =>
        {
            var sug = new ModBinding("a", "modsgaming", "s", new Uri("https://modsgaming.ru/load/x/38-1-0-1"), false);
            var mod = new GarageMod(Mod("a", "toyota_sienna.zip", "1.0"), null, null, null, false, false, null, null, null, null) { Suggested = sug };
            var vm = new GarageViewModel(new Fixed(mod), () => Now);
            vm.Refresh();
            vm.Selected = vm.Cards.Single();
            var d = vm.Details!;
            Assert.Eq(MainAction.ConfirmPage, d.Action);
            Assert.Eq("Следить на ModsGaming", d.ActionLabel);
            Assert.Eq("Это другой мод", d.AltLabel);
            Assert.Eq(sug.Url, d.PageUrl);
            Assert.True(d.OpenPageLink, "страницу можно открыть и проверить");
            Assert.True(vm.Cards.Single().Subtitle.Contains("нашли страницу", StringComparison.Ordinal), vm.Cards.Single().Subtitle);
            Assert.True(d.Facts.Any(f => f.Label == "Следим на" && f.Value.Contains("modsgaming.ru", StringComparison.Ordinal)), "в данных — где нашли");
        });
    }

    private sealed class Sync(Action<UpdateProgress> on) : IProgress<UpdateProgress>
    {
        public void Report(UpdateProgress value) => on(value);
    }

    private sealed class Fixed(GarageMod mod) : IGarageBackend
    {
        public GarageSnapshot Load() => new(new[] { mod }, true, null);
        public bool IsGameRunning() => false;
        public Core.Library.ToggleResult SetEnabled(string fileName, bool enabled) => new(true, Core.Library.ToggleWay.GameDb, "");
    }
}
