using System.IO.Compression;
using System.Text;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Packs;
using BeamModManager.Core.Previews;
using BeamModManager.Core.Text;
using BeamModManager.Core.Updates;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Previews;

namespace BeamModManager.Core.Tests;

/// <summary>Название мода на карточке (2026-10-18): репозиторий → info.json → сайт → имя файла; имя файла — в панели.</summary>
public static class ModTitleTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 18, 12, 0, 0, TimeSpan.Zero);

    private static byte[] Zip(params (string Name, string Text)[] entries)
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true))
            foreach (var (n, txt) in entries)
            {
                using var w = new StreamWriter(z.CreateEntry(n).Open(), Encoding.UTF8);
                w.Write(txt);
            }
        return ms.ToArray();
    }

    private static byte[] Civic() => Zip(("vehicles/civic_ek9/info.json", "{\"Brand\":\"Honda\",\"Name\":\"Civic EK9\",}"),
        ("vehicles/civic_ek9/civic.jbeam", "{}"));

    private sealed class Temp : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "bmm-titles-" + Guid.NewGuid().ToString("N"));
        public Temp() => Directory.CreateDirectory(Root);
        public string Put(string name, byte[] data) { var p = Path.Combine(Root, name); File.WriteAllBytes(p, data); return p; }
        public void Dispose() { try { Directory.Delete(Root, true); } catch { } }
    }

    private static ArchiveTitle TitleOf(byte[] zip)
    {
        using var d = new Temp();
        return ArchiveOverviewReader.TitleOf(d.Put("m.zip", zip));
    }

    /// <summary>Папка mods, библиотека, кэш картинок и названий, гараж — как в программе.</summary>
    private sealed class Folder : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "bmm-titles-" + Guid.NewGuid().ToString("N"));
        public readonly InMemoryLibraryStore Store = new();
        public readonly ModInstaller Inst;
        public readonly PreviewCache Cache;
        public readonly PreviewLoader Loader;
        public GarageViewModel Vm;
        public DateTimeOffset Now = T0;
        public string Mods => Path.Combine(Root, "mods");

        public Folder(params (string File, byte[] Data)[] files)
        {
            Directory.CreateDirectory(Mods);
            foreach (var (f, d) in files) File.WriteAllBytes(Path.Combine(Mods, f), d);
            Inst = new ModInstaller(ModLayout.InAppFolder(Mods, Path.Combine(Root, "app")), () => Now = Now.AddSeconds(1), 3);
            Cache = new PreviewCache(Path.Combine(Root, "app", "previews"));
            Loader = new PreviewLoader(Cache, _ => null);
            Vm = Open(Cache);
        }

        /// <summary>Гараж заново (как при следующем запуске), с этим или новым кэшем.</summary>
        public GarageViewModel Open(PreviewCache cache)
        {
            var vm = new GarageViewModel(new LibraryGarageBackend(Store, Inst, cache, () => false), () => T0);
            vm.Refresh();
            return vm;
        }

        /// <summary>То, что делает окно: фон читает архивы карточек и сообщает гаражу названия.</summary>
        public void ReadTitlesInBackground()
        {
            var todo = Vm.AllCards.Where(c => !c.Data.ArchiveTitleKnown && c.Data.ArchivePath is not null).Select(c => (c.Id, c.Data.ArchivePath!)).ToList();
            var got = new List<(string, string?)>();
            Loader.FillAsync(todo, (_, _) => { }, default, (id, t) => { lock (got) got.Add((id, t)); }).Wait();
            foreach (var (id, t) in got) Vm.ArchiveTitleRead(id, t);
        }

        public ModCard Card(string file) => Vm.AllCards.Single(c => c.FileName == file);
        public void Dispose() { try { Directory.Delete(Root, true); } catch { } }
    }

    public static void Register(TestRunner t)
    {
        t.Group("Название мода: из архива");

        t.Run("одна своя машина — марка и модель из её info.json, в том числе в обёрточной папке и рядом с vehicles/common", () =>
        {
            Assert.True(TitleOf(Civic()) is { Known: true, Title: "Honda Civic EK9" }, "простая");
            Assert.Eq("Honda Civic EK9", TitleOf(Zip(("civic/vehicles/civic_ek9/info.json", "{\"Brand\":\"Honda\",\"Name\":\"Honda Civic EK9\"}"),
                ("civic/vehicles/civic_ek9/a.jbeam", "{}"))).Title, "в обёртке, марка уже в имени — не повторяется");
            Assert.Eq("Hirochi Raptor", TitleOf(Zip(("vehicles/raptor/info.json", "{\"Brand\":\"Hirochi\",\"Name\":\"Raptor\"}"),
                ("vehicles/common/wheels.jbeam", "{}"))).Title, "common — не машина");
        });

        t.Run("своя карта — её title; ключ перевода игры — не название", () =>
        {
            Assert.Eq("Mudstone Valley", TitleOf(Zip(("levels/mudstone/info.json", "{\"title\":\"Mudstone Valley\"}"), ("levels/mudstone/main.level.json", "{}"))).Title);
            Assert.True(TitleOf(Zip(("levels/mudstone/info.json", "{\"title\":\"levels.mudstone.info.title\"}"))) is { Known: true, Title: null }, "ключ перевода");
        });

        t.Run("покраска и тюнинг стандартной машины или карты, пак, мод без info.json — названия нет, но архив прочитан", () =>
        {
            Assert.True(TitleOf(Zip(("vehicles/etk800/info.json", "{\"Brand\":\"ETK\",\"Name\":\"800 Series\"}"), ("vehicles/etk800/skin.dds", "x"))) is { Known: true, Title: null }, "покраска ETK 800");
            Assert.True(TitleOf(Zip(("vehicles/Pickup/info.json", "{\"Name\":\"D-Series\"}"))) is { Known: true, Title: null }, "регистр папки не важен");
            Assert.True(TitleOf(Zip(("levels/italy/info.json", "{\"title\":\"Italy\"}"))) is { Known: true, Title: null }, "правка стандартной карты");
            Assert.True(TitleOf(Zip(("vehicles/a/info.json", "{\"Name\":\"A\"}"), ("vehicles/b/info.json", "{\"Name\":\"B\"}"))) is { Known: true, Title: null }, "пак из двух машин");
            Assert.True(TitleOf(Zip(("vehicles/a/info.json", "{\"Name\":\"A\"}"), ("levels/l/info.json", "{\"title\":\"L\"}"))) is { Known: true, Title: null }, "машина и карта");
            Assert.True(TitleOf(Zip(("vehicles/mycar/mycar.jbeam", "{}"))) is { Known: true, Title: null }, "без info.json — не имя папки");
            Assert.True(TitleOf(Zip(("vehicles/mycar/info.json", "{ это не json"))) is { Known: true, Title: null }, "битый info.json");
            Assert.True(TitleOf(Zip(("vehicles/mycar/info.json", "{\"Name\":\"" + new string('x', 120) + "\"}"))) is { Known: true, Title: null }, "слишком длинное");
        });

        t.Run("не zip — прочитан, названия нет; файла нет — не прочитан, спросим ещё раз", () =>
        {
            Assert.True(TitleOf(Encoding.UTF8.GetBytes("не zip")) is { Known: true, Title: null }, "не zip");
            Assert.True(ArchiveOverviewReader.TitleOf(Path.Combine(Path.GetTempPath(), "нет-" + Guid.NewGuid().ToString("N") + ".zip")) is { Known: false }, "нет файла");
        });

        t.Group("Название мода: какое показывать");

        t.Run("порядок: репозиторий → info.json → сайт → имя файла; пустые и слишком длинные пропускаются", () =>
        {
            Assert.Eq("Repo", ModTitles.Choose("Repo", "Info", "Site", "file"));
            Assert.Eq("Info", ModTitles.Choose(null, "Info", "Site", "file"));
            Assert.Eq("Info", ModTitles.Choose("  ", "Info", "Site", "file"));
            Assert.Eq("Site", ModTitles.Choose(null, null, "Site", "file"));
            Assert.Eq("file", ModTitles.Choose(null, null, null, "file"));
            Assert.Eq("file", ModTitles.Choose(null, new string('x', 81), null, "file"));
        });

        t.Run("заголовок сайта без «мод … для BeamNG.drive», версий и метки платного", () =>
        {
            Assert.Eq("BMW E34", ModTitles.Site("МОД BMW E34 v1.2 (0.32) ДЛЯ BEAMNG.DRIVE"));
            Assert.Eq("Ibishu Miramar Dirt Race", ModTitles.Site("Ibishu Miramar Dirt Race [0.9.0]"));
            Assert.Eq("Lada Niva 4x4", ModTitles.Site("Lada Niva 4x4 версия 3.3 для BeamNG.drive (v0.39.x)"));
            Assert.Eq("Ford Crown Victoria V8", ModTitles.Site("Ford Crown Victoria V8 [PAID]"));
            Assert.True(ModTitles.Site("  ") is null && ModTitles.Site(null) is null, "пусто");
        });

        t.Group("Название мода: кэш и карточка");

        t.Run("кэш: название и «названия нет» переживают перезапуск; изменившийся архив — читается заново; мусор в файле не мешает", () =>
        {
            using var d = new Temp();
            var dir = Path.Combine(d.Root, "previews");
            var a = new ArchiveStamp("a.zip", 10, new DateTime(2026, 1, 1, 0, 0, 0, DateTimeKind.Utc));
            var b = a with { FileName = "b.zip" };
            var c1 = new PreviewCache(dir);
            Assert.False(c1.TryGetTitle(a, out _), "пока не читали");
            c1.PutTitle(a, "Honda Civic EK9");
            c1.PutTitle(b, null);
            c1.SaveTitles();
            var c2 = new PreviewCache(dir);
            Assert.True(c2.TryGetTitle(a, out var ta) && ta == "Honda Civic EK9", "название");
            Assert.True(c2.TryGetTitle(b, out var tb) && tb is null, "названия нет — тоже знание");
            Assert.False(c2.TryGetTitle(a with { Size = 11 }, out _), "архив изменился");
            c2.Prune(new[] { a });
            Assert.False(new PreviewCache(dir).TryGetTitle(b, out _), "убран вместе с архивом");
            File.WriteAllText(Path.Combine(dir, "titles.json"), "{ не json");
            Assert.False(new PreviewCache(dir).TryGetTitle(a, out _), "повреждённый файл — читаем заново, без падения");
        });

        t.Run("карточка: сначала по файлу, фон дочитал — название машины; в панели строка «Файл»; при следующем запуске — сразу из кэша", () =>
        {
            using var f = new Folder(("honda_civic_ek9_v1.6.zip", Civic()));
            var card = f.Card("honda_civic_ek9_v1.6.zip");
            Assert.Eq("honda civic ek9", card.Name, "догадка по файлу, пока архив не читали");
            Assert.False(card.Data.ArchiveTitleKnown, "фону есть что читать");
            f.Vm.Selected = card;
            f.ReadTitlesInBackground();
            Assert.Eq("Honda Civic EK9", card.Name);
            Assert.Eq("Honda Civic EK9", f.Vm.Details!.Name, "заголовок панели");
            Assert.True(f.Vm.Details.Facts.Any(x => x is { Label: "Файл", Value: "honda_civic_ek9_v1.6.zip" }), "имя файла в панели");
            f.Vm.Refresh();
            Assert.Eq("Honda Civic EK9", f.Card("honda_civic_ek9_v1.6.zip").Name, "перечитали гараж — название то же");

            var next = f.Open(new PreviewCache(Path.Combine(f.Root, "app", "previews")));
            var again = next.AllCards.Single();
            Assert.True(again is { Name: "Honda Civic EK9", Data.ArchiveTitleKnown: true }, "следующий запуск: из кэша, архив не открываем");
            Assert.False(f.Loader.EnsureTitle(again.Data.ArchivePath!).Fresh, "второй раз не читается");
        });

        t.Run("поиск по моду находит и по названию, и по имени файла", () =>
        {
            using var f = new Folder(("hc_ek9_v1.6.zip", Civic()), ("other.zip", Zip(("vehicles/other/x.jbeam", "{}"))));
            f.ReadTitlesInBackground();
            f.Vm.SearchText = "civic";
            Assert.Eq("hc_ek9_v1.6.zip", f.Vm.Cards.Single().FileName, "по названию");
            f.Vm.SearchText = "hc ek9";
            Assert.Eq("hc_ek9_v1.6.zip", f.Vm.Cards.Single().FileName, "по файлу");
        });

        t.Run("в архиве названия нет — заголовок страницы, где следим; нет и его — имя файла", () =>
        {
            using var f = new Folder(("bmw_e34_by_kirill.zip", Zip(("vehicles/e34/e34.jbeam", "{}"))), ("skin.zip", Zip(("vehicles/etk800/info.json", "{\"Name\":\"800\"}"))));
            var bmw = f.Card("bmw_e34_by_kirill.zip");
            f.Store.UpsertBinding(new ModBinding(bmw.Id, "modsgaming", "1", new Uri("https://modsgaming.ru/1"), true));
            f.Store.SaveCheck(new CheckState(bmw.Id, "modsgaming", T0, T0, "1.2", 100, LastTitle: "МОД BMW E34 v1.2 (0.32) ДЛЯ BEAMNG.DRIVE"));
            f.Vm.Refresh();
            f.ReadTitlesInBackground();
            Assert.Eq("BMW E34", f.Card("bmw_e34_by_kirill.zip").Name, "с сайта");
            Assert.Eq("skin", f.Card("skin.zip").Name, "покраска ETK — имя файла, а не «ETK 800»");
        });

        t.Run("удалённый мод в «Удалённых» — с тем же названием", () =>
        {
            using var f = new Folder(("honda_civic_ek9_v1.6.zip", Civic()), ("b.zip", Zip(("vehicles/b/b.jbeam", "{}"))));
            f.ReadTitlesInBackground();
            f.Vm.Delete(f.Card("honda_civic_ek9_v1.6.zip"));
            Assert.True(f.Vm.Toast is { Text: "Удалён «Honda Civic EK9»" }, "тост: " + f.Vm.Toast?.Text);
            f.Vm.CommitPending();
            f.Vm.Refresh();
            f.Vm.Filter = GarageFilter.Deleted;
            Assert.Eq("Honda Civic EK9", f.Vm.Cards.Single().Name);
        });

        t.Group("Название мода: заголовок страницы при проверке");

        t.Run("открыли карточку сайта — заголовок запомнен; лента и пустой заголовок его не стирают", () =>
        {
            var store = new InMemoryLibraryStore();
            store.UpsertMod(new InstalledMod("m", "civic", "civic.zip", "1.5", 100, T0));
            store.UpsertBinding(new ModBinding("m", "modland", "1", new Uri("https://modland.net/1"), true));
            var b = new TrackedBinding("m", "modland", "1");
            UpdateCheckService.ApplyCard(store, b, new Listing("modland", new Uri("https://modland.net/1"), "1", "Honda Civic EK9 [1.6]") { ModVersion = "1.6" }, T0);
            Assert.Eq("Honda Civic EK9 [1.6]", store.GetCheck("m", "modland")!.LastTitle);
            UpdateCheckService.ApplyFeedBaseline(store, b, new FeedEntry("1", "1.7", null, null), T0.AddHours(1));
            Assert.Eq("Honda Civic EK9 [1.6]", store.GetCheck("m", "modland")!.LastTitle, "лента");
            UpdateCheckService.ApplyCard(store, b, new Listing("modland", new Uri("https://modland.net/1"), "1", " ") { ModVersion = "1.7" }, T0.AddHours(2));
            Assert.Eq("Honda Civic EK9 [1.6]", store.GetCheck("m", "modland")!.LastTitle, "пустой заголовок");
        });
    }
}
