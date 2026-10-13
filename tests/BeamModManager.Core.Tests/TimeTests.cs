using System.IO.Compression;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Net;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Updates;

namespace BeamModManager.Core.Tests;

/// <summary>Всё, что зависит от времени: перевод часов назад и вперёд, пояса, лимиты. Часы везде подменяются.</summary>
public static class TimeTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);

    private sealed class Clock
    {
        public DateTimeOffset Now = T0;
        public readonly List<TimeSpan> Delays = new();
        public Task Delay(TimeSpan t, CancellationToken _) { Delays.Add(t); Now += t; return Task.CompletedTask; }
    }

    private sealed class Pages : IPageFetcher
    {
        public int Calls;
        public int Status = 200;
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct) { Calls++; return Task.FromResult(new RawResponse(Status, "<html>ok</html>")); }
    }

    private static readonly PoliteFetchOptions Opt = new()
    {
        MinDelay = TimeSpan.FromSeconds(2), Jitter = TimeSpan.Zero, MaxRetries = 0,
        CacheTtl = TimeSpan.FromMinutes(30), FailuresBeforeCooldown = 3, Cooldown = TimeSpan.FromMinutes(10),
    };

    private static (PoliteFetcher F, Pages P, Clock C) Make()
    {
        var c = new Clock(); var p = new Pages();
        return (new PoliteFetcher(p, Opt, () => c.Now, c.Delay, new Random(1)), p, c);
    }

    private static Uri U(string path) => new("https://a.test" + path);

    private static MemoryStream Mod(int size = 40)
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true))
        {
            using (var w = z.CreateEntry("vehicles/car/info.json").Open()) w.Write(new byte[10]);
            { var data = new byte[size]; new Random(size).NextBytes(data); using var w = z.CreateEntry("vehicles/car/car.jbeam").Open(); w.Write(data); }
        }
        ms.Position = 0;
        return ms;
    }

    public static void Register(TestRunner t)
    {
        t.Group("Время: перевод часов");

        t.Run("часы ушли назад на час: пауза между запросами остаётся обычной, а не часовой", async () =>
        {
            var (f, _, c) = Make();
            await f.GetAsync(U("/1"));
            c.Now -= TimeSpan.FromHours(1);
            c.Delays.Clear();
            await f.GetAsync(U("/2"));
            Assert.True(c.Delays.Count == 1 && c.Delays[0] <= TimeSpan.FromSeconds(2.01), $"ждали {string.Join(",", c.Delays)}");
        });

        t.Run("часы ушли назад: страница из кэша свежей не считается", async () =>
        {
            var (f, p, c) = Make();
            await f.GetAsync(U("/1"));
            c.Now -= TimeSpan.FromHours(5);
            var again = await f.GetAsync(U("/1"));
            Assert.False(again.FromCache, "не из кэша");
            Assert.Eq(2, p.Calls);
        });

        t.Run("часы ушли вперёд за срок: кэш устарел; в пределах срока — работает как раньше", async () =>
        {
            var (f, p, c) = Make();
            await f.GetAsync(U("/1"));
            c.Now += TimeSpan.FromMinutes(10);
            Assert.True((await f.GetAsync(U("/1"))).FromCache, "в пределах срока");
            c.Now += TimeSpan.FromMinutes(40);
            Assert.False((await f.GetAsync(U("/1"))).FromCache, "после срока");
        });

        t.Run("пауза после серии сбоев не растягивается на сутки, если часы перевели назад", async () =>
        {
            var (f, p, c) = Make();
            p.Status = 500;
            for (var i = 0; i < 3; i++) await f.GetAsync(U($"/{i}"));
            Assert.Eq(BeamModManager.Core.Sources.SourceStatus.Unavailable, f.StatusOf("a.test"));
            c.Now -= TimeSpan.FromDays(1);
            Assert.Eq(BeamModManager.Core.Sources.SourceStatus.Ok, f.StatusOf("a.test"));
            p.Status = 200;
            Assert.True((await f.GetAsync(U("/later"))).IsSuccess, "сайт снова опрашивается");
        });

        t.Run("пауза после сбоев держится положенное время при нормальных часах", async () =>
        {
            var (f, p, c) = Make();
            p.Status = 500;
            for (var i = 0; i < 3; i++) await f.GetAsync(U($"/{i}"));
            c.Now += TimeSpan.FromMinutes(5);
            Assert.Eq(BeamModManager.Core.Sources.SourceStatus.Unavailable, f.StatusOf("a.test"));
            c.Now += TimeSpan.FromMinutes(6);
            Assert.Eq(BeamModManager.Core.Sources.SourceStatus.Ok, f.StatusOf("a.test"));
        });

        t.Group("Время: проверка обновлений");

        t.Run("время проверки в будущем (часы были неверными): мод считается непроверенным, а не вечно свежим", () =>
        {
            var b = new TrackedBinding("m", "x", "id", T0.AddDays(30), T0.AddDays(-5), "1.0", 100);
            var plan = UpdateCheckPlanner.PlanFeeds(new[] { b }, new Dictionary<string, SourceStatus>(), T0);
            Assert.Eq(1, plan.Due.Count);
            return Task.CompletedTask;
        });

        t.Run("время в другом часовом поясе: сравнивается момент, а не часы на стене", () =>
        {
            var checkedAt = new DateTimeOffset(2026, 10, 13, 14, 0, 0, TimeSpan.FromHours(2));   // = 12:00 UTC
            var b = new TrackedBinding("m", "x", "id", checkedAt, checkedAt, "1.0", 100);
            var fresh = UpdateCheckPlanner.PlanFeeds(new[] { b }, new Dictionary<string, SourceStatus>(), T0.AddHours(1));
            Assert.Eq(0, fresh.Due.Count);
            var stale = UpdateCheckPlanner.PlanFeeds(new[] { b }, new Dictionary<string, SourceStatus>(), T0.AddHours(7));
            Assert.Eq(1, stale.Due.Count);
            return Task.CompletedTask;
        });

        t.Run("лимит времени проверки считается и по секундомеру: застывшие или отстающие часы проверку не растягивают", async () =>
        {
            var store = new InMemoryLibraryStore();
            for (var i = 0; i < 60; i++)
            {
                store.UpsertMod(new InstalledMod($"m{i}", $"m{i}", $"m{i}.zip", "1.0", null, T0.AddDays(-90)));
                store.UpsertBinding(new ModBinding($"m{i}", "x", $"s{i}", new Uri($"https://x.test/s{i}"), true));
                store.SaveCheck(new CheckState($"m{i}", "x", T0.AddDays(-30), T0.AddDays(-60), "1.0", null));
            }
            var site = new SlowSite("x", TimeSpan.FromMilliseconds(30));
            var opt = new UpdateCheckOptions { MaxDuration = TimeSpan.FromMilliseconds(250), MaxCardChecksPerSource = 500 };
            var r = await UpdateRunner.RunAsync(store, new IModSource[] { site }, T0, opt, clock: () => T0);   // часы стоят на месте
            Assert.True(r.StoppedByTime, "остановлено по времени");
            Assert.True(site.CardCalls < 30, $"карточек {site.CardCalls}");
        });

        t.Run("координатор: после перевода часов назад принудительная проверка не получает вечный «прошлый отчёт»", async () =>
        {
            var now = T0;
            var store = new InMemoryLibraryStore();
            store.UpsertMod(new InstalledMod("m", "m", "m.zip", "1.0", null, T0.AddDays(-90)));
            store.UpsertBinding(new ModBinding("m", "x", "s", new Uri("https://x.test/s"), true));
            var site = new SlowSite("x", TimeSpan.Zero);
            var c = new UpdateCoordinator(store, () => new IModSource[] { site }, () => now);
            await c.CheckAsync(force: true);
            var calls = site.FeedCalls;
            now = T0.AddHours(-3);                           // часы ушли назад
            await c.CheckAsync(force: true);
            Assert.True(site.FeedCalls > calls, "проверка выполнена заново");
        });

        t.Group("Время: резервные копии");

        t.Run("часы ушли назад между установками: порядок копий не путается, восстановление берёт самую последнюю версию, лишние удаляются старые", () =>
        {
            var root = Path.Combine(Path.GetTempPath(), "bmm-time-" + Guid.NewGuid().ToString("N"));
            try
            {
                var layout = new ModLayout(Path.Combine(root, "mods"), Path.Combine(root, "dis"), Path.Combine(root, "bak"));
                var now = T0;
                var inst = new ModInstaller(layout, () => now, keepBackups: 3);
                inst.Install(Mod(1000), "car.zip");                                               // v1
                now = T0.AddMinutes(1);  inst.Install(Mod(2000), "car.zip", new InstallOptions { ReplaceExisting = true }); // v2 (v1 в резерв)
                now = T0.AddDays(-2);    inst.Install(Mod(3000), "car.zip", new InstallOptions { ReplaceExisting = true }); // v3, часы назад (v2 в резерв)
                now = T0.AddDays(-2).AddMinutes(1); inst.Install(Mod(4000), "car.zip", new InstallOptions { ReplaceExisting = true }); // v4 (v3 в резерв)
                var backups = inst.Backups("car.zip");
                Assert.Eq(3, backups.Count);
                // самая свежая копия — v3 (30 байт данных), и именно она в конце списка
                var sizes = backups.Select(b => new FileInfo(b).Length).ToList();
                Assert.True(sizes[2] > sizes[1] && sizes[1] > sizes[0], $"порядок по возрастанию размера v1<v2<v3: {string.Join(",", sizes)}");
                var restored = inst.RestoreLatestBackup("car.zip");
                Assert.True(restored.Success, restored.Message ?? "");
                Assert.Eq(sizes[2], new FileInfo(Path.Combine(layout.ModsDir, "car.zip")).Length);
            }
            finally { try { Directory.Delete(root, true); } catch { } }
            return Task.CompletedTask;
        });
    }

    /// <summary>Сайт, у которого каждая карточка отвечает не сразу (настоящая задержка).</summary>
    private sealed class SlowSite : IModSource
    {
        private readonly TimeSpan _delay;
        public int CardCalls, FeedCalls;
        public SlowSite(string id, TimeSpan delay) { Id = id; DisplayName = id; _delay = delay; }
        public string Id { get; }
        public string DisplayName { get; }
        public bool SupportsSearch => false;
        public Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default)
        {
            Interlocked.Increment(ref FeedCalls);
            return Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        }
        public async Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default)
        {
            Interlocked.Increment(ref CardCalls);
            if (_delay > TimeSpan.Zero) await Task.Delay(_delay, ct);
            var id = url.AbsolutePath.Trim('/');
            return new Listing(Id, url, id, id) { ModVersion = "1.0" };
        }
        public Task<IReadOnlyList<Listing>> SearchAsync(string q, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? s, CancellationToken ct = default) => FeedPageAsync(1, ct);
        public Task<IReadOnlyList<Offer>> GetOffersAsync(Listing l, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Offer>>(Array.Empty<Offer>());
    }
}
