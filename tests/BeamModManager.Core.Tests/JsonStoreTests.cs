using BeamModManager.Core.Library;
using BeamModManager.Core.Sources;

namespace BeamModManager.Core.Tests;

public static class JsonStoreTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 15, 12, 0, 0, TimeSpan.Zero);
    private static string NewPath() => Path.Combine(Path.GetTempPath(), "bmm-lib-" + Guid.NewGuid().ToString("N"), "library.json");

    public static void Register(TestRunner t)
    {
        t.Group("Библиотека в файле");

        t.Run("всё переживает перезапуск: моды, привязки, проверки, сайты, пропуски, история имён, отказы", () =>
        {
            var p = NewPath();
            var s = new JsonLibraryStore(p, () => T0);
            s.UpsertMod(new InstalledMod("e34", "BMW E34", "bmw_e34_v2.zip", "2.0", 100, T0, false));
            s.UpsertMod(new InstalledMod("m3", "BMW M3", "m3.zip", null, 50, T0));
            s.UpsertBinding(new ModBinding("e34", "stmods", "x", new Uri("https://stmods.org/x/"), true));
            s.SaveCheck(new CheckState("e34", "stmods", T0, T0.AddDays(-1), "2.1", 120, 1));
            s.SetSourceStatus("modland", SourceStatus.NeedsHumanCheck);
            s.SetDismissedUpdate("e34", "stmods:2.1");
            s.AddFileHistory("e34", "bmw_e34_v1.zip");
            s.SetCheckingDeclined("m3", true);

            var r = new JsonLibraryStore(p);
            Assert.Eq(0, r.LoadWarnings.Count);
            Assert.Eq(2, r.Mods().Count);
            Assert.Eq(new InstalledMod("e34", "BMW E34", "bmw_e34_v2.zip", "2.0", 100, T0, false), r.GetMod("e34"));
            Assert.Eq("https://stmods.org/x/", r.Bindings().Single().Url.ToString());
            Assert.Eq(new CheckState("e34", "stmods", T0, T0.AddDays(-1), "2.1", 120, 1), r.GetCheck("e34", "stmods"));
            Assert.True(r.GetSourceStatus("modland") == SourceStatus.NeedsHumanCheck, "статус сайта");
            Assert.Eq("stmods:2.1", r.GetDismissedUpdate("e34"));
            Assert.True(r.FileHistory("e34").SequenceEqual(new[] { "bmw_e34_v1.zip" }), "история");
            Assert.True(r.IsCheckingDeclined("m3") && !r.IsCheckingDeclined("e34"), "отказ");
            Assert.False(File.Exists(p + ".tmp"), "временного нет");
        });

        t.Run("серия изменений в одной записи: файл пишется один раз, в конце", () =>
        {
            var p = NewPath();
            var s = new JsonLibraryStore(p);
            using (s.Batch())
            {
                s.UpsertMod(new InstalledMod("a", "A", "a.zip", null, 1, T0));
                using (s.Batch()) s.UpsertMod(new InstalledMod("b", "B", "b.zip", null, 1, T0));
                Assert.False(File.Exists(p), "внутри серии на диск не пишем");
            }
            Assert.True(File.Exists(p), "после серии записано");
            Assert.Eq(2, new JsonLibraryStore(p).Mods().Count);
        });

        t.Run("удаление мода убирает его привязки, проверки и отказ и в файле", () =>
        {
            var p = NewPath();
            var s = new JsonLibraryStore(p);
            s.UpsertMod(new InstalledMod("a", "A", "a.zip", null, 1, T0));
            s.UpsertBinding(new ModBinding("a", "stmods", "x", new Uri("https://stmods.org/x/"), true));
            s.SaveCheck(new CheckState("a", "stmods", T0, null, "1", 1));
            s.RemoveMod("a");
            var r = new JsonLibraryStore(p);
            Assert.Eq(0, r.Mods().Count + r.Bindings().Count + r.Checks().Count);
        });

        t.Run("повреждённый файл откладывается, библиотека начинается заново и снова пишется", () =>
        {
            var p = NewPath();
            Directory.CreateDirectory(Path.GetDirectoryName(p)!);
            File.WriteAllText(p, "{ это не json");
            var s = new JsonLibraryStore(p, () => T0);
            Assert.Eq(1, s.LoadWarnings.Count);
            Assert.Eq(0, s.Mods().Count);
            Assert.Eq(1, Directory.GetFiles(Path.GetDirectoryName(p)!, "library.json.bad-*").Length);
            s.UpsertMod(new InstalledMod("a", "A", "a.zip", null, 1, T0));
            Assert.Eq(1, new JsonLibraryStore(p).Mods().Count);
        });

        t.Run("плохие записи пропускаются с предупреждением, хорошие читаются", () =>
        {
            var p = NewPath();
            Directory.CreateDirectory(Path.GetDirectoryName(p)!);
            File.WriteAllText(p, """
            { "Schema": 1,
              "Mods": [ {"Id":"a","Name":"A","FileName":"a.zip","InstalledAtUtc":"2026-10-15T12:00:00+00:00","Enabled":true},
                        {"Id":"","Name":"?","FileName":"b.zip","InstalledAtUtc":"2026-10-15T12:00:00+00:00"} ],
              "Bindings": [ {"ModId":"нет","Source":"stmods","SourceId":"x","Url":"https://stmods.org/x/","Confirmed":true} ],
              "Checks": [], "Sources": {}, "Dismissed": {}, "History": {}, "Declined": [] }
            """);
            var s = new JsonLibraryStore(p);
            Assert.Eq(1, s.Mods().Count);
            Assert.Eq(0, s.Bindings().Count);
            Assert.Eq(1, s.LoadWarnings.Count);
        });

        t.Run("файл другого вида (массив, пустой объект) не роняет программу", () =>
        {
            foreach (var body in new[] { "[1,2,3]", "{}", "null" })
            {
                var p = NewPath();
                Directory.CreateDirectory(Path.GetDirectoryName(p)!);
                File.WriteAllText(p, body);
                var s = new JsonLibraryStore(p);
                Assert.Eq(0, s.Mods().Count);
                Assert.Eq(1, s.LoadWarnings.Count);
            }
        });

        t.Run("сверка с папкой и проверка обновлений работают поверх файла так же, как в памяти", () =>
        {
            var p = NewPath();
            var s = new JsonLibraryStore(p);
            LibrarySync.Reconcile(s, new[] { new BeamModManager.Core.Install.DiskMod("car_v1.0.zip", true, 10) });
            var id = s.Mods().Single().Id;
            s.UpsertBinding(new ModBinding(id, "stmods", "car", new Uri("https://stmods.org/car/"), true));
            Assert.Eq(1, UpdateCheckService.Tracked(new JsonLibraryStore(p)).Count);
        });
    }
}

