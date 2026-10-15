using System.IO.Compression;
using BeamModManager.Core.Game;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;

namespace BeamModManager.Core.Tests;

/// <summary>db.json игры: настоящие файлы с компьютера друга заказчика (0.39.4, tests/fixtures/game).</summary>
public static class GameDbTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 15, 12, 0, 0, TimeSpan.Zero);
    private static readonly Func<bool> NotRunning = () => false;

    private static string Fixture(string name)
    {
        var dir = AppContext.BaseDirectory;
        while (dir != null && !Directory.Exists(Path.Combine(dir, "tests", "fixtures"))) dir = Path.GetDirectoryName(dir);
        return Path.Combine(dir!, "tests", "fixtures", "game", name);
    }

    private static MemoryStream Zip(int size)
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true))
        {
            using (var w = z.CreateEntry("vehicles/car/info.json").Open()) w.Write(new byte[20]);
            using var w2 = z.CreateEntry("vehicles/car/car.jbeam").Open();
            w2.Write(Enumerable.Range(0, size).Select(i => (byte)(i % 251)).ToArray());
        }
        ms.Position = 0;
        return ms;
    }

    /// <summary>Папка mods с настоящим db.json и пустыми архивами под его имена.</summary>
    private sealed class Game : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "bmm-game-" + Guid.NewGuid().ToString("N"));
        public readonly ModInstaller Inst;
        public readonly InMemoryLibraryStore Store = new();
        public string Mods => Path.Combine(Root, "mods");
        public string Db => GameModDb.PathIn(Mods);
        public Game(string? fixture = "db-5mods-bmw-off-0.39.4.json")
        {
            Directory.CreateDirectory(Mods);
            if (fixture is not null)
            {
                File.Copy(Fixture(fixture), Db);
                foreach (var f in new[] { "BMW_M2_G87_inf.zip", "LXR_Sliva_S14_V1.1.zip", "mudstone_valley.zip", "TrafficReborn_Free_v1.7.0.0.zip", "w164_kenemation_2.7.zip" })
                    using (var s = File.Create(Path.Combine(Mods, f))) Zip(10).CopyTo(s);
            }
            Inst = new ModInstaller(ModLayout.InAppFolder(Mods, Path.Combine(Root, "app")), () => T0, 3);
        }
        public void Dispose() { try { Directory.Delete(Root, true); } catch { } }
    }

    public static void Register(TestRunner t)
    {
        t.Group("db.json игры (данные с Windows, 0.39.4)");

        t.Run("пустой db.json после первого запуска: формат 1.1, модов нет", () =>
        {
            var r = GameModDb.Read(Fixture("db-empty-0.39.4.json"));
            Assert.True(r.Exists, "есть");
            Assert.Eq(1.1, r.FormatVersion);
            Assert.Eq(0, r.Mods.Count);
            Assert.Eq(0, r.Warnings.Count);
        });

        t.Run("пять модов: имена, тип, включённость, размер, дата появления", () =>
        {
            var r = GameModDb.Read(Fixture("db-5mods-0.39.4.json"));
            Assert.Eq(5, r.Mods.Count);
            Assert.Eq(0, r.Warnings.Count);
            Assert.True(r.Mods.All(m => m.Active), "все включены: игра включает положенные вручную");
            var bmw = r.Find("bmw_m2_g87_inf.zip")!;
            Assert.Eq("BMW_M2_G87_inf.zip", bmw.FileName);
            Assert.Eq("vehicle", bmw.ModType);
            Assert.Eq(331770968L, bmw.FileSize);
            Assert.True(bmw.InModsRoot, "в корне mods");
            Assert.True(bmw.DateAdded > new DateTimeOffset(2026, 10, 1, 0, 0, 0, TimeSpan.Zero), "дата из секунд Unix");
            Assert.Eq("unknown", r.Find("TrafficReborn_Free_v1.7.0.0.zip")!.ModType);
            Assert.Eq("terrain", r.Find("MUDSTONE_VALLEY.zip")!.ModType);
            Assert.True(r.Find("нет.zip") is null, "чужого нет");
        });

        t.Run("выключенный в игре мод: только active = false", () =>
        {
            var on = GameModDb.Read(Fixture("db-5mods-0.39.4.json"));
            var off = GameModDb.Read(Fixture("db-5mods-bmw-off-0.39.4.json"));
            Assert.False(off.Find("BMW_M2_G87_inf.zip")!.Active, "выключен");
            Assert.Eq(4, off.Mods.Count(m => m.Active));
            Assert.True(on.Mods.Zip(off.Mods).All(p => p.First with { Active = true } == p.Second with { Active = true }), "остальное то же");
        });

        t.Run("мод из репозитория: Unique ID, версия на сайте, номер ресурса, страница", () =>
        {
            var m = GameModDb.Read(Fixture("db-5mods-0.39.4.json")).Find("mudstone_valley.zip")!;
            Assert.True(m.Repo is not null, "есть данные репозитория");
            Assert.Eq("M7P2C4JGW", m.Repo!.UniqueId);
            Assert.Eq(".901", m.Repo.Version);
            Assert.Eq(34443L, m.Repo.ResourceId);
            Assert.Eq("ATypicalWhiteGuy", m.Repo.Author);
            Assert.Eq("https://beamng.com/resources/34443/", m.Repo.PageUrl!.ToString());
            Assert.True(m.Repo.LastUpdate is not null, "дата обновления");
            Assert.True(GameModDb.Read(Fixture("db-5mods-0.39.4.json")).Find("BMW_M2_G87_inf.zip")!.Repo is null, "у мода не из репозитория нет");
        });

        t.Run("терпимость: мусор, не объект, записи без имени, нет файла", () =>
        {
            Assert.False(GameModDb.Read(Path.Combine(Path.GetTempPath(), "нет-" + Guid.NewGuid() + ".json")).Exists, "файла нет");
            Assert.Eq(1, GameModDb.Parse("{ не json").Warnings.Count);
            Assert.Eq(1, GameModDb.Parse("[1,2]").Warnings.Count);
            var r = GameModDb.Parse("""{"header":{"version":1.1},"mods":{"a":5,"b":{"active":false},"c":{"filename":"C.zip","active":"да","dateAdded":-5,"stat":{"filesize":"12"}}}}""");
            Assert.Eq(1, r.Mods.Count);
            Assert.Eq(2, r.Warnings.Count);
            Assert.True(r.Mods[0].Active, "непонятный active — включён, как у игры");
            Assert.True(r.Mods[0].DateAdded is null, "отрицательная дата отброшена");
            Assert.Eq(12L, r.Mods[0].FileSize);
        });

        t.Run("запись флажка: меняется только active, остальное и данные репозитория на месте, копия рядом", () =>
        {
            using var g = new Game("db-5mods-0.39.4.json");
            var before = GameModDb.Read(g.Db);
            var r = GameModDb.SetActive(g.Db, "w164_kenemation_2.7.zip", false, NotRunning);
            Assert.Eq(SetActiveOutcome.Done, r.Outcome);
            var after = GameModDb.Read(g.Db);
            Assert.False(after.Find("w164_kenemation_2.7.zip")!.Active, "выключен");
            Assert.True(before.Mods.Where(m => m.Key != "w164_kenemation_2.7").SequenceEqual(after.Mods.Where(m => m.Key != "w164_kenemation_2.7")), "остальные записи те же");
            Assert.Eq(before.Find("mudstone_valley.zip")!.Repo!, after.Find("mudstone_valley.zip")!.Repo!);
            Assert.True(File.Exists(g.Db + GameModDb.BackupSuffix), "копия прежнего");
            Assert.Eq(181320L, new FileInfo(g.Db + GameModDb.BackupSuffix).Length);
            var raw = File.ReadAllText(g.Db);
            Assert.True(raw.Contains("\"hashes\"") && raw.Contains("discord.gg"), "неизвестные поля и описание сохранены");
            Assert.False(File.ReadAllBytes(g.Db).Take(3).SequenceEqual(new byte[] { 0xEF, 0xBB, 0xBF }), "без BOM, как у игры");
            Assert.False(File.Exists(g.Db + ".bmm-tmp"), "временного нет");
        });

        t.Run("запись: игра запущена — не пишем; уже как надо — файл не трогаем; несколько сразу", () =>
        {
            using var g = new Game();
            var stamp = File.GetLastWriteTimeUtc(g.Db);
            Assert.Eq(SetActiveOutcome.GameRunning, GameModDb.SetActive(g.Db, "LXR_Sliva_S14_V1.1.zip", false, () => true).Outcome);
            Assert.Eq(SetActiveOutcome.Unchanged, GameModDb.SetActive(g.Db, "BMW_M2_G87_inf.zip", false, NotRunning).Outcome);
            Assert.Eq(stamp, File.GetLastWriteTimeUtc(g.Db));
            Assert.False(File.Exists(g.Db + GameModDb.BackupSuffix), "без изменений копии нет");
            var many = GameModDb.SetActive(g.Db, new Dictionary<string, bool> { ["BMW_M2_G87_inf.zip"] = true, ["lxr_sliva_s14_v1.1.zip"] = false, ["нового.zip"] = false }, NotRunning);
            Assert.Eq(SetActiveOutcome.Done, many.Outcome);
            Assert.True(many.NotInDb.SequenceEqual(new[] { "нового.zip" }), "неизвестный перечислен");
            var r = GameModDb.Read(g.Db);
            Assert.True(r.Find("BMW_M2_G87_inf.zip")!.Active && !r.Find("LXR_Sliva_S14_V1.1.zip")!.Active, "оба изменены");
        });

        t.Run("запись: нет db.json, мода нет в списке, файл испорчен — ничего не портим", () =>
        {
            using var g = new Game(fixture: null);
            Assert.Eq(SetActiveOutcome.NoDb, GameModDb.SetActive(g.Db, "a.zip", false, NotRunning).Outcome);
            File.WriteAllText(g.Db, File.ReadAllText(Fixture("db-empty-0.39.4.json")));
            Assert.Eq(SetActiveOutcome.NotInDb, GameModDb.SetActive(g.Db, "a.zip", false, NotRunning).Outcome);
            File.WriteAllText(g.Db, "{ испорчен");
            Assert.Eq(SetActiveOutcome.Unreadable, GameModDb.SetActive(g.Db, "a.zip", false, NotRunning).Outcome);
            Assert.Eq("{ испорчен", File.ReadAllText(g.Db));
        });

        t.Run("запись: игра успела изменить файл, пока мы читали, — не перезаписываем", () =>
        {
            using var g = new Game();
            var calls = 0;
            Func<bool> sneaky = () =>
            {
                if (++calls == 2) { File.AppendAllText(g.Db, " "); File.SetLastWriteTimeUtc(g.Db, DateTime.UtcNow.AddSeconds(5)); }
                return false;
            };
            var r = GameModDb.SetActive(g.Db, "LXR_Sliva_S14_V1.1.zip", false, sneaky);
            Assert.Eq(SetActiveOutcome.ChangedMeanwhile, r.Outcome);
            Assert.True(GameModDb.Read(g.Db).Find("LXR_Sliva_S14_V1.1.zip")!.Active, "не изменён");
        });

        t.Run("запись: игра запустилась, пока мы читали, — не пишем", () =>
        {
            using var g = new Game();
            var calls = 0;
            var r = GameModDb.SetActive(g.Db, "LXR_Sliva_S14_V1.1.zip", false, () => ++calls >= 2);
            Assert.Eq(SetActiveOutcome.GameRunning, r.Outcome);
            Assert.True(GameModDb.Read(g.Db).Find("LXR_Sliva_S14_V1.1.zip")!.Active, "не изменён");
            Assert.False(File.Exists(g.Db + GameModDb.BackupSuffix), "копии нет: ничего не писали");
        });

        t.Run("версия игры из лога и ini; папка пользователя из startup.ini", () =>
        {
            Assert.Eq("0.39.4.0", GameInfo.VersionFromLog("  0.00413|D|logging| -- Log started - v 0.39.4.0 - x64 - build 20972 - 2026-10-15 13:33:32 +0300 -----"));
            Assert.True(GameInfo.VersionFromLog("мусор") is null, "нет версии");
            Assert.Eq("0.39.4.0", GameInfo.VersionFromIni("[general]\nversion=0.39.4.0\n"));
            var ini = File.ReadAllText(Fixture("startup-0.39.4.ini"));
            Assert.True(GameInfo.UserRootFromStartupIniText(ini, @"E:\beam\game") is null, "пустой UserPath и закомментированный «.\\» не считаются");
            var abs = GameInfo.UserRootFromStartupIniText("[filesystem]\nUserPath = D:\\BeamNG-user\n", "/game");
            Assert.True(abs is not null && abs.Contains("BeamNG-user"), "абсолютный");
            var rel = GameInfo.UserRootFromStartupIniText("[filesystem]\r\nUserPath = user\r\n", Path.GetTempPath());
            Assert.Eq(Path.GetFullPath(Path.Combine(Path.GetTempPath(), "user")), rel);
            Assert.True(GameInfo.UserRootFromStartupIniText("[other]\nUserPath = X\n", "/g") is null, "другой раздел");
        });

        t.Group("Библиотека и db.json");

        t.Run("сверка: выключенный в игре мод в библиотеке выключен, файл на месте", () =>
        {
            using var g = new Game();
            GameDbSync.Reconcile(g.Store, g.Inst);
            Assert.Eq(5, g.Store.Mods().Count);
            Assert.False(g.Store.Mods().Single(m => m.FileName == "BMW_M2_G87_inf.zip").Enabled, "BMW выключен");
            Assert.Eq(4, g.Store.Mods().Count(m => m.Enabled));
            Assert.True(File.Exists(Path.Combine(g.Mods, "BMW_M2_G87_inf.zip")), "файл в mods");
        });

        t.Run("мод из репозитория: версия и название с сайта, привязка к beamng.com подтверждена", () =>
        {
            using var g = new Game();
            GameDbSync.Reconcile(g.Store, g.Inst);
            var db = GameModDb.Read(g.Db);
            Assert.Eq(1, GameDbSync.ApplyRepoInfo(g.Store, db));
            var m = g.Store.Mods().Single(x => x.FileName == "mudstone_valley.zip");
            Assert.Eq(".901", m.Version);
            Assert.Eq("Mudstone Valley Ultra 4 & Off Road Racing", m.Name);
            var b = g.Store.Bindings().Single();
            Assert.True(b.Confirmed && b.Source == "beamng" && b.SourceId == "34443", "привязка");
            Assert.Eq(1, BeamModManager.Core.Library.UpdateCheckService.Tracked(g.Store).Count);
            Assert.Eq(0, GameDbSync.ApplyRepoInfo(g.Store, db)); // повтор ничего не добавляет
        });

        t.Run("мод из репозитория: отказ «Не проверять» и своё название пользователя не трогаются", () =>
        {
            using var g = new Game();
            GameDbSync.Reconcile(g.Store, g.Inst);
            var m = g.Store.Mods().Single(x => x.FileName == "mudstone_valley.zip");
            g.Store.UpsertMod(m with { Name = "Моя трасса" });
            BindingActions.DeclineChecking(g.Store, m.Id);
            Assert.Eq(0, GameDbSync.ApplyRepoInfo(g.Store, GameModDb.Read(g.Db)));
            Assert.Eq("Моя трасса", g.Store.GetMod(m.Id)!.Name);
            Assert.Eq(0, g.Store.Bindings().Count);
        });

        t.Run("выключить и включить: флажком в db.json, файл не двигается", () =>
        {
            using var g = new Game();
            var off = GameDbSync.SetEnabled(g.Inst, "LXR_Sliva_S14_V1.1.zip", false, NotRunning);
            Assert.True(off.Success && off.Way == ToggleWay.GameDb, off.Message);
            Assert.False(GameModDb.Read(g.Db).Find("LXR_Sliva_S14_V1.1.zip")!.Active, "в db.json выключен");
            Assert.True(File.Exists(Path.Combine(g.Mods, "LXR_Sliva_S14_V1.1.zip")), "файл на месте");
            var on = GameDbSync.SetEnabled(g.Inst, "BMW_M2_G87_inf.zip", true, NotRunning);
            Assert.True(on.Success && on.Way == ToggleWay.GameDb, on.Message);
            Assert.True(GameModDb.Read(g.Db).Find("BMW_M2_G87_inf.zip")!.Active, "включён");
        });

        t.Run("выключить, пока игра запущена: отказ с понятной причиной, ничего не меняется", () =>
        {
            using var g = new Game();
            var r = GameDbSync.SetEnabled(g.Inst, "LXR_Sliva_S14_V1.1.zip", false, () => true);
            Assert.False(r.Success, "отказ");
            Assert.True(r.Message.Contains("Закройте игру"), r.Message);
            Assert.True(GameModDb.Read(g.Db).Find("LXR_Sliva_S14_V1.1.zip")!.Active, "не изменён");
        });

        t.Run("новый мод, которого игра ещё не видела: выключение переносом, включение возвращает и ничего не ломает", () =>
        {
            using var g = new Game();
            Assert.True(g.Inst.Install(Zip(30), "new_car.zip").Success, "установлен");
            var off = GameDbSync.SetEnabled(g.Inst, "new_car.zip", false, NotRunning);
            Assert.True(off.Success && off.Way == ToggleWay.MoveFile, off.Message);
            Assert.False(File.Exists(Path.Combine(g.Mods, "new_car.zip")), "убран из mods");
            GameDbSync.Reconcile(g.Store, g.Inst);
            Assert.False(g.Store.Mods().Single(m => m.FileName == "new_car.zip").Enabled, "в библиотеке выключен");
            var on = GameDbSync.SetEnabled(g.Inst, "new_car.zip", true, NotRunning);
            Assert.True(on.Success && on.Way == ToggleWay.MoveFile, on.Message);
            Assert.True(File.Exists(Path.Combine(g.Mods, "new_car.zip")), "вернулся");
            var on2 = GameDbSync.SetEnabled(g.Inst, "new_car.zip", true, NotRunning);
            Assert.True(on2.Success && on2.Way == ToggleWay.None, "включён и так: игра включит сама");
        });

        t.Run("обновление выключенного в игре мода под новым именем: новая версия тоже выключена", () =>
        {
            using var g = new Game();
            var before = GameModDb.Read(g.Db);
            var u = g.Inst.Update("BMW_M2_G87_inf.zip", Zip(40), "BMW_M2_G87_inf_v2.zip");
            Assert.True(u.Success, u.Install.Message ?? "");
            var k = GameDbSync.KeepDisabledAfterUpdate(g.Inst, before, "BMW_M2_G87_inf.zip", "BMW_M2_G87_inf_v2.zip");
            Assert.True(k.Success && k.Way == ToggleWay.MoveFile, k.Message);
            Assert.False(File.Exists(Path.Combine(g.Mods, "BMW_M2_G87_inf_v2.zip")), "не в mods");
            var k2 = GameDbSync.KeepDisabledAfterUpdate(g.Inst, before, "LXR_Sliva_S14_V1.1.zip", "LXR_Sliva_S14_V1.2.zip");
            Assert.True(k2.Way == ToggleWay.None, "включённую не трогаем");
        });
    }
}
