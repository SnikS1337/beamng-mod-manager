using System.IO.Compression;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Settings;

namespace BeamModManager.Core.Tests;

/// <summary>Библиотека должна подстраиваться под папку модов: ручные правки, внешние удаления, недоступный диск.</summary>
public static class SyncTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);
    private static DiskMod D(string f, long size = 1000, bool on = true) => new(f, on, size);

    private static void Bind(InMemoryLibraryStore s, InstalledMod m, string site = "modland", string? ver = "1.0", long? size = 1000)
    {
        s.UpsertBinding(new ModBinding(m.Id, site, "x", new Uri("https://www.modland.net/x"), true));
        s.SaveCheck(new CheckState(m.Id, site, T0, T0, ver, size));
    }

    private static MemoryStream Zip(int size)
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true))
        {
            using (var w = z.CreateEntry("vehicles/car/info.json").Open()) w.Write(new byte[20]);
            var rnd = new Random(size); var b = new byte[size]; rnd.NextBytes(b);
            using (var w = z.CreateEntry("vehicles/car/car.jbeam").Open()) w.Write(b);
        }
        ms.Position = 0; return ms;
    }

    public static void Register(TestRunner t)
    {
        t.Group("Синхронизация библиотеки с папкой");

        t.Run("новые файлы попадают в библиотеку с названием и версией из имени", () =>
        {
            var s = new InMemoryLibraryStore();
            var r = LibrarySync.Reconcile(s, new[] { D("Audi_A3_v1.2.zip"), D("pickup.zip", 5, false) });
            Assert.Eq(2, r.Added.Count);
            var m = s.Mods().Single(x => x.FileName == "Audi_A3_v1.2.zip");
            Assert.Eq("1.2", m.Version);
            Assert.True(m.Enabled);
            Assert.False(s.Mods().Single(x => x.FileName == "pickup.zip").Enabled);
        });

        t.Run("повторная сверка без изменений ничего не меняет", () =>
        {
            var s = new InMemoryLibraryStore();
            var disk = new[] { D("a_v1.0.zip"), D("b.zip", 7) };
            LibrarySync.Reconcile(s, disk);
            var r = LibrarySync.Reconcile(s, disk);
            Assert.False(r.AnyChange, "второй проход пустой");
        });

        t.Run("файл удалён с диска: запись, привязки и проверки уходят; остальные целы", () =>
        {
            var s = new InMemoryLibraryStore();
            LibrarySync.Reconcile(s, new[] { D("a.zip"), D("b.zip") });
            var a = s.Mods().Single(x => x.FileName == "a.zip"); Bind(s, a);
            var r = LibrarySync.Reconcile(s, new[] { D("b.zip") });
            Assert.Eq(1, r.Removed.Count);
            Assert.Eq(1, s.Mods().Count);
            Assert.Eq(0, s.Bindings().Count);
            Assert.Eq(0, s.Checks().Count);
        });

        t.Run("папка недоступна или пуста при непустой библиотеке: ничего не стираем", () =>
        {
            var s = new InMemoryLibraryStore();
            LibrarySync.Reconcile(s, new[] { D("a.zip") });
            Bind(s, s.Mods().Single());
            var r1 = LibrarySync.Reconcile(s, Array.Empty<DiskMod>(), folderAvailable: false);
            var r2 = LibrarySync.Reconcile(s, Array.Empty<DiskMod>());
            Assert.True(r1.Skipped && r2.Skipped, "обе сверки пропущены");
            Assert.Eq(1, s.Mods().Count);
            Assert.Eq(1, s.Bindings().Count);
        });

        t.Run("регистр имени не создаёт «удалённый + новый» (Windows не различает регистр)", () =>
        {
            var s = new InMemoryLibraryStore();
            LibrarySync.Reconcile(s, new[] { D("Audi.zip") });
            Bind(s, s.Mods().Single());
            var r = LibrarySync.Reconcile(s, new[] { D("AUDI.zip") });
            Assert.False(r.AnyChange, "тот же файл");
            Assert.Eq(1, s.Bindings().Count);
        });

        t.Run("один файл и во включённых, и в отключённых: одна запись, включённый приоритетнее", () =>
        {
            var s = new InMemoryLibraryStore();
            LibrarySync.Reconcile(s, new[] { D("a.zip", 10, false), D("a.zip", 10, true) });
            Assert.Eq(1, s.Mods().Count);
            Assert.True(s.Mods().Single().Enabled);
        });

        t.Run("отключили/включили: запись следует за файлом", () =>
        {
            var s = new InMemoryLibraryStore();
            LibrarySync.Reconcile(s, new[] { D("a.zip") });
            LibrarySync.Reconcile(s, new[] { D("a.zip", 1000, false) });
            Assert.False(s.Mods().Single().Enabled);
        });

        t.Group("Синхронизация: ручное обновление");

        t.Run("ручное обновление с новым именем файла: привязка, проверка и «пропустить» переехали, версия новая, обновления нет", () =>
        {
            var s = new InMemoryLibraryStore();
            LibrarySync.Reconcile(s, new[] { D("audi_a3_v1.0.zip", 1000) });
            var old = s.Mods().Single(); Bind(s, old, ver: "1.5", size: 1200);
            Assert.Eq(1, UpdateCheckService.Pending(s).Count);
            UpdateCheckService.Dismiss(s, UpdateCheckService.Pending(s).Single());
            var r = LibrarySync.Reconcile(s, new[] { D("audi_a3_v1.5.zip", 1200) });
            Assert.Eq(1, r.Renamed.Count);
            var m = s.Mods().Single();
            Assert.Eq("1.5", m.Version);
            Assert.Eq(1, s.Bindings().Count(b => b.ModId == m.Id));
            Assert.True(s.GetCheck(m.Id, "modland") is not null, "проверка переехала");
            Assert.Eq(0, UpdateCheckService.Pending(s, includeDismissed: true).Count);
        });

        t.Run("переименование неоднозначно (два старых или два новых с одним названием): привязки не переносим", () =>
        {
            var s = new InMemoryLibraryStore();
            LibrarySync.Reconcile(s, new[] { D("car_v1.0.zip"), D("car_v1.1.zip") });
            foreach (var m in s.Mods()) Bind(s, m);
            var r = LibrarySync.Reconcile(s, new[] { D("car_v2.0.zip") });
            Assert.Eq(0, r.Renamed.Count);
            Assert.Eq(1, s.Mods().Count);
            Assert.Eq(0, s.Bindings().Count);
        });

        t.Run("файл заменили вручную под тем же именем, размер совпал с сайтом: версия берётся с сайта", () =>
        {
            var s = new InMemoryLibraryStore();
            LibrarySync.Reconcile(s, new[] { D("audi.zip", 1000) });
            var m = s.Mods().Single(); s.UpsertMod(m with { Version = "1.0" });
            Bind(s, m, ver: "1.5", size: 1300);
            Assert.Eq(1, UpdateCheckService.Pending(s).Count);
            LibrarySync.Reconcile(s, new[] { D("audi.zip", 1300) });
            Assert.Eq("1.5", s.Mods().Single().Version);
            Assert.Eq(0, UpdateCheckService.Pending(s).Count);
        });

        t.Run("файл заменили вручную, размер не совпал ни с чем: старая версия не остаётся (она точно неверна)", () =>
        {
            var s = new InMemoryLibraryStore();
            LibrarySync.Reconcile(s, new[] { D("audi.zip", 1000) });
            var m = s.Mods().Single(); s.UpsertMod(m with { Version = "9.9" });
            Bind(s, m, ver: "1.5", size: 5000);
            LibrarySync.Reconcile(s, new[] { D("audi.zip", 2000) });
            Assert.Null(s.Mods().Single().Version);
        });

        t.Run("менеджер поставил обновление: запись обновлена сразу, «обновление» исчезает", () =>
        {
            var s = new InMemoryLibraryStore();
            LibrarySync.Reconcile(s, new[] { D("audi.zip", 1000) });
            var m = s.Mods().Single(); s.UpsertMod(m with { Version = "1.0" });
            Bind(s, m, ver: "1.5", size: 1300);
            Assert.Eq(1, UpdateCheckService.Pending(s).Count);
            LibrarySync.RecordInstalled(s, "audi.zip", 1300, "1.5", true, T0);
            Assert.Eq(0, UpdateCheckService.Pending(s).Count);
            Assert.Eq(1, s.Bindings().Count);
            Assert.Eq(1, s.Mods().Count);
            // и последующая сверка с диском не вносит расхождений
            Assert.False(LibrarySync.Reconcile(s, new[] { D("audi.zip", 1300) }).AnyChange, "сверка после установки пустая");
        });

        t.Run("менеджер поставил новый мод: появляется в библиотеке; версия сайта важнее версии в имени файла", () =>
        {
            var s = new InMemoryLibraryStore();
            LibrarySync.RecordInstalled(s, "car_v1.0.zip", 500, "1.1", true, T0);
            Assert.Eq("1.1", s.Mods().Single().Version);
        });

        t.Run("коллизия идентификаторов: чужой Id совпал с ключом нового файла — оба живы", () =>
        {
            var s = new InMemoryLibraryStore();
            s.UpsertMod(new InstalledMod("b.zip", "Other", "other.zip", null, 1, T0));
            LibrarySync.Reconcile(s, new[] { D("other.zip", 1), D("b.zip", 1) });
            Assert.Eq(2, s.Mods().Count);
            Assert.Eq(2, s.Mods().Select(m => m.Id).Distinct().Count());
        });

        t.Run("параллельные сверки и записи установленного не падают и не дублируют записи", () =>
        {
            var s = new InMemoryLibraryStore();
            var disk = Enumerable.Range(0, 50).Select(i => D($"m{i}_v1.{i}.zip", 100 + i)).ToArray();
            Parallel.For(0, 40, i =>
            {
                if (i % 2 == 0) LibrarySync.Reconcile(s, disk);
                else LibrarySync.RecordInstalled(s, $"m{i}_v1.{i}.zip", 100 + i, null, true, T0);
            });
            Assert.Eq(50, s.Mods().Count);
            Assert.Eq(50, s.Mods().Select(m => LibrarySync.KeyOf(m.FileName)).Distinct().Count());
        });

        t.Group("Синхронизация: настоящая папка");

        t.Run("установка → сверка → отключение → удаление → папка пропала", () =>
        {
            var root = Path.Combine(Path.GetTempPath(), "bmm-sync-" + Guid.NewGuid().ToString("N"));
            try
            {
                var layout = new ModLayout(Path.Combine(root, "mods"), Path.Combine(root, "bmm", "disabled"), Path.Combine(root, "bmm", "backups"));
                var inst = new ModInstaller(layout);
                var s = new InMemoryLibraryStore();
                Assert.True(LibrarySync.Reconcile(s, inst).SkippedReason is null || s.Mods().Count == 0, "пустая папка и пустая библиотека — не сбой");
                Assert.True(inst.Install(Zip(300), "audi_v1.0.zip").Success, "установка");
                Assert.Eq(1, LibrarySync.Reconcile(s, inst).Added.Count);
                Assert.Eq(new FileInfo(Path.Combine(layout.ModsDir, "audi_v1.0.zip")).Length, s.Mods().Single().SizeBytes!.Value);
                Assert.True(inst.Disable("audi_v1.0.zip").Success, "отключение");
                LibrarySync.Reconcile(s, inst);
                Assert.False(s.Mods().Single().Enabled);
                Bind(s, s.Mods().Single());
                Directory.Move(layout.ModsDir, layout.ModsDir + "-gone");
                var gone = LibrarySync.Reconcile(s, inst);
                Assert.True(gone.Skipped, "папка пропала — привязки целы");
                Assert.Eq(1, s.Bindings().Count);
                Directory.Move(layout.ModsDir + "-gone", layout.ModsDir);
                Assert.True(inst.Remove("audi_v1.0.zip").Success, "удаление");
                Assert.True(inst.Install(Zip(10), "other.zip").Success, "ещё мод");
                var r = LibrarySync.Reconcile(s, inst);
                Assert.Eq(1, r.Removed.Count);
                Assert.Eq(0, s.Bindings().Count);
            }
            finally { try { Directory.Delete(root, true); } catch { } }
        });

        t.Group("Обновление мода: смена имени файла");

        t.Run("новая версия под другим именем: старая уходит в резервные, включена одна", () =>
        {
            using var sb = new Box();
            sb.Inst.Install(Zip(300), "audi_v1.0.zip");
            var u = sb.Inst.Update("audi_v1.0.zip", Zip(400), "audi_v1.5.zip");
            Assert.True(u.Success && u.OldRemoved, "обновление удалось");
            Assert.Eq("audi_v1.5.zip", string.Join("|", sb.Inst.Scan().Select(x => x.FileName)));
            Assert.Eq(1, sb.Inst.Backups("audi_v1.0.zip").Count);
        });

        t.Run("отключённый мод после обновления остаётся отключённым", () =>
        {
            using var sb = new Box();
            sb.Inst.Install(Zip(300), "audi_v1.0.zip");
            sb.Inst.Disable("audi_v1.0.zip");
            var u = sb.Inst.Update("audi_v1.0.zip", Zip(400), "audi_v1.5.zip");
            Assert.True(u.Success && !u.WasEnabled, "обновление удалось, мод был отключён");
            var scan = sb.Inst.Scan();
            Assert.Eq(1, scan.Count);
            Assert.False(scan[0].Enabled);
            Assert.Eq("audi_v1.5.zip", scan[0].FileName);
        });

        t.Run("то же имя файла: заменяется на месте, копия старой в резервных", () =>
        {
            using var sb = new Box();
            sb.Inst.Install(Zip(300), "audi.zip");
            var u = sb.Inst.Update("audi.zip", Zip(500), "audi.zip");
            Assert.True(u.Success, "обновление");
            Assert.Eq(1, sb.Inst.Scan().Count);
        });

        t.Run("файл с именем новой версии уже лежит в папке: отказ, старый цел", () =>
        {
            using var sb = new Box();
            sb.Inst.Install(Zip(300), "audi_v1.0.zip");
            sb.Inst.Install(Zip(310), "audi_v1.5.zip");
            var u = sb.Inst.Update("audi_v1.0.zip", Zip(400), "audi_v1.5.zip");
            Assert.False(u.Success, "отказ");
            Assert.Eq(2, sb.Inst.Scan().Count);
        });

        t.Run("плохой архив при обновлении: старая версия не тронута, новой нет", () =>
        {
            using var sb = new Box();
            sb.Inst.Install(Zip(300), "audi_v1.0.zip");
            var u = sb.Inst.Update("audi_v1.0.zip", new MemoryStream(new byte[] { 1, 2, 3, 4 }), "audi_v1.5.zip");
            Assert.False(u.Success, "не прошёл проверку");
            Assert.Eq("audi_v1.0.zip", string.Join("|", sb.Inst.Scan().Select(x => x.FileName)));
        });

        t.Run("старую версию убрать не удалось: новая откатывается, остаётся ровно одна", () =>
        {
            using var sb = new Box();
            sb.Inst.Install(Zip(300), "audi_v1.0.zip");
            // Резервная папка занята файлом: убрать старую версию в резервные нельзя (на Windows то же даёт файл, занятый игрой).
            Directory.CreateDirectory(Path.GetDirectoryName(sb.BackupDir)!);
            File.WriteAllText(sb.BackupDir, "x");
            var u = sb.Inst.Update("audi_v1.0.zip", Zip(400), "audi_v1.5.zip");
            Assert.False(u.Success, "обновление не удалось");
            Assert.False(u.OldRemoved, "старая осталась");
            Assert.Eq("audi_v1.0.zip", string.Join("|", sb.Inst.Scan().Select(x => x.FileName)));
        });

        t.Run("старого файла уже нет (удалили руками): обычная установка", () =>
        {
            using var sb = new Box();
            var u = sb.Inst.Update("gone.zip", Zip(300), "audi_v1.5.zip");
            Assert.True(u.Success, "установилось");
            Assert.Eq(1, sb.Inst.Scan().Count);
        });

        t.Run("недопустимые имена при обновлении отклоняются", () =>
        {
            using var sb = new Box();
            Assert.False(sb.Inst.Update("bad:name.zip", Zip(10), "ok.zip").Success, "старое имя");
            Assert.False(sb.Inst.Update("ok.zip", Zip(10), "readme.txt").Success, "новое имя");
        });

        t.Run("обновление под новым именем: тот же мод в библиотеке, привязка и профиль целы, обновление исчезло", () =>
        {
            using var sb = new Box();
            var s = new InMemoryLibraryStore();
            sb.Inst.Install(Zip(300), "audi_v1.0.zip");
            LibrarySync.Reconcile(s, sb.Inst);
            var mod = s.Mods().Single();
            Bind(s, mod, ver: "1.5", size: 400);
            Assert.Eq(1, UpdateCheckService.Pending(s).Count);
            var profile = new ModProfile("Дрифт", new HashSet<string> { "audi_v1.0.zip" });

            var u = sb.Inst.Update("audi_v1.0.zip", Zip(400), "audi_v1.5.zip");
            var size = new FileInfo(u.Install.Path!).Length;
            LibrarySync.RecordInstalled(s, "audi_v1.5.zip", size, "1.5", true, T0, replacesFileName: "audi_v1.0.zip");

            Assert.Eq(1, s.Mods().Count);
            Assert.Eq(mod.Id, s.Mods().Single().Id);
            Assert.Eq(1, s.Bindings().Count);
            Assert.Eq(0, UpdateCheckService.Pending(s).Count);
            Assert.False(LibrarySync.Reconcile(s, sb.Inst).AnyChange, "диск и библиотека совпадают");

            var p2 = ProfileService.ApplyRenames(profile, new[] { ("audi_v1.0.zip", "audi_v1.5.zip") });
            var plan = ProfileService.Plan(p2, sb.Inst.Scan());
            Assert.True(plan.IsEmpty && plan.Missing.Count == 0, "профиль применять не нужно: всё на месте");
            var planOld = ProfileService.Plan(profile, sb.Inst.Scan());
            Assert.Eq(1, planOld.ToDisable.Count); // без переноса новый файл был бы отключён
        });

        t.Run("переименования в профиле: цепочки, регистр, зацикливание, пустой список", () =>
        {
            var p = new ModProfile("x", new HashSet<string> { "A.zip", "keep.zip" });
            Assert.Eq(p, ProfileService.ApplyRenames(p, Array.Empty<(string, string)>()));
            var r = ProfileService.ApplyRenames(p, new[] { ("a.zip", "b.zip"), ("b.zip", "c.zip") });
            Assert.True(r.EnabledFiles.SetEquals(new[] { "c.zip", "keep.zip" }), "цепочка свернулась");
            var loop = ProfileService.ApplyRenames(p, new[] { ("a.zip", "b.zip"), ("b.zip", "a.zip") });
            Assert.Eq(2, loop.EnabledFiles.Count);
        });
    }

    private sealed class Box : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "bmm-upd-" + Guid.NewGuid().ToString("N"));
        public readonly ModInstaller Inst;
        public string BackupDir => Path.Combine(Root, "bmm", "backups");
        public Box()
        {
            var layout = new ModLayout(Path.Combine(Root, "mods"), Path.Combine(Root, "bmm", "disabled"), BackupDir);
            Inst = new ModInstaller(layout);
        }
        public void Dispose() { try { Directory.Delete(Root, true); } catch { } }
    }
}

