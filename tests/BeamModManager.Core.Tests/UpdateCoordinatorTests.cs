using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Updates;

namespace BeamModManager.Core.Tests;

public static class UpdateCoordinatorTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);

    private sealed class Clock { public DateTimeOffset Now = T0; }

    private sealed class Site : IModSource
    {
        public List<Listing> Feed = new();
        public Exception? FeedFails;
        public Task? Gate;
        public int FeedCalls;
        public Site(string id) { Id = id; DisplayName = id; }
        public string Id { get; }
        public string DisplayName { get; }
        public bool SupportsSearch => false;
        public async Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default)
        {
            Interlocked.Increment(ref FeedCalls);
            if (Gate is not null) await Gate;
            ct.ThrowIfCancellationRequested();
            if (FeedFails is not null) throw FeedFails;
            return page == 1 ? Feed : Array.Empty<Listing>();
        }
        public Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default) => Task.FromResult<Listing?>(null);
        public Task<IReadOnlyList<Listing>> SearchAsync(string q, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? s, CancellationToken ct = default) => FeedPageAsync(1, ct);
        public Task<IReadOnlyList<Offer>> GetOffersAsync(Listing l, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Offer>>(Array.Empty<Offer>());
    }

    private static Listing L(string id, string ver) => new("x", new Uri($"https://x.test/{id}"), id, id) { ModVersion = ver };

    private static InMemoryLibraryStore Store(int n = 3)
    {
        var s = new InMemoryLibraryStore();
        for (var i = 0; i < n; i++)
        {
            s.UpsertMod(new InstalledMod($"m{i}", $"m{i}", $"m{i}.zip", "1.0", null, T0.AddDays(-60)));
            s.UpsertBinding(new ModBinding($"m{i}", "x", $"m{i}", new Uri($"https://x.test/m{i}"), true));
            s.SaveCheck(new CheckState($"m{i}", "x", T0.AddDays(-2), T0.AddDays(-2), "1.0", null));
        }
        return s;
    }

    public static void Register(TestRunner t)
    {
        t.Group("Серия нажатий «проверить обновления»");

        t.Run("двойной клик: вторая проверка не запускается, оба получают одну и ту же", async () =>
        {
            var gate = new TaskCompletionSource();
            var site = new Site("x") { Gate = gate.Task, Feed = { L("m0", "1.0"), L("m1", "1.0"), L("m2", "1.0") } };
            var c = new UpdateCoordinator(Store(), () => new IModSource[] { site }, () => T0);
            var a = c.CheckAsync();
            Assert.True(c.IsRunning, "идёт");
            var b = c.CheckAsync();
            Assert.True(ReferenceEquals(a, b), "та же задача");
            gate.SetResult();
            var r = await a;
            Assert.Eq(2, site.FeedCalls); // страница 1 и пустая страница 2: одна проверка, а не две
            Assert.Eq(3, r.UpToDate);
            Assert.False(c.IsRunning, "завершена");
        });

        t.Run("отмена и сразу новое нажатие: старая прервана, новая проверка выполнена целиком", async () =>
        {
            var gate = new TaskCompletionSource();
            var site = new Site("x") { Gate = gate.Task, Feed = { L("m0", "1.0") } };
            var c = new UpdateCoordinator(Store(1), () => new IModSource[] { site }, () => T0);
            var a = c.CheckAsync();
            c.Cancel();
            var b = c.CheckAsync();
            Assert.False(ReferenceEquals(a, b), "это новая проверка, а не прерванная");
            gate.SetResult();
            Assert.True((await a).Cancelled, "старая прервана");
            var rb = await b;
            Assert.False(rb.Cancelled, "новая не прервана");
            Assert.Eq(1, rb.UpToDate);
            Assert.False(c.IsRunning, "всё завершено");
        });

        t.Run("«принудительно» во время обычной проверки тоже присоединяется, а не запускает вторую", async () =>
        {
            var gate = new TaskCompletionSource();
            var site = new Site("x") { Gate = gate.Task, Feed = { L("m0", "1.0") } };
            var c = new UpdateCoordinator(Store(1), () => new IModSource[] { site }, () => T0);
            var a = c.CheckAsync();
            var b = c.CheckAsync(force: true);
            Assert.True(ReferenceEquals(a, b), "та же задача");
            gate.SetResult();
            await a;
            Assert.Eq(2, site.FeedCalls); // страница 1 и пустая страница 2: одна проверка, а не две
        });

        t.Run("нажатие сразу после завершённой проверки: сайты не трогаются, свежие посчитаны", async () =>
        {
            var clock = new Clock();
            var site = new Site("x") { Feed = { L("m0", "1.0"), L("m1", "1.0"), L("m2", "1.0") } };
            var c = new UpdateCoordinator(Store(), () => new IModSource[] { site }, () => clock.Now);
            await c.CheckAsync();
            var calls = site.FeedCalls;
            clock.Now = T0.AddMinutes(10);
            var r = await c.CheckAsync();
            Assert.Eq(calls, site.FeedCalls);
            Assert.Eq(0, r.Requests);
            Assert.Eq(3, r.FreshSkipped);
            Assert.True(r.Complete, "нечего проверять — не ошибка");
        });

        t.Run("«принудительно» чаще раза в минуту: возвращается прошлый отчёт; через минуту — новая проверка", async () =>
        {
            var clock = new Clock();
            var cleared = 0;
            var site = new Site("x") { Feed = { L("m0", "1.0") } };
            var c = new UpdateCoordinator(Store(1), () => new IModSource[] { site }, () => clock.Now, clearCache: () => cleared++);
            await c.CheckAsync(force: true);
            var calls = site.FeedCalls;
            clock.Now = T0.AddSeconds(20);
            var r = await c.CheckAsync(force: true);
            Assert.True(r.Reused, "прошлый отчёт");
            Assert.Eq(calls, site.FeedCalls);
            Assert.Eq(1, cleared);
            clock.Now = T0.AddSeconds(90);
            var r2 = await c.CheckAsync(force: true);
            Assert.False(r2.Reused, "новая проверка");
            Assert.True(site.FeedCalls > calls, "сайт опрошен");
            Assert.Eq(2, cleared); // кэш страниц сброшен, иначе «заново» дало бы копию получасовой давности
        });

        t.Run("сайт упал: нажатия в течение 5 минут его не опрашивают, потом пробуют снова", async () =>
        {
            var clock = new Clock();
            var site = new Site("x") { FeedFails = new SourceException(SourceStatus.Unavailable, "502") };
            var c = new UpdateCoordinator(Store(), () => new IModSource[] { site }, () => clock.Now);
            var r1 = await c.CheckAsync();
            Assert.Eq(SourceStatus.Unavailable, r1.Sources.Single().Status);
            var calls = site.FeedCalls;
            clock.Now = T0.AddMinutes(2);
            var r2 = await c.CheckAsync();
            Assert.Eq(calls, site.FeedCalls);
            Assert.True(r2.Sources.Single().Note!.Contains("недавно не отвечал"), "объяснено почему");
            clock.Now = T0.AddMinutes(6);
            site.FeedFails = null; site.Feed.Add(L("m0", "1.0"));
            var r3 = await c.CheckAsync();
            Assert.True(site.FeedCalls > calls, "после паузы пробуем");
            Assert.Eq(SourceStatus.Ok, r3.Sources.Single().Status);
        });

        t.Run("«принудительно» снимает паузу после сбоя", async () =>
        {
            var clock = new Clock();
            var site = new Site("x") { FeedFails = new SourceException(SourceStatus.Unavailable, "502") };
            var c = new UpdateCoordinator(Store(), () => new IModSource[] { site }, () => clock.Now);
            await c.CheckAsync();
            var calls = site.FeedCalls;
            clock.Now = T0.AddMinutes(2);
            await c.CheckAsync(force: true);
            Assert.True(site.FeedCalls > calls, "пользователь настоял");
        });

        t.Run("проверка «вы человек?»: сайт пропускается всеми нажатиями, пока её не пройдут", async () =>
        {
            var clock = new Clock();
            var reset = new List<string>();
            var site = new Site("x") { FeedFails = new SourceException(SourceStatus.NeedsHumanCheck, "challenge") };
            var store = Store();
            var c = new UpdateCoordinator(store, () => new IModSource[] { site }, () => clock.Now, resetSource: reset.Add);
            await c.CheckAsync();
            var calls = site.FeedCalls;
            clock.Now = T0.AddHours(8);
            var r = await c.CheckAsync();
            Assert.Eq(calls, site.FeedCalls);
            Assert.Eq(SourceStatus.NeedsHumanCheck, r.Sources.Single().Status);

            c.ResolveHumanCheck("x");
            Assert.True(reset.SequenceEqual(new[] { "x" }), "сетевой слой сброшен");
            site.FeedFails = null; site.Feed.AddRange(new[] { L("m0", "1.0"), L("m1", "1.0"), L("m2", "1.0") });
            var r2 = await c.CheckAsync();
            Assert.True(site.FeedCalls > calls, "после прохождения опрошен");
            Assert.Eq(SourceStatus.Ok, r2.Sources.Single().Status);
        });

        t.Run("отмена: проверка завершается отчётом, следующее нажатие работает", async () =>
        {
            var gate = new TaskCompletionSource();
            var site = new Site("x") { Gate = gate.Task, Feed = { L("m0", "1.0") } };
            var c = new UpdateCoordinator(Store(1), () => new IModSource[] { site }, () => T0);
            var a = c.CheckAsync();
            c.Cancel();
            gate.SetResult();
            var r = await a;
            Assert.True(r.Cancelled, "прервана");
            Assert.False(c.IsRunning, "не висит");
            var r2 = await c.CheckAsync(force: true); // отменённый отчёт не считается «недавней проверкой»
            Assert.False(r2.Reused, "новая проверка");
            Assert.False(r2.Cancelled, "доведена до конца");
        });

        t.Run("исключение внутри проверки не оставляет координатор «занятым»", async () =>
        {
            var c = new UpdateCoordinator(Store(1), () => throw new InvalidOperationException("boom"), () => T0);
            var threw = false;
            try { await c.CheckAsync(); } catch (InvalidOperationException) { threw = true; }
            Assert.True(threw, "ошибка дошла до вызывающего");
            Assert.False(c.IsRunning, "флаг сброшен");
        });

        t.Run("чтение библиотеки из другого потока во время проверки не ломает хранилище", async () =>
        {
            var store = Store(50);
            var site = new Site("x");
            for (var i = 0; i < 50; i++) site.Feed.Add(L($"m{i}", "1.0"));
            var c = new UpdateCoordinator(store, () => new IModSource[] { site }, () => T0);
            var run = c.CheckAsync();
            var reads = Task.Run(() => { for (var i = 0; i < 2000; i++) { _ = store.Mods(); _ = UpdateCheckService.Pending(store); } });
            await Task.WhenAll(run, reads);
            Assert.Eq(50, run.Result.UpToDate);
        });
    }
}
