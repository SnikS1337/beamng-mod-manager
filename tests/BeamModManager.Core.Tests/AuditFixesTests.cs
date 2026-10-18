using System.Net.Http;
using System.Net.Sockets;
using System.Security.Authentication;
using System.Text;
using BeamModManager.Core.Net;
using BeamModManager.Core.Safety;
using BeamModManager.Core.Sources;

namespace BeamModManager.Core.Tests;

/// <summary>Аудит перед завершением (2026-10-18): сеть, запись файлов, понятные причины сбоев.</summary>
public static class AuditFixesTests
{
    /// <summary>Random, который замечает, если в него зашли из двух потоков сразу (такой Random ломается).</summary>
    private sealed class GuardRandom : Random
    {
        private int _inside;
        public volatile bool Overlapped;
        protected override double Sample()
        {
            if (Interlocked.Increment(ref _inside) > 1) Overlapped = true;
            Thread.SpinWait(5000); // шире окно: без замка два потока здесь встречаются почти наверняка
            Interlocked.Decrement(ref _inside);
            return 0.5;
        }
    }

    private sealed class Pages : IPageFetcher
    {
        private readonly Func<Uri, int, object> _reply;
        public int Calls;
        public TaskCompletionSource? Hold;
        public Pages(Func<Uri, int, object> reply) => _reply = reply;
        public async Task<RawResponse> GetAsync(Uri url, CancellationToken ct)
        {
            var n = Interlocked.Increment(ref Calls);
            if (Hold is { } h) await h.Task;
            await Task.Yield();
            return _reply(url, n) switch { Exception e => throw e, RawResponse r => r, _ => throw new InvalidOperationException() };
        }
    }

    private static readonly PoliteFetchOptions Fast = new()
    {
        MinDelay = TimeSpan.FromMilliseconds(1), Jitter = TimeSpan.FromMilliseconds(1), MaxRetries = 2,
        DefaultBackoff = TimeSpan.FromMilliseconds(1), MaxBackoff = TimeSpan.FromMilliseconds(5),
    };

    private static Task NoWait(TimeSpan _, CancellationToken __) => Task.CompletedTask;

    public static void Register(TestRunner t)
    {
        t.Group("Аудит: сеть и файлы");

        t.Run("разброс пауз: общий Random не трогают два сайта одновременно", async () =>
        {
            var rnd = new GuardRandom();
            var clock = DateTimeOffset.UtcNow;
            var f = new PoliteFetcher(new Pages((_, _) => new RawResponse(200, "<html>ok</html>")), Fast, () => clock, NoWait, rnd);
            var jobs = Enumerable.Range(0, 24).Select(h => Task.Run(async () =>
            {
                for (var i = 0; i < 6; i++) await f.GetAsync(new Uri($"https://h{h}.test/{i}"));
            }));
            await Task.WhenAll(jobs);
            Assert.False(rnd.Overlapped, "Random трогали из двух потоков сразу");
        });

        t.Run("одна и та же страница одновременно из двух мест — в сеть один раз, второй берёт из кэша", async () =>
        {
            var pages = new Pages((_, _) => new RawResponse(200, "<html>ok</html>")) { Hold = new TaskCompletionSource() };
            var f = new PoliteFetcher(pages, Fast, () => DateTimeOffset.UtcNow, NoWait, new Random(1));
            var url = new Uri("https://a.test/page");
            var first = f.GetAsync(url);
            var second = f.GetAsync(url);
            pages.Hold.SetResult();
            var r = await Task.WhenAll(first, second);
            Assert.Eq(1, pages.Calls);
            Assert.True(r.All(x => x.IsSuccess) && r.Count(x => x.FromCache) == 1, "второй ответ — из кэша");
        });

        t.Run("502/504/520–524: один повтор; 500 не повторяется", async () =>
        {
            foreach (var code in new[] { 502, 504, 520, 522, 524 })
            {
                var once = new Pages((_, n) => n == 1 ? new RawResponse(code, "") : new RawResponse(200, "<html>ok</html>"));
                var ok = await new PoliteFetcher(once, Fast, () => DateTimeOffset.UtcNow, NoWait, new Random(1)).GetAsync(new Uri("https://a.test/x"));
                Assert.True(ok.IsSuccess && once.Calls == 2, $"{code}: после повтора страница");

                var always = new Pages((_, _) => new RawResponse(code, ""));
                var bad = await new PoliteFetcher(always, Fast, () => DateTimeOffset.UtcNow, NoWait, new Random(1)).GetAsync(new Uri("https://a.test/x"));
                Assert.Eq(2, always.Calls);
                Assert.True(bad.Source == SourceStatus.Unavailable && bad.Note!.Contains(code.ToString()), $"{code}: {bad.Note}");
            }
            var e500 = new Pages((_, _) => new RawResponse(500, ""));
            await new PoliteFetcher(e500, Fast, () => DateTimeOffset.UtcNow, NoWait, new Random(1)).GetAsync(new Uri("https://a.test/x"));
            Assert.Eq(1, e500.Calls);
        });

        t.Run("причина сетевого сбоя — словами, без системного текста", async () =>
        {
            Assert.Eq("сайт не найден: нет сети или адрес изменился",
                NetErrors.Describe(new HttpRequestException(HttpRequestError.NameResolutionError, "No such host is known.")));
            Assert.Eq("сайт не найден: нет сети или адрес изменился",
                NetErrors.Describe(new HttpRequestException("x", new SocketException((int)SocketError.HostNotFound))));
            Assert.Eq("нет связи с сайтом", NetErrors.Describe(new HttpRequestException(HttpRequestError.ConnectionError, "refused")));
            Assert.Eq("не удалось установить защищённое соединение", NetErrors.Describe(new HttpRequestException("x", new AuthenticationException("tls"))));
            Assert.Eq("сайт не ответил вовремя", NetErrors.Describe(new TaskCanceledException()));
            Assert.Eq("соединение оборвалось", NetErrors.Describe(new IOException("reset")));
            Assert.Eq("разметка изменилась", NetErrors.ForUser(new InvalidOperationException("разметка изменилась")));

            var down = new Pages((_, _) => new HttpRequestException(HttpRequestError.NameResolutionError, "No such host is known. (a.test:443)"));
            var r = await new PoliteFetcher(down, Fast, () => DateTimeOffset.UtcNow, NoWait, new Random(1)).GetAsync(new Uri("https://a.test/x"));
            Assert.Eq("сайт не найден: нет сети или адрес изменился", r.Note);
        });

        t.Run("запись на диск до замены: тот же текст в UTF-8 без BOM, файл перезаписывается", () =>
        {
            var dir = Path.Combine(Path.GetTempPath(), "bmm-durable-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(dir);
            try
            {
                var p = Path.Combine(dir, "settings.json");
                DurableFile.WriteAllText(p, "{\"длинный\": \"текст\"}");
                DurableFile.WriteAllText(p, "{\"a\": 1}");
                var bytes = File.ReadAllBytes(p);
                Assert.True(bytes.Length > 0 && bytes[0] == (byte)'{', "без BOM");
                Assert.Eq("{\"a\": 1}", Encoding.UTF8.GetString(bytes));
            }
            finally { Directory.Delete(dir, true); }
        });
    }
}
