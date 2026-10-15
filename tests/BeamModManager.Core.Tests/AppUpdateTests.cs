using System.IO.Compression;
using System.Net;
using System.Security.Cryptography;
using BeamModManager.Core.AppUpdate;
using BeamModManager.Ui.AppUpdate;

namespace BeamModManager.Core.Tests;

public static class AppUpdateTests
{
    /// <summary>Подделка сети: адрес → ответ; считает обращения; может «зависнуть» до сигнала.</summary>
    private sealed class FakeNet : HttpMessageHandler
    {
        public readonly Dictionary<string, Func<HttpResponseMessage>> Routes = new();
        public readonly List<string> Calls = new();
        public TaskCompletionSource? Gate;

        protected override async Task<HttpResponseMessage> SendAsync(HttpRequestMessage req, CancellationToken ct)
        {
            lock (Calls) Calls.Add(req.RequestUri!.ToString());
            if (Gate is { } g) await g.Task.WaitAsync(ct);
            return Routes.TryGetValue(req.RequestUri!.ToString(), out var f) ? f() : new HttpResponseMessage(HttpStatusCode.NotFound);
        }
    }

    private static HttpResponseMessage Ok(byte[] body) => new(HttpStatusCode.OK) { Content = new ByteArrayContent(body) };
    private static HttpResponseMessage Redirect(string to) { var r = new HttpResponseMessage(HttpStatusCode.Found); r.Headers.Location = new Uri(to); return r; }

    private const string ManifestUrl = "https://github.com/o/r/releases/latest/download/latest.json";
    private const string ZipUrl = "https://github.com/o/r/releases/download/v0.1.30/ModManagerBeamNG.zip";
    private const string BlobUrl = "https://objects.githubusercontent.com/blob/zip";

