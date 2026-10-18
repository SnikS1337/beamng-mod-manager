using System.IO.Compression;
using BeamModManager.Core.Install;
using BeamModManager.Core.Packs;

namespace BeamModManager.Core.Tests;

/// <summary>Разбор скачанного или перетащенного архива (IncomingAnalyzer): что в нём и чем станет каждый мод.</summary>
public static class IncomingTests
{
    private sealed class Dir : IDisposable
    {
        public readonly string Path = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "bmm-in-" + Guid.NewGuid().ToString("N"));
        public Dir() => Directory.CreateDirectory(Path);
        public string this[string name] => System.IO.Path.Combine(Path, name);
        public void Dispose() { try { Directory.Delete(Path, true); } catch (IOException) { } }
    }

    private static byte[] ZipBytes(params string[] entries)
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, leaveOpen: true))
            foreach (var e in entries)
            {
                var entry = z.CreateEntry(e);
                if (e.EndsWith('/')) continue;
                using var w = new StreamWriter(entry.Open());
                w.Write("x " + e);
            }
        return ms.ToArray();
    }

    private static string Zip(Dir d, string name, params string[] entries)
    {
        var p = d[name];
        File.WriteAllBytes(p, ZipBytes(entries));
        return p;
    }

    /// <summary>Архив, внутри которого zip-файлы (пак): имя → записи внутреннего.</summary>
    private static string ZipOfZips(Dir d, string name, params (string Name, string[] Entries)[] inner)
    {
        var p = d[name];
        using var z = ZipFile.Open(p, ZipArchiveMode.Create);
        foreach (var (n, es) in inner)
        {
            var e = z.CreateEntry(n);
            using var s = e.Open();
            s.Write(ZipBytes(es));
        }
        return p;
    }

    /// <summary>«Распаковщик RAR»: кладёт в папку заготовленные файлы (путь → содержимое; zip — байтами).</summary>
    private sealed class FakeExtractor(Dictionary<string, byte[]> files, string? error = null) : IArchiveExtractor
    {
        public string? Extract(string archivePath, string targetDir, long maxBytes, CancellationToken ct)
        {
            if (error is not null) return error;
            foreach (var (rel, data) in files)
            {
                var p = System.IO.Path.Combine(targetDir, rel);
                Directory.CreateDirectory(System.IO.Path.GetDirectoryName(p)!);
                File.WriteAllBytes(p, data);
            }
            return null;
        }
    }

    private static InstalledArchive Installed(Dir d, string name, bool enabled, params string[] entries)
    {
        var p = Zip(d, "installed-" + name, entries);
        ArchiveContent c;
        using (var f = File.OpenRead(p)) c = PackContentReader.Read(f);
        return new InstalledArchive(name, enabled, new FileInfo(p).Length, p, c);
    }

    public static void Register(TestRunner t)
    {
        t.Group("Скачанный или перетащенный архив");

        t.Run("zip-мод встаёт как есть; новый — «новый», с тем же именем — обновление того, выключенность сохраняется", () =>
        {
            using var d = new Dir();
            var src = Zip(d, "honda_civic.zip", "vehicles/civic/civic.jbeam", "vehicles/civic/info.json");
            var r = IncomingAnalyzer.Analyze(src, Array.Empty<InstalledArchive>(), d["work"], null);
            Assert.True(r.Problem is null && r.Mods.Count == 1, r.Problem ?? "");
            var m = r.Mods[0];
            Assert.Eq(IncomingKind.New, m.Kind);
            Assert.Eq("honda_civic.zip", m.FileName);
            Assert.Eq(src, m.ZipPath);
            Assert.False(m.Repacked || m.Blocked, "как есть");
            var old = Installed(d, "honda_civic.zip", enabled: false, "vehicles/civic/old.jbeam");
            var u = IncomingAnalyzer.Analyze(src, new[] { old }, d["work2"], null).Mods[0];
            Assert.True(u.Kind == IncomingKind.Update && u.Replaces == "honda_civic.zip" && !u.ReplacesEnabled, "обновление выключенного");
        });

        t.Run("обновление узнаётся по составу при другом имени файла и по названию без версии; неоднозначное — новый", () =>
        {
            using var d = new Dir();
            var src = Zip(d, "civic_ek9_v2.zip", "vehicles/civic/civic.jbeam");
            var a = Installed(d, "honda_1.zip", true, "vehicles/civic/x.jbeam");
            var b = Installed(d, "other.zip", true, "vehicles/golf/x.jbeam");
            var m = IncomingAnalyzer.Analyze(src, new[] { a, b }, d["w1"], null).Mods[0];
            Assert.True(m.Kind == IncomingKind.Update && m.Replaces == "honda_1.zip", $"{m.Kind} {m.Replaces}");
            // два одинаково похожих — не гадаем
            var c = Installed(d, "honda_2.zip", true, "vehicles/civic/y.jbeam");
            Assert.Eq(IncomingKind.New, IncomingAnalyzer.Analyze(src, new[] { a, c }, d["w2"], null).Mods[0].Kind);
            // по названию без версии (состава нет: только скрипты)
            var lua = Zip(d, "speedo_v1.5.zip", "lua/ge/extensions/speedo.lua");
            var old = Installed(d, "speedo_v1.0.zip", true, "lua/ge/extensions/speedo.lua.old");
            var s = IncomingAnalyzer.Analyze(lua, new[] { old }, d["w3"], null).Mods[0];
            Assert.True(s.Kind == IncomingKind.Update && s.Replaces == "speedo_v1.0.zip", $"{s.Kind} {s.Replaces}");
        });

        t.Run("ровно этот файл уже стоит — «уже установлен»", () =>
        {
            using var d = new Dir();
            var src = Zip(d, "map.zip", "levels/town/info.json");
            var same = d["installed-map.zip"];
            File.Copy(src, same);
            var inst = new InstalledArchive("map_v1.zip", true, new FileInfo(same).Length, same, ArchiveContent.Empty);
            var m = IncomingAnalyzer.Analyze(src, new[] { inst }, d["w"], null).Mods[0];
            Assert.True(m.Kind == IncomingKind.AlreadyInstalled && m.Replaces == "map_v1.zip", $"{m.Kind}");
        });

        t.Run("zip с модами-zip внутри — каждый отдельно; не моды внутри пропускаются", () =>
        {
            using var d = new Dir();
            var src = ZipOfZips(d, "pack.zip",
                ("cars/audi.zip", new[] { "vehicles/audi/a.jbeam" }),
                ("cars/bmw.zip", new[] { "vehicles/bmw/b.jbeam" }),
                ("readme.zip", new[] { "readme.txt" }));
            var r = IncomingAnalyzer.Analyze(src, Array.Empty<InstalledArchive>(), d["w"], null);
            Assert.Eq(2, r.Mods.Count);
            Assert.Eq("audi.zip,bmw.zip", string.Join(",", r.Mods.Select(m => m.FileName)));
            Assert.True(r.Mods.All(m => m.ZipPath.StartsWith(d["w"])), "собрано во временной папке");
        });

        t.Run("мод в лишней папке перепаковывается без неё; исходник не меняется", () =>
        {
            using var d = new Dir();
            var src = Zip(d, "Ford GT (1).zip", "Ford GT/vehicles/fordgt/a.jbeam", "Ford GT/vehicles/fordgt/info.json");
            var before = File.ReadAllBytes(src);
            var r = IncomingAnalyzer.Analyze(src, Array.Empty<InstalledArchive>(), d["w"], null);
            var m = r.Mods.Single();
            Assert.True(m.Repacked, "перепакован");
            Assert.Eq("Ford GT.zip", m.FileName); // «(1)» от браузера убран
            using (var z = ZipFile.OpenRead(m.ZipPath))
                Assert.True(z.Entries.Any(e => e.FullName == "vehicles/fordgt/a.jbeam") && z.Entries.All(e => !e.FullName.StartsWith("Ford GT/")), "без лишней папки");
            Assert.True(before.SequenceEqual(File.ReadAllBytes(src)), "исходник не тронут");
            Assert.Eq(1, m.Overview.Vehicles.Count);
        });

        t.Run("не мод — понятная причина; повреждённый zip — мод с запретом проверки", () =>
        {
            using var d = new Dir();
            var src = Zip(d, "photos.zip", "img/1.jpg", "img/2.jpg", "notes.txt");
            var r = IncomingAnalyzer.Analyze(src, Array.Empty<InstalledArchive>(), d["w"], null);
            Assert.True(r.Mods.Count == 0 && r.Problem!.StartsWith("В архиве нет мода BeamNG"), r.Problem ?? "");
            var bad = d["broken.zip"];
            File.WriteAllBytes(bad, new byte[] { 0x50, 0x4B, 3, 4, 1, 2, 3 });
            var b = IncomingAnalyzer.Analyze(bad, Array.Empty<InstalledArchive>(), d["w2"], null);
            Assert.True(b.Mods.Count == 1 && b.Mods[0].Blocked, "ставить нельзя");
        });

        t.Run("RAR и 7z: zip внутри, папка мода (собирается в zip), ошибка распаковщика, без распаковщика", () =>
        {
            using var d = new Dir();
            var rar = d["Пак машин.rar"];
            File.WriteAllBytes(rar, new byte[] { 1 });
            var withZips = new FakeExtractor(new()
            {
                ["Пак/audi_r8.zip"] = ZipBytes("vehicles/r8/a.jbeam"),
                ["Пак/readme.txt"] = new byte[] { 1 },
            });
            var r1 = IncomingAnalyzer.Analyze(rar, Array.Empty<InstalledArchive>(), d["w1"], withZips);
            Assert.True(r1.Problem is null && r1.Mods.Single().FileName == "audi_r8.zip", r1.Problem ?? "");

            var withFolder = new FakeExtractor(new()
            {
                ["Lada Niva/vehicles/niva/niva.jbeam"] = new byte[] { 1 },
                ["Lada Niva/vehicles/niva/info.json"] = "{\"Name\":\"Niva\"}"u8.ToArray(),
            });
            var r2 = IncomingAnalyzer.Analyze(d["Пак машин.rar"], Array.Empty<InstalledArchive>(), d["w2"], withFolder);
            var m = r2.Mods.Single();
            Assert.Eq("Lada Niva.zip", m.FileName);
            using (var z = ZipFile.OpenRead(m.ZipPath)) Assert.True(z.Entries.Any(e => e.FullName == "vehicles/niva/niva.jbeam"), "папка мода в корне zip");

            var flat = new FakeExtractor(new() { ["vehicles/niva/niva.jbeam"] = new byte[] { 1 } });
            Assert.Eq("Пак машин.zip", IncomingAnalyzer.Analyze(rar, Array.Empty<InstalledArchive>(), d["w3"], flat).Mods.Single().FileName);

            var fail = IncomingAnalyzer.Analyze(rar, Array.Empty<InstalledArchive>(), d["w4"], new FakeExtractor(new(), "этот RAR не открывается"));
            Assert.Eq("этот RAR не открывается", fail.Problem);
            Assert.Eq("RAR и 7z здесь не открываются.", IncomingAnalyzer.Analyze(rar, Array.Empty<InstalledArchive>(), d["w5"], null).Problem);
        });

        t.Run("имена: «(1)» браузера убирается, недопустимые знаки заменяются, одинаковые получают номер", () =>
        {
            Assert.Eq("mod.zip", IncomingAnalyzer.CleanName("mod (1).zip"));
            Assert.Eq("mod.zip", IncomingAnalyzer.CleanName("mod.rar"));
            Assert.Eq("a_b.zip", IncomingAnalyzer.CleanName("a:b.zip"));
            Assert.Eq("mod.zip", IncomingAnalyzer.CleanName(".zip"));
            Assert.True(IncomingAnalyzer.IsCandidate(@"C:\Downloads\car.7z") && IncomingAnalyzer.IsCandidate("x.RAR"), "rar и 7z");
            Assert.False(IncomingAnalyzer.IsCandidate("x.zip.crdownload") || IncomingAnalyzer.IsCandidate(".x.zip") || IncomingAnalyzer.IsCandidate("x.exe"), "недокачанное и не архивы");
            using var d = new Dir();
            var src = ZipOfZips(d, "p.zip", ("a/car.zip", new[] { "vehicles/a/a.jbeam" }), ("b/car.zip", new[] { "vehicles/b/b.jbeam" }));
            Assert.Eq("car.zip,car_2.zip", string.Join(",", IncomingAnalyzer.Analyze(src, Array.Empty<InstalledArchive>(), d["w"], null).Mods.Select(m => m.FileName)));
        });

        t.Run("форма архива: мод, мод в лишней папке, не мод", () =>
        {
            Assert.True(IncomingAnalyzer.ShapeOf(new[] { "vehicles/a/b.jbeam" }).IsMod, "мод");
            Assert.True(IncomingAnalyzer.ShapeOf(new[] { "lua/ge/x.lua" }).IsMod, "скрипт-мод");
            Assert.Eq("Car", IncomingAnalyzer.ShapeOf(new[] { "Car/", "Car/vehicles/a/b.jbeam" }).WrapFolder);
            var none = IncomingAnalyzer.ShapeOf(new[] { "a/b.txt", "c/d.txt" });
            Assert.True(!none.IsMod && none.WrapFolder is null, "не мод");
            Assert.False(IncomingAnalyzer.ShapeOf(Array.Empty<string>()).IsMod, "пусто");
        });
    }
}
