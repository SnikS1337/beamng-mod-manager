using System.IO.Compression;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Net;
using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Text;

namespace BeamModManager.Core.Tests;

/// <summary>Второй проход ревизии: параллельные операции, резервные копии, разбор имён.</summary>
public static class Revision2Tests
{
    private static MemoryStream Mod(int size = 50)
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true))
        {
            using (var w = z.CreateEntry("vehicles/car/info.json").Open()) w.Write(new byte[20]);
            using (var w = z.CreateEntry("vehicles/car/car.jbeam").Open()) w.Write(new byte[size]);
        }
        ms.Position = 0;
        return ms;
    }

    private sealed class Recording : IPageFetcher
    {
        public readonly List<Uri> Requested = new();
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct) { Requested.Add(url); return Task.FromResult(new RawResponse(200, "<html><h1>x</h1></html>")); }
    }

    private sealed class Box : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "bmm-r2-" + Guid.NewGuid().ToString("N"));
        public readonly ModLayout Layout;
        public readonly ModInstaller Inst;
        private long _tick;
        public Box(int keep = 3)
        {
            Layout = new ModLayout(Path.Combine(Root, "mods"), Path.Combine(Root, "bmm", "disabled"), Path.Combine(Root, "bmm", "backups"));
            Inst = new ModInstaller(Layout, () => new DateTimeOffset(2026, 10, 13, 12, 0, 0, TimeSpan.Zero).AddMilliseconds(Interlocked.Increment(ref _tick)), keep);
        }
        public void Dispose() { try { Directory.Delete(Root, true); } catch { } }
        public int Copies(string name) => Scan(Layout.ModsDir, name) + Scan(Layout.DisabledDir, name);
        private static int Scan(string dir, string name) => Directory.Exists(dir) && File.Exists(Path.Combine(dir, name)) ? 1 : 0;
    }

    public static void Register(TestRunner t)
    {
        t.Group("Ревизия 2: установка и параллельность");

        t.Run("8 одновременных установок одного имени: успешна одна, файл один, без исключений", () =>
        {
            using var b = new Box();
            var results = new InstallResult[8];
            Parallel.For(0, 8, new ParallelOptions { MaxDegreeOfParallelism = 8 }, i => results[i] = b.Inst.Install(Mod(40 + i), "car.zip"));
            Assert.Eq(1, results.Count(r => r.Success));
            Assert.Eq(7, results.Count(r => r.Error == InstallError.AlreadyExists));
            Assert.Eq(1, b.Copies("car.zip"));
            Assert.Eq(0, Directory.GetFiles(b.Layout.ModsDir, ".*.part").Length);
        });

        t.Run("одновременные включение/отключение: мод всегда ровно в одном месте", () =>
        {
            using var b = new Box();
            b.Inst.Install(Mod(), "car.zip");
            Parallel.For(0, 200, new ParallelOptions { MaxDegreeOfParallelism = 8 }, i =>
            {
                if (i % 2 == 0) b.Inst.Disable("car.zip"); else b.Inst.Enable("car.zip");
            });
            Assert.Eq(1, b.Copies("car.zip"));
        });

        t.Run("одновременное обновление и удаление: файл и резервные копии целы", () =>
        {
            using var b = new Box();
            b.Inst.Install(Mod(), "car.zip");
            var tasks = Enumerable.Range(0, 20).Select(i => Task.Run(() =>
            {
                if (i % 3 == 0) b.Inst.Remove("car.zip");
                else b.Inst.Install(Mod(30 + i), "car.zip", new InstallOptions { ReplaceExisting = true });
            })).ToArray();
            Task.WaitAll(tasks);
            Assert.True(b.Copies("car.zip") <= 1, "не больше одной копии");
            Assert.True(b.Inst.Backups("car.zip").Count <= 3, "резервных не больше лимита");
            Assert.Eq(0, Directory.GetFiles(b.Layout.ModsDir, ".*.part").Length);
        });

        t.Run("резервные копии: лимит 0 или 1 не стирает единственную копию при удалении", () =>
        {
            foreach (var keep in new[] { 0, 1 })
            {
                using var b = new Box(keep);
                b.Inst.Install(Mod(), "car.zip");
                var r = b.Inst.Remove("car.zip");
                Assert.True(r.Success, "удалён");
                Assert.Eq(1, b.Inst.Backups("car.zip").Count);
            }
        });

        t.Run("восстановление при лимите 1 не теряет данные", () =>
        {
            using var b = new Box(1);
            b.Inst.Install(Mod(30), "car.zip");
            b.Inst.Install(Mod(60), "car.zip", new InstallOptions { ReplaceExisting = true });
            var r = b.Inst.RestoreLatestBackup("car.zip");
            Assert.True(r.Success, r.Message ?? "");
            Assert.Eq(1, b.Copies("car.zip"));
            Assert.Eq(1, b.Inst.Backups("car.zip").Count);
        });

        t.Run("восстановление отключённого мода кладёт файл в отключённые, а не создаёт дубликат", () =>
        {
            using var b = new Box();
            b.Inst.Install(Mod(30), "car.zip");
            b.Inst.Install(Mod(60), "car.zip", new InstallOptions { ReplaceExisting = true });
            b.Inst.Disable("car.zip");
            var r = b.Inst.RestoreLatestBackup("car.zip");
            Assert.True(r.Success, r.Message ?? "");
            Assert.Eq(1, b.Copies("car.zip"));
            Assert.True(File.Exists(Path.Combine(b.Layout.DisabledDir, "car.zip")), "осталось среди отключённых");
        });

        t.Run("недозаписанные временные файлы убираются, свежие не трогаются", () =>
        {
            using var b = new Box();
            Directory.CreateDirectory(b.Layout.ModsDir);
            var old = Path.Combine(b.Layout.ModsDir, ".aaa.part");
            var fresh = Path.Combine(b.Layout.ModsDir, ".bbb.part");
            File.WriteAllText(old, "x"); File.WriteAllText(fresh, "x");
            var clock = new DateTime(2026, 10, 13, 12, 0, 0, DateTimeKind.Utc);
            File.SetLastWriteTimeUtc(old, clock.AddHours(-5)); File.SetLastWriteTimeUtc(fresh, clock);
            Assert.Eq(1, b.Inst.CleanupStaleTemp(TimeSpan.FromHours(1)));
            Assert.False(File.Exists(old), "старый убран");
            Assert.True(File.Exists(fresh), "свежий на месте");
        });

        t.Run("список модов не включает .zipx и прочие «похожие» расширения", () =>
        {
            using var b = new Box();
            Directory.CreateDirectory(b.Layout.ModsDir);
            File.WriteAllText(Path.Combine(b.Layout.ModsDir, "a.zip"), "x");
            File.WriteAllText(Path.Combine(b.Layout.ModsDir, "b.zipx"), "x");
            File.WriteAllText(Path.Combine(b.Layout.ModsDir, "c.zip.bak"), "x");
            Assert.Eq(1, b.Inst.Scan().Count);
        });

        t.Run("имя файла: символы, недопустимые в Windows, отклоняются на любой платформе", () =>
        {
            foreach (var bad in new[] { "a:b.zip", "a*.zip", "a?.zip", "a\"b.zip", "a<b.zip", "a>b.zip", "a|b.zip", "car.zip:stream" })
                Assert.Null(ModInstaller.SafeFileName(bad));
            Assert.Eq("Машина №1 (v2).zip", ModInstaller.SafeFileName("Машина №1 (v2).zip"));
        });

        t.Run("папка рядом с корнем диска не падает", () =>
        {
            var l = ModLayout.Beside("/");
            Assert.True(l.DisabledDir.Contains("BeamModManager"), "путь построен");
        });

        t.Group("Ревизия 2: папка игры");

        string Tmp() { var d = Path.Combine(Path.GetTempPath(), "bmm-gf-" + Guid.NewGuid().ToString("N")); Directory.CreateDirectory(d); return d; }

        t.Run("новая раскладка игры (0.37+): BeamNG.drive\\current\\mods находится из любой из трёх папок", () =>
        {
            var root = Tmp();
            var mods = Path.Combine(root, "BeamNG", "BeamNG.drive", "current", "mods");
            Directory.CreateDirectory(mods);
            Assert.Eq(mods, GameFolders.Resolve(Path.Combine(root, "BeamNG", "BeamNG.drive")).ModsDir);
            Assert.Eq(mods, GameFolders.Resolve(Path.Combine(root, "BeamNG", "BeamNG.drive", "current")).ModsDir);
            Assert.Eq(mods, GameFolders.Resolve(Path.Combine(root, "BeamNG")).ModsDir);
        });

        t.Run("путь в кавычках (как копирует проводник), с переменной окружения; относительный путь отклоняется", () =>
        {
            var d = Tmp(); var mods = Path.Combine(d, "mods"); Directory.CreateDirectory(mods);
            Assert.True(GameFolders.Resolve("\"" + mods + "\"").Ok, "кавычки");
            Environment.SetEnvironmentVariable("BMM_TEST_DIR", d);
            Assert.True(GameFolders.Resolve(Path.Combine("%BMM_TEST_DIR%", "mods")).Ok, "переменная");
            Assert.False(GameFolders.Resolve("mods").Ok, "относительный путь");
            Assert.Eq(FolderProblem.Empty, GameFolders.Resolve("\"\"").Problem);
        });

        t.Run("служебные папки в папке менеджера: у разных mods разные, внутри mods ничего нет", () =>
        {
            var a = ModLayout.InAppFolder("/g/a/mods", "/app");
            var b = ModLayout.InAppFolder("/g/b/mods", "/app");
            Assert.True(a.DisabledDir != b.DisabledDir, "разные папки");
            Assert.True(a.DisabledDir.StartsWith("/app") && a.BackupDir.StartsWith("/app"), "в папке менеджера");
            Assert.Eq(a.DisabledDir, ModLayout.InAppFolder("/g/a/mods/", "/app").DisabledDir);
        });

        t.Group("Ревизия 2: адаптеры");

        t.Run("карточка по адресу чужого сайта или не http: сайт не опрашивается, ответ «страницы нет»", async () =>
        {
            var rec = new Recording();
            var src = new ModlandSource(new PoliteFetcher(rec, new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero }));
            foreach (var u in new[] { "https://evil.example/x", "https://modland.net.evil.example/x", "http://127.0.0.1/admin", "file:///etc/passwd", "ftp://www.modland.net/x" })
                Assert.Null(await src.GetListingAsync(new Uri(u)));
            Assert.Eq(0, rec.Requested.Count);
            await src.GetListingAsync(new Uri("https://www.modland.net/some-mod"));
            await src.GetListingAsync(new Uri("https://modland.net/other-mod"));
            Assert.Eq(2, rec.Requested.Count);
        });

        t.Run("поисковый запрос из одних точек не превращается в переход по адресу", async () =>
        {
            var rec = new Recording();
            var src = new ModlandSource(new PoliteFetcher(rec, new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero }));
            Assert.Eq(0, (await src.SearchAsync("..")).Count);
            Assert.Eq(0, rec.Requested.Count);
        });

        t.Group("Ревизия 2: архивы");

        t.Run("архив с десятками тысяч файлов прямо в vehicles/: чтение не квадратичное", () =>
        {
            var ms = new MemoryStream();
            using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true))
                for (var i = 0; i < 40_000; i++) z.CreateEntry($"vehicles/f{i}", CompressionLevel.NoCompression);
            ms.Position = 0;
            var sw = System.Diagnostics.Stopwatch.StartNew();
            BeamModManager.Core.Packs.PackContentReader.Read(ms);
            Assert.True(sw.Elapsed < TimeSpan.FromSeconds(5), $"заняло {sw.Elapsed.TotalSeconds:0.0} с");
        });

        t.Run("пути «./vehicles/…» понимаются как обычные; «... » и «.. » в пути — небезопасно", () =>
        {
            Stream One(string name) { var m = new MemoryStream(); using (var z = new ZipArchive(m, ZipArchiveMode.Create, true)) { using var w = z.CreateEntry(name).Open(); w.WriteByte(1); } m.Position = 0; return m; }
            var ok = BeamModManager.Core.Safety.ArchiveInspector.Inspect(One("./vehicles/car/a.jbeam"));
            Assert.False(ok.HasWarnings, "«./» не считается лишней папкой");
            Assert.True(BeamModManager.Core.Safety.ArchiveInspector.Inspect(One("vehicles/.. /x")).IsBlocked, "«.. »");
            Assert.True(BeamModManager.Core.Safety.ArchiveInspector.Inspect(One("vehicles/.../x")).IsBlocked, "«...»");
            var c = BeamModManager.Core.Packs.PackContentReader.Read(One("./vehicles/car/a.jbeam"));
            Assert.True(c.Vehicles.Contains("car"), "машина найдена");
        });

        t.Group("Ревизия 2: разбор имён");

        t.Run("имя файла: версия через точку, «V8» — двигатель, метка сайта", () =>
        {
            var a = BindingSuggester.FromFileName("BMW_M3_E30_v1.2.zip");
            Assert.Eq("BMW M3 E30", a.Name); Assert.Eq("1.2", a.Version);
            var b = BindingSuggester.FromFileName("Ford_V8.zip");
            Assert.Eq("Ford V8", b.Name); Assert.Null(b.Version);
            var c = BindingSuggester.FromFileName("stmods_car.zip");
            Assert.Eq("car", c.Name);
            var d = BindingSuggester.FromFileName("xstmods.zip");
            Assert.Eq("xstmods", d.Name);
        });

        t.Run("заголовок: «(Premium)» и «(VIP)» в скобках — комплектация, а не метка платного", () =>
        {
            Assert.False(TitleParser.HasPaidMarker("Car Pro (Premium) 3.1"));
            Assert.False(TitleParser.HasPaidMarker("Limo (VIP)"));
            Assert.True(TitleParser.HasPaidMarker("Car [Premium]"));
            Assert.True(TitleParser.HasPaidMarker("Car (PAID)"));
        });
    }
}