    /// <summary>Архив как у настоящего выпуска: exe (начинается с MZ, больше мегабайта, не сжимается) и инструкция.</summary>
    private static byte[] ReleaseZip(bool withExe = true, bool mz = true)
    {
        var exe = new byte[1_200_000];
        new Random(5).NextBytes(exe);
        if (mz) { exe[0] = (byte)'M'; exe[1] = (byte)'Z'; }
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true))
        {
            if (withExe) using (var s = z.CreateEntry("BeamModManager.exe").Open()) s.Write(exe);
            using (var s = z.CreateEntry("Как запустить.txt").Open()) s.Write("x"u8);
        }
        return ms.ToArray();
    }

    private static string Sha(byte[] b) => Convert.ToHexString(SHA256.HashData(b)).ToLowerInvariant();

    private static string ManifestJson(string version, byte[] zip, string url = ZipUrl, string? sha = null) =>
        $$"""{"version":"{{version}}","url":"{{url}}","size":{{zip.Length}},"sha256":"{{sha ?? Sha(zip)}}","notes":"Что нового","published":"2026-10-15T12:00:00Z"}""";

    private sealed class Rig : IDisposable
    {
        public readonly string Dir = Path.Combine(Path.GetTempPath(), "bmm-upd-" + Guid.NewGuid().ToString("N"));
        public readonly FakeNet Net = new();
        public readonly string Exe;
        public DateTimeOffset Now = new(2026, 10, 15, 12, 0, 0, TimeSpan.Zero);
        public Version ArchiveVersion = new(0, 1, 30);
        public readonly AppUpdater U;

        public Rig(string current = "0.1.29")
        {
            Directory.CreateDirectory(Path.Combine(Dir, "app"));
            Exe = Path.Combine(Dir, "app", "BeamModManager.exe");
            File.WriteAllText(Exe, "старая версия");
            var http = new HttpClient(Net);
            U = new AppUpdater(new UpdateDownloader(http), new Uri(ManifestUrl), AppVersion.Parse(current)!, Exe, Path.Combine(Dir, "update"),
                _ => ArchiveVersion, () => Now);
        }

        public byte[] Publish(string version = "0.1.30", byte[]? zip = null, string? sha = null)
        {
            zip ??= ReleaseZip();
            Net.Routes[ManifestUrl] = () => Ok(System.Text.Encoding.UTF8.GetBytes(ManifestJson(version, zip, sha: sha)));
            Net.Routes[ZipUrl] = () => Redirect(BlobUrl);
            Net.Routes[BlobUrl] = () => Ok(zip);
            return zip;
        }

        public void Dispose() { try { Directory.Delete(Dir, true); } catch (IOException) { } }
    }

    public static void Register(TestRunner t)
    {
        t.Group("Самообновление: описание версии");

        t.Run("правильное описание разбирается; «v0.1.30» и «0.1.30.0» — одна версия", () =>
        {
            var zip = ReleaseZip();
            var (m, e) = UpdateManifest.Parse(ManifestJson("v0.1.30", zip));
            Assert.True(e is null, e ?? "");
            Assert.Eq(new Version(0, 1, 30, 0), m!.Version);
            Assert.Eq(zip.Length, (int)m.Size);
            Assert.Eq("0.1.30", AppVersion.Display(m.Version));
        });

        t.Run("настоящий latest.json со сборочной машины (0.1.30) разбирается", () =>
        {
            var dir = AppContext.BaseDirectory;
            while (dir != null && !Directory.Exists(Path.Combine(dir, "tests", "fixtures"))) dir = Path.GetDirectoryName(dir);
            var (m, e) = UpdateManifest.Parse(File.ReadAllText(Path.Combine(dir!, "tests", "fixtures", "update", "latest-0.1.30.json")));
            Assert.True(e is null, e ?? "");
            Assert.Eq("0.1.30", AppVersion.Display(m!.Version));
            Assert.Eq("https://github.com/SnikS1337/ModManagerBeamNG/releases/download/v0.1.30/ModManagerBeamNG.zip", m.Url.ToString());
            Assert.True(m.Size > UpdateManifest.MinSize && m.Published is not null, "размер и дата");
        });

        t.Run("недоверенное описание отклоняется: http, чужой сайт, нет суммы, неправдоподобный размер, мусор", () =>
        {
            var zip = ReleaseZip();
            Assert.True(UpdateManifest.Parse(ManifestJson("0.1.30", zip, url: "http://github.com/o/r/x.zip")).Error is not null, "http");
            Assert.True(UpdateManifest.Parse(ManifestJson("0.1.30", zip, url: "https://evil.example/x.zip")).Error is not null, "чужой сайт");
            Assert.True(UpdateManifest.Parse(ManifestJson("0.1.30", zip, url: "https://github.com.evil.example/x.zip")).Error is not null, "похожий адрес");
            Assert.True(UpdateManifest.Parse(ManifestJson("0.1.30", zip, sha: "abc")).Error is not null, "сумма");
            Assert.True(UpdateManifest.Parse(ManifestJson("0.1.30", new byte[100])).Error is not null, "крошечный файл");
            Assert.True(UpdateManifest.Parse(ManifestJson("версия", zip)).Error is not null, "версия");
            Assert.True(UpdateManifest.Parse("{ сломано").Error is not null, "мусор");
            Assert.True(UpdateManifest.Parse("[1]").Error is not null, "не объект");
        });

        t.Run("новее — только строго больше: та же и старая версия не предлагаются", () =>
        {
            Assert.True(AppVersion.IsNewer(new Version(0, 1, 30), new Version(0, 1, 29, 0)), "новее");
            Assert.False(AppVersion.IsNewer(new Version(0, 1, 30), new Version(0, 1, 30, 0)), "та же");
            Assert.False(AppVersion.IsNewer(new Version(0, 1, 9), new Version(0, 1, 30)), "старая (9 < 30 как числа)");
        });

        t.Group("Самообновление: сеть");

        t.Run("описание и архив через перенаправление на хранилище GitHub; сумма сходится", async () =>
        {
            using var r = new Rig();
            var zip = r.Publish();
            var d = new UpdateDownloader(new HttpClient(r.Net));
            var (m, e) = await d.GetManifestAsync(new Uri(ManifestUrl), CancellationToken.None);
            Assert.True(m is not null, e ?? "");
            var target = Path.Combine(r.Dir, "u.zip");
            var err = await d.DownloadAsync(m!, target, null, CancellationToken.None);
            Assert.True(err is null, err ?? "");
            Assert.True(File.ReadAllBytes(target).SequenceEqual(zip), "те же байты");
        });

        t.Run("перенаправление на чужой сайт не выполняется", async () =>
        {
            using var r = new Rig();
            r.Publish();
            r.Net.Routes[ZipUrl] = () => Redirect("https://evil.example/zip");
            var d = new UpdateDownloader(new HttpClient(r.Net));
            var (m, _) = await d.GetManifestAsync(new Uri(ManifestUrl), CancellationToken.None);
            var err = await d.DownloadAsync(m!, Path.Combine(r.Dir, "u.zip"), null, CancellationToken.None);
            Assert.True(err is not null, "отказ");
            Assert.False(r.Net.Calls.Any(c => c.Contains("evil")), "к чужому сайту не ходили");
        });

        t.Run("подменённый или оборванный файл не принимается и не остаётся на диске", async () =>
        {
            using var r = new Rig();
            var zip = r.Publish();
            var bad = (byte[])zip.Clone();
            bad[^1] ^= 0xFF;
            r.Net.Routes[BlobUrl] = () => Ok(bad); // тот же размер, другие байты
            var d = new UpdateDownloader(new HttpClient(r.Net));
            var (m, _) = await d.GetManifestAsync(new Uri(ManifestUrl), CancellationToken.None);
            var target = Path.Combine(r.Dir, "u.zip");
            Assert.True((await d.DownloadAsync(m!, target, null, CancellationToken.None))!.Contains("контрольная сумма"), "сумма");
            Assert.False(File.Exists(target) || File.Exists(target + ".part"), "ничего не осталось");
            r.Net.Routes[BlobUrl] = () => Ok(zip[..(zip.Length / 2)]);
            Assert.True(await d.DownloadAsync(m!, target, null, CancellationToken.None) is not null, "оборвано");
            r.Net.Routes[BlobUrl] = () => Ok(zip.Concat(new byte[10]).ToArray());
            Assert.True(await d.DownloadAsync(m!, target, null, CancellationToken.None) is not null, "больше обещанного");
            Assert.False(File.Exists(target + ".part"), "частичный файл убран");
        });

        t.Run("выпусков нет или сети нет: понятная причина, без исключения", async () =>
        {
            using var r = new Rig();
            var d = new UpdateDownloader(new HttpClient(r.Net));
            Assert.Eq("выпусков пока нет", (await d.GetManifestAsync(new Uri(ManifestUrl), CancellationToken.None)).Error);
            r.Net.Routes[ManifestUrl] = () => throw new HttpRequestException("нет сети");
            Assert.Eq("нет связи с GitHub", (await d.GetManifestAsync(new Uri(ManifestUrl), CancellationToken.None)).Error);
        });

        t.Group("Самообновление: установка");

        t.Run("из архива берётся только программа, версия сверяется с обещанной", () =>
        {
            using var r = new Rig();
            var zipPath = Path.Combine(r.Dir, "u.zip");
            File.WriteAllBytes(zipPath, ReleaseZip());
            var (exe, e) = UpdateInstaller.Extract(zipPath, Path.Combine(r.Dir, "st"), new Version(0, 1, 30), _ => new Version(0, 1, 30, 0));
            Assert.True(exe is not null && File.Exists(exe), e ?? "");
            Assert.True(UpdateInstaller.Extract(zipPath, Path.Combine(r.Dir, "st2"), new Version(0, 1, 30), _ => new Version(0, 1, 31)).Error!.Contains("версия"), "не та версия");
            Assert.False(File.Exists(Path.Combine(r.Dir, "st2", "BeamModManager.exe")), "чужая версия удалена");
            File.WriteAllBytes(zipPath, ReleaseZip(withExe: false));
            Assert.True(UpdateInstaller.Extract(zipPath, Path.Combine(r.Dir, "st3"), new Version(0, 1, 30), _ => new Version(0, 1, 30)).Error is not null, "нет программы");
            File.WriteAllBytes(zipPath, ReleaseZip(mz: false));
            Assert.True(UpdateInstaller.Extract(zipPath, Path.Combine(r.Dir, "st4"), new Version(0, 1, 30), _ => new Version(0, 1, 30)).Error is not null, "не программа Windows");
        });

        t.Run("подмена: старая → *.old, новая на место; старая копия с прошлого раза не мешает; откат и уборка", () =>
        {
            using var r = new Rig();
            var neu = Path.Combine(r.Dir, "new.exe");
            File.WriteAllText(neu, "новая");
            File.WriteAllText(r.Exe + ".old", "совсем старая");
            Assert.True(UpdateInstaller.Swap(r.Exe, neu) is null, "встала");
            Assert.Eq("новая", File.ReadAllText(r.Exe));
            Assert.Eq("старая версия", File.ReadAllText(r.Exe + ".old"));
            Assert.True(UpdateInstaller.Rollback(r.Exe), "откат");
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
            Assert.True(UpdateInstaller.Swap(r.Exe, Path.Combine(r.Dir, "нет.exe")) is not null, "нет файла — отказ");
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
            File.WriteAllText(r.Exe + ".old", "x");
            Directory.CreateDirectory(Path.Combine(r.Dir, "update", "0.1.30"));
            UpdateInstaller.Cleanup(r.Exe, Path.Combine(r.Dir, "update"));
            Assert.False(File.Exists(r.Exe + ".old") || Directory.Exists(Path.Combine(r.Dir, "update")), "убрано");
        });

        t.Group("Самообновление: порядок действий и защита от нажатий");

        t.Run("вышла новая версия: найдена; та же или старше — «последняя версия»", async () =>
        {
            using var r = new Rig();
            r.Publish("0.1.30");
            await r.U.CheckAsync(manual: false);
            Assert.Eq(UpdateStage.Available, r.U.State.Stage);
            using var same = new Rig("0.1.30");
            same.Publish("0.1.30");
            await same.U.CheckAsync(false);
            Assert.Eq(UpdateStage.UpToDate, same.U.State.Stage);
            using var newer = new Rig("0.2.0");
            newer.Publish("0.1.30");
            await newer.U.CheckAsync(false);
            Assert.Eq(UpdateStage.UpToDate, newer.U.State.Stage);
        });

        t.Run("много нажатий «Проверить» во время проверки — одно обращение к сети; повтор по кнопке не чаще раза в 10 с", async () =>
        {
            using var r = new Rig();
            r.Publish();
            r.Net.Gate = new TaskCompletionSource();
            var tasks = Enumerable.Range(0, 10).Select(_ => r.U.CheckAsync(manual: true)).ToList();
            Assert.True(r.U.Busy, "идёт проверка");
            r.Net.Gate.SetResult();
            await Task.WhenAll(tasks);
            Assert.Eq(1, r.Net.Calls.Count);
            await r.U.CheckAsync(manual: true);
            Assert.Eq(1, r.Net.Calls.Count);
            r.Now = r.Now.AddSeconds(11);
            await r.U.CheckAsync(manual: true);
            Assert.Eq(2, r.Net.Calls.Count);
        });

        t.Run("много нажатий «Обновить» — одна загрузка; в конце новая версия на месте старой", async () =>
        {
            using var r = new Rig();
            r.Publish();
            await r.U.CheckAsync(false);
            r.Net.Gate = new TaskCompletionSource();
            var tasks = Enumerable.Range(0, 5).Select(_ => r.U.InstallAsync()).ToList();
            tasks.Add(r.U.CheckAsync(manual: true)); // проверка во время загрузки тоже ничего нового не запускает
            r.Net.Gate.SetResult();
            await Task.WhenAll(tasks);
            Assert.Eq(1, r.Net.Calls.Count(c => c == BlobUrl));
            Assert.Eq(UpdateStage.ReadyToRestart, r.U.State.Stage);
            Assert.True(File.ReadAllBytes(r.Exe).Take(2).SequenceEqual("MZ"u8.ToArray()), "новая программа на месте");
            Assert.Eq("старая версия", File.ReadAllText(r.Exe + ".old"));
            await r.U.InstallAsync();
            Assert.Eq(1, r.Net.Calls.Count(c => c == BlobUrl));
        });

        t.Run("сбой загрузки: программа не тронута, обновление можно повторить, причина видна", async () =>
        {
            using var r = new Rig();
            r.Publish(sha: new string('0', 64));
            await r.U.CheckAsync(false);
            await r.U.InstallAsync();
            Assert.Eq(UpdateStage.Available, r.U.State.Stage);
            Assert.True(r.U.State.Error!.Contains("контрольная сумма"), r.U.State.Error);
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
            Assert.False(File.Exists(r.Exe + ".old"), "старую не трогали");
        });

        t.Run("в архиве другая версия, чем обещано: не ставится", async () =>
        {
            using var r = new Rig();
            r.Publish();
            r.ArchiveVersion = new Version(0, 1, 28);
            await r.U.CheckAsync(false);
            await r.U.InstallAsync();
            Assert.Eq(UpdateStage.Available, r.U.State.Stage);
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
        });

        t.Run("закрыли программу во время загрузки: отмена без следов", async () =>
        {
            using var r = new Rig();
            r.Publish();
            await r.U.CheckAsync(false);
            using var cts = new CancellationTokenSource();
            r.Net.Gate = new TaskCompletionSource();
            var task = r.U.InstallAsync(cts.Token);
            cts.Cancel();
            await task;
            Assert.Eq(UpdateStage.Available, r.U.State.Stage);
            Assert.Eq("загрузка отменена", r.U.State.Error);
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
        });
    
        t.Group("Самообновление: плашка в окне");

        static async Task Until(Func<bool> cond)
        {
            for (var i = 0; i < 200 && !cond(); i++) await Task.Delay(10);
        }

        t.Run("автопроверка молчит, если обновлений нет или нет выпусков", async () =>
        {
            using var r = new Rig("0.1.30");
            var b = new UpdateBanner(r.U, a => a(), () => { });
            await b.CheckOnStartAsync();
            await Task.Delay(50);
            Assert.False(b.Visible, "нет выпусков — тихо");
            r.Publish("0.1.30");
            r.Now = r.Now.AddMinutes(1);
            await b.CheckOnStartAsync();
            await Task.Delay(50);
            Assert.False(b.Visible, "последняя версия — тихо");
        });

        t.Run("одно «Обновить» → загрузка → один перезапуск, сколько бы раз ни нажали", async () =>
        {
            using var r = new Rig();
            r.Publish();
            var restarts = 0;
            var b = new UpdateBanner(r.U, a => { lock (r) a(); }, () => Interlocked.Increment(ref restarts));
            await b.CheckOnStartAsync();
            await Until(() => b.Visible);
            Assert.Eq("Вышла новая версия 0.1.30", b.Text);
            Assert.Eq("Обновить", b.ActionLabel);
            r.Net.Gate = new TaskCompletionSource();
            for (var i = 0; i < 5; i++) b.Act();
            Assert.True(b.ActionLabel is null && !b.CanDismiss, "во время загрузки кнопок нет");
            b.Dismiss();
            Assert.True(b.Visible, "«Позже» не прячет загрузку");
            r.Net.Gate.SetResult();
            await Until(() => restarts > 0);
            await Task.Delay(100);
            b.Act();
            Assert.Eq(1, restarts);
            Assert.Eq(1, r.Net.Calls.Count(c => c == BlobUrl));
            Assert.True(b.Text.StartsWith("Версия 0.1.30 готова"), b.Text);
        });

        t.Run("«Позже» прячет до следующего раза; «Проверить» по кнопке показывает результат", async () =>
        {
            using var r = new Rig("0.1.30");
            r.Publish("0.1.30");
            var b = new UpdateBanner(r.U, a => a(), () => { });
            await b.CheckNowAsync();
            await Until(() => b.Text.StartsWith("У вас"));
            Assert.Eq("У вас последняя версия 0.1.30", b.Text);
            b.Dismiss();
            Assert.False(b.Visible, "спрятано");
            Assert.False(new UpdateBanner(null, a => a(), () => { }).Supported, "без обновлятора — нечего показывать");
        });
    }
}