public static class JsonStoreFailureTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 15, 12, 0, 0, TimeSpan.Zero);

    public static void Register(TestRunner t)
    {
        t.Group("Библиотека в файле: сбои диска");

        t.Run("запись не удалась: исключения нет, правка в памяти, ошибка видна, следующая запись догоняет", () =>
        {
            var dir = Path.Combine(Path.GetTempPath(), "bmm-lf-" + Guid.NewGuid().ToString("N"));
            var p = Path.Combine(dir, "library.json");
            var s = new JsonLibraryStore(p);
            Directory.CreateDirectory(p); // на месте файла папка: переименование не пройдёт
            s.UpsertMod(new InstalledMod("a", "A", "a.zip", null, 1, T0));
            Assert.Eq(1, s.Mods().Count);
            Assert.True(s.SaveError is not null, "ошибка запомнена");
            Assert.False(s.Flush(), "пока не записано");
            Directory.Delete(p);
            Assert.True(s.Flush(), "записалось");
            Assert.True(s.SaveError is null, "ошибки больше нет");
            Assert.Eq(1, new JsonLibraryStore(p).Mods().Count);
            Assert.Eq(0, Directory.GetFiles(dir, "*.tmp").Length);
        });

        t.Run("те же данные ещё раз: файл не переписывается (сверка при каждом возврате в окно)", () =>
        {
            var p = Path.Combine(Path.GetTempPath(), "bmm-lf-" + Guid.NewGuid().ToString("N"), "library.json");
            var s = new JsonLibraryStore(p);
            var m = new InstalledMod("a", "A", "a.zip", "1", 1, T0);
            var b = new ModBinding("a", "stmods", "x", new Uri("https://stmods.org/x/"), true);
            var c = new CheckState("a", "stmods", T0, null, "1", 1);
            s.UpsertMod(m); s.UpsertBinding(b); s.SaveCheck(c); s.SetSourceStatus("stmods", BeamModManager.Core.Sources.SourceStatus.Ok); s.SetDismissedUpdate("a", "2");
            File.SetLastWriteTimeUtc(p, new DateTime(2000, 1, 1, 0, 0, 0, DateTimeKind.Utc));
            s.UpsertMod(m with { }); s.UpsertBinding(b with { }); s.SaveCheck(c with { });
            s.SetSourceStatus("stmods", BeamModManager.Core.Sources.SourceStatus.Ok); s.SetDismissedUpdate("a", "2");
            Assert.Eq(2000, File.GetLastWriteTimeUtc(p).Year);
            s.UpsertMod(m with { Version = "2" });
            Assert.True(File.GetLastWriteTimeUtc(p).Year > 2000, "изменение записано");
        });

        t.Run("файл занят при запуске: библиотека его не перезаписывает", () =>
        {
            var dir = Path.Combine(Path.GetTempPath(), "bmm-lf-" + Guid.NewGuid().ToString("N"));
            var p = Path.Combine(dir, "library.json");
            var s0 = new JsonLibraryStore(p);
            s0.UpsertMod(new InstalledMod("a", "A", "a.zip", null, 1, T0));
            s0.UpsertBinding(new ModBinding("a", "stmods", "x", new Uri("https://stmods.org/x/"), true));
            var before = File.ReadAllText(p);

            JsonLibraryStore s;
            using (new FileStream(p, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
                s = new JsonLibraryStore(p);
            if (s.LoadWarnings.Count == 0) return; // система не держит блокировку (так бывает вне Windows): проверять нечего
            Assert.Eq(0, s.Mods().Count);
            s.UpsertMod(new InstalledMod("b", "B", "b.zip", null, 1, T0));
            Assert.Eq(before, File.ReadAllText(p));
            Assert.True(s.SaveError is not null, "объяснено, почему не пишем");
        });
    }
}
