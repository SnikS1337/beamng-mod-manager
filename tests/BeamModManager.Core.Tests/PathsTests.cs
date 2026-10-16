using System.IO.Compression;
using System.Text;
using BeamModManager.Core.Game;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Previews;
using BeamModManager.Core.Safety;
using BeamModManager.Core.Settings;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Previews;

namespace BeamModManager.Core.Tests;

/// <summary>
/// Необычные папки Windows: игра в Steam на другом диске с перенесённой папкой пользователя, обновление игры, русские
/// буквы, пробелы и значки в путях и именах модов, пути длиннее 260 символов, файлы «только в облаке», пропавший диск.
/// На сборочной машине эти же проверки идут и на настоящей Windows.
/// </summary>
public static class PathsTests
{
    private sealed class Tmp : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "bmm-paths-" + Guid.NewGuid().ToString("N")[..8]);
        public Tmp() => Directory.CreateDirectory(Root);
        public string Dir(params string[] parts)
        {
            var d = Path.Combine(new[] { Root }.Concat(parts).ToArray());
            Directory.CreateDirectory(d);
            return d;
        }
        public void Dispose() { try { Directory.Delete(Root, true); } catch (IOException) { } catch (UnauthorizedAccessException) { } }
    }

    private static string Esc(string path) => path.Replace("\\", "\\\\");

    private static string Vdf(params (string Path, bool HasGame)[] libs)
    {
        var sb = new StringBuilder("\"libraryfolders\"\n{\n");
        for (var i = 0; i < libs.Length; i++)
        {
            sb.Append($"\t\"{i}\"\n\t{{\n\t\t\"path\"\t\t\"{Esc(libs[i].Path)}\"\n\t\t\"label\"\t\t\"\"\n\t\t\"contentid\"\t\t\"123\"\n\t\t\"apps\"\n\t\t{{\n");
            sb.Append("\t\t\t\"228980\"\t\t\"1000\"\n");
            if (libs[i].HasGame) sb.Append("\t\t\t\"284160\"\t\t\"40000000000\"\n");
            sb.Append("\t\t}\n\t}\n");
        }
        return sb.Append("}\n").ToString();
    }

    private static byte[] ZipWith(params (string Name, byte[] Data)[] files)
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true))
            foreach (var (n, d) in files)
                using (var s = z.CreateEntry(n).Open()) s.Write(d);
        return ms.ToArray();
    }

    private static ModInstaller Installer(string mods, string app) =>
        new(ModLayout.InAppFolder(mods, app), () => DateTimeOffset.UtcNow, () => 3);

    private const string Db = """
    {
      "header":{"version":1.1},
      "mods":{
        "машина bmw e34 (v3.3) [final] & co":{"active":true,"dirname":"/mods/","filename":"Машина BMW E34 (v3.3) [final] & co.zip","fullpath":"/mods/Машина BMW E34 (v3.3) [final] & co.zip","modType":"vehicle","modname":"машина bmw e34 (v3.3) [final] & co"},
        "100% drift #1 🚗":{"active":true,"dirname":"/mods/","filename":"100% Drift #1 🚗.zip","fullpath":"/mods/100% Drift #1 🚗.zip","modType":"vehicle","modname":"100% drift #1 🚗"}
      }
    }
    """;

    private static readonly string[] OddNames =
    {
        "Машина BMW E34 (v3.3) [final] & co.zip",
        "100% Drift #1 🚗.zip",
        "Trailer   with  spaces.zip",
        "ÆØÅ — ẞtraße, 中文, عربي.zip",
        "a'b`c~d!e@f$g^h=i+j{k}l;m,n.zip",
    };

    public static void Register(TestRunner t)
    {
        t.Group("Папки: Steam и перенесённая папка пользователя");

        t.Run("libraryfolders.vdf: библиотеки с BeamNG первыми, «\\\\» в путях раскрыты, русские буквы целы", () =>
        {
            var libs = SteamLibraries.Parse(Vdf((@"C:\Program Files (x86)\Steam", false), (@"D:\Игры\Steam Library", true), (@"E:\SteamLibrary", false)));
            Assert.Eq(3, libs.Count);
            Assert.Eq(@"D:\Игры\Steam Library", libs[0]);
            Assert.Eq(@"C:\Program Files (x86)\Steam", libs[1]);
        });

        t.Run("старый вид файла («1» → путь строкой) и комментарии тоже читаются", () =>
        {
            var old = "// старый формат\n\"LibraryFolders\"\n{\n\t\"TimeNextStatsReport\"\t\t\"1700000000\"\n\t\"ContentStatsID\"\t\t\"-1\"\n\t\"1\"\t\t\"D:\\\\SteamLibrary\"\n\t\"2\"\t\t\"F:\\\\Мои игры\"\n}\n";
            var libs = SteamLibraries.Parse(old);
            Assert.Eq(2, libs.Count);
            Assert.Eq(@"D:\SteamLibrary", libs[0]);
            Assert.Eq(@"F:\Мои игры", libs[1]);
        });

        t.Run("испорченный файл — пустой список, без исключений", () =>
        {
            string[] junk =
            {
                "", "   ", "{", "}", "\"libraryfolders\"", "\"libraryfolders\" {", "\"libraryfolders\" { \"0\" { \"path\" \"C:\\x", "\"a\" } }",
                "\0\u0001binary\u00ff", "просто текст", string.Concat(Enumerable.Repeat("\"k\" {", 200)) + string.Concat(Enumerable.Repeat("}", 200)),
                "\"libraryfolders\" { \"0\" { \"path\" \"\" } }",
            };
            foreach (var j in junk) Assert.Eq(0, SteamLibraries.Parse(j).Count);
        });

        t.Run("папки игры по Steam: библиотека на другом диске с русскими буквами; библиотека без игры не в счёт", () =>
        {
            using var tmp = new Tmp();
            var steam = tmp.Dir("Steam");
            var lib = tmp.Dir("Диск D", "Игры Steam");
            var empty = tmp.Dir("Пустая библиотека");
            var game = tmp.Dir("Диск D", "Игры Steam", "steamapps", "common", "BeamNG.drive");
            File.WriteAllText(Path.Combine(tmp.Dir("Steam", "steamapps"), "libraryfolders.vdf"), Vdf((steam, false), (empty, false), (lib, true)));
            var dirs = SteamLibraries.GameDirs(steam);
            Assert.Eq(1, dirs.Count);
            Assert.Eq(Path.GetFullPath(game), dirs[0]);
            Assert.Eq(0, SteamLibraries.GameDirs(tmp.Dir("Нет Steam")).Count);
        });

        t.Run("автопоиск: папка пользователя из startup.ini игры важнее стандартной", () =>
        {
            using var tmp = new Tmp();
            var game = tmp.Dir("D", "SteamLibrary", "steamapps", "common", "BeamNG.drive");
            var custom = tmp.Dir("D", "BeamNG пользователь (мой)");
            var customMods = tmp.Dir("D", "BeamNG пользователь (мой)", "current", "mods");
            File.WriteAllText(Path.Combine(game, "startup.ini"), $"[filesystem]\nUserPath = \"{custom}\"\n");
            var std = tmp.Dir("C", "BeamNG", "BeamNG.drive");
            tmp.Dir("C", "BeamNG", "BeamNG.drive", "current", "mods");
            Assert.Eq(customMods, GameFolders.AutoDetect(new[] { game }, new[] { std }));
        });

        t.Run("автопоиск: UserPath закомментирован, пуст или указывает в пустоту — стандартное место", () =>
        {
            using var tmp = new Tmp();
            var game = tmp.Dir("game");
            var std = tmp.Dir("std");
            var stdMods = tmp.Dir("std", "current", "mods");
            foreach (var ini in new[] { "[filesystem]\n;UserPath = D:\\x\n", "[filesystem]\nUserPath =\n", $"[filesystem]\nUserPath = {Path.Combine(tmp.Root, "нет такой")}\n", "[other]\nUserPath = D:\\x\n", "" })
            {
                File.WriteAllText(Path.Combine(game, "startup.ini"), ini);
                Assert.Eq(stdMods, GameFolders.AutoDetect(new[] { game }, new[] { std }));
            }
            File.Delete(Path.Combine(game, "startup.ini"));
            Assert.Eq(stdMods, GameFolders.AutoDetect(new[] { game, Path.Combine(tmp.Root, "нет игры") }, new[] { Path.Combine(tmp.Root, "нет"), std }));
            Assert.True(GameFolders.AutoDetect(Array.Empty<string>(), new[] { Path.Combine(tmp.Root, "нет") }) is null, "ничего нет");
        });

        t.Run("автопоиск: относительный UserPath считается от папки игры", () =>
        {
            using var tmp = new Tmp();
            var game = tmp.Dir("Steam", "BeamNG.drive");
            var mods = tmp.Dir("Steam", "Пользователь BeamNG", "current", "mods");
            File.WriteAllText(Path.Combine(game, "startup.ini"), "[filesystem]\r\nUserPath = ../Пользователь BeamNG\r\n");
            Assert.Eq(Path.GetFullPath(mods), Path.GetFullPath(GameFolders.AutoDetect(new[] { game }, Array.Empty<string>())!));
        });

        t.Run("старое устройство папок (по версиям): берётся самая новая по числам — 0.100 новее 0.39", () =>
        {
            using var tmp = new Tmp();
            var root = tmp.Dir("BeamNG.drive");
            foreach (var v in new[] { "0.38", "0.39", "0.100", "0.9" }) tmp.Dir("BeamNG.drive", v, "mods");
            tmp.Dir("BeamNG.drive", "mods_backup", "mods");
            tmp.Dir("BeamNG.drive", "0.200"); // без mods
            Assert.Eq(Path.Combine(root, "0.100", "mods"), GameFolders.AutoDetect(Array.Empty<string>(), new[] { root }));
        });

        t.Group("Папки: игра обновилась");

        t.Run("выбрана mods старой версии, рядом current\\mods — подсказываем current", () =>
        {
            using var tmp = new Tmp();
            var old = tmp.Dir("BeamNG.drive", "0.36", "mods");
            var cur = tmp.Dir("BeamNG.drive", "current", "mods");
            Assert.Eq(cur, GameFolders.NewerModsFolder(old));
            Assert.Eq(cur, GameFolders.NewerModsFolder(old + Path.DirectorySeparatorChar));
        });

        t.Run("рядом версии новее: самая новая с папкой mods; новая без mods не в счёт", () =>
        {
            using var tmp = new Tmp();
            var old = tmp.Dir("U", "0.38", "mods");
            tmp.Dir("U", "0.39", "mods");
            var newest = tmp.Dir("U", "0.100", "mods");
            tmp.Dir("U", "0.200");
            tmp.Dir("U", "0.37", "mods");
            Assert.Eq(newest, GameFolders.NewerModsFolder(old));
            Assert.True(GameFolders.NewerModsFolder(newest) is null, "самая новая — подсказывать нечего");
        });

        t.Run("current, просто mods, не mods, несуществующий и странный путь — подсказки нет", () =>
        {
            using var tmp = new Tmp();
            var cur = tmp.Dir("U", "current", "mods");
            tmp.Dir("U", "0.39", "mods");
            Assert.True(GameFolders.NewerModsFolder(cur) is null, "current");
            Assert.True(GameFolders.NewerModsFolder(tmp.Dir("X", "mods")) is null, "просто mods");
            Assert.True(GameFolders.NewerModsFolder(tmp.Dir("U", "0.39")) is null, "не mods");
            Assert.True(GameFolders.NewerModsFolder(Path.Combine(tmp.Root, "нет", "0.1", "mods")) is null, "нет папки");
            Assert.True(GameFolders.NewerModsFolder("") is null, "пусто");
            Assert.True(GameFolders.NewerModsFolder("mods") is null, "относительный");
        });

        t.Group("Папки: пути с русскими буквами, пробелами и кавычками");

        t.Run("путь из проводника в кавычках и с %переменной% (значение по-русски)", () =>
        {
            using var tmp = new Tmp();
            var mods = tmp.Dir("Пользователь Иван", "BeamNG", "current", "mods");
            Environment.SetEnvironmentVariable("BMM_TEST_HOME", Path.Combine(tmp.Root, "Пользователь Иван"));
            try
            {
                Assert.Eq(mods, GameFolders.CleanPath($"\"{mods}\""));
                Assert.Eq(mods, GameFolders.CleanPath($"  '{mods}'  "));
                var viaVar = GameFolders.Resolve(Path.Combine("%BMM_TEST_HOME%", "BeamNG"));
                Assert.True(viaVar.Ok, viaVar.Message);
                Assert.Eq(mods, viaVar.ModsDir);
            }
            finally { Environment.SetEnvironmentVariable("BMM_TEST_HOME", null); }
        });

        t.Run("папка mods с косой чертой в конце, «Mods» с большой буквы, файл вместо папки, неполный путь", () =>
        {
            using var tmp = new Tmp();
            var mods = tmp.Dir("Мои игры", "Mods");
            Assert.True(GameFolders.Resolve(mods + Path.DirectorySeparatorChar).Ok, "с чертой");
            Assert.True(GameFolders.Resolve(mods).Ok, "Mods");
            var file = Path.Combine(tmp.Root, "Мои игры", "mods.zip");
            File.WriteAllText(file, "x");
            Assert.Eq(FolderProblem.NotADirectory, GameFolders.Resolve(file).Problem);
            Assert.Eq(FolderProblem.NotFound, GameFolders.Resolve("Мои игры/mods").Problem);
        });

        t.Group("Папки: странные имена модов");

        t.Run("имена с русскими буквами, скобками, %, #, значками и пробелами видны в списке", () =>
        {
            using var tmp = new Tmp();
            var mods = tmp.Dir("Иван Петров", "BeamNG", "current", "mods");
            foreach (var n in OddNames) File.WriteAllBytes(Path.Combine(mods, n), new byte[] { 1, 2, 3 });
            File.WriteAllBytes(Path.Combine(mods, "не мод.zipx"), new byte[1]);
            File.WriteAllBytes(Path.Combine(tmp.Dir("Иван Петров", "BeamNG", "current", "mods", "repo"), "Репо мод.zip"), new byte[5]);
            File.WriteAllBytes(Path.Combine(tmp.Dir("Иван Петров", "BeamNG", "current", "mods", "unpacked", "x"), "внутри.zip"), new byte[5]);
            var inst = Installer(mods, tmp.Dir("app"));
            var all = inst.ScanDetailed();
            foreach (var n in OddNames) Assert.True(all.Any(d => d.FileName == n && d.Enabled && d.SizeBytes == 3), "виден: " + n);
            Assert.True(all.Any(d => d.FileName == "Репо мод.zip" && d.SubFolder == "repo"), "из repo");
            Assert.False(all.Any(d => d.FileName.EndsWith(".zipx") || d.FileName == "внутри.zip"), "лишнего нет");
            foreach (var n in OddNames) Assert.Eq(n, ModInstaller.SafeFileName(n));
        });

        t.Run("db.json с русскими именами и значками: выключение пишет UTF-8 как есть, игра прочтёт то же имя", () =>
        {
            using var tmp = new Tmp();
            var mods = tmp.Dir("Иван", "current", "mods");
            File.WriteAllText(Path.Combine(mods, "db.json"), Db, new UTF8Encoding(false));
            foreach (var n in OddNames.Take(2)) File.WriteAllBytes(Path.Combine(mods, n), new byte[] { 1 });
            var inst = Installer(mods, tmp.Dir("app"));
            var r = GameDbSync.SetEnabled(inst, OddNames[0], false, () => false);
            Assert.True(r.Success, r.Message);
            Assert.Eq(ToggleWay.GameDb, r.Way);
            var text = File.ReadAllText(Path.Combine(mods, "db.json"), Encoding.UTF8);
            Assert.True(text.Contains("Машина BMW E34") && !text.Contains("\\u041C"), "кириллица не экранирована");
            Assert.True(text.Contains("🚗") || text.Contains("\\uD83D"), "значок на месте");
            var db = GameModDb.Read(Path.Combine(mods, "db.json"));
            Assert.False(db.Find(OddNames[0])!.Active, "выключен");
            Assert.True(db.Find(OddNames[1])!.Active, "второй не тронут");
            Assert.True(GameDbSync.SetEnabled(inst, OddNames[1], false, () => false).Success, "значок");
            Assert.False(GameModDb.Read(Path.Combine(mods, "db.json")).Find(OddNames[1])!.Active, "выключен по имени со значком");
        });

        t.Run("мод, которого игра не видела: выключение переносом и обратно — имя не портится", () =>
        {
            using var tmp = new Tmp();
            var mods = tmp.Dir("Мои документы", "mods");
            var inst = Installer(mods, tmp.Dir("Приложение"));
            foreach (var n in OddNames)
            {
                File.WriteAllBytes(Path.Combine(mods, n), new byte[] { 7 });
                var off = GameDbSync.SetEnabled(inst, n, false, () => false);
                Assert.True(off.Success && off.Way == ToggleWay.MoveFile, off.Message);
                Assert.False(File.Exists(Path.Combine(mods, n)), "убран");
                Assert.True(GameDbSync.SetEnabled(inst, n, true, () => false).Success, "вернулся");
                Assert.True(File.Exists(Path.Combine(mods, n)), "на месте: " + n);
            }
        });

        t.Group("Папки: пути длиннее 260 символов");

        static string LongDir(Tmp tmp)
        {
            var parts = new List<string> { "Очень длинная папка пользователя с пробелами и русскими буквами" };
            for (var i = 0; i < 4; i++) parts.Add($"вложенная папка номер {i} с длинным-длинным названием для проверки");
            parts.AddRange(new[] { "BeamNG", "current", "mods" });
            var d = tmp.Dir(parts.ToArray());
            Assert.True(d.Length > 300, $"путь {d.Length} символов");
            return d;
        }

        t.Run("папка mods глубже 260 символов: список, выключение через db.json и переносом, картинка", () =>
        {
            using var tmp = new Tmp();
            var mods = LongDir(tmp);
            var longName = new string('Ж', 120) + " мод.zip"; // 240 байт: в Linux имя файла до 255 байт, в Windows до 255 знаков
            var zip = ZipWith(("vehicles/car/default.png", PreviewTests.Png(640, 360)), ("info.json", "{}"u8.ToArray()));
            File.WriteAllBytes(Path.Combine(mods, longName), zip);
            File.WriteAllBytes(Path.Combine(mods, "short.zip"), zip);
            File.WriteAllText(Path.Combine(mods, "db.json"), """{"header":{"version":1.1},"mods":{"short":{"active":true,"filename":"short.zip","fullpath":"/mods/short.zip","dirname":"/mods/"}}}""");
            var inst = Installer(mods, tmp.Dir("app"));
            Assert.True(Path.Combine(mods, longName).Length > 450, "файл глубже 450 символов");
            Assert.Eq(2, inst.ScanDetailed().Count);
            Assert.True(GameDbSync.SetEnabled(inst, "short.zip", false, () => false).Success, "через db.json");
            Assert.False(GameModDb.Read(GameModDb.PathIn(mods)).Find("short.zip")!.Active, "записано");
            var off = GameDbSync.SetEnabled(inst, longName, false, () => false);
            Assert.True(off.Success, off.Message);
            Assert.True(GameDbSync.SetEnabled(inst, longName, true, () => false).Success, "вернулся");
            var pick = PreviewPicker.Pick(Path.Combine(mods, longName));
            Assert.True(pick.Found, pick.Warning ?? "картинка");
        });

        t.Run("библиотека и копии модов в длинной папке приложения сохраняются и читаются", () =>
        {
            using var tmp = new Tmp();
            var app = LongDir(tmp);
            var store = new JsonLibraryStore(Path.Combine(app, "library.json"));
            store.UpsertMod(new InstalledMod("m", "Мод", "Мод.zip", "1.0", 10, DateTimeOffset.UtcNow));
            store.Flush();
            Assert.Eq("Мод", new JsonLibraryStore(Path.Combine(app, "library.json")).GetMod("m")!.Name);
            var cache = new PreviewCache(Path.Combine(app, "previews"));
            var zipPath = Path.Combine(app, "a.zip");
            File.WriteAllBytes(zipPath, ZipWith(("vehicles/car/default.png", PreviewTests.Png(640, 360))));
            var loader = new PreviewLoader(cache, b => b);
            Assert.True(loader.Ensure(zipPath) is { } p && File.Exists(p), "картинка в кэше");
        });

        t.Group("Папки: облако и пропавший диск");

        t.Run("файл «только в облаке» узнаётся по признакам OneDrive", () =>
        {
            Assert.False(CloudFiles.IsCloudOnly(FileAttributes.Normal), "обычный");
            Assert.False(CloudFiles.IsCloudOnly(FileAttributes.Archive | FileAttributes.ReadOnly | FileAttributes.ReparsePoint), "закреплённый (уже скачан)");
            Assert.True(CloudFiles.IsCloudOnly(FileAttributes.Offline), "офлайн");
            Assert.True(CloudFiles.IsCloudOnly((FileAttributes)0x400000 | FileAttributes.Archive), "по требованию");
            Assert.True(CloudFiles.IsCloudOnly((FileAttributes)0x40000), "при открытии");
            Assert.False(CloudFiles.IsCloudOnly(Path.Combine(Path.GetTempPath(), "нет-такого-файла.zip")), "нет файла — не облако");
        });

        t.Run("архив в облаке: картинку не достаём (не качаем мод) и не запоминаем «картинок нет»", () =>
        {
            using var tmp = new Tmp();
            var zip = Path.Combine(tmp.Root, "облачный мод.zip");
            File.WriteAllBytes(zip, ZipWith(("vehicles/car/default.png", PreviewTests.Png(640, 360))));
            var cache = new PreviewCache(tmp.Dir("previews"));
            var loader = new PreviewLoader(cache, b => b);
            if (OperatingSystem.IsWindows())
            {
                File.SetAttributes(zip, FileAttributes.Offline);
                Assert.True(PreviewPicker.Pick(zip).Warning!.Contains("облаке"), "облако");
                Assert.True(loader.Ensure(zip) is null, "без картинки");
                Assert.Eq(CacheState.Missing, cache.State(ArchiveStamp.Of(zip)));
                File.SetAttributes(zip, FileAttributes.Normal); // скачали
            }
            Assert.True(loader.Ensure(zip) is not null, "скачанный — с картинкой");
        });

        t.Run("папка mods пропала после запуска (диск отключили): «не нашли папку», вернулась — моды снова видны", () =>
        {
            using var tmp = new Tmp();
            var mods = tmp.Dir("Флешка", "mods");
            File.WriteAllBytes(Path.Combine(mods, "Мод.zip"), ZipWith(("info.json", "{}"u8.ToArray())));
            var backend = new LibraryGarageBackend(new InMemoryLibraryStore(), Installer(mods, tmp.Dir("app")), new PreviewCache(tmp.Dir("p")), () => false);
            Assert.Eq(1, backend.Load().Mods.Count);
            Directory.Move(mods, mods + "-нет");
            var gone = backend.Load();
            Assert.False(gone.FolderAvailable, "папки нет");
            Directory.Move(mods + "-нет", mods);
            Assert.Eq(1, backend.Load().Mods.Count);
        });
    }
}
