using System.IO.Compression;
using System.Text;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Previews;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Incoming;

namespace BeamModManager.Core.Tests;

/// <summary>Установка из скачанного или перетащенного архива: окно проверки, плашка «Скачан мод», установка и «Отменить».</summary>
public static class IncomingScreenTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 18, 12, 0, 0, TimeSpan.Zero);

    private static byte[] Zip(params (string Name, string Text)[] entries)
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, leaveOpen: true))
            foreach (var (n, t) in entries)
            {
                using var w = new StreamWriter(z.CreateEntry(n).Open(), Encoding.UTF8);
                w.Write(t);
            }
        return ms.ToArray();
    }

    private static byte[] Car(string folder, string extra = "") =>
        Zip(($"vehicles/{folder}/info.json", $"{{\"Name\":\"{folder.ToUpperInvariant()}\"}}"), ($"vehicles/{folder}/{folder}.jbeam", "{}" + extra));

    private sealed class Folder : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "bmm-incoming-" + Guid.NewGuid().ToString("N"));
        public readonly InMemoryLibraryStore Store = new();
        public readonly ModInstaller Inst;
        public readonly LibraryGarageBackend Backend;
        public readonly GarageViewModel Vm;
        public readonly IncomingViewModel In;
        public DateTimeOffset Now = T0;
        public string Mods => Path.Combine(Root, "mods");
        public string Downloads => Path.Combine(Root, "Downloads");

        public Folder(params (string File, byte[] Data)[] files)
        {
            Directory.CreateDirectory(Mods);
            Directory.CreateDirectory(Downloads);
            foreach (var (f, d) in files) File.WriteAllBytes(Path.Combine(Mods, f), d);
            Inst = new ModInstaller(ModLayout.InAppFolder(Mods, Path.Combine(Root, "app")), () => Now = Now.AddSeconds(1), 3);
            Backend = new LibraryGarageBackend(Store, Inst, new PreviewCache(Path.Combine(Root, "app", "previews")), () => false,
                incomingRoot: Path.Combine(Root, "incoming"));
            Vm = new GarageViewModel(Backend, () => T0);
            Vm.Refresh();
            In = new IncomingViewModel(Backend, Vm, a => a());
        }

        public string Download(string name, byte[] data)
        {
            var p = Path.Combine(Downloads, name);
            File.WriteAllBytes(p, data);
            return p;
        }

        public bool InMods(string f) => File.Exists(Path.Combine(Mods, f));
        public bool Disabled(string f) => File.Exists(Path.Combine(Inst.Layout.DisabledDir, f));
        public int IncomingLeft => Directory.Exists(Path.Combine(Root, "incoming")) ? Directory.GetDirectories(Path.Combine(Root, "incoming")).Length : 0;
        public void Dispose() { try { Directory.Delete(Root, true); } catch { } }
    }

    public static void Register(TestRunner t)
    {
        t.Group("Установка из скачанного и перетащенного архива");

        t.Run("перетащили новый мод: «Проверяем…» → проверено → «Установить» → в папке, в библиотеке, выбран; «Отменить» — как было", async () =>
        {
            using var f = new Folder(("audi.zip", Car("audi")));
            var src = f.Download("bmw_m3.zip", Car("bmw"));
            var task = f.In.OfferAsync(src, IncomingOrigin.Dropped);
            Assert.True(f.In.Open, "окно сразу");
            await task;
            Assert.True(!f.In.Busy && f.In.Problem is null && f.In.Items.Count == 1, f.In.Problem ?? "");
            var it = f.In.Items[0];
            Assert.Eq("Новый мод", it.KindText);
            Assert.Eq("BMW", it.Title);
            Assert.Eq("Архив «bmw_m3.zip» проверен", f.In.Title);
            Assert.True(f.In.CanInstall && f.In.InstallLabel == "Установить" && f.In.Note is null && !f.In.Several, "одна кнопка, без галочек");
            Assert.True(it.Checks.Any(c => c.Level == CheckLevel.Ok && c.Text.StartsWith("Опасных файлов")), "проверка");
            f.In.Install();
            Assert.False(f.In.Open, "окно закрылось");
            Assert.True(f.InMods("bmw_m3.zip") && File.Exists(src), "в папке; скачанный файл не тронут");
            Assert.Eq("Установлен «BMW»", f.Vm.Toast!.Text);
            Assert.True(f.Vm.Toast.CanUndo, "можно отменить");
            Assert.Eq("bmw_m3.zip", f.Vm.Selected?.FileName);
            Assert.True(f.Store.Mods().Any(m => m.FileName == "bmw_m3.zip"), "запись в библиотеке");
            f.Vm.Undo();
            Assert.False(f.InMods("bmw_m3.zip"), "убран");
            Assert.False(f.Store.Mods().Any(m => m.FileName == "bmw_m3.zip"), "записи нет");
            Assert.Eq(0, f.IncomingLeft);
        });

        t.Run("обновление под новым именем: старый уходит в копии, привязка к сайту остаётся у мода; «Отменить» возвращает старый", async () =>
        {
            using var f = new Folder(("civic_v1.zip", Car("civic")));
            var rec = f.Store.Mods().Single();
            f.Store.UpsertBinding(new ModBinding(rec.Id, "modland", "77", new Uri("https://modland.net/77"), true));
            var src = f.Download("civic_v2 (1).zip", Car("civic", " v2"));
            await f.In.OfferAsync(src, IncomingOrigin.Dropped);
            var it = f.In.Items.Single();
            Assert.Eq("Обновление: заменит civic_v1.zip", it.KindText);
            Assert.Eq("civic_v2.zip", it.FileName);
            Assert.Eq("Обновить", f.In.InstallLabel);
            Assert.True(f.In.Note!.StartsWith("Прежняя версия уйдёт в копии"), "про копии");
            f.In.Install();
            Assert.True(f.InMods("civic_v2.zip") && !f.InMods("civic_v1.zip"), "одна версия");
            Assert.Eq("«CIVIC» обновлён", f.Vm.Toast!.Text);
            var now = f.Store.Mods().Single();
            Assert.Eq("civic_v2.zip", now.FileName);
            Assert.True(f.Store.Bindings().Any(b => b.ModId == now.Id && b.Source == "modland"), "привязка у того же мода");
            f.Vm.Undo();
            Assert.True(f.InMods("civic_v1.zip") && !f.InMods("civic_v2.zip"), "вернулся старый");
            Assert.Eq("civic_v1.zip", f.Store.Mods().Single().FileName);
            Assert.True(f.Store.Bindings().Any(b => b.ModId == f.Store.Mods().Single().Id), "привязка на месте");
        });

        t.Run("обновление выключенного остаётся выключенным; то же имя — «Отменить» возвращает прежнее содержимое", async () =>
        {
            using var f = new Folder(("golf.zip", Car("golf")));
            Assert.True(f.Inst.Disable("golf.zip").Success, "выключили");
            f.Vm.Refresh();
            var v1 = File.ReadAllBytes(Path.Combine(f.Inst.Layout.DisabledDir, "golf.zip"));
            await f.In.OfferAsync(f.Download("golf.zip", Car("golf", " new")), IncomingOrigin.Dropped);
            Assert.Eq("Обновление выключенного: останется выключенным", f.In.Items.Single().KindText);
            f.In.Install();
            Assert.True(f.Disabled("golf.zip") && !f.InMods("golf.zip"), "по-прежнему выключен");
            Assert.False(v1.SequenceEqual(File.ReadAllBytes(Path.Combine(f.Inst.Layout.DisabledDir, "golf.zip"))), "новое содержимое");
            f.Vm.Undo();
            Assert.True(v1.SequenceEqual(File.ReadAllBytes(Path.Combine(f.Inst.Layout.DisabledDir, "golf.zip"))), "прежнее содержимое");
        });

        t.Run("уже установленный ставить нечего; не мод — причина и «Понятно»", async () =>
        {
            var car = Car("audi");
            using var f = new Folder(("audi.zip", car));
            await f.In.OfferAsync(f.Download("audi (2).zip", car), IncomingOrigin.Dropped);
            Assert.True(f.In.Items.Single().KindText.StartsWith("Уже установлен") && !f.In.CanInstall, "кнопка недоступна");
            f.In.Close();
            await f.In.OfferAsync(f.Download("photos.zip", Zip(("a.jpg", "x"))), IncomingOrigin.Dropped);
            Assert.True(f.In.Problem!.StartsWith("В архиве нет мода BeamNG") && !f.In.CanInstall, f.In.Problem);
            Assert.Eq("«photos.zip»: ставить нечего", f.In.Title);
            f.In.Close();
            Assert.False(f.In.Open, "закрыто");
        });

        t.Run("скачанное: не моды и уже стоящие молча; мод — плашка, один раз; «Посмотреть» открывает, «Не сейчас» убирает", async () =>
        {
            var car = Car("audi");
            using var f = new Folder(("audi.zip", car));
            await f.In.OfferAsync(f.Download("photos.zip", Zip(("a.jpg", "x"))), IncomingOrigin.Downloaded);
            await f.In.OfferAsync(f.Download("audi_copy.zip", car), IncomingOrigin.Downloaded);
            Assert.True(f.In.OfferText is null && !f.In.Open, "молча");
            var bmw = f.Download("bmw.zip", Car("bmw"));
            await f.In.OfferAsync(bmw, IncomingOrigin.Downloaded);
            Assert.Eq("Скачан мод «BMW»", f.In.OfferText);
            await f.In.OfferAsync(bmw, IncomingOrigin.Downloaded); // тот же файл ещё раз (несколько событий папки)
            var golf = f.Download("golf.zip", Car("golf"));
            await f.In.OfferAsync(golf, IncomingOrigin.Downloaded);
            f.In.DismissOffer();
            Assert.Eq("Скачан мод «GOLF»", f.In.OfferText); // следующий, а не повтор bmw
            f.In.Review();
            Assert.True(f.In.Open && f.In.OfferText is null && f.In.Items.Single().Title == "GOLF", "окно с этим архивом");
            f.In.Close();
            Assert.True(f.In.OfferText is null && !f.In.Open, "очередь пуста");
            Assert.Eq(0, f.IncomingLeft);
        });

        t.Run("несколько модов: галочки, «Установить 2», снятая не ставится; ошибка одного — в тосте", async () =>
        {
            using var f = new Folder(("audi.zip", Car("audi")));
            var ms = new MemoryStream();
            using (var z = new ZipArchive(ms, ZipArchiveMode.Create, leaveOpen: true))
                foreach (var (n, d) in new[] { ("bmw.zip", Car("bmw")), ("golf.zip", Car("golf")), ("lada.zip", Car("lada")) })
                    using (var s = z.CreateEntry(n).Open()) s.Write(d);
            await f.In.OfferAsync(f.Download("pack.zip", ms.ToArray()), IncomingOrigin.Dropped);
            Assert.True(f.In.Several && f.In.InstallLabel == "Установить 3", f.In.InstallLabel);
            f.In.Items[2].Include = false;
            Assert.Eq("Установить 2", f.In.InstallLabel);
            File.WriteAllBytes(Path.Combine(f.Mods, "golf.zip"), Car("other")); // пока смотрели, файл с тем же именем появился
            f.In.Install();
            Assert.True(f.InMods("bmw.zip") && !f.InMods("lada.zip"), "снятая галочка не ставится");
            Assert.True(f.Vm.Toast!.Text.StartsWith("Установлено 1 из 2. «GOLF»:"), f.Vm.Toast.Text);
        });
    
        t.Group("Совместимость с версией игры");

        t.Run("версии: первые два числа, диапазон — по последней; строка для панели", () =>
        {
            Assert.Eq((0, 38), BeamModManager.Core.Game.GameCompat.Parse("v0.38.3")!.Value);
            Assert.Eq((0, 38), BeamModManager.Core.Game.GameCompat.Parse("0.38.x")!.Value);
            Assert.Eq((0, 38), BeamModManager.Core.Game.GameCompat.Parse("0.36–0.38")!.Value);
            Assert.True(BeamModManager.Core.Game.GameCompat.Parse("новая") is null && BeamModManager.Core.Game.GameCompat.Parse(null) is null, "мусор");
            Assert.Eq(BeamModManager.Core.Game.CompatLevel.Same, BeamModManager.Core.Game.GameCompat.Compare("0.39", "0.39.4.0"));
            Assert.Eq(BeamModManager.Core.Game.CompatLevel.ModOlder, BeamModManager.Core.Game.GameCompat.Compare("0.38.5", "0.39.4.0"));
            Assert.Eq(BeamModManager.Core.Game.CompatLevel.ModNewer, BeamModManager.Core.Game.GameCompat.Compare("0.40", "0.39.4"));
            Assert.Eq(BeamModManager.Core.Game.CompatLevel.ModOlder, BeamModManager.Core.Game.GameCompat.Compare("0.9", "0.10"));
            Assert.Eq("0.38 — у вас 0.39, может не работать", BeamModManager.Core.Game.GameCompat.Describe("0.38", "0.39.4.0"));
            Assert.Eq("0.39, как у вас", BeamModManager.Core.Game.GameCompat.Describe("0.39.1", "0.39.4.0"));
            Assert.Eq("0.40 — у вас 0.39, нужна игра новее", BeamModManager.Core.Game.GameCompat.Describe("0.40", "0.39"));
            Assert.True(BeamModManager.Core.Game.GameCompat.Describe(null, "0.39") is null && BeamModManager.Core.Game.GameCompat.Describe("0.38", null) is null, "не знаем — строки нет");
        });

        t.Run("сайт пишет версию игры — запоминается и не теряется, когда при следующей проверке не разобрана; в панели «Для игры»", () =>
        {
            using var f = new Folder(("civic_v1.zip", Car("civic")));
            File.WriteAllText(Path.Combine(f.Root, "beamng.log"), "Log started - v 0.39.4.0 - x64 - build 20972\n");
            var rec = f.Store.Mods().Single();
            f.Store.UpsertBinding(new ModBinding(rec.Id, "modland", "77", new Uri("https://modland.net/77"), true));
            var tb = new BeamModManager.Core.Updates.TrackedBinding(rec.Id, "modland", "77");
            var page = new BeamModManager.Core.Models.Listing("modland", new Uri("https://modland.net/77"), "77", "Civic") { ModVersion = "1" };
            UpdateCheckService.ApplyCard(f.Store, tb, page with { GameVersion = "0.38" }, T0);
            UpdateCheckService.ApplyCard(f.Store, tb, page, T0.AddHours(7));
            Assert.Eq("0.38", f.Store.GetCheck(rec.Id, "modland")!.LastGameVersion);
            f.Vm.Refresh();
            f.Vm.Selected = f.Vm.Cards.Single();
            var fact = f.Vm.Details!.Facts.Single(x => x.Label == "Для игры");
            Assert.Eq("0.38 — у вас 0.39, может не работать", fact.Value);
            Assert.True(fact.Attention, "янтарным");
        });
    }
}
