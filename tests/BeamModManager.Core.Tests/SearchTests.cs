using BeamModManager.Core.Models;
using BeamModManager.Core.Search;
using BeamModManager.Core.Sources;

namespace BeamModManager.Core.Tests;

public static class SearchTests
{
    private const long MB = 1024 * 1024;

    private sealed class Fake : IModSource
    {
        private readonly Func<CancellationToken, Task<IReadOnlyList<Listing>>> _search;
        public Fake(string id, Func<CancellationToken, Task<IReadOnlyList<Listing>>> search) { Id = id; DisplayName = id.ToUpperInvariant(); _search = search; }
        public Fake(string id, params Listing[] items) : this(id, _ => Task.FromResult<IReadOnlyList<Listing>>(items)) { }
        public string Id { get; }
        public string DisplayName { get; }
        public bool SupportsSearch => true;
        public Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<IReadOnlyList<Listing>> SearchAsync(string query, CancellationToken ct = default) => _search(ct);
        public Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default) => Task.FromResult<Listing?>(null);
        public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? since, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<IReadOnlyList<Offer>> GetOffersAsync(Listing listing, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Offer>>(Array.Empty<Offer>());
    }

    private static Listing L(string source, string id, string title) =>
        new(source, new Uri($"https://{source}.test/{id}"), id, title);

    private static Offer Modsfire(string id) =>
        new(OfferKind.ExternalHost, "modsfire", new Uri($"https://modsfire.com/{id}"), id);

    public static void Register(TestRunner t)
    {
        t.Group("Общий поиск");

        t.Run("один мод на трёх сайтах с общим файлом ModsFire: одна карточка, три источника", async () =>
        {
            var offers = new[] { Modsfire("9nDwN00BJ04Ef55") };
            var svc = new SearchService(new IModSource[]
            {
                new Fake("modland", L("modland", "a", "Toyota Sienna 2011-2017 2.2") with { Offers = offers, Price = PriceStatus.Free }),
                new Fake("modsgaming", L("modsgaming", "b", "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE") with { Offers = offers }),
                new Fake("stmods", L("stmods", "c", "Toyota Sienna (2011-2017) версия 2.2 для BeamNG.drive (v0.39.x)") with { Offers = offers }),
            });
            var r = await svc.SearchAsync("sienna");
            Assert.Eq(1, r.Cards.Count);
            Assert.Eq(3, r.Cards[0].Listings.Count);
            Assert.Eq(PriceSummary.Free, r.Cards[0].Price);
        });

        t.Run("одинаковое название без подтверждений: две карточки, но помечены «возможно то же»", async () =>
        {
            var svc = new SearchService(new IModSource[]
            {
                new Fake("modland", L("modland", "a", "Toyota Sienna 2011-2017 2.2") with { SizeBytes = 61 * MB }),
                new Fake("modsgaming", L("modsgaming", "b", "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE")),
            });
            var r = await svc.SearchAsync("sienna");
            Assert.Eq(2, r.Cards.Count);
            Assert.True(r.Cards.All(c => c.PossibleSameAs.Count == 1), "каждая указывает на другую");
        });

        t.Run("разные моды не объединяются и не помечаются", async () =>
        {
            var svc = new SearchService(new IModSource[]
            {
                new Fake("modland", L("modland", "a", "Toyota AE86 RLA v1.0.7")),
                new Fake("stmods", L("stmods", "b", "Toyota Sienna (2011-2017) версия 1.2 для BeamNG.drive (v0.39.x)")),
            });
            var r = await svc.SearchAsync("toyota");
            Assert.Eq(2, r.Cards.Count);
            Assert.True(r.Cards.All(c => c.PossibleSameAs.Count == 0), "без пометок");
        });

        t.Run("две записи одного сайта в одну карточку не попадают", async () =>
        {
            var o = new[] { Modsfire("SAMEFILEID123") };
            var svc = new SearchService(new IModSource[]
            {
                new Fake("stmods", L("stmods", "a", "Car X версия 1.0") with { Offers = o }, L("stmods", "b", "Car X версия 1.0") with { Offers = o }),
            });
            Assert.Eq(2, (await svc.SearchAsync("car")).Cards.Count);
        });

        t.Run("платный на одном сайте, бесплатный на другом: итог Mixed, цена каждого источника на месте", async () =>
        {
            var o = new[] { Modsfire("FILEIDAAAAAA1") };
            var svc = new SearchService(new IModSource[]
            {
                new Fake("modland", L("modland", "a", "[PAID] Car X 1.0") with { Price = PriceStatus.Paid, Offers = o }),
                new Fake("modsgaming", L("modsgaming", "b", "Car X") with { Price = PriceStatus.Free, Offers = o }),
            });
            var c = (await svc.SearchAsync("car")).Cards.Single();
            Assert.Eq(PriceSummary.Mixed, c.Price);
            Assert.Eq(PriceStatus.Paid, c.Listings.Single(l => l.Source == "modland").Price);
        });

        t.Run("сайт упал: остальные результаты есть, причина в отчёте", async () =>
        {
            var svc = new SearchService(new IModSource[]
            {
                new Fake("modland", _ => throw new InvalidOperationException("разметка изменилась")),
                new Fake("stmods", L("stmods", "b", "Car X версия 1.0")),
            });
            var r = await svc.SearchAsync("car");
            Assert.Eq(1, r.Cards.Count);
            var rep = r.Sources.Single(s => s.SourceId == "modland");
            Assert.Eq(SourceStatus.Unavailable, rep.Status);
            Assert.True(rep.Note!.Contains("разметка"), "причина");
        });

        t.Run("сайт завис: по таймауту отчёт «недоступен», остальные не ждут вечно", async () =>
        {
            var svc = new SearchService(new IModSource[]
            {
                new Fake("slow", async ct => { await Task.Delay(Timeout.Infinite, ct); return Array.Empty<Listing>(); }),
                new Fake("stmods", L("stmods", "b", "Car X версия 1.0")),
            }, options: new SearchOptions { PerSourceTimeout = TimeSpan.FromMilliseconds(100) });
            var r = await svc.SearchAsync("car");
            Assert.Eq(1, r.Cards.Count);
            Assert.Eq(SourceStatus.Unavailable, r.Sources.Single(s => s.SourceId == "slow").Status);
        });

        t.Run("отмена пользователем пробрасывается", async () =>
        {
            using var cts = new CancellationTokenSource(); cts.Cancel();
            var svc = new SearchService(new IModSource[] { new Fake("slow", async ct => { await Task.Delay(Timeout.Infinite, ct); return Array.Empty<Listing>(); }) });
            var threw = false;
            try { await svc.SearchAsync("car", cts.Token); } catch (OperationCanceledException) { threw = true; }
            Assert.True(threw, "ожидалась отмена");
        });

        t.Run("источник с проверкой «вы человек?» пропускается без вызова", async () =>
        {
            var called = false;
            var svc = new SearchService(new IModSource[]
            {
                new Fake("modland", _ => { called = true; return Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>()); }),
                new Fake("stmods", L("stmods", "b", "Car X версия 1.0")),
            }, id => id == "modland" ? SourceStatus.NeedsHumanCheck : SourceStatus.Ok);
            var r = await svc.SearchAsync("car");
            Assert.False(called, "не вызывался");
            Assert.Eq(SourceStatus.NeedsHumanCheck, r.Sources.Single(s => s.SourceId == "modland").Status);
            Assert.Eq(1, r.Cards.Count);
        });

