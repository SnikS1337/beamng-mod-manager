using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Net;
using BeamModManager.Core.Search;
using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Updates;

namespace BeamModManager.Core.Tests;

/// <summary>Сквозные сценарии «как будет пользоваться человек»: первый запуск, нет интернета, пустые списки, странные данные.</summary>
public static class ScenarioTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);

    private sealed class Offline : IPageFetcher
    {
        public int Calls;
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct) { Interlocked.Increment(ref Calls); throw new HttpRequestException("Нет сети"); }
    }

    private static PoliteFetcher Polite(IPageFetcher f) =>
        new(f, new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero, DefaultBackoff = TimeSpan.Zero });

    private static InMemoryLibraryStore Library(int n, string source)
    {
        var s = new InMemoryLibraryStore();
        for (var i = 0; i < n; i++)
        {
            s.UpsertMod(new InstalledMod($"m{i}", $"m{i}", $"m{i}.zip", "1.0", null, T0.AddDays(-90)));
            s.UpsertBinding(new ModBinding($"m{i}", source, $"s{i}", new Uri($"https://{source}.test/s{i}"), true));
            s.SaveCheck(new CheckState($"m{i}", source, T0.AddDays(-20), T0.AddDays(-60), "1.0", null));
        }
        return s;
    }

    public static void Register(TestRunner t)
    {
        t.Group("Сценарии: первый запуск и пустые данные");

        t.Run("пустая библиотека: проверка ничего не запрашивает и заканчивается нормально", async () =>
        {
            var off = new Offline();
            var src = new ModlandSource(Polite(off));
            var c = new UpdateCoordinator(new InMemoryLibraryStore(), () => new IModSource[] { src }, () => T0);
            var r = await c.CheckAsync();
            Assert.Eq(0, off.Calls);
            Assert.Eq(0, r.Findings.Count);
            Assert.True(r.Complete, "нечего проверять — это не сбой");
        });

        t.Run("ни одного подключённого сайта (все отключены): проверка не падает", async () =>
        {
            var c = new UpdateCoordinator(Library(5, "modland"), () => Array.Empty<IModSource>(), () => T0);
            var r = await c.CheckAsync();
            Assert.Eq(0, r.Findings.Count);
            Assert.False(r.Complete, "сайт не опрошен — это видно в отчёте");
        });

        t.Run("только неподтверждённые привязки: в проверке не участвуют, запросов нет", async () =>
        {
            var s = Library(3, "modland");
            foreach (var b in s.Bindings()) s.UpsertBinding(b with { Confirmed = false });
            var off = new Offline();
            var c = new UpdateCoordinator(s, () => new IModSource[] { new ModlandSource(Polite(off)) }, () => T0);
            await c.CheckAsync();
            Assert.Eq(0, off.Calls);
        });

        t.Group("Сценарии: нет интернета");

        t.Run("полностью нет сети: каждый сайт отмечен недоступным, запросов немного, сохранённые данные не тронуты", async () =>
        {
            var off = new Offline();
            var polite = Polite(off);
            var s = Library(100, "modland");
            var before = s.Checks().ToDictionary(c => c.ModId);
            var c = new UpdateCoordinator(s, () => new IModSource[] { new ModlandSource(polite) }, () => T0);
            var r = await c.CheckAsync();
            Assert.Eq(SourceStatus.Unavailable, r.Sources.Single().Status);
            Assert.True(off.Calls <= 9, $"запросов в пустоту: {off.Calls}");
            Assert.False(r.Complete, "не всё проверено");
            foreach (var cs in s.Checks()) Assert.Eq(before[cs.ModId], cs);
            Assert.Eq(0, r.Findings.Count);
        });

        t.Run("поиск без сети: ответ с отчётами по сайтам, без исключения", async () =>
        {
            var off = new Offline();
            var p = Polite(off);
            var search = new SearchService(new IModSource[] { new ModlandSource(p), new StModsSource(p), new ModsGamingSource(p) });
            var r = await search.SearchAsync("audi a3");
            Assert.Eq(0, r.Cards.Count);
            Assert.Eq(3, r.Sources.Count);
            Assert.True(r.Sources.All(x => x.Status != SourceStatus.Ok), "все отмечены как недоступные");
        });

        t.Group("Сценарии: странные данные");

        t.Run("поисковые запросы со спецсимволами, эмодзи, очень длинные, пустые: ни исключений, ни кривых адресов", async () =>
        {
            var off = new Offline();
            var p = Polite(off);
            var search = new SearchService(new IModSource[] { new ModlandSource(p), new StModsSource(p), new ModsGamingSource(p) });
            foreach (var q in new[] { "", " ", "a", "%", "?&=#", "../..", "audi 🚗 a3", "‮audi", new string('я', 3000), "a\0b", "a\r\nb", "\"'<script>" })
                await search.SearchAsync(q);
            Assert.True(off.Calls > 0, "часть запросов дошла до сети");
        });

        t.Run("два установленных мода привязаны к одной странице: обе строки обновляются, лишних запросов нет (кэш)", async () =>
        {
            var s = new InMemoryLibraryStore();
            foreach (var id in new[] { "a", "b" })
            {
                s.UpsertMod(new InstalledMod(id, id, id + ".zip", "1.0", null, T0.AddDays(-90)));
                s.UpsertBinding(new ModBinding(id, "modland", "same", new Uri("https://www.modland.net/same"), true));
                s.SaveCheck(new CheckState(id, "modland", T0.AddDays(-20), T0.AddDays(-60), "1.0", null));
            }
            var page = Pages.Modland("same", "Audi A3 v1.5");
            var fetch = new CountingPages(page);
            var c = new UpdateCoordinator(s, () => new IModSource[] { new ModlandSource(Polite(new CachingStub(fetch))) }, () => T0);
            var r = await c.CheckAsync();
            Assert.Eq(2, r.Findings.Count);
        });
    }

    // Минимальные заглушки страниц для сценария с одной страницей на двух модах.
    private sealed class CountingPages { public readonly string Body; public int Calls; public CountingPages(string b) { Body = b; } }
    private sealed class CachingStub : IPageFetcher
    {
        private readonly CountingPages _p;
        public CachingStub(CountingPages p) { _p = p; }
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct)
        {
            _p.Calls++;
            return Task.FromResult(url.AbsolutePath.Contains("same") ? new RawResponse(200, _p.Body) : new RawResponse(200, "<html></html>"));
        }
    }
    private static class Pages
    {
        public static string Modland(string slug, string title) => $"<html><head><title>{title} - BeamNG.drive</title></head><body><h1>{title}</h1></body></html>";
    }
}
