using System.Net;
using System.Net.Http.Headers;
using System.Text;
using BeamModManager.Core.Hosts;
using BeamModManager.Core.Net;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Text;
using BeamModManager.Core.Versioning;

namespace BeamModManager.Core.Tests;

/// <summary>Регрессионные тесты на ошибки, найденные при ревизии кода.</summary>
public static class BugHuntTests
{
    private const long MB = 1024 * 1024;

    private sealed class Handler : HttpMessageHandler
    {
        private readonly Func<HttpRequestMessage, Task<HttpResponseMessage>> _f;
        public Handler(Func<HttpRequestMessage, Task<HttpResponseMessage>> f) { _f = f; }
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage r, CancellationToken ct) => _f(r);
    }

    private sealed class Throwing : IPageFetcher
    {
        public int Calls;
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct)
        {
            Calls++;
            throw new TaskCanceledException("The request was canceled due to the configured HttpClient.Timeout");
        }
    }

    private sealed class CancelOnCall : IPageFetcher
    {
        private readonly CancellationTokenSource _cts;
        public CancelOnCall(CancellationTokenSource cts) { _cts = cts; }
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct) { _cts.Cancel(); ct.ThrowIfCancellationRequested(); return Task.FromResult(new RawResponse(200, "x")); }
    }

    private sealed class Echo : IPageFetcher
    {
        public int Calls;
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct) { Calls++; return Task.FromResult(new RawResponse(200, "<html>" + url + "</html>")); }
    }

    public static void Register(TestRunner t)
    {
        t.Group("Ревизия: размеры и версии");

        t.Run("размер: округление на сайте («316,40 МБ») не считается изменением", () =>
        {
            var real = (long)(316.4037 * MB);
            var shown = (long)(316.40 * MB);
            Assert.False(SizeComparer.Differs(real, shown), "округление до сотых");
            Assert.False(SizeComparer.Differs((long)(1.24 * 1024 * MB), (long)(1.2 * 1024 * MB)), "«1,2 ГБ» вместо 1,24 ГБ");
            Assert.False(SizeComparer.Differs(100, 100), "равны");
        });
        t.Run("размер: заметное изменение — отличие; неизвестное и нулевое — нет", () =>
        {
            Assert.True(SizeComparer.Differs(100 * MB, 130 * MB), "+30%");
            Assert.True(SizeComparer.Differs(300 * 1024, 1024 * 1024), "малый файл заметно вырос");
            Assert.False(SizeComparer.Differs(null, 5), "нет данных");
            Assert.False(SizeComparer.Differs(0, 5), "ноль не размер");
        });
        t.Run("решение об обновлении: одинаковая версия и округлённый размер — обновления нет", () =>
        {
            var real = (long)(316.4037 * MB);
            var shown = (long)(316.40 * MB);
            Assert.Eq(UpdateStatus.None, UpdateDecider.Decide(new VersionSnapshot("5.1", real), new VersionSnapshot("5.1", shown)));
            Assert.Eq(UpdateStatus.Possible, UpdateDecider.Decide(new VersionSnapshot("5.1", real), new VersionSnapshot("5.1", real + 40 * MB)));
        });

        t.Run("версии: beta раньше релиза, rc после beta, alpha перед beta", () =>
        {
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("0.3.1-beta", "0.3.1"));
            Assert.Eq(VersionOrder.Older, VersionComparer.Compare("0.3.1", "0.3.1-beta"));
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.0 beta", "1.0 rc"));
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.0-alpha", "1.0-beta"));
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.0-rc1", "1.0-rc2"));
            Assert.Eq(VersionOrder.Same, VersionComparer.Compare("1.0-beta", "1.0-beta"));
            Assert.Eq(VersionOrder.Same, VersionComparer.Compare("1.0-beta1", "1.0-beta"));
        });
        t.Run("версии: число важнее метки; слова-«не метки» не считаются ими", () =>
        {
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.0.9-beta", "1.1.0-alpha"));
            Assert.Eq(VersionOrder.Same, VersionComparer.Compare("2.0 Bold Edition", "2.0")); // «b» из Bold меткой не считается
            Assert.Eq(VersionOrder.Same, VersionComparer.Compare("1.2 Adventure", "1.2"));
        });
        t.Run("версии: огромные числа и мусор не вызывают исключений", () =>
        {
            Assert.Eq(VersionOrder.Unknown, VersionComparer.Compare("1.99999999999999999999", "1.0"));
            Assert.Eq(VersionOrder.Unknown, VersionComparer.Compare(new string('9', 5000) + ".1", "1.0"));
            foreach (var x in new[] { "", " ", "..", "v", "1.", ".1", "1..2", "-1.0" }) VersionComparer.Compare(x, "1.0");
        });

        t.Run("ModsFire: ссылки /d/{id} и /install_mod/{id} дают тот же идентификатор файла, что и /download/", () =>
        {
            var a = HostLinkParser.TryParse("https://modsfire.com/d/9nDwN00BJ04Ef55");
            var b = HostLinkParser.TryParse("https://modsfire.com/install_mod/9nDwN00BJ04Ef55/?manager=ava");
            var c = HostLinkParser.TryParse("https://modsfire.com/9nDwN00BJ04Ef55");
            Assert.Eq("9nDwN00BJ04Ef55", a?.FileId);
            Assert.Eq("9nDwN00BJ04Ef55", b?.FileId);
            Assert.Eq(c?.FileId, a?.FileId);
        });

        t.Group("Ревизия: сетевой слой");

        t.Run("страница в windows-1251 читается правильно", async () =>
        {
            var bytes = Encoding.GetEncoding(1251).GetBytes("<html><title>Тойота Сиенна</title></html>");
            var http = new HttpClient(new Handler(_ =>
            {
                var resp = new HttpResponseMessage(HttpStatusCode.OK) { Content = new ByteArrayContent(bytes) };
                resp.Content.Headers.ContentType = new MediaTypeHeaderValue("text/html") { CharSet = "windows-1251" };
                return Task.FromResult(resp);
            }));
            var r = await new HttpPageFetcher(http).GetAsync(new Uri("https://x.test/"), CancellationToken.None);
            Assert.True(r.Body.Contains("Тойота Сиенна"), r.Body);
        });

        t.Run("таймаут HttpClient: повторы с паузой, потом отказ сайта (не исключение и не отмена)", async () =>
        {
            var inner = new Throwing();
            var now = new DateTimeOffset(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);
            var f = new PoliteFetcher(inner, new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero, MaxRetries = 2 },
                () => now, (d, _) => { now += d; return Task.CompletedTask; });
            var r = await f.GetAsync(new Uri("https://slow.test/a"));
            Assert.Eq(SourceStatus.Unavailable, r.Source);
            Assert.Eq(3, inner.Calls);
            Assert.True(r.Note!.Contains("не ответил"), r.Note);
        });

        t.Run("настоящая отмена пользователя по-прежнему отменяет, а не превращается в отказ сайта", async () =>
        {
            var cts = new CancellationTokenSource();
            var inner = new CancelOnCall(cts);
            var f = new PoliteFetcher(inner, new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero }, delay: (_, _) => Task.CompletedTask);
            var threw = false;
            try { await f.GetAsync(new Uri("https://x.test/a"), cts.Token); }
            catch (OperationCanceledException) { threw = true; }
            Assert.True(threw, "отмена дошла наверх");
        });

        t.Run("кэш страниц не растёт бесконечно", async () =>
        {
            var inner = new Echo();
            var now = new DateTimeOffset(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);
            var f = new PoliteFetcher(inner, new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero, MaxCacheEntries = 10 },
                () => now = now.AddSeconds(1), (_, _) => Task.CompletedTask);
            for (var i = 0; i < 100; i++) await f.GetAsync(new Uri($"https://x.test/{i}"));
            // Самые старые вытеснены: страница 0 запрашивается снова, свежая 99 берётся из кэша.
            var before = inner.Calls;
            await f.GetAsync(new Uri("https://x.test/99"));
            Assert.Eq(before, inner.Calls);
            await f.GetAsync(new Uri("https://x.test/0"));
            Assert.Eq(before + 1, inner.Calls);
        });

        t.Group("Ревизия: проверка архивов");

        t.Run("архив: исполняемый файл с точкой/пробелом на конце, альтернативный поток, управляющие символы — блокировка", () =>
        {
            foreach (var bad in new[] { "vehicles/a/run.exe.", "vehicles/a/run.exe ", "vehicles/a/x.txt:evil.exe", "vehicles/a/a\u0001b.jbeam", "vehicles/a/link.lnk", "vehicles/a/s.hta" })
            {
                var ms = new MemoryStream();
                using (var z = new System.IO.Compression.ZipArchive(ms, System.IO.Compression.ZipArchiveMode.Create, true))
                {
                    var e = z.CreateEntry(bad);
                    using var w = new StreamWriter(e.Open()); w.Write("x");
                }
                ms.Position = 0;
                Assert.True(BeamModManager.Core.Safety.ArchiveInspector.Inspect(ms).IsBlocked, bad);
            }
        });

        t.Run("архив: обрезанный и случайный файл не вызывают исключения", () =>
        {
            var ms = new MemoryStream();
            using (var z = new System.IO.Compression.ZipArchive(ms, System.IO.Compression.ZipArchiveMode.Create, true))
                for (var i = 0; i < 20; i++) { var e = z.CreateEntry($"vehicles/a/f{i}.jbeam"); using var w = new StreamWriter(e.Open()); w.Write(new string('x', 500)); }
            var bytes = ms.ToArray();
            for (var cut = 0; cut < bytes.Length; cut += 37)
            {
                var r = BeamModManager.Core.Safety.ArchiveInspector.Inspect(new MemoryStream(bytes, 0, cut));
                Assert.True(r.IsBlocked || r.EntryCount >= 0, "без исключения");
                BeamModManager.Core.Packs.PackContentReader.Read(new MemoryStream(bytes, 0, cut));
            }
            var rng = new Random(7);
            for (var k = 0; k < 200; k++)
            {
                var copy = (byte[])bytes.Clone();
                for (var m = 0; m < 8; m++) copy[rng.Next(copy.Length)] = (byte)rng.Next(256);
                BeamModManager.Core.Safety.ArchiveInspector.Inspect(new MemoryStream(copy));
                BeamModManager.Core.Packs.PackContentReader.Read(new MemoryStream(copy));
            }
        });
    }
}