        t.Run("слишком короткий запрос: пусто, сайты не опрашиваются", async () =>
        {
            var called = false;
            var svc = new SearchService(new IModSource[] { new Fake("a", _ => { called = true; return Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>()); }) });
            var r = await svc.SearchAsync(" a ");
            Assert.False(called, "не вызывался");
            Assert.Eq(0, r.Cards.Count);
        });

        t.Run("сортировка: точное совпадение с запросом выше, объединённые карточки выше при равенстве", async () =>
        {
            var svc = new SearchService(new IModSource[]
            {
                new Fake("modland", L("modland", "a", "Honda Civic Type R Pack Deluxe"), L("modland", "b", "Civic")),
            });
            var r = await svc.SearchAsync("civic");
            Assert.Eq("Civic", r.Cards[0].Title);
        });

        t.Run("цепочка A~B, B~C при несовместимых A и C: не склеивается в одну", async () =>
        {
            // A и B делят файл, B и C делят другой файл, A и C разные по названию.
            var svc = new SearchService(new IModSource[]
            {
                new Fake("s1", L("s1", "a", "Alpha Roadster") with { Offers = new[] { Modsfire("AAAAAAAAAA111") } }),
                new Fake("s2", L("s2", "b", "Beta Hauler") with { Offers = new[] { Modsfire("AAAAAAAAAA111"), Modsfire("BBBBBBBBBB222") } }),
                new Fake("s3", L("s3", "c", "Gamma Sedan") with { Offers = new[] { Modsfire("BBBBBBBBBB222") } }),
            });
            var r = await svc.SearchAsync("car");
            Assert.True(r.Cards.All(c => c.Listings.Count <= 2), "не больше двух в карточке");
            Assert.True(r.Cards.Count >= 2, "минимум две карточки");
        });
    }
}
