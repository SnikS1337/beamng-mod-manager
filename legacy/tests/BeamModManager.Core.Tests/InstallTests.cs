using System.IO.Compression;
using BeamModManager.Core.Install;

namespace BeamModManager.Core.Tests;

public static class InstallTests
{
    private static MemoryStream Zip(params (string Name, int Size)[] entries)
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true))
            foreach (var (n, s) in entries)
            {
                using var w = z.CreateEntry(n).Open();
                w.Write(Enumerable.Range(0, s).Select(i => (byte)(i % 251)).ToArray());
            }
        ms.Position = 0;
        return ms;
    }

    private static MemoryStream GoodMod(int size = 50) => Zip(("vehicles/car/info.json", 20), ("vehicles/car/car.jbeam", size));

    private sealed class Sandbox : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "bmm-test-" + Guid.NewGuid().ToString("N"));
        public readonly ModLayout Layout;
        public DateTimeOffset Now = new(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);
        public readonly ModInstaller Inst;
        public Sandbox(int keep = 3)
        {
            Layout = new ModLayout(Path.Combine(Root, "mods"), Path.Combine(Root, "bmm", "disabled"), Path.Combine(Root, "bmm", "backups"));
            Inst = new ModInstaller(Layout, () => Now = Now.AddSeconds(1), keep);
        }
        public void Dispose() { try { Directory.Delete(Root, true); } catch { } }
    }

    public static void Register(TestRunner t)
    {
        t.Group("Установка модов (фальшивая папка)");

        t.Run("обычный мод кладётся в папку mods, считается хеш", () =>
        {
            using var sb = new Sandbox();
            var r = sb.Inst.Install(GoodMod(), "car.zip");
            Assert.True(r.Success, r.Message ?? "");
            Assert.True(File.Exists(Path.Combine(sb.Layout.ModsDir, "car.zip")), "файл на месте");
            Assert.Eq(64, r.Sha256!.Length);
            Assert.Eq(0, Directory.GetFiles(sb.Layout.ModsDir, "*.part").Length + Directory.GetFiles(sb.Layout.ModsDir, ".*.part").Length);
        });

        t.Run("исполняемый файл внутри: блокировка, в папке mods ничего не остаётся", () =>
        {
            using var sb = new Sandbox();
            var r = sb.Inst.Install(Zip(("vehicles/x/run.exe", 10), ("vehicles/x/a.jbeam", 10)), "bad.zip");
            Assert.False(r.Success);
            Assert.Eq(InstallError.Blocked, r.Error);
            Assert.Eq(0, Directory.GetFileSystemEntries(sb.Layout.ModsDir).Length);
        });

        t.Run("zip-slip: блокировка", () =>
        {
            using var sb = new Sandbox();
            Assert.Eq(InstallError.Blocked, sb.Inst.Install(Zip(("../evil.lua", 5)), "slip.zip").Error);
        });

        t.Run("не архив: блокировка", () =>
        {
            using var sb = new Sandbox();
            var r = sb.Inst.Install(new MemoryStream("текст"u8.ToArray()), "fake.zip");
            Assert.Eq(InstallError.Blocked, r.Error);
        });

        t.Run("предупреждения без разрешения: не ставим; с разрешением: ставим", () =>
        {
            using var sb = new Sandbox();
            var odd = () => Zip(("mymod/vehicles/a.jbeam", 10), ("mymod/info.json", 10));
            Assert.Eq(InstallError.HasWarnings, sb.Inst.Install(odd(), "odd.zip").Error);
            Assert.False(File.Exists(Path.Combine(sb.Layout.ModsDir, "odd.zip")));
            Assert.True(sb.Inst.Install(odd(), "odd.zip", new InstallOptions { AllowWarnings = true }).Success, "с разрешением");
        });

        t.Run("опасные имена файлов отклоняются", () =>
        {
            using var sb = new Sandbox();
            foreach (var n in new[] { "", "mod.exe", ".hidden.zip", "a/", "bad:name.zip\0" })
                Assert.Eq(InstallError.BadName, sb.Inst.Install(GoodMod(), n).Error);
        });

        t.Run("путь в имени отбрасывается, остаётся имя файла", () =>
        {
            using var sb = new Sandbox();
            var r = sb.Inst.Install(GoodMod(), "../../up/evil.zip");
            Assert.True(r.Success, r.Message ?? "");
            Assert.Eq(Path.Combine(sb.Layout.ModsDir, "evil.zip"), r.Path);
        });

        t.Run("повторная установка без разрешения заменить: отказ", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(GoodMod(), "car.zip");
            Assert.Eq(InstallError.AlreadyExists, sb.Inst.Install(GoodMod(), "car.zip").Error);
        });

        t.Run("обновление: старый файл уходит в резервную копию, новый на месте", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(GoodMod(50), "car.zip");
            var oldLen = new FileInfo(Path.Combine(sb.Layout.ModsDir, "car.zip")).Length;
            var r = sb.Inst.Install(GoodMod(500), "car.zip", new InstallOptions { ReplaceExisting = true });
            Assert.True(r.Success, r.Message ?? "");
            Assert.True(new FileInfo(r.Path!).Length != oldLen, "файл заменён");
            var backups = sb.Inst.Backups("car.zip");
            Assert.Eq(1, backups.Count);
            Assert.Eq(oldLen, new FileInfo(backups[0]).Length);
        });

        t.Run("неудачное обновление (опасный архив) оставляет старый файл нетронутым", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(GoodMod(50), "car.zip");
            var before = File.ReadAllBytes(Path.Combine(sb.Layout.ModsDir, "car.zip"));
            var r = sb.Inst.Install(Zip(("a/b.exe", 5)), "car.zip", new InstallOptions { ReplaceExisting = true });
            Assert.False(r.Success);
            Assert.True(before.SequenceEqual(File.ReadAllBytes(Path.Combine(sb.Layout.ModsDir, "car.zip"))), "старый файл цел");
            Assert.Eq(0, sb.Inst.Backups("car.zip").Count);
        });

        t.Run("резервных копий хранится не больше заданного числа, самые старые удаляются", () =>
        {
            using var sb = new Sandbox(keep: 2);
            sb.Inst.Install(GoodMod(10), "car.zip");
            for (var i = 1; i <= 5; i++)
                sb.Inst.Install(GoodMod(10 + i * 10), "car.zip", new InstallOptions { ReplaceExisting = true });
            Assert.Eq(2, sb.Inst.Backups("car.zip").Count);
        });

        t.Run("отключить и включить: файл переезжает туда и обратно", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(GoodMod(), "car.zip");
            Assert.True(sb.Inst.Disable("car.zip").Success, "отключён");
            Assert.False(File.Exists(Path.Combine(sb.Layout.ModsDir, "car.zip")));
            Assert.True(File.Exists(Path.Combine(sb.Layout.DisabledDir, "car.zip")), "в отключённых");
            Assert.False(sb.Inst.Scan().Single().Enabled);
            Assert.True(sb.Inst.Enable("car.zip").Success, "включён");
            Assert.True(sb.Inst.Scan().Single().Enabled, "виден как включённый");
        });

        t.Run("отключить несуществующий мод: отказ без исключения", () =>
        {
            using var sb = new Sandbox();
            Assert.False(sb.Inst.Disable("нет.zip").Success);
        });

        t.Run("установка поверх отключённого мода: без разрешения замены — отказ, дубль не создаётся", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(GoodMod(10), "car.zip"); sb.Inst.Disable("car.zip");
            var r = sb.Inst.Install(GoodMod(20), "car.zip");
            Assert.Eq(InstallError.AlreadyExists, r.Error);
            Assert.False(File.Exists(Path.Combine(sb.Layout.ModsDir, "car.zip")), "во включённых ничего не появилось");
        });

        t.Run("обновление отключённого мода заменяет отключённую копию и оставляет мод отключённым", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(GoodMod(10), "car.zip"); sb.Inst.Disable("car.zip");
            var r = sb.Inst.Install(GoodMod(300), "car.zip", new InstallOptions { ReplaceExisting = true });
            Assert.True(r.Success, r.Message ?? "");
            Assert.False(File.Exists(Path.Combine(sb.Layout.ModsDir, "car.zip")), "во включённых нет");
            Assert.True(File.Exists(Path.Combine(sb.Layout.DisabledDir, "car.zip")), "отключённая копия обновлена");
            Assert.Eq(1, sb.Inst.Backups("car.zip").Count); // прежняя версия в резервных
            Assert.True(sb.Inst.Enable("car.zip").Success, "включить обратно можно");
        });

        t.Run("файл с тем же именем и среди включённых, и среди отключённых: включение отказывает, установка отказывает, профиль не падает", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(GoodMod(10), "car.zip");
            Directory.CreateDirectory(sb.Layout.DisabledDir);
            File.Copy(Path.Combine(sb.Layout.ModsDir, "car.zip"), Path.Combine(sb.Layout.DisabledDir, "car.zip"));
            Assert.False(sb.Inst.Enable("car.zip").Success, "включение");
            Assert.Eq(InstallError.AlreadyExists, sb.Inst.Install(GoodMod(5), "car.zip", new InstallOptions { ReplaceExisting = true }).Error);
            var plan = ProfileService.Plan(new ModProfile("p", new HashSet<string> { "car.zip" }), sb.Inst.Scan());
            Assert.Eq(0, plan.ToEnable.Count); // включённая копия уже есть
        });

        t.Run("профиль: одно имя в разном регистре дважды — без исключения", () =>
        {
            var plan = ProfileService.Plan(new ModProfile("p", new HashSet<string> { "A.zip" }),
                new[] { ("a.zip", true), ("A.ZIP", false) });
            Assert.Eq(0, plan.ToDisable.Count);
        });

        t.Run("резервные копии в одну и ту же миллисекунду: порядок верный, лишняя удаляется старая", () =>
        {
            using var sb = new Sandbox(keep: 2);
            var inst = new ModInstaller(sb.Layout, () => new DateTimeOffset(2026, 10, 13, 12, 0, 0, TimeSpan.Zero), 2); // часы стоят
            inst.Install(GoodMod(10), "car.zip");
            inst.Install(GoodMod(11), "car.zip", new InstallOptions { ReplaceExisting = true }); // копия версии 10
            inst.Install(GoodMod(12), "car.zip", new InstallOptions { ReplaceExisting = true }); // копия версии 11
            inst.Install(GoodMod(13), "car.zip", new InstallOptions { ReplaceExisting = true }); // копия версии 12, самая старая (10) удалена
            var b = inst.Backups("car.zip");
            Assert.Eq(2, b.Count);
            var sizes = b.Select(f => new FileInfo(f).Length).ToList();
            Assert.True(sizes[0] < sizes[1], "старая раньше новой");
            Assert.True(inst.RestoreLatestBackup("car.zip").Success, "восстановление");
            // восстановилась версия 12, а не 11
            Assert.Eq(sizes[1], new FileInfo(Path.Combine(sb.Layout.ModsDir, "car.zip")).Length);
        });

        t.Run("слишком большой файл: загрузка прерывается, ничего не остаётся", () =>
        {
            using var sb = new Sandbox();
            var r = sb.Inst.Install(GoodMod(5000), "car.zip", new InstallOptions { MaxArchiveBytes = 100 });
            Assert.Eq(InstallError.TooLarge, r.Error);
            Assert.Eq(0, Directory.GetFileSystemEntries(sb.Layout.ModsDir).Length);
        });

        t.Run("особые имена Windows, имена с точкой/пробелом на конце, управляющие и RTL-символы, слишком длинное имя — отклоняются", () =>
        {
            using var sb = new Sandbox();
            foreach (var n in new[] { "CON.zip", "nul.zip", "Com1.zip", "LPT9.zip", "aux.v2.zip", "x .zip", "x..zip", ".zip", "a\u0001b.zip", "a\u202Eb.zip", new string('a', 200) + ".zip", "readme.txt", "a.zip.exe" })
                Assert.Eq(InstallError.BadName, sb.Inst.Install(GoodMod(), n).Error);
            foreach (var ok in new[] { "my_car-1.2.zip", "Автомобиль.zip", "console.zip", "com10.zip", "a b.zip" })
                Assert.True(ModInstaller.SafeFileName(ok) is not null, ok);
        });

        t.Run("ошибка при отключении/включении/удалении не бросает исключение", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(GoodMod(10), "car.zip");
            // Папка отключённых занята файлом с тем же именем: создать каталог нельзя.
            Directory.CreateDirectory(Path.GetDirectoryName(sb.Layout.DisabledDir)!);
            File.WriteAllText(sb.Layout.DisabledDir, "я файл, а не папка");
            var r = sb.Inst.Disable("car.zip");
            Assert.False(r.Success, "отказ");
            Assert.True(File.Exists(Path.Combine(sb.Layout.ModsDir, "car.zip")), "мод остался на месте");
        });

        t.Run("сироты .part от оборванной установки не мешают Scan", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(GoodMod(10), "car.zip");
            File.WriteAllText(Path.Combine(sb.Layout.ModsDir, ".abc.part"), "обрыв");
            Assert.Eq(1, sb.Inst.Scan().Count);
        });

        t.Run("удаление не окончательное: копия остаётся в резервных, восстановление возвращает", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(GoodMod(77), "car.zip");
            var size = new FileInfo(Path.Combine(sb.Layout.ModsDir, "car.zip")).Length;
            Assert.True(sb.Inst.Remove("car.zip").Success, "удалён");
            Assert.Eq(0, sb.Inst.Scan().Count);
            Assert.True(sb.Inst.RestoreLatestBackup("car.zip").Success, "восстановлен");
            Assert.Eq(size, new FileInfo(Path.Combine(sb.Layout.ModsDir, "car.zip")).Length);
        });

        t.Run("профиль: план включает нужные, отключает лишние, сообщает об отсутствующих", () =>
        {
            using var sb = new Sandbox();
            foreach (var n in new[] { "a.zip", "b.zip", "c.zip" }) sb.Inst.Install(GoodMod(), n);
            sb.Inst.Disable("c.zip");
            var profile = new ModProfile("дрифт", new HashSet<string> { "a.zip", "c.zip", "нет.zip" });
            var plan = ProfileService.Plan(profile, sb.Inst.Scan());
            Assert.Eq("c.zip", plan.ToEnable.Single());
            Assert.Eq("b.zip", plan.ToDisable.Single());
            Assert.Eq("нет.zip", plan.Missing.Single());
        });

        t.Run("применение профиля меняет состояние; повторный план пуст", () =>
        {
            using var sb = new Sandbox();
            foreach (var n in new[] { "a.zip", "b.zip", "c.zip" }) sb.Inst.Install(GoodMod(), n);
            sb.Inst.Disable("c.zip");
            var profile = new ModProfile("p", new HashSet<string> { "a.zip", "c.zip" });
            var res = ProfileService.Apply(sb.Inst, ProfileService.Plan(profile, sb.Inst.Scan()));
            Assert.Eq(0, res.Failed.Count);
            var state = sb.Inst.Scan().ToDictionary(x => x.FileName, x => x.Enabled);
            Assert.True(state["a.zip"] && state["c.zip"] && !state["b.zip"], "a и c включены, b отключён");
            Assert.True(ProfileService.Plan(profile, sb.Inst.Scan()).IsEmpty, "план пуст");
        });

        t.Run("сбой одного файла не останавливает применение профиля", () =>
        {
            using var sb = new Sandbox();
            foreach (var n in new[] { "a.zip", "b.zip" }) sb.Inst.Install(GoodMod(), n);
            var plan = new ProfilePlan(Array.Empty<string>(), new[] { "призрак.zip", "a.zip", "b.zip" }, Array.Empty<string>());
            var res = ProfileService.Apply(sb.Inst, plan);
            Assert.Eq(1, res.Failed.Count);
            Assert.Eq(2, res.Disabled.Count);
        });

        t.Run("Beside: служебные папки рядом с mods, а не внутри", () =>
        {
            var l = ModLayout.Beside(Path.Combine(Path.GetTempPath(), "x", "mods"));
            Assert.False(l.DisabledDir.StartsWith(Path.Combine(Path.GetTempPath(), "x", "mods") + Path.DirectorySeparatorChar));
        });
    }
}
