using System.Net;
using BeamModManager.Core.Net;
using BeamModManager.Core.Sources;

namespace BeamModManager.Core.Tests;

public static class NetTests
{
    private sealed class Clock
    {
        public DateTimeOffset Now = new(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);
        public Task Delay(TimeSpan t, CancellationToken _) { Now += t; return Task.CompletedTask; }
    }

    private sealed class Script : IPageFetcher
    {
        private readonly Func<Uri, int, object> _reply; // RawResponse или Exception
        private readonly Clock _clock;
        public readonly List<(Uri Url, DateTimeOffset At)> Calls = new();
        public int Concurrent, MaxConcurrent;
        public Script(Clock clock, Func<Uri, int, object> reply) { _clock = clock; _reply = reply; }

        public async Task<RawResponse> GetAsync(Uri url, CancellationToken ct)
        {
            var n = Interlocked.Increment(ref Concurrent);
            MaxConcurrent = Math.Max(MaxConcurrent, n);
            Calls.Add((url, _clock.Now));
            await Task.Yield();
            Interlocked.Decrement(ref Concurrent);
            return _reply(url, Calls.Count) switch
            {
                Exception e => throw e,
                RawResponse r => r,
                _ => throw new InvalidOperationException(),
            };
        }
    }

    private static readonly PoliteFetchOptions Opt = new()
    {
        MinDelay = TimeSpan.FromSeconds(2), Jitter = TimeSpan.Zero, MaxRetries = 2,
        DefaultBackoff = TimeSpan.FromSeconds(5), MaxBackoff = TimeSpan.FromSeconds(60),
        CacheTtl = TimeSpan.FromMinutes(30), FailuresBeforeCooldown = 3, Cooldown = TimeSpan.FromMinutes(10),
    };

    private static (PoliteFetcher F, Script S, Clock C) Make(Func<Uri, int, object> reply, PoliteFetchOptions? o = null)
    {
        var c = new Clock();
        var s = new Script(c, reply);
        return (new PoliteFetcher(s, o ?? Opt, () => c.Now, c.Delay, new Random(1)), s, c);
    }

    private static Uri U(string host, string path = "/a") => new($"https://{host}{path}");
    private static RawResponse Ok(string body = "<html>ok</html>") => new(200, body);
    private const string Challenge = "<html><head><title>Один момент…</title></head><body>Проверяем, человек ли вы</body></html>";

