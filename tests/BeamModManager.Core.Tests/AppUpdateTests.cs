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

        public Rig(string current = "0.1.29", string? skipped = null)
        {
            Directory.CreateDirectory(Path.Combine(Dir, "app"));
            Exe = Path.Combine(Dir, "app", "BeamModManager.exe");
            File.WriteAllText(Exe, "старая версия");
            var http = new HttpClient(Net);
            U = new AppUpdater(new UpdateDownloader(http), new Uri(ManifestUrl), AppVersion.Parse(current)!, Exe, Path.Combine(Dir, "update"),
                _ => ArchiveVersion, () => Now, AppVersion.Parse(skipped));
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
        static async Task Until(Func<bool> cond)
        {
            for (var i = 0; i < 500 && !cond(); i++) await Task.Delay(10); // до 5 с: под нагрузкой пул потоков отвечает не сразу
        }

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

        t.Run("версии с 0.2: «0.2» и «0.2.0» одна версия, показываются без хвостового нуля, новее любой 0.1.x", () =>
        {
            Assert.Eq(AppVersion.Parse("0.2"), AppVersion.Parse("v0.2.0.0"));
            Assert.Eq("0.2", AppVersion.Display(AppVersion.Parse("0.2")!));
            Assert.Eq("0.2.1", AppVersion.Display(AppVersion.Parse("0.2.1")!));
            Assert.Eq("1.0", AppVersion.Display(AppVersion.Parse("1.0.0")!));
            Assert.Eq("0.2.0.4", AppVersion.Display(AppVersion.Parse("0.2.0.4")!));
            Assert.True(AppVersion.IsNewer(AppVersion.Parse("0.2")!, AppVersion.Parse("0.1.129")!), "мост: 0.2 новее 0.1.129");
            Assert.False(AppVersion.IsNewer(AppVersion.Parse("0.2.0")!, AppVersion.Parse("0.2")!), "та же версия");
            var (m, e) = UpdateManifest.Parse(ManifestJson("0.2", ReleaseZip()));
            Assert.True(e is null && m!.Version == new Version(0, 2, 0, 0), e ?? "");
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

        t.Run("подмена идёт через копию рядом с программой: после неё нет ни «*.new», ни частичных файлов", () =>
        {
            using var r = new Rig();
            var neu = Path.Combine(r.Dir, "elsewhere", "BeamModManager.exe"); // папка обновления — в другом месте (на другом диске)
            Directory.CreateDirectory(Path.GetDirectoryName(neu)!);
            File.WriteAllText(neu, "новая");
            var moves = new List<(string, string)>();
            Assert.True(UpdateInstaller.Swap(r.Exe, neu, (a, b) => { moves.Add((a, b)); File.Move(a, b); }) is null, "встала");
            Assert.Eq("новая", File.ReadAllText(r.Exe));
            Assert.False(File.Exists(r.Exe + UpdateInstaller.NewSuffix), "копия стала программой");
            // переименования только внутри папки программы (на одном диске — мгновенно); новая версия копируется заранее
            Assert.Eq(2, moves.Count);
            Assert.True(moves.All(m => Path.GetDirectoryName(m.Item1) == Path.GetDirectoryName(r.Exe) && Path.GetDirectoryName(m.Item2) == Path.GetDirectoryName(r.Exe)), "только рядом с exe");
            Assert.Eq((r.Exe, r.Exe + UpdateInstaller.OldSuffix), moves[0]);
            Assert.Eq((r.Exe + UpdateInstaller.NewSuffix, r.Exe), moves[1]);
            Assert.True(File.Exists(neu), "подготовленный файл не тронут (его уберёт уборка)");
        });

        t.Run("копию не положить рядом с программой: текущая не тронута, причина понятна", () =>
        {
            using var r = new Rig();
            var neu = Path.Combine(r.Dir, "new.exe");
            File.WriteAllText(neu, "новая");
            Directory.CreateDirectory(r.Exe + UpdateInstaller.NewSuffix); // место занято папкой — записать нельзя
            var err = UpdateInstaller.Swap(r.Exe, neu);
            Assert.True(err is not null, "отказ");
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
            Assert.False(File.Exists(r.Exe + UpdateInstaller.OldSuffix), "текущую не отодвигали");
        });

        t.Run("оставшийся от оборванной попытки «*.new» не мешает", () =>
        {
            using var r = new Rig();
            File.WriteAllText(r.Exe + UpdateInstaller.NewSuffix, "обрывок");
            var neu = Path.Combine(r.Dir, "new.exe");
            File.WriteAllText(neu, "новая целиком");
            Assert.True(UpdateInstaller.Swap(r.Exe, neu) is null, "встала");
            Assert.Eq("новая целиком", File.ReadAllText(r.Exe));
        });

        t.Run("новая не встала на место: прежняя возвращается, «*.new» убран", () =>
        {
            using var r = new Rig();
            var neu = Path.Combine(r.Dir, "new.exe");
            File.WriteAllText(neu, "новая");
            var err = UpdateInstaller.Swap(r.Exe, neu, (a, b) =>
            {
                if (a.EndsWith(UpdateInstaller.NewSuffix)) throw new IOException("файл занят");
                File.Move(a, b);
            });
            Assert.True(err!.Contains("не встала"), err);
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
            Assert.False(File.Exists(r.Exe + UpdateInstaller.NewSuffix), "копия убрана");
        });

        t.Run("текущую не отодвинуть (нет прав): ничего не тронуто, совет перенести папку", () =>
        {
            using var r = new Rig();
            var neu = Path.Combine(r.Dir, "new.exe");
            File.WriteAllText(neu, "новая");
            var err = UpdateInstaller.Swap(r.Exe, neu, (a, b) => throw new UnauthorizedAccessException());
            Assert.True(err!.Contains("нет прав"), err);
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
            Assert.False(File.Exists(r.Exe + UpdateInstaller.NewSuffix) || File.Exists(r.Exe + UpdateInstaller.OldSuffix), "следов нет");
        });

        t.Run("и новая не встала, и прежняя не вернулась: сказано, что переименовать", () =>
        {
            using var r = new Rig();
            var neu = Path.Combine(r.Dir, "new.exe");
            File.WriteAllText(neu, "новая");
            var first = true;
            var err = UpdateInstaller.Swap(r.Exe, neu, (a, b) =>
            {
                if (first) { first = false; File.Move(a, b); return; }
                throw new IOException("занято");
            });
            Assert.True(err!.Contains("BeamModManager.exe.old") && err.Contains("обратно"), err);
            Assert.Eq("старая версия", File.ReadAllText(r.Exe + UpdateInstaller.OldSuffix));
        });

        t.Run("программы нет на месте: подмена отказывается и ничего не создаёт", () =>
        {
            using var r = new Rig();
            var neu = Path.Combine(r.Dir, "new.exe");
            File.WriteAllText(neu, "новая");
            File.Delete(r.Exe);
            Assert.True(UpdateInstaller.Swap(r.Exe, neu) is not null, "отказ");
            Assert.False(File.Exists(r.Exe) || File.Exists(r.Exe + UpdateInstaller.NewSuffix), "ничего не появилось");
        });

        t.Run("папка программы с русскими буквами, пробелами и скобками: подмена и откат работают", () =>
        {
            using var r = new Rig();
            var dir = Path.Combine(r.Dir, "Мои программы (2)", "Mod Manager — BeamNG");
            Directory.CreateDirectory(dir);
            var exe = Path.Combine(dir, "BeamModManager.exe");
            File.WriteAllText(exe, "старая");
            var neu = Path.Combine(r.Dir, "обновление", "BeamModManager.exe");
            Directory.CreateDirectory(Path.GetDirectoryName(neu)!);
            File.WriteAllText(neu, "новая");
            Assert.True(UpdateInstaller.Swap(exe, neu) is null, "встала");
            Assert.Eq("новая", File.ReadAllText(exe));
            Assert.True(UpdateInstaller.Rollback(exe), "откат");
            Assert.Eq("старая", File.ReadAllText(exe));
        });

        t.Run("подготовка и подмена порознь: до подмены программа не тронута; без подготовки подменять нечего", async () =>
        {
            using var r = new Rig();
            Assert.Eq("обновление не подготовлено", r.U.Commit());
            r.Publish();
            await r.U.CheckAsync(false);
            Assert.Eq("обновление не подготовлено", r.U.Commit());
            await r.U.InstallAsync();
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
            Assert.False(File.Exists(r.Exe + UpdateInstaller.OldSuffix), "текущую ещё не отодвигали");
            File.Delete(r.Exe + UpdateInstaller.NewSuffix); // пропала (антивирус унёс)
            Assert.Eq("подготовленное обновление пропало", r.U.Commit());
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
        });

        t.Run("подготовка: нет прав на папку программы — ошибка видна сразу на плашке, а не при выходе", async () =>
        {
            using var r = new Rig();
            r.Publish();
            Directory.CreateDirectory(r.Exe + UpdateInstaller.NewSuffix); // место для копии занято — как запрет записи
            await r.U.CheckAsync(false);
            await r.U.InstallAsync();
            Assert.Eq(UpdateStage.Available, r.U.State.Stage);
            Assert.True(r.U.State.Error is not null, "причина видна");
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
        });

        t.Run("подмена не удалась: прежняя версия на месте, «*.new» убран", () =>
        {
            using var r = new Rig();
            var neu = Path.Combine(r.Dir, "n.exe");
            File.WriteAllText(neu, "новая");
            Assert.True(UpdateInstaller.Stage(r.Exe, neu) is null, "подготовлено");
            var err = UpdateInstaller.Commit(r.Exe, (a, b) => { if (a.EndsWith(UpdateInstaller.NewSuffix)) throw new IOException("занято"); File.Move(a, b); });
            Assert.True(err!.Contains("не встала"), err);
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
            Assert.False(File.Exists(r.Exe + UpdateInstaller.NewSuffix), "убран");
        });

        t.Group("Самообновление: новая версия не запустилась");

        t.Run("новая версия падает при запуске и возвращает прежнюю сама: себя в «*.bad», прежнюю на место", () =>
        {
            using var r = new Rig();
            var neu = Path.Combine(r.Dir, "new.exe");
            File.WriteAllText(neu, "новая");
            Assert.True(UpdateInstaller.Swap(r.Exe, neu) is null, "встала");
            Assert.True(UpdateInstaller.RollbackFromRunning(r.Exe), "вернула");
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
            Assert.Eq("новая", File.ReadAllText(r.Exe + UpdateInstaller.BadSuffix));
            Assert.False(File.Exists(r.Exe + UpdateInstaller.OldSuffix), "прежняя уже на месте");
            UpdateInstaller.Cleanup(r.Exe, Path.Combine(r.Dir, "update"));
            Assert.False(File.Exists(r.Exe + UpdateInstaller.BadSuffix), "неудачная убрана при следующем запуске");
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
        });

        t.Run("возвращать нечего (обычный запуск, без «*.old»): ничего не трогается", () =>
        {
            using var r = new Rig();
            Assert.False(UpdateInstaller.RollbackFromRunning(r.Exe), "нечего");
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
            Assert.False(File.Exists(r.Exe + UpdateInstaller.BadSuffix), "себя не переименовала");
            Assert.False(UpdateInstaller.Rollback(r.Exe), "и прежнему процессу нечего");
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
        });

        t.Run("новая умерла сама (прежний процесс возвращает себя): новый файл заменяется прежним", () =>
        {
            using var r = new Rig();
            var neu = Path.Combine(r.Dir, "new.exe");
            File.WriteAllText(neu, "новая");
            UpdateInstaller.Swap(r.Exe, neu);
            File.WriteAllText(r.Exe + UpdateInstaller.BadSuffix, "старый мусор");
            Assert.True(UpdateInstaller.Rollback(r.Exe), "вернул");
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
            Assert.False(File.Exists(r.Exe + UpdateInstaller.OldSuffix), "старой копии нет");
        });

        t.Run("уборка: «*.old», «*.new», «*.bad» и папка обновления; без них — ничего не ломает", () =>
        {
            using var r = new Rig();
            foreach (var suf in new[] { UpdateInstaller.OldSuffix, UpdateInstaller.NewSuffix, UpdateInstaller.BadSuffix })
                File.WriteAllText(r.Exe + suf, "x");
            Directory.CreateDirectory(Path.Combine(r.Dir, "update", "0.1.30", "exe"));
            UpdateInstaller.Cleanup(r.Exe, Path.Combine(r.Dir, "update"));
            Assert.Eq(1, Directory.GetFiles(Path.GetDirectoryName(r.Exe)!).Length);
            Assert.False(Directory.Exists(Path.Combine(r.Dir, "update")), "папки нет");
            UpdateInstaller.Cleanup(r.Exe, Path.Combine(r.Dir, "update")); // второй раз — тихо
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
        });

        t.Run("не запустившаяся версия запоминается; мусор в файле и его отсутствие — ничего не пропускаем", () =>
        {
            using var r = new Rig();
            var path = Path.Combine(r.Dir, "cfg", "update-skip.txt");
            Assert.True(UpdateInstaller.ReadSkipped(path) is null, "файла нет");
            UpdateInstaller.WriteSkipped(path, new Version(0, 1, 58));
            Assert.Eq(new Version(0, 1, 58, 0), UpdateInstaller.ReadSkipped(path));
            File.WriteAllText(path, "сломано");
            Assert.True(UpdateInstaller.ReadSkipped(path) is null, "мусор");
        });

        t.Run("не запустившуюся версию больше не предлагаем; следующую — предлагаем", async () =>
        {
            Rig Make(string skipped) => new Rig("0.1.57", skipped);
            using (var r = Make("0.1.58"))
            {
                r.Publish("0.1.58");
                await r.U.CheckAsync(manual: false);
                Assert.Eq(UpdateStage.UpToDate, r.U.State.Stage);
                Assert.True(r.U.State.Error!.Contains("0.1.58"), r.U.State.Error);
                await r.U.InstallAsync();
                Assert.False(File.Exists(r.Exe + UpdateInstaller.OldSuffix), "и не ставится");
            }
            using (var r = Make("0.1.58"))
            {
                r.Publish("0.1.59");
                r.ArchiveVersion = new Version(0, 1, 59);
                await r.U.CheckAsync(manual: false);
                Assert.Eq(UpdateStage.Available, r.U.State.Stage);
            }
        });

        t.Run("плашка: автопроверка о пропущенной версии молчит, по кнопке объясняет", async () =>
        {
            using var r = new Rig("0.1.57", "0.1.58");
            r.Publish("0.1.58");
            var b = new UpdateBanner(r.U, a => a(), () => { });
            await b.CheckOnStartAsync();
            await Task.Delay(50);
            Assert.False(b.Visible, "молчит");
            r.Now = r.Now.AddMinutes(1);
            await b.CheckNowAsync();
            await Until(() => b.Text.StartsWith("У вас"));
            Assert.Eq("У вас версия 0.1.57: версия 0.1.58 не запустилась у вас, ждём следующую", b.Text);
            Assert.True(b.ActionLabel is null, "кнопки «Обновить» нет");
        });

        t.Group("Самообновление: выпуски из папки (проверка на сборочной машине)");

        t.Run("файлы отдаются по последней части адреса; чужое имя и выход из папки — 404", async () =>
        {
            using var r = new Rig();
            var feed = Path.Combine(r.Dir, "feed");
            Directory.CreateDirectory(feed);
            File.WriteAllText(Path.Combine(feed, "latest.json"), "{}");
            File.WriteAllText(Path.Combine(r.Dir, "secret.txt"), "нельзя");
            using var http = new HttpClient(new LocalFeedHandler(feed));
            var ok = await http.GetAsync(ManifestUrl);
            Assert.Eq(HttpStatusCode.OK, ok.StatusCode);
            Assert.Eq("{}", await ok.Content.ReadAsStringAsync());
            Assert.Eq(HttpStatusCode.NotFound, (await http.GetAsync(ZipUrl)).StatusCode);
            Assert.Eq(HttpStatusCode.NotFound, (await http.GetAsync("https://github.com/o/r/..%2Fsecret.txt")).StatusCode);
            Assert.Eq(HttpStatusCode.NotFound, (await http.GetAsync("https://github.com/")).StatusCode);
        });

        t.Run("полный круг через папку: найдено, скачано, проверено, подменено — как с GitHub", async () =>
        {
            using var r = new Rig();
            var feed = Path.Combine(r.Dir, "feed");
            Directory.CreateDirectory(feed);
            var zip = ReleaseZip();
            File.WriteAllBytes(Path.Combine(feed, "ModManagerBeamNG.zip"), zip);
            File.WriteAllText(Path.Combine(feed, "latest.json"), ManifestJson("0.1.30", zip));
            var u = new AppUpdater(new UpdateDownloader(new HttpClient(new LocalFeedHandler(feed))), new Uri(ManifestUrl),
                new Version(0, 1, 29), r.Exe, Path.Combine(r.Dir, "update"), _ => new Version(0, 1, 30));
            await u.CheckAsync(false);
            Assert.Eq(UpdateStage.Available, u.State.Stage);
            await u.InstallAsync();
            Assert.Eq(UpdateStage.ReadyToRestart, u.State.Stage);
            Assert.True(u.Commit() is null, "встала");
            Assert.True(File.ReadAllBytes(r.Exe).Take(2).SequenceEqual("MZ"u8.ToArray()), "новая на месте");
            Assert.False(File.Exists(r.Exe + UpdateInstaller.NewSuffix), "без «*.new»");
            // подменённый архив в папке не проходит проверку суммы так же, как скачанный
            var bad = (byte[])zip.Clone();
            bad[^1] ^= 1;
            File.WriteAllBytes(Path.Combine(feed, "ModManagerBeamNG.zip"), bad);
            using var r2 = new Rig();
            var u2 = new AppUpdater(new UpdateDownloader(new HttpClient(new LocalFeedHandler(feed))), new Uri(ManifestUrl),
                new Version(0, 1, 29), r2.Exe, Path.Combine(r2.Dir, "update"), _ => new Version(0, 1, 30));
            await u2.CheckAsync(false);
            await u2.InstallAsync();
            Assert.True(u2.State.Error!.Contains("контрольная сумма"), u2.State.Error);
            Assert.Eq("старая версия", File.ReadAllText(r2.Exe));
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
            // до выхода работающая программа не тронута: новая лежит рядом
            Assert.Eq("старая версия", File.ReadAllText(r.Exe));
            Assert.True(File.ReadAllBytes(r.Exe + UpdateInstaller.NewSuffix).Take(2).SequenceEqual("MZ"u8.ToArray()), "новая рядом");
            Assert.True(r.U.Commit() is null, "подмена на выходе");
            Assert.True(File.ReadAllBytes(r.Exe).Take(2).SequenceEqual("MZ"u8.ToArray()), "новая программа на месте");
            Assert.Eq("старая версия", File.ReadAllText(r.Exe + ".old"));
            Assert.False(File.Exists(r.Exe + UpdateInstaller.NewSuffix), "«*.new» стал программой");
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
            var b = new UpdateBanner(r.U, a => { lock (r) a(); }, () => Interlocked.Increment(ref restarts), delay: _ => Task.CompletedTask);
            await b.CheckOnStartAsync();
            await Until(() => b.Visible);
            Assert.Eq("Вышла новая версия 0.1.30", b.Text);
            Assert.Eq("Обновить", b.ActionLabel);
            r.Net.Gate = new TaskCompletionSource();
            for (var i = 0; i < 5; i++) b.Act();
            Assert.True(b.ActionLabel is null && !b.CanDismiss, "во время загрузки кнопок нет");
            Assert.True(b.CanCancel && b.Phase == BannerPhase.Downloading && b.Progress is not null, "загрузка: полоса и «Отменить»");
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
    
        t.Run("плашка: «Отменить» во время загрузки — снова «Вышла новая версия», без ошибки; потом можно обновиться", async () =>
        {
            using var r = new Rig();
            r.Publish();
            var restarts = 0;
            var b = new UpdateBanner(r.U, a => { lock (r) a(); }, () => Interlocked.Increment(ref restarts), delay: _ => Task.CompletedTask);
            await b.CheckOnStartAsync();
            await Until(() => b.Visible);
            r.Net.Gate = new TaskCompletionSource();
            b.Act();
            await Until(() => b.CanCancel);
            b.Cancel();
            r.Net.Gate.SetResult();
            await Until(() => b.Phase == BannerPhase.Message && b.ActionLabel == "Обновить");
            Assert.Eq("Вышла новая версия 0.1.30", b.Text);
            Assert.True(b.Progress is null && !b.CanCancel, "полосы нет");
            Assert.Eq(0, restarts);
            r.Net.Gate = null;
            b.Act();
            await Until(() => restarts > 0);
            Assert.Eq(1, restarts);
        });

        t.Run("плашка: сколько скачано и сколько осталось; «осталось» округляется и не мелькает", () =>
        {
            Assert.Eq("", UpdateBanner.Left(double.NaN));
            Assert.Eq("несколько секунд", UpdateBanner.Left(3));
            Assert.Eq("~10 с", UpdateBanner.Left(11));
            Assert.Eq("~55 с", UpdateBanner.Left(56));
            Assert.Eq("~1 мин", UpdateBanner.Left(58));
            Assert.Eq("~3 мин", UpdateBanner.Left(170));
            var now = new DateTimeOffset(2026, 10, 18, 12, 0, 0, TimeSpan.Zero);
            var fake = new FakeUpdater { Stage = UpdateStage.Downloading, Size = 60L * 1048576, P = 0.1 };
            var b = new UpdateBanner(fake, a => a(), () => { }, () => now);
            Assert.Eq("6 из 60 МБ", b.Detail);
            now = now.AddSeconds(2); fake.Set(0.3); // 12 МБ за 2 с — 6 МБ/с, осталось 42 МБ ≈ 7 с
            Assert.Eq("18 из 60 МБ, ~5 с", b.Detail);
            Assert.True(b.Phase == BannerPhase.Downloading && Math.Abs(b.Progress!.Value - 0.3) < 1e-9, "полоса");
            fake.Set(1);
            Assert.True(b.Phase == BannerPhase.Verifying && b.Detail is null && !b.CanCancel, "проверка: без «Отменить» и цифр");
            Assert.True(b.Text.StartsWith("Проверяем и распаковываем версию 0.1.30"), b.Text);
            fake.Stage = UpdateStage.Available; fake.Error = "контрольная сумма не совпала"; fake.Set(1);
            Assert.True(b.Phase == BannerPhase.Failed && b.Progress == 1 && b.ActionLabel == "Повторить", "сбой: полоса стоит, «Повторить»");
        });

        t.Run("плашка: готово — отсчёт 3, 2, 1 и один перезапуск; «Перезапустить сейчас» не ждёт", async () =>
        {
            var fake = new FakeUpdater { Stage = UpdateStage.ReadyToRestart, Size = 1 };
            var texts = new List<string>();
            var restarts = 0;
            var gate = new TaskCompletionSource();
            var b = new UpdateBanner(fake, a => { lock (texts) a(); }, () => Interlocked.Increment(ref restarts), delay: _ => gate.Task);
            b.PropertyChanged += (_, e) => { if (e.PropertyName == nameof(UpdateBanner.Text)) texts.Add(b.Text); };
            await Until(() => b.Text.EndsWith("3 с"));
            Assert.Eq("Версия 0.1.30 готова. Перезапуск через 3 с", b.Text);
            Assert.True(b.Phase == BannerPhase.Ready && b.ActionLabel == "Перезапустить сейчас" && b.Progress is null, "галочка вместо полосы");
            b.Act();
            Assert.Eq(1, restarts);
            Assert.Eq("Версия 0.1.30 готова. Перезапускаем…", b.Text);
            gate.SetResult();
            await Task.Delay(100);
            Assert.Eq(1, restarts);
        });
    }

    /// <summary>Подделка обновлятора для плашки: состояние задаётся напрямую.</summary>
    private sealed class FakeUpdater : IAppUpdater
    {
        public UpdateStage Stage = UpdateStage.Idle;
        public long Size;
        public double P;
        public string? Error;
        public Version Current => new(0, 1, 29);
        public UpdateState State => new(Stage, new UpdateManifest(new Version(0, 1, 30, 0), new Uri("https://github.com/o/r/releases/download/v0.1.30/a.zip"), Size, new string('0', 64), "", null), P, Error);
        public event Action? Changed;
        public void Set(double p) { P = p; Changed?.Invoke(); }
        public Task CheckAsync(bool manual, CancellationToken ct = default) => Task.CompletedTask;
        public Task InstallAsync(CancellationToken ct = default) => Task.CompletedTask;
        public void CancelInstall() { }
    }
}
