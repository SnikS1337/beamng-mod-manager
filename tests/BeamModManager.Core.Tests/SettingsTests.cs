using BeamModManager.Core.Library;
using BeamModManager.Core.Settings;

namespace BeamModManager.Core.Tests;

public static class SettingsTests
{
    private static string Tmp()
    {
        var d = Path.Combine(Path.GetTempPath(), "bmm-settings-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(d);
        return d;
    }

    private static readonly DateTimeOffset Now = new(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);

    public static void Register(TestRunner t)
    {
        t.Group("Настройки");

        t.Run("по умолчанию загрузка выключена, файла нет — без предупреждений", () =>
        {
            var r = new SettingsStore(Path.Combine(Tmp(), "settings.json")).Load();
            Assert.False(r.Settings.AllowDownloads, "загрузка выключена");
            Assert.False(r.HadProblems, "без предупреждений");
            Assert.Eq(6, r.Settings.CheckTtlHours);
        });

        t.Run("сохранение и чтение возвращают то же самое", () =>
        {
            var p = Path.Combine(Tmp(), "sub", "settings.json");
            var st = new SettingsStore(p);
            st.Save(new AppSettings { AllowDownloads = true, ModsFolder = "C:\\x\\mods", CheckTtlHours = 12, DisabledSources = new[] { "STMods", "modland", "stmods" } });
            var r = st.Load();
            Assert.True(r.Settings.AllowDownloads, "включено");
            Assert.Eq("C:\\x\\mods", r.Settings.ModsFolder);
            Assert.Eq(12, r.Settings.CheckTtlHours);
            Assert.True(r.Settings.DisabledSources.SequenceEqual(new[] { "modland", "stmods" }), "источники в нижнем регистре без повторов");
            Assert.False(File.Exists(p + ".tmp"), "временного файла не осталось");
        });

        t.Run("значения вне диапазона приводятся к допустимым", () =>
        {
            var s = new AppSettings { CheckTtlHours = 0, KeepBackups = 999, MaxCardChecksPerSource = 1, AutoCheckEveryHours = -3, ModsFolder = "   " }.Normalize();
            Assert.Eq(1, s.CheckTtlHours);
            Assert.Eq(20, s.KeepBackups);
            Assert.Eq(5, s.MaxCardChecksPerSource);
            Assert.Eq(0, s.AutoCheckEveryHours);
            Assert.Null(s.ModsFolder);
        });

        t.Run("повреждённый файл: настройки по умолчанию, загрузка выключена, файл отложен в .bad", () =>
        {
            var d = Tmp(); var p = Path.Combine(d, "settings.json");
            File.WriteAllText(p, "{ \"AllowDownloads\": tr");
            var r = new SettingsStore(p, () => Now).Load();
            Assert.False(r.Settings.AllowDownloads, "загрузка выключена");
            Assert.True(r.HadProblems, "предупреждение есть");
            Assert.False(File.Exists(p), "испорченный файл убран с места");
            Assert.Eq(1, Directory.GetFiles(d, "settings.json.bad-*").Length);
        });

        t.Run("не объект (например, массив) и пустой файл: безопасные настройки", () =>
        {
            foreach (var text in new[] { "[1,2]", "", "null", "42" })
            {
                var p = Path.Combine(Tmp(), "settings.json");
                File.WriteAllText(p, text);
                var r = new SettingsStore(p, () => Now).Load();
                Assert.False(r.Settings.AllowDownloads, text);
                Assert.True(r.HadProblems, text);
            }
        });

        t.Run("поле неверного типа сбрасывается, остальные настройки сохраняются", () =>
        {
            var p = Path.Combine(Tmp(), "settings.json");
            File.WriteAllText(p, "{ \"AllowDownloads\": \"yes\", \"CheckTtlHours\": \"много\", \"KeepBackups\": 7, \"ModsFolder\": 5, \"DisabledSources\": [\"modland\", 3] }");
            var r = new SettingsStore(p).Load();
            Assert.False(r.Settings.AllowDownloads, "«yes» строкой загрузку не включает");
            Assert.Eq(6, r.Settings.CheckTtlHours);
            Assert.Eq(7, r.Settings.KeepBackups);
            Assert.Null(r.Settings.ModsFolder);
            Assert.True(r.Settings.DisabledSources.SequenceEqual(new[] { "modland" }), "элемент не строка пропущен");
            Assert.True(r.Warnings.Count >= 3, "каждая проблема названа");
        });

        t.Run("числом 1 или строкой \"true\" загрузка не включается, только true", () =>
        {
            foreach (var v in new[] { "1", "\"true\"", "\"True\"", "[true]" })
            {
                var p = Path.Combine(Tmp(), "settings.json");
                File.WriteAllText(p, "{ \"AllowDownloads\": " + v + " }");
                Assert.False(new SettingsStore(p).Load().Settings.AllowDownloads, v);
            }
            var ok = Path.Combine(Tmp(), "settings.json");
            File.WriteAllText(ok, "{ \"AllowDownloads\": true }");
            Assert.True(new SettingsStore(ok).Load().Settings.AllowDownloads, "true включает");
        });

        t.Run("лишние поля и комментарии в файле не мешают; слишком большой файл отвергается", () =>
        {
            var p = Path.Combine(Tmp(), "settings.json");
            File.WriteAllText(p, "{ // моё\n \"Unknown\": {\"a\":1}, \"KeepBackups\": 5, }");
            Assert.Eq(5, new SettingsStore(p).Load().Settings.KeepBackups);
            File.WriteAllText(p, new string(' ', 2 * 1024 * 1024));
            var r = new SettingsStore(p, () => Now).Load();
            Assert.True(r.HadProblems, "большой файл");
        });

        t.Run("запись поверх существующего файла атомарна: итог целый, старое значение заменено", () =>
        {
            var p = Path.Combine(Tmp(), "settings.json");
            var st = new SettingsStore(p);
            st.Save(new AppSettings { KeepBackups = 4 });
            st.Save(new AppSettings { KeepBackups = 9 });
            Assert.Eq(9, st.Load().Settings.KeepBackups);
        });

        t.Run("параллельные сохранения и чтения не портят файл", () =>
        {
            var p = Path.Combine(Tmp(), "settings.json");
            var st = new SettingsStore(p);
            Parallel.For(0, 40, i =>
            {
                if (i % 2 == 0) st.Save(new AppSettings { KeepBackups = 1 + i % 20 });
                else _ = st.Load();
            });
            Assert.False(st.Load().HadProblems, "файл читается без проблем");
        });

        t.Run("настройки превращаются в параметры проверки; отключённый сайт определяется", () =>
        {
            var s = new AppSettings { CheckTtlHours = 24, MaxCardChecksPerSource = 40, DisabledSources = new[] { "stmods" } }.Normalize();
            var o = s.ToCheckOptions();
            Assert.Eq(TimeSpan.FromHours(24), o.Ttl);
            Assert.Eq(40, o.MaxCardChecksPerSource);
            Assert.False(s.IsSourceEnabled("STMods"), "отключён");
            Assert.True(s.IsSourceEnabled("modland"), "включён");
        });

        t.Group("Папка игры");

        t.Run("пустой путь, файл, несуществующая папка", () =>
        {
            Assert.Eq(FolderProblem.Empty, GameFolders.Resolve("  ").Problem);
            var d = Tmp(); var f = Path.Combine(d, "a.txt"); File.WriteAllText(f, "x");
            Assert.Eq(FolderProblem.NotADirectory, GameFolders.Resolve(f).Problem);
            Assert.Eq(FolderProblem.NotFound, GameFolders.Resolve(Path.Combine(d, "нет")).Problem);
        });

        t.Run("указана сама папка mods (в любом регистре и со слэшем в конце)", () =>
        {
            var d = Tmp(); var mods = Path.Combine(d, "0.39", "Mods"); Directory.CreateDirectory(mods);
            var r = GameFolders.Resolve(mods + Path.DirectorySeparatorChar);
            Assert.True(r.Ok, r.Message);
            Assert.Eq(mods + Path.DirectorySeparatorChar, r.ModsDir);
        });

        t.Run("указана папка пользователя: берётся mods самой новой версии, числа сравниваются как числа", () =>
        {
            var d = Tmp();
            foreach (var v in new[] { "0.38", "0.39", "0.100", "mods-old", "latest" }) Directory.CreateDirectory(Path.Combine(d, v, "mods"));
            Directory.CreateDirectory(Path.Combine(d, "0.40")); // версия без mods
            var r = GameFolders.Resolve(d);
            Assert.True(r.Ok, r.Message);
            Assert.Eq(Path.Combine(d, "0.100", "mods"), r.ModsDir);
            var all = GameFolders.FindModsFolders(d);
            Assert.Eq(3, all.Count);
            Assert.Eq(Path.Combine(d, "0.39", "mods"), all[1]);
        });

        t.Run("внутри нет mods: понятная ошибка, ничего не создаётся", () =>
        {
            var d = Tmp();
            var r = GameFolders.Resolve(d);
            Assert.Eq(FolderProblem.NoModsFolder, r.Problem);
            Assert.Eq(0, Directory.GetDirectories(d).Length);
        });

        t.Run("проверка записи: пробный файл не остаётся", () =>
        {
            var d = Tmp(); var mods = Path.Combine(d, "mods"); Directory.CreateDirectory(mods);
            Assert.True(GameFolders.Resolve(mods).Ok, "записывать можно");
            Assert.Eq(0, Directory.GetFiles(mods).Length);
        });

        t.Run("явная папка важнее папки пользователя; ничего не выбрано — Empty", () =>
        {
            var d = Tmp(); var a = Path.Combine(d, "a", "mods"); var b = Path.Combine(d, "b", "mods");
            Directory.CreateDirectory(a); Directory.CreateDirectory(b);
            Assert.Eq(a, GameFolders.FromSettings(new AppSettings { ModsFolder = a, UserFolder = Path.Combine(d, "b") }).ModsDir);
            Assert.Eq(b, GameFolders.FromSettings(new AppSettings { UserFolder = Path.Combine(d, "b") }).ModsDir);
            Assert.Eq(FolderProblem.Empty, GameFolders.FromSettings(new AppSettings()).Problem);
        });

        t.Group("«Пропустить эту версию»");
        var T0 = new DateTimeOffset(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);

        InMemoryLibraryStore Store(string installed, string seen, string source = "x")
        {
            var s = new InMemoryLibraryStore();
            s.UpsertMod(new InstalledMod("a", "A", "a.zip", installed, null, T0));
            s.UpsertBinding(new ModBinding("a", source, "a", new Uri("https://x.test/a"), true));
            s.SaveCheck(new CheckState("a", source, T0, T0, seen, null));
            return s;
        }

        t.Run("пропущенное обновление скрыто и не возвращается при повторных проверках", () =>
        {
            var s = Store("1.0", "2.0");
            var f = UpdateCheckService.Pending(s).Single();
            UpdateCheckService.Dismiss(s, f);
            Assert.Eq(0, UpdateCheckService.Pending(s).Count);
            Assert.Eq(0, UpdateCheckService.Pending(s).Count); // повторное чтение то же
        });

        t.Run("скрытое видно в списке «скрытые» с пометкой и возвращается кнопкой «вернуть»", () =>
        {
            var s = Store("1.0", "2.0");
            UpdateCheckService.Dismiss(s, UpdateCheckService.Pending(s).Single());
            var all = UpdateCheckService.Pending(s, includeDismissed: true);
            Assert.True(all.Single().IsDismissed, "помечено");
            UpdateCheckService.Restore(s, "a");
            Assert.False(UpdateCheckService.Pending(s).Single().IsDismissed, "снова в списке");
        });

        t.Run("вышла другая версия: обновление показывается снова", () =>
        {
            var s = Store("1.0", "2.0");
            UpdateCheckService.Dismiss(s, UpdateCheckService.Pending(s).Single());
            s.SaveCheck(new CheckState("a", "x", T0, T0, "2.1", null));
            Assert.Eq("2.1", UpdateCheckService.Pending(s).Single().LatestVersion);
        });

        t.Run("версия без номера (изменился только размер): пропуск работает по размеру", () =>
        {
            var s = new InMemoryLibraryStore();
            s.UpsertMod(new InstalledMod("a", "A", "a.zip", null, 100, T0));
            s.UpsertBinding(new ModBinding("a", "x", "a", new Uri("https://x.test/a"), true));
            s.SaveCheck(new CheckState("a", "x", T0, T0, null, 200));
            var f = UpdateCheckService.Pending(s).Single();
            UpdateCheckService.Dismiss(s, f);
            Assert.Eq(0, UpdateCheckService.Pending(s).Count);
            s.SaveCheck(new CheckState("a", "x", T0, T0, null, 300));
            Assert.Eq(1, UpdateCheckService.Pending(s).Count);
        });

        t.Run("два сайта: скрытое обновление с одного не прячет другое, более новое", () =>
        {
            var s = Store("1.0", "2.0", "x");
            s.UpsertBinding(new ModBinding("a", "y", "a", new Uri("https://y.test/a"), true));
            s.SaveCheck(new CheckState("a", "y", T0, T0, "3.0", null));
            UpdateCheckService.Dismiss(s, UpdateCheckService.Pending(s).Single());
            Assert.Eq(1, UpdateCheckService.Pending(s).Count);
        });

        t.Run("мод удалён: пометка пропуска исчезает вместе с ним; для несуществующего мода не создаётся", () =>
        {
            var s = Store("1.0", "2.0");
            UpdateCheckService.Dismiss(s, UpdateCheckService.Pending(s).Single());
            s.RemoveMod("a");
            Assert.Null(s.GetDismissedUpdate("a"));
            s.SetDismissedUpdate("нет", "1.0");
            Assert.Null(s.GetDismissedUpdate("нет"));
        });
    }
}
