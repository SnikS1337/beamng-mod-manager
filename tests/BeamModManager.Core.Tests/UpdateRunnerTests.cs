using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Updates;

namespace BeamModManager.Core.Tests;

public static class UpdateRunnerTests
{
    private static readonly DateTimeOffset Now = new(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);

    private sealed class Site : IModSource
    {
        public Dictionary<string, Listing> Cards = new();
        public List<Listing> Feed = new();
        public SourceException? FeedFails, CardFails;
        public int FeedCalls, CardCalls;
        public Site(string id) { Id = id; DisplayName = id; }
        public string Id { get; }
        public string DisplayName { get; }
        public bool SupportsSearch => false;
        public Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default)
        {
            FeedCalls++;
            if (FeedFails is not null) throw FeedFails;
            return Task.FromResult<IReadOnlyList<Listing>>(page == 1 ? Feed : Array.Empty<Listing>());
        }
        public Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default)
        {
            CardCalls++;
            if (CardFails is not null) throw CardFails;
            return Task.FromResult(Cards.GetValueOrDefault(url.AbsoluteUri));
        }
        public Task<IReadOnlyList<Listing>> SearchAsync(string q, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? s, CancellationToken ct = default) => FeedPageAsync(1, ct);
        public Task<IReadOnlyList<Offer>> GetOffersAsync(Listing l, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Offer>>(Array.Empty<Offer>());
    }

    private static string U(string src, string id) => $"https://{src}.test/{id}";
    private static Listing L(string src, string id, string? ver, long? size = null) =>
        new(src, new Uri(U(src, id)), id, id) { ModVersion = ver, SizeBytes = size };

    private static void Add(InMemoryLibraryStore s, string modId, string src, string id, string? ver,
        string? lastVer = null, DateTimeOffset? checkedAt = null)
    {
        s.UpsertMod(new InstalledMod(modId, modId, modId + ".zip", ver, null, Now.AddDays(-60)));
        s.UpsertBinding(new ModBinding(modId, src, id, new Uri(U(src, id)), true));
        if (checkedAt is not null) s.SaveCheck(new CheckState(modId, src, checkedAt.Value, checkedAt, lastVer, null));
    }

    public static void Register(TestRunner t)
    {
        t.Group("Кнопка «проверить обновления»");
        var old = Now.AddDays(-2);

        t.Run("новая версия найдена по ленте и карточке; без изменений — только лента", async () =>
        {
            var s = new InMemoryLibraryStore();
            Add(s, "a", "x", "a", "1.0", "1.0", old);
            Add(s, "b", "x", "b", "1.0", "1.0", old);
            var site = new Site("x") { Feed = { L("x", "a", "1.1"), L("x", "b", "1.0") } };
            site.Cards[U("x", "a")] = L("x", "a", "1.1");
            var r = await UpdateRunner.RunAsync(s, new[] { site }, Now);
            Assert.Eq(1, r.Findings.Count);
            Assert.Eq("a", r.Findings[0].ModId);
            Assert.Eq("1.1", r.Findings[0].LatestVersion);
            Assert.Eq(1, site.CardCalls); // b подтверждён лентой без открытия карточки
            Assert.Eq(1, r.UpToDate);
            Assert.True(r.Complete, "всё проверено");
        });

        t.Run("200 модов, у всех свежая лента: запросов столько, сколько страниц ленты, карточки не открываются", async () =>
        {
            var s = new InMemoryLibraryStore();
            var site = new Site("x");
            for (var i = 0; i < 200; i++) { Add(s, $"m{i}", "x", $"m{i}", "1.0", "1.0", old); site.Feed.Add(L("x", $"m{i}", "1.0")); }
            var r = await UpdateRunner.RunAsync(s, new[] { site }, Now);
            Assert.Eq(0, site.CardCalls);
            Assert.True(r.Requests <= 10, $"запросов {r.Requests}");
            Assert.Eq(200, r.UpToDate);
        });

        t.Run("мода нет в ленте: открывается карточка (запасной путь)", async () =>
        {
            var s = new InMemoryLibraryStore();
            Add(s, "a", "x", "a", "1.0", "1.0", Now.AddDays(-5));
            var site = new Site("x");
            site.Cards[U("x", "a")] = L("x", "a", "2.0");
            var r = await UpdateRunner.RunAsync(s, new[] { site }, Now);
            Assert.Eq(1, site.CardCalls);
            Assert.Eq("2.0", r.Findings.Single().LatestVersion);
        });

        t.Run("проверка «вы человек?» на ленте: сайт помечается, остальные сайты проверяются", async () =>
        {
            var s = new InMemoryLibraryStore();
            Add(s, "a", "x", "a", "1.0", "1.0", old);
            Add(s, "b", "y", "b", "1.0", "1.0", old);
            var x = new Site("x") { FeedFails = new SourceException(SourceStatus.NeedsHumanCheck, "challenge") };
            var y = new Site("y") { Feed = { L("y", "b", "1.5") } };
            y.Cards[U("y", "b")] = L("y", "b", "1.5");
            var r = await UpdateRunner.RunAsync(s, new IModSource[] { x, y }, Now);
            Assert.Eq(SourceStatus.NeedsHumanCheck, s.GetSourceStatus("x"));
            Assert.Eq("b", r.Findings.Single().ModId);
            Assert.False(r.Complete, "неполная проверка");
            Assert.Eq(SourceStatus.NeedsHumanCheck, r.Sources.Single(z => z.Source == "x").Status);
            Assert.Eq(0, x.CardCalls);
        });

        t.Run("следующий запуск пропускает сайт с проверкой, пока её не пройдут", async () =>
        {
            var s = new InMemoryLibraryStore();
            Add(s, "a", "x", "a", "1.0", "1.0", old);
            s.SetSourceStatus("x", SourceStatus.NeedsHumanCheck);
            var x = new Site("x");
            var r = await UpdateRunner.RunAsync(s, new[] { x }, Now);
            Assert.Eq(0, x.FeedCalls);
            Assert.Eq(0, r.Requests);
            Assert.True(r.Sources.Single().Note!.Contains("человек"), "сказано почему");
        });

        t.Run("сайт недоступен: прошлые данные и найденное раньше обновление остаются", async () =>
        {
            var s = new InMemoryLibraryStore();
            Add(s, "a", "x", "a", "1.0", "1.5", old); // раньше видели 1.5
            var x = new Site("x") { FeedFails = new SourceException(SourceStatus.Unavailable, "502") };
            var r = await UpdateRunner.RunAsync(s, new[] { x }, Now);
            Assert.Eq("1.5", r.Findings.Single().LatestVersion);
            Assert.Eq(SourceStatus.Unavailable, r.Sources.Single().Status);
            Assert.Eq(SourceStatus.Ok, s.GetSourceStatus("x")); // недоступность не «залипает»
        });

        t.Run("карточки нет (404): мод попадает в «пропали с сайта», состояние не портится", async () =>
        {
            var s = new InMemoryLibraryStore();
            Add(s, "a", "x", "a", "1.0", "1.0", Now.AddDays(-5));
            var x = new Site("x");
            var r = await UpdateRunner.RunAsync(s, new[] { x }, Now);
            Assert.True(r.Sources.Single().MissingMods.SequenceEqual(new[] { "a" }), "пропавший мод назван");
            Assert.Eq("1.0", s.GetCheck("a", "x")!.LastVersion);
        });

        t.Run("повторный запуск сразу после проверки ничего не запрашивает", async () =>
        {
            var s = new InMemoryLibraryStore();
            Add(s, "a", "x", "a", "1.0", "1.0", old);
            var x = new Site("x") { Feed = { L("x", "a", "1.0") } };
            await UpdateRunner.RunAsync(s, new[] { x }, Now);
            var before = x.FeedCalls;
            var r = await UpdateRunner.RunAsync(s, new[] { x }, Now.AddMinutes(5));
            Assert.Eq(before, x.FeedCalls);
            Assert.Eq(0, r.Requests);
        });

        t.Run("принудительная проверка игнорирует срок давности", async () =>
        {
            var s = new InMemoryLibraryStore();
            Add(s, "a", "x", "a", "1.0", "1.0", Now.AddMinutes(-5));
            var x = new Site("x") { Feed = { L("x", "a", "1.0") } };
            await UpdateRunner.RunAsync(s, new[] { x }, Now, new UpdateCheckOptions { Force = true });
            Assert.True(x.FeedCalls > 0, "ленту прочитали");
        });

        t.Run("привязка к сайту без адаптера: понятная пометка, остальное работает", async () =>
        {
            var s = new InMemoryLibraryStore();
            Add(s, "a", "zzz", "a", "1.0", "1.0", old);
            var r = await UpdateRunner.RunAsync(s, Array.Empty<IModSource>(), Now);
            Assert.True(r.Sources.Single().Note!.Contains("адаптер"), "пометка");
        });

        t.Run("отмена прерывает проверку", async () =>
        {
            var s = new InMemoryLibraryStore();
            Add(s, "a", "x", "a", "1.0", "1.0", old);
            using var cts = new CancellationTokenSource();
            cts.Cancel();
            var threw = false;
            try { await UpdateRunner.RunAsync(s, new[] { new Site("x") }, Now, ct: cts.Token); }
            catch (OperationCanceledException) { threw = true; }
            Assert.True(threw, "ожидалась отмена");
        });
    }
}