    public static void Register(TestRunner t)
    {
        t.Group("Сетевой слой");

        t.Run("запросы к одному сайту идут с паузой не меньше минимальной", async () =>
        {
            var (f, s, _) = Make((_, _) => Ok());
            await f.GetAsync(U("a.test", "/1")); await f.GetAsync(U("a.test", "/2")); await f.GetAsync(U("a.test", "/3"));
            Assert.True(s.Calls[1].At - s.Calls[0].At >= TimeSpan.FromSeconds(2), "пауза 1-2");
            Assert.True(s.Calls[2].At - s.Calls[1].At >= TimeSpan.FromSeconds(2), "пауза 2-3");
        });

        t.Run("разные сайты друг друга не ждут", async () =>
        {
            var (f, s, _) = Make((_, _) => Ok());
            await f.GetAsync(U("a.test")); await f.GetAsync(U("b.test"));
            Assert.Eq(s.Calls[0].At, s.Calls[1].At);
        });

        t.Run("параллельные запросы к одному сайту выстраиваются в очередь", async () =>
        {
            var (f, s, _) = Make((_, _) => Ok());
            await Task.WhenAll(Enumerable.Range(0, 6).Select(i => f.GetAsync(U("a.test", $"/{i}"))));
            Assert.Eq(1, s.MaxConcurrent);
            Assert.Eq(6, s.Calls.Count);
        });

        t.Run("кэш: повторный запрос не идёт в сеть, после TTL идёт", async () =>
        {
            var (f, s, c) = Make((_, _) => Ok());
            await f.GetAsync(U("a.test"));
            var again = await f.GetAsync(U("a.test"));
            Assert.True(again.FromCache, "из кэша");
            Assert.Eq(1, s.Calls.Count);
            c.Now += TimeSpan.FromMinutes(31);
            await f.GetAsync(U("a.test"));
            Assert.Eq(2, s.Calls.Count);
        });

        t.Run("429 с Retry-After: ждём указанное время и получаем страницу", async () =>
        {
            var (f, s, _) = Make((_, n) => n == 1 ? new RawResponse(429, "", TimeSpan.FromSeconds(10)) : Ok());
            var r = await f.GetAsync(U("a.test"));
            Assert.True(r.IsSuccess, "успех после повтора");
            Assert.True(s.Calls[1].At - s.Calls[0].At >= TimeSpan.FromSeconds(10), "ждали Retry-After");
        });

        t.Run("Retry-After больше потолка обрезается", async () =>
        {
            var (f, s, _) = Make((_, n) => n == 1 ? new RawResponse(429, "", TimeSpan.FromHours(3)) : Ok());
            await f.GetAsync(U("a.test"));
            Assert.True(s.Calls[1].At - s.Calls[0].At <= TimeSpan.FromSeconds(62), "обрезано до 60 с");
        });

        t.Run("429 без конца: попыток не больше 1 + MaxRetries, статус «недоступен»", async () =>
        {
            var (f, s, _) = Make((_, _) => new RawResponse(429, ""));
            var r = await f.GetAsync(U("a.test"));
            Assert.Eq(3, s.Calls.Count);
            Assert.Eq(SourceStatus.Unavailable, r.Source);
        });

        t.Run("страница проверки: статус NeedsHumanCheck, сайт больше не опрашивается", async () =>
        {
            var (f, s, _) = Make((_, _) => new RawResponse(403, Challenge));
            var r = await f.GetAsync(U("a.test", "/1"));
            Assert.Eq(SourceStatus.NeedsHumanCheck, r.Source);
            var r2 = await f.GetAsync(U("a.test", "/2"));
            Assert.Eq(SourceStatus.NeedsHumanCheck, r2.Source);
            Assert.Eq(1, s.Calls.Count);
            Assert.Eq(SourceStatus.NeedsHumanCheck, f.StatusOf("a.test"));
        });

        t.Run("после ResetHost сайт опрашивается снова", async () =>
        {
            var blocked = true;
            var (f, s, _) = Make((_, _) => blocked ? new RawResponse(403, Challenge) : Ok());
            await f.GetAsync(U("a.test", "/1"));
            blocked = false; f.ResetHost("a.test");
            var r = await f.GetAsync(U("a.test", "/2"));
            Assert.True(r.IsSuccess, "успех после прохождения проверки");
            Assert.Eq(2, s.Calls.Count);
        });

        t.Run("проверка на одном сайте не мешает другому", async () =>
        {
            var (f, _, _) = Make((u, _) => u.Host == "a.test" ? new RawResponse(403, Challenge) : Ok());
            await f.GetAsync(U("a.test"));
            Assert.True((await f.GetAsync(U("b.test"))).IsSuccess, "b работает");
        });

        t.Run("404: страницы нет, но это не сбой сайта", async () =>
        {
            var (f, s, _) = Make((_, _) => new RawResponse(404, "нет"));
            for (var i = 0; i < 5; i++) await f.GetAsync(U("a.test", $"/{i}"));
            var r = await f.GetAsync(U("a.test", "/x"));
            Assert.True(r.IsNotFound, "404");
            Assert.Eq(SourceStatus.Ok, r.Source);
            Assert.Eq(6, s.Calls.Count);
        });

        t.Run("три сбоя подряд: сайт отключается на время, потом снова доступен", async () =>
        {
            var (f, s, c) = Make((_, _) => new RawResponse(500, "ошибка"));
            for (var i = 0; i < 3; i++) await f.GetAsync(U("a.test", $"/{i}"));
            var before = s.Calls.Count;
            var r = await f.GetAsync(U("a.test", "/next"));
            Assert.Eq(SourceStatus.Unavailable, r.Source);
            Assert.Eq(before, s.Calls.Count);
            c.Now += TimeSpan.FromMinutes(11);
            await f.GetAsync(U("a.test", "/later"));
            Assert.Eq(before + 1, s.Calls.Count);
        });

        t.Run("успешный ответ сбрасывает счётчик сбоев", async () =>
        {
            var (f, _, _) = Make((u, _) => u.AbsolutePath == "/ok" ? Ok() : new RawResponse(500, ""));
            await f.GetAsync(U("a.test", "/1")); await f.GetAsync(U("a.test", "/2"));
            await f.GetAsync(U("a.test", "/ok"));
            await f.GetAsync(U("a.test", "/3")); await f.GetAsync(U("a.test", "/4"));
            Assert.Eq(SourceStatus.Ok, f.StatusOf("a.test"));
        });

        t.Run("сетевая ошибка: одна повторная попытка и затем успех", async () =>
        {
            var (f, s, _) = Make((_, n) => n == 1 ? new HttpRequestException("обрыв") : Ok());
            Assert.True((await f.GetAsync(U("a.test"))).IsSuccess, "успех");
            Assert.Eq(2, s.Calls.Count);
        });

        t.Run("HttpPageFetcher: читает тело и Retry-After, ставит User-Agent", async () =>
        {
            using var listener = new HttpListener();
            var port = new Random().Next(20000, 40000);
            listener.Prefixes.Add($"http://127.0.0.1:{port}/");
            listener.Start();
            string? ua = null;
            var server = Task.Run(async () =>
            {
                for (var i = 0; i < 2; i++)
                {
                    var ctx = await listener.GetContextAsync();
                    ua = ctx.Request.UserAgent;
                    if (ctx.Request.Url!.AbsolutePath == "/busy") { ctx.Response.StatusCode = 429; ctx.Response.Headers["Retry-After"] = "7"; }
                    else { var b = System.Text.Encoding.UTF8.GetBytes("<html>привет</html>"); ctx.Response.OutputStream.Write(b); }
                    ctx.Response.Close();
                }
            });
            var f = new HttpPageFetcher();
            var ok = await f.GetAsync(new Uri($"http://127.0.0.1:{port}/page"), CancellationToken.None);
            var busy = await f.GetAsync(new Uri($"http://127.0.0.1:{port}/busy"), CancellationToken.None);
            await server;
            Assert.Eq(200, ok.Status);
            Assert.True(ok.Body.Contains("привет"), "тело");
            Assert.Eq(429, busy.Status);
            Assert.Eq(TimeSpan.FromSeconds(7), busy.RetryAfter);
            Assert.True(ua!.StartsWith("BeamModManager/"), "User-Agent");
        });
    }
}