public static class DownloadPolicyTests
{
    private static Listing L(PriceStatus p = PriceStatus.Free, string src = "modland") =>
        new(src, new Uri("https://www.modland.net/x"), "x", "Audi") { Price = p };
    private static Offer O(string url, OfferKind k = OfferKind.ExternalHost) => new(k, new Uri(url).Host, new Uri(url));
    // все сайты включены: политику проверяем на Modland (по умолчанию он с 2026-10-18 выключен)
    private static AppSettings On => new() { AllowDownloads = true, DisabledSources = Array.Empty<string>() };

    public static void Register(TestRunner t)
    {
        t.Group("Политика загрузки");

        t.Run("настройка выключена (по умолчанию): всегда отказ, даже для бесплатного на известном хосте", () =>
            Assert.False(DownloadPolicy.Evaluate(new AppSettings(), L(), O("https://modsfire.com/abc")).Allowed, "отказ"));

        t.Run("включена, бесплатный, известный хост: разрешено без вопросов", () =>
        {
            var d = DownloadPolicy.Evaluate(On, L(), O("https://modsfire.com/abc"));
            Assert.True(d.Allowed && !d.NeedsConfirmation, "разрешено");
        });

        t.Run("цена неизвестна: разрешено, но с подтверждением", () =>
        {
            var d = DownloadPolicy.Evaluate(On, L(PriceStatus.Unknown), O("https://www.mediafire.com/file/abc"));
            Assert.True(d.Allowed && d.NeedsConfirmation, "нужно подтверждение");
        });

        t.Run("платный: отказ по цене, по типу ссылки и по хосту", () =>
        {
            Assert.False(DownloadPolicy.Evaluate(On, L(PriceStatus.Paid), O("https://modsfire.com/a")).Allowed, "по цене");
            Assert.False(DownloadPolicy.Evaluate(On, L(), O("https://modsfire.com/a", OfferKind.PaidLink)).Allowed, "по типу");
            Assert.False(DownloadPolicy.Evaluate(On, L(), O("https://www.patreon.com/x")).Allowed, "по хосту");
        });

        t.Run("чужой хост, http и похожие на известные домены: отказ", () =>
        {
            foreach (var u in new[] { "https://evil.example/a", "http://modsfire.com/a", "https://modsfire.com.evil.example/a", "https://notmodsfire.com/a", "https://evilmega.nz/a" })
                Assert.False(DownloadPolicy.Evaluate(On, L(), O(u)).Allowed, u);
        });

        t.Run("поддомен известного хоста разрешён; сайт-источник отключён: отказ", () =>
        {
            Assert.True(DownloadPolicy.Evaluate(On, L(), O("https://www.mediafire.com/file/a")).Allowed, "поддомен");
            Assert.False(DownloadPolicy.Evaluate(On with { DisabledSources = new[] { "modland" } }, L(), O("https://modsfire.com/a")).Allowed, "сайт отключён");
        });
    }
}
