using System.IO.Compression;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
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
