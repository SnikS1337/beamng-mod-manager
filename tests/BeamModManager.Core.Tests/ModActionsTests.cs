using System.IO.Compression;
using System.Text;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Packs;
using BeamModManager.Core.Previews;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Text;

namespace BeamModManager.Core.Tests;

/// <summary>«Моды»: удаление с «Отменить», «Прошлые версии», «Внутри архива», меню карточки — поверх настоящей папки.</summary>
public static class ModActionsTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 17, 12, 0, 0, TimeSpan.Zero);

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

    private static byte[] Car(string folder = "car", string extra = "") =>
        Zip(($"vehicles/{folder}/info.json", "{\"Name\":\"Car\"}"), ($"vehicles/{folder}/{folder}.jbeam", "{}" + extra));

    /// <summary>Папка mods во временной папке, своя библиотека и гараж поверх них.</summary>
    private sealed class Folder : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "bmm-actions-" + Guid.NewGuid().ToString("N"));
        public readonly InMemoryLibraryStore Store = new();
        public readonly ModInstaller Inst;
        public readonly LibraryGarageBackend Backend;
        public readonly GarageViewModel Vm;
        public DateTimeOffset Now = T0;
        public string Mods => Path.Combine(Root, "mods");

        public Folder(params (string File, byte[] Data)[] files)
        {
            Directory.CreateDirectory(Mods);
            foreach (var (f, d) in files) File.WriteAllBytes(Path.Combine(Mods, f), d);
            Inst = new ModInstaller(ModLayout.InAppFolder(Mods, Path.Combine(Root, "app")), () => Now = Now.AddSeconds(1), 3);
            Backend = new LibraryGarageBackend(Store, Inst, new PreviewCache(Path.Combine(Root, "app", "previews")), () => false);
            Vm = new GarageViewModel(Backend, () => T0);
            Vm.Refresh();
        }

        public ModCard Card(string file) => Vm.Cards.Single(c => c.FileName == file);
        public void Dispose() { try { Directory.Delete(Root, true); } catch { } }
    }

    public static void Register(TestRunner t)
    {
        t.Group("Моды: удаление с «Отменить»");

        t.Run("удалили — карточки нет, файл в копиях, запись держится; «Отменить» возвращает с той же страницей", () =>
        {
            using var f = new Folder(("audi_v1.0.zip", Car()), ("bmw.zip", Car("bmw")));
            var audi = f.Card("audi_v1.0.zip");
            f.Store.UpsertBinding(new ModBinding(audi.Id, "modland", "a", new Uri("https://www.modland.net/audi"), true));
            f.Vm.Refresh();
            f.Vm.Selected = audi;
            f.Vm.DeleteSelected();
            Assert.True(f.Vm.Cards.All(c => c.FileName != "audi_v1.0.zip"), "карточки нет");
            Assert.True(f.Vm.Selected is null, "выбор не переходит к соседу");
            Assert.False(File.Exists(Path.Combine(f.Mods, "audi_v1.0.zip")), "файла в mods нет");
            Assert.Eq(1, f.Inst.ListBackups("audi_v1.0.zip").Count);
            Assert.True(f.Vm.Toast is { CanUndo: true, Text: "Удалён «audi»" }, "тост: " + f.Vm.Toast?.Text);
            Assert.Eq("1 мод", f.Vm.CountText);
            f.Vm.Refresh(); // окно перечитало (вернулись в окно) — запись не пропала
            Assert.True(f.Store.GetMod(audi.Id) is not null, "запись цела, пока тост открыт");

            f.Vm.Undo();
            Assert.True(File.Exists(Path.Combine(f.Mods, "audi_v1.0.zip")), "файл на месте");
            Assert.Eq(0, f.Inst.ListBackups("audi_v1.0.zip").Count);
            var back = f.Card("audi_v1.0.zip");
            Assert.Eq(audi.Id, back.Id);
            Assert.True(back.Data.Binding?.Url.Host == "www.modland.net", "страница та же");
            Assert.True(ReferenceEquals(f.Vm.Selected, back), "вернувшийся выбран");
        });

        t.Run("тост погас — мод забыт, копия осталась в «Удалённых»; последний мод тоже уходит", () =>
        {
            using var f = new Folder(("only.zip", Car()));
            f.Vm.Delete(f.Card("only.zip"));
            f.Vm.DismissToast(f.Vm.Toast!.Serial);
            f.Vm.Refresh();
            Assert.Eq(0, f.Vm.Cards.Count);
            Assert.Eq(0, f.Store.Mods().Count, "в пустой папке запись не держится");
            Assert.Eq(EmptyAction.OpenSearch, f.Vm.Empty!.Action);
            Assert.True(VersionHistory.Deleted(f.Inst, f.Store).Any(d => d.FileName == "only.zip"), "в удалённых");
        });

        t.Run("новый тост доводит удаление; отменить можно только своё", () =>
        {
            using var f = new Folder(("a.zip", Car("a")), ("b.zip", Car("b")));
            var aId = f.Card("a.zip").Id;
            f.Vm.Delete(f.Card("a.zip"));
            f.Vm.Toggle(f.Card("b.zip"));
            Assert.True(f.Store.GetMod(aId) is null, "удаление стало окончательным");
            f.Vm.Undo(); // отменяет выключение b, а не удаление a
            Assert.True(f.Card("b.zip").Enabled, "b включён обратно");
            Assert.True(f.Vm.Cards.All(c => c.FileName != "a.zip"), "a не вернулся");
        });

        t.Run("окно закрыли с открытым тостом — удаление доводится", () =>
        {
            using var f = new Folder(("a.zip", Car("a")), ("b.zip", Car("b")));
            var id = f.Card("a.zip").Id;
            f.Vm.Delete(f.Card("a.zip"));
            f.Vm.CommitPending();
            Assert.True(f.Store.GetMod(id) is null, "забыт");
            f.Vm.CommitPending(); // второй раз — ничего
            Assert.Eq(1, f.Store.Mods().Count);
        });

        t.Run("выключенный переносом мод возвращается в папку выключенных", () =>
        {
            using var f = new Folder(("a.zip", Car("a")), ("b.zip", Car("b")));
            // игра этот файл не видела (db.json нет): выключение переносит его в папку менеджера
            f.Vm.Toggle(f.Card("a.zip"));
            Assert.True(File.Exists(Path.Combine(f.Inst.Layout.DisabledDir, "a.zip")), "выключен переносом");
            f.Vm.Delete(f.Card("a.zip"));
            Assert.False(File.Exists(Path.Combine(f.Inst.Layout.DisabledDir, "a.zip")), "удалён");
            f.Vm.Undo();
            Assert.True(File.Exists(Path.Combine(f.Inst.Layout.DisabledDir, "a.zip")), "вернулся выключенным");
            Assert.False(f.Card("a.zip").Enabled, "и показан выключенным");
        });

        t.Run("файл с тем же именем положили заново, пока висел тост: запись остаётся новому файлу", () =>
        {
            using var f = new Folder(("a.zip", Car("a")), ("b.zip", Car("b")));
            var id = f.Card("a.zip").Id;
            f.Vm.Delete(f.Card("a.zip"));
            File.WriteAllBytes(Path.Combine(f.Mods, "a.zip"), Car("a", "x"));
            f.Vm.Refresh();
            Assert.True(f.Vm.Cards.Any(c => c.FileName == "a.zip"), "новый файл виден");
            f.Vm.DismissToast(f.Vm.Toast!.Serial);
            Assert.True(f.Store.GetMod(id) is not null, "запись не стёрта");
            var r = f.Inst.Unremove("a.zip", f.Inst.ListBackups("a.zip")[0].Id, false);
            Assert.False(r.Success, "поверх нового файла копия не встаёт");
        });

        t.Run("ожидающее закрытия игры переключение удалённому не нужно", () =>
        {
            var b = new FakeBackend();
            b.Mods.Add(Garage("A"));
            b.Mods.Add(Garage("B"));
            var vm = new GarageViewModel(b, () => T0);
            vm.Refresh();
            b.Running = true;
            vm.Toggle(vm.Cards[0]);
            Assert.True(vm.HasPending, "ждёт игры");
            vm.Delete(vm.Cards[0]);
            Assert.False(vm.HasPending, "ожидание снято");
        });

        t.Run("мод из репозитория игры не удаляется: удаляют в самой игре", () =>
        {
            using var f = new Folder(("a.zip", Car("a")));
            Directory.CreateDirectory(Path.Combine(f.Mods, ModInstaller.RepoFolder));
            File.WriteAllBytes(Path.Combine(f.Mods, ModInstaller.RepoFolder, "repo_car.zip"), Car("rc"));
            f.Vm.Refresh();
            var repo = f.Card("repo_car.zip");
            f.Vm.Delete(repo);
            Assert.True(f.Vm.Toast is { CanUndo: false } toast && toast.Text.Contains("в самой игре"), "объяснили: " + f.Vm.Toast?.Text);
            Assert.True(File.Exists(Path.Combine(f.Mods, ModInstaller.RepoFolder, "repo_car.zip")), "файл цел");
            Assert.Eq(0, f.Backend.PastVersions(repo.Data).Count);
        });

        t.Run("сменили папку, пока висел тост: «Отменить» возвращает файл в прежнюю", () =>
        {
            using var a = new Folder(("a.zip", Car("a")), ("b.zip", Car("b")));
            using var other = new Folder(("x.zip", Car("x")));
            var sw = new SwitchingBackend { Inner = a.Backend };
            var vm = new GarageViewModel(sw, () => T0);
            vm.Refresh();
            vm.Delete(vm.Cards.Single(c => c.FileName == "a.zip"));
            sw.Inner = other.Backend;
            vm.Undo();
            Assert.True(File.Exists(Path.Combine(a.Mods, "a.zip")), "в прежней папке");
            Assert.False(File.Exists(Path.Combine(other.Mods, "a.zip")), "в новую не попал");
        });

        t.Run("длинное название в тосте обрезается", () =>
        {
            Assert.Eq("«Audi»", Ru.Quoted(" Audi ", 34));
            Assert.Eq("«abcdefghi…»", Ru.Quoted("abcdefghijklmn", 10));
            Assert.Eq("1\u00A0240", Ru.Number(1240));
            Assert.Eq("12\u00A0500\u00A0000", Ru.Number(12_500_000));
            Assert.Eq("7", Ru.Number(7));
        });

        t.Run("«Удалённые»: фильтр появляется с первым удалённым, «Вернуть» ставит файл назад, последний — фильтр уходит", () =>
        {
            using var f = new Folder(("audi_v1.0.zip", Car("a")), ("bmw.zip", Car("b")));
            Assert.Eq(4, f.Vm.Filters.Count);
            // картинка архива была в кэше — у удалённого она остаётся
            var cache = new PreviewCache(Path.Combine(f.Root, "app", "previews"));
            var stamp = ArchiveStamp.Of(Path.Combine(f.Mods, "audi_v1.0.zip"));
            cache.PutImage(stamp, new byte[] { 1 });
            f.Vm.Delete(f.Card("audi_v1.0.zip"));
            Assert.Eq(4, f.Vm.Filters.Count, "пока тост открыт, мод ещё не в удалённых");
            f.Vm.DismissToast(f.Vm.Toast!.Serial);
            f.Vm.Refresh();
            Assert.Eq(5, f.Vm.Filters.Count);
            Assert.Eq("Удалённые 1", f.Vm.Filters[4].Label);
            Assert.Eq("1 мод", f.Vm.CountText, "удалённые в число модов не входят");
            f.Vm.Filter = GarageFilter.Deleted;
            var gone = f.Vm.Cards.Single();
            Assert.True(gone is { IsDeleted: true, Enabled: false, ChipText: "Удалён", Name: "audi" }, "карточка: " + gone.Name);
            Assert.True(gone.Subtitle.StartsWith("1.0 · удалён"), gone.Subtitle);
            Assert.Eq(cache.ImagePath(stamp), gone.PreviewPath);
            f.Vm.Selected = gone;
            Assert.True(f.Vm.Details is { Action: MainAction.Restore, ActionLabel: "Вернуть", CanDelete: false, ToggleLabel: null }, "панель");
            Assert.Eq("Открыть|Вернуть", string.Join("|", f.Vm.MenuFor(gone).Select(m => m.Label)));
            f.Vm.Toggle(gone);
            f.Vm.Delete(gone);
            Assert.False(File.Exists(Path.Combine(f.Mods, "audi_v1.0.zip")), "включить или удалить удалённый нельзя");

            f.Vm.RestoreDeleted(gone);
            Assert.True(File.Exists(Path.Combine(f.Mods, "audi_v1.0.zip")), "файл на месте");
            Assert.Eq("Возвращён «audi»", f.Vm.Toast!.Text);
            Assert.Eq(GarageFilter.All, f.Vm.Filter, "удалённых больше нет — назад во «Все»");
            Assert.Eq(4, f.Vm.Filters.Count);
            Assert.Eq(2, f.Vm.Cards.Count);
            new LibraryGarageBackend(f.Store, f.Inst, cache, () => false).Load(); // перечитывание после запуска не трогает картинку
        });

        t.Run("удалённый после обновления (прежнее имя) — не в «Удалённых», а в прошлых версиях", () =>
        {
            using var f = new Folder(("audi_v1.0.zip", Car("a")));
            f.Inst.Update("audi_v1.0.zip", new MemoryStream(Car("a", "2")), "audi_v2.0.zip");
            f.Vm.Refresh();
            Assert.Eq(4, f.Vm.Filters.Count, "прежняя версия — не удалённый мод");
        });

        t.Group("Моды: прошлые версии");

        t.Run("после обновления под новым именем видна прошлая; «Вернуть» и «Отменить»", () =>
        {
            using var f = new Folder(("audi_v1.0.zip", Car()));
            var id = f.Card("audi_v1.0.zip").Id;
            Assert.True(f.Inst.Update("audi_v1.0.zip", new MemoryStream(Car("car", "v2")), "audi_v2.0.zip").Success, "обновили");
            f.Vm.Refresh();
            var card = f.Card("audi_v2.0.zip");
            Assert.Eq(id, card.Id);
            f.Vm.Selected = card;
            f.Vm.ExtrasTask.GetAwaiter().GetResult();
            Assert.Eq(1, f.Vm.Versions.Count);
            Assert.Eq("Версия 1.0", f.Vm.Versions[0].Title);
            Assert.True(f.Vm.Versions[0].Detail.Contains("Б"), "размер: " + f.Vm.Versions[0].Detail);

            f.Vm.RestoreVersion(f.Vm.Versions[0]);
            Assert.True(f.Vm.Toast is { CanUndo: true, Text: "Возвращена версия 1.0" }, "тост: " + f.Vm.Toast?.Text);
            Assert.True(File.Exists(Path.Combine(f.Mods, "audi_v1.0.zip")) && !File.Exists(Path.Combine(f.Mods, "audi_v2.0.zip")), "на месте 1.0");
            Assert.Eq(id, f.Card("audi_v1.0.zip").Id);
            Assert.True(ReferenceEquals(f.Vm.Selected, f.Card("audi_v1.0.zip")), "выбор остался");
            f.Vm.ExtrasTask.GetAwaiter().GetResult();
            Assert.Eq("Версия 2.0", f.Vm.Versions[0].Title, "теперь в прошлых 2.0");

            f.Vm.Undo();
            Assert.True(File.Exists(Path.Combine(f.Mods, "audi_v2.0.zip")) && !File.Exists(Path.Combine(f.Mods, "audi_v1.0.zip")), "снова 2.0");
            Assert.Eq(1, f.Vm.Cards.Count);
        });

        t.Run("быстрый выбор: показываются версии последней выбранной", () =>
        {
            using var f = new Folder(("a_v1.zip", Car("a")), ("b.zip", Car("b")));
            f.Inst.Update("a_v1.zip", new MemoryStream(Car("a", "2")), "a_v2.zip");
            f.Vm.Refresh();
            f.Vm.Selected = f.Card("a_v2.zip");
            f.Vm.Selected = f.Card("b.zip");
            f.Vm.ExtrasTask.GetAwaiter().GetResult();
            Assert.Eq(0, f.Vm.Versions.Count);
            f.Vm.Selected = null;
            Assert.True(f.Vm.Inside is null && f.Vm.Versions.Count == 0, "без выбора пусто");
        });

        t.Group("Моды: внутри архива");

        t.Run("машина по info.json (вольный JSON), обёрточная папка, скрипты, общие детали не машина", () =>
        {
            var zip = Zip(
                ("pack/vehicles/etk800/info.json", "{ // машина\n \"Brand\": \"ETK\", \"Name\": \"800-Series\", }"),
                ("pack/vehicles/etk800/lua/ctrl.lua", "--"),
                ("pack/vehicles/etk800/lua/ui.LUA", "--"),
                ("pack/vehicles/common/wheels.jbeam", "{}"),
                ("pack/vehicles/hatch/hatch.jbeam", "{}"));
            var o = ArchiveOverviewReader.Read(new MemoryStream(zip));
            Assert.Eq("ETK 800-Series|hatch", string.Join("|", o.Vehicles));
            Assert.Eq(2, o.LuaFiles);
            Assert.Eq(5, o.Files);
            var d = GarageViewModel.Describe(o);
            Assert.Eq("Машины — ETK 800-Series, hatch|Скрипты — есть, 2 файла Lua", string.Join("|", d.Facts.Take(2).Select(x => x.Label + " — " + x.Value)));
        });

        t.Run("карта; ключ перевода вместо имени не показываем; предупреждения проверки словами", () =>
        {
            var zip = Zip(("levels/mudstone/info.json", "{\"title\":\"levels.mudstone.info.title\"}"), ("levels/mudstone/main.level.json", "{}"),
                ("tools/run.exe", "MZ"));
            var d = GarageViewModel.Describe(ArchiveOverviewReader.Read(new MemoryStream(zip)));
            Assert.Eq("Карта — mudstone", d.Facts[0].Label + " — " + d.Facts[0].Value);
            Assert.Eq("Скрипты — нет", d.Facts[1].Label + " — " + d.Facts[1].Value);
            Assert.True(d.Warnings.Any(w => w.Contains("Исполняемый")), "предупреждение");
        });

        t.Run("повреждённый архив — «не читается»; сотня машин — первые по имени, без зависания", () =>
        {
            var broken = GarageViewModel.Describe(ArchiveOverviewReader.Read(new MemoryStream(Encoding.UTF8.GetBytes("не zip"))));
            Assert.Eq(0, broken.Facts.Count);
            Assert.True(broken.Warnings.Single().Contains("не читается"), "словами");
            var many = Enumerable.Range(0, 120).SelectMany(i => new[] { ($"vehicles/c{i:000}/info.json", $"{{\"Name\":\"Car {i:000}\"}}") }).ToArray();
            var o = ArchiveOverviewReader.Read(new MemoryStream(Zip(many)));
            Assert.Eq(120, o.Vehicles.Count);
            Assert.True(o.Vehicles.Count(v => v.StartsWith("Car ")) <= 24, "info.json читаем не у всех");
            Assert.True(GarageViewModel.Describe(o).Facts[0].Value.EndsWith("и ещё 117"), "до трёх имён");
        });

        t.Run("правая панель читает архив выбранного мода; файла нет — блока нет", () =>
        {
            using var f = new Folder(("a.zip", Car("a")));
            f.Vm.Selected = f.Card("a.zip");
            f.Vm.ExtrasTask.GetAwaiter().GetResult();
            Assert.Eq("Машина — Car", f.Vm.Inside!.Facts[0].Label + " — " + f.Vm.Inside.Facts[0].Value);
            var gone = f.Card("a.zip").Data with { ArchivePath = null };
            Assert.True(f.Backend.Inside(gone) is null, "нет архива — нет блока");
        });

        t.Group("Моды: меню карточки");

        t.Run("тот же набор, что в панели; опасный пункт последним; «Обновить» только с загрузкой", () =>
        {
            var b = new FakeBackend();
            var m = Garage("A");
            b.Mods.Add(m with { Update = new UpdateFinding(m.Mod.Id, "A", "beamng", BeamModManager.Core.Versioning.UpdateStatus.Likely, "1.0", "1.5", new Uri("https://www.beamng.com/resources/2/")) });
            b.Mods.Add(Garage("B", on: false, tracked: false));
            var vm = new GarageViewModel(b, () => T0);
            vm.Refresh();
            string Menu(int i) => string.Join("|", vm.MenuFor(vm.Cards[i]).Select(e => e.Label + (e.Danger ? "!" : "")));
            Assert.Eq("Открыть|Выключить|Открыть страницу|Удалить!", Menu(0));
            Assert.Eq("Открыть|Включить|Удалить!", Menu(1));
            vm.DownloadsAllowed = true;
            Assert.Eq("Открыть|Выключить|Открыть страницу|Обновить до 1.5|Удалить!", Menu(0));
            Assert.Eq("https://www.beamng.com/resources/2/", GarageViewModel.PageOf(vm.Cards[0])!.AbsoluteUri, "страница с новой версией");
        });
    }

    private static GarageMod Garage(string name, bool on = true, bool tracked = true)
    {
        var id = name.ToLowerInvariant();
        var m = new InstalledMod(id, name, id + ".zip", "1.0", 1000, T0.AddDays(-1), on);
        var b = tracked ? new ModBinding(id, "beamng", "1", new Uri("https://www.beamng.com/resources/1/"), true) : null;
        return new GarageMod(m, null, b, null, false, false, null, null, "vehicle", null);
    }

    private sealed class FakeBackend : IGarageBackend
    {
        public List<GarageMod> Mods = new();
        public bool Running;
        public GarageSnapshot Load() => new(Mods.ToList(), true, null);
        public bool IsGameRunning() => Running;
        public ToggleResult SetEnabled(string fileName, bool enabled) => new(true, ToggleWay.GameDb, "");
        public DeleteResult Delete(GarageMod mod)
        {
            Mods.RemoveAll(m => m.Mod.Id == mod.Mod.Id);
            return new(true, null, new DeletedTicket(mod.Mod.Id, mod.Mod.FileName, "1", false));
        }
    }
}
