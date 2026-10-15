using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Previews;
using BeamModManager.Core.Versioning;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Text;

namespace BeamModManager.Core.Tests;

public static class GarageTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 15, 12, 0, 0, TimeSpan.Zero);

    private sealed class FakeBackend : IGarageBackend
    {
        public List<GarageMod> Mods = new();
        public bool Folder = true;
        public bool Running;
        public DateTimeOffset? LastChecked;
        public readonly List<(string File, bool On)> Calls = new();
        public Func<string, bool, ToggleResult>? Override;

        public GarageSnapshot Load() => new(Mods.ToList(), Folder, LastChecked);
        public bool IsGameRunning() => Running;

        public ToggleResult SetEnabled(string fileName, bool enabled)
        {
            Calls.Add((fileName, enabled));
            if (Override?.Invoke(fileName, enabled) is { } r) return r;
            var i = Mods.FindIndex(m => m.Mod.FileName == fileName);
            Mods[i] = Mods[i] with { Mod = Mods[i].Mod with { Enabled = enabled } };
            return new ToggleResult(true, ToggleWay.GameDb, enabled ? "Включён." : "Выключен.");
        }
    }

    private static GarageMod Mod(string name, bool on = true, string? ver = "1.0", string? latest = null,
        UpdateStatus st = UpdateStatus.Likely, bool tracked = true, bool missing = false, bool dismissed = false, string? type = "vehicle")
    {
        var id = name.ToLowerInvariant().Replace(' ', '_');
        var m = new InstalledMod(id, name, id + ".zip", ver, 300L * 1024 * 1024, T0.AddDays(-3), on);
        var b = tracked ? new ModBinding(id, "beamng", "1", new Uri("https://www.beamng.com/resources/1/"), true) : null;
        var u = latest is null ? null : new UpdateFinding(id, name, "beamng", st, ver, latest, b!.Url) { IsDismissed = dismissed };
        return new GarageMod(m, u, b, tracked ? T0.AddMinutes(-18) : null, missing, false, null, null, type, null);
    }

    private static (GarageViewModel Vm, FakeBackend B) Make(params GarageMod[] mods)
    {
        var b = new FakeBackend { Mods = mods.ToList(), LastChecked = T0.AddMinutes(-18) };
        var vm = new GarageViewModel(b, () => T0);
        vm.Refresh();
        return (vm, b);
    }

    public static void Register(TestRunner t)
    {
        t.Group("Гараж: карточки и фильтры");

        t.Run("статусы читаются словами; плашка только у явного обновления, точка у сомнительных", () =>
        {
            var (vm, _) = Make(Mod("Audi", latest: "1.5"), Mod("BMW", latest: "1.0", st: UpdateStatus.Possible),
                Mod("Chevy", missing: true), Mod("Dacia", on: false), Mod("Echo", ver: "2.1"), Mod("Ford", latest: "2.0", dismissed: true));
            string S(string n) => vm.Cards.Single(c => c.Name == n).StatusText;
            Assert.Eq("1.0 → 1.5", S("Audi"));
            Assert.Eq("Возможно, обновлён", S("BMW"));
            Assert.Eq("Страница пропала", S("Chevy"));
            Assert.Eq("Выключен", S("Dacia"));
            Assert.Eq("2.1", S("Echo"));
            Assert.Eq("1.0", S("Ford"));
            Assert.True(vm.Cards.Single(c => c.Name == "Audi").HasUpdateBadge, "плашка");
            Assert.False(vm.Cards.Single(c => c.Name == "BMW").HasUpdateBadge, "у «возможно» плашки нет");
            Assert.True(vm.Cards.Where(c => c.HasWarningDot).Select(c => c.Name).SequenceEqual(new[] { "BMW", "Chevy" }), "точки");
        });

        t.Run("фильтры показывают числа до нажатия; скрытое обновление не считается", () =>
        {
            var (vm, _) = Make(Mod("A", latest: "2"), Mod("B", on: false, latest: "2", dismissed: true), Mod("C", tracked: false), Mod("D", on: false, tracked: false));
            Assert.True(vm.Filters.Select(f => f.Count).SequenceEqual(new[] { 4, 2, 1, 2 }), "числа");
            Assert.Eq("Нужно обновить 1", vm.Filters[2].Label);
            Assert.Eq("Не проверяются 2", vm.Filters[3].Label);
            Assert.Eq("Все", vm.Filters[0].Label);
            Assert.Eq("4 мода", vm.CountText);
            Assert.Eq("4 мода, 1 ждёт обновления", vm.Summary);
            vm.Filter = GarageFilter.NotTracked;
            Assert.True(vm.Cards.Select(c => c.Name).SequenceEqual(new[] { "C", "D" }), "не проверяются");
            Assert.True(vm.Filters.Count(f => f.IsActive) == 1 && vm.Filters[3].IsActive, "активен один");
            vm.Filter = GarageFilter.Enabled;
            Assert.True(vm.Cards.Select(c => c.Name).SequenceEqual(new[] { "A", "C" }), "включённые");
        });

        t.Run("смена фильтра не пересоздаёт карточки: те же экземпляры (для плавного перестроения)", () =>
        {
            var (vm, _) = Make(Mod("A"), Mod("B", on: false), Mod("C"));
            var a = vm.Cards[0];
            vm.Filter = GarageFilter.Enabled;
            vm.Filter = GarageFilter.All;
            Assert.True(ReferenceEquals(a, vm.Cards[0]), "та же карточка");
            Assert.Eq(3, vm.Cards.Count);
        });

        t.Run("«Найти в гараже»: по словам, без регистра, «ё» = «е», и по имени файла", () =>
        {
            var (vm, _) = Make(Mod("Ёлка Racing"), Mod("BMW M2 G87"), Mod("Lada"));
            vm.SearchText = "елка";
            Assert.Eq("Ёлка Racing", vm.Cards.Single().Name);
            vm.SearchText = "m2 bmw";
            Assert.Eq("BMW M2 G87", vm.Cards.Single().Name);
            vm.SearchText = "g87";
            Assert.Eq(1, vm.Cards.Count);
            vm.SearchText = "bmw_m2";
            Assert.Eq("BMW M2 G87", vm.Cards.Single().Name);
            vm.SearchText = "audi";
            Assert.Eq(0, vm.Cards.Count);
            Assert.Eq("Нет такого мода", vm.Empty!.Title);
            Assert.Eq(EmptyAction.SearchSites, vm.Empty.Action);
            vm.SearchText = "";
            Assert.True(vm.Empty is null, "снова всё видно");
        });

        t.Run("пустые состояния из таблицы DESIGN_FLOWS 4", () =>
        {
            var (vm, b) = Make();
            Assert.Eq("В гараже пока пусто", vm.Empty!.Title);
            Assert.Eq("Модов пока нет", vm.Summary);
            b.Folder = false; vm.Refresh();
            Assert.Eq(EmptyAction.PickFolder, vm.Empty!.Action);
            b.Folder = true; b.Mods.Add(Mod("A")); vm.Refresh();
            vm.Filter = GarageFilter.NeedsUpdate;
            Assert.Eq("Всё свежее", vm.Empty!.Title);
            Assert.Eq("Проверено 18 минут назад", vm.Empty.Text);
            vm.Filter = GarageFilter.NotTracked;
            Assert.Eq("У всех модов есть страница", vm.Empty!.Title);
            Assert.True(vm.Empty.ActionLabel is null, "без кнопки");
        });

        t.Run("строка «много включённых» после порога, «Понятно» убирает её до перезапуска", () =>
        {
            var (vm, _) = Make(Enumerable.Range(0, 31).Select(i => Mod("M" + i)).ToArray());
            Assert.Eq("Включено 31 мод. Игра может грузиться дольше", vm.ManyEnabledBanner);
            vm.DismissBanner();
            vm.Refresh();
            Assert.True(vm.ManyEnabledBanner is null, "не возвращается");
            var (vm2, _) = Make(Enumerable.Range(0, 30).Select(i => Mod("M" + i)).ToArray());
            Assert.True(vm2.ManyEnabledBanner is null, "30 — ещё нет");
        });

        t.Group("Гараж: выбор и правая панель");

        t.Run("выбор переживает обновление и фильтр, пропадает, если карточку скрыли", () =>
        {
            var (vm, b) = Make(Mod("A"), Mod("B", on: false));
            vm.Selected = vm.Cards[1];
            Assert.True(vm.Cards[1].IsSelected && !vm.Cards[0].IsSelected, "отмечена одна");
            vm.Refresh();
            Assert.Eq("B", vm.Selected!.Name);
            vm.Filter = GarageFilter.Enabled;
            Assert.True(vm.Selected is null, "скрыта фильтром");
            Assert.False(vm.Details is not null, "панель пуста");
        });

        t.Run("стрелки: по строке и между строками, у краёв на месте", () =>
        {
            var (vm, _) = Make(Enumerable.Range(0, 7).Select(i => Mod("M" + i)).ToArray());
            vm.MoveSelection(1, 0, 3);
            Assert.Eq("M0", vm.Selected!.Name);
            vm.MoveSelection(0, 1, 3);
            Assert.Eq("M3", vm.Selected!.Name);
            vm.MoveSelection(0, 1, 3);
            Assert.Eq("M6", vm.Selected!.Name);
            vm.MoveSelection(0, 1, 3);
            Assert.Eq("M6", vm.Selected!.Name);
            vm.MoveSelection(1, 0, 3);
            Assert.Eq("M6", vm.Selected!.Name);
            vm.MoveSelection(-1, 0, 3);
            Assert.Eq("M5", vm.Selected!.Name);
        });

        t.Run("главная кнопка по состоянию: обновить, найти замену, включить; данные мода словами", () =>
        {
            var (vm, _) = Make(Mod("A", latest: "1.5"), Mod("B", missing: true), Mod("C", on: false), Mod("D", tracked: false));
            vm.Selected = vm.Cards[0];
            Assert.Eq(MainAction.OpenPage, vm.Details!.Action);
            Assert.Eq("Открыть страницу", vm.Details.ActionLabel);
            Assert.Eq("Новая версия 1.5 на beamng.com. Скачайте её там, менеджер заметит обновление сам", vm.Details.ActionHint);
            Assert.False(vm.Details.OpenPageLink, "внизу «Открыть страницу» не дублируется");
            vm.DownloadsAllowed = true;
            vm.Selected = null;
            vm.Selected = vm.Cards[0];
            Assert.Eq("Обновить до 1.5", vm.Details!.ActionLabel);
            Assert.True(vm.Details.OpenPageLink, "при включённой загрузке ссылка внизу есть");
            Assert.True(vm.Details.Facts.Contains(new Fact("Следим на", "beamng.com")), "сайт без www");
            Assert.True(vm.Details.Facts.Contains(new Fact("Проверен", "18 минут назад")), "когда проверен");
            Assert.True(vm.Details.Facts.Contains(new Fact("Размер", "300 МБ")), "размер");
            vm.Selected = vm.Cards[1];
            Assert.Eq(MainAction.FindReplacement, vm.Details!.Action);
            vm.Selected = vm.Cards[2];
            Assert.Eq(MainAction.Enable, vm.Details!.Action);
            Assert.True(vm.Details.ToggleLabel is null, "«Включить» уже главная кнопка, внизу не дублируем");
            vm.Selected = vm.Cards[3];
            Assert.Eq(MainAction.None, vm.Details!.Action);
            Assert.True(vm.Details.Facts.Contains(new Fact("Следим на", "Страница не указана")), "без страницы");
        });

        t.Run("нигде нет «Играть»", () =>
        {
            var (vm, _) = Make(Mod("A", latest: "1.5"), Mod("B", on: false), Mod("C"));
            foreach (var c in vm.Cards)
            {
                vm.Selected = c;
                var d = vm.Details!;
                Assert.False(string.Join(" ", new[] { d.ActionLabel, d.ActionHint, d.ToggleLabel ?? "" }).Contains("Игра", StringComparison.OrdinalIgnoreCase), "без кнопки игры");
            }
            Assert.False(Enum.GetNames<MainAction>().Any(n => n.Contains("Play")), "и в действиях");
        });

        t.Group("Гараж: включить и выключить");

        t.Run("игра закрыта: сразу в db.json, тост «Выключен» с «Отменить», отмена возвращает", () =>
        {
            var (vm, b) = Make(Mod("A"));
            vm.Selected = vm.Cards[0];
            vm.ToggleSelected();
            Assert.Eq(("a.zip", false), b.Calls.Single());
            Assert.False(vm.Cards[0].Enabled, "выключен");
            Assert.Eq("Выключен", vm.Cards[0].StatusText);
            Assert.True(vm.Toast is { Text: "Выключен", CanUndo: true }, "тост");
            vm.Undo();
            Assert.Eq(("a.zip", true), b.Calls[1]);
            Assert.True(vm.Cards[0].Enabled, "вернули");
            Assert.False(vm.Toast!.CanUndo, "отмену отмены не предлагаем");
        });

        t.Run("игра запущена: карточка сразу в новом виде с подписью, в файл ничего не пишется", () =>
        {
            var (vm, b) = Make(Mod("A"));
            b.Running = true;
            vm.Toggle(vm.Cards[0]);
            Assert.Eq(0, b.Calls.Count);
            Assert.False(vm.Cards[0].Enabled, "уже серая");
            Assert.Eq("Выключится, когда закроете игру", vm.Cards[0].PendingText);
            Assert.Eq("Выключен. Применится, когда закроете игру", vm.Toast!.Text);
            Assert.True(vm.HasPending, "ждёт");
            vm.Refresh();
            Assert.Eq("Выключится, когда закроете игру", vm.Cards[0].PendingText);

            b.Running = false;
            Assert.Eq(0, vm.OnGameExited());
            Assert.Eq(("a.zip", false), b.Calls.Single());
            Assert.True(vm.Cards[0].PendingText is null && !vm.Cards[0].Enabled && !vm.HasPending, "применено");
        });

        t.Run("игра запущена: «Отменить» и повторное нажатие снимают ожидание", () =>
        {
            var (vm, b) = Make(Mod("A"), Mod("B"));
            b.Running = true;
            vm.Toggle(vm.Cards[0]);
            vm.Undo();
            Assert.True(vm.Cards[0].Enabled && vm.Cards[0].PendingText is null, "отменено");
            vm.Toggle(vm.Cards[1]);
            vm.Toggle(vm.Cards[1]);
            Assert.False(vm.HasPending, "туда-обратно — ждать нечего");
            b.Running = false;
            vm.OnGameExited();
            Assert.Eq(0, b.Calls.Count);
        });

        t.Run("переключили в самой игре так же, как ждали: ожидание снимается само", () =>
        {
            var (vm, b) = Make(Mod("A"));
            b.Running = true;
            vm.Toggle(vm.Cards[0]);
            b.Mods[0] = b.Mods[0] with { Mod = b.Mods[0].Mod with { Enabled = false } };
            vm.Refresh();
            Assert.False(vm.HasPending, "совпало");
        });

        t.Run("игру запустили между проверкой и записью: изменение встаёт в очередь", () =>
        {
            var (vm, b) = Make(Mod("A"));
            b.Override = (_, _) => { b.Running = true; return new ToggleResult(false, ToggleWay.Failed, "Закройте игру"); };
            vm.Toggle(vm.Cards[0]);
            Assert.Eq("Выключится, когда закроете игру", vm.Cards[0].PendingText);
            b.Override = null;
            b.Running = false;
            vm.OnGameExited();
            Assert.False(vm.Cards[0].Enabled || vm.HasPending, "применено после закрытия");
        });

        t.Run("ошибка записи без игры: карточка как была, тост с причиной без «Отменить»", () =>
        {
            var (vm, b) = Make(Mod("A"));
            b.Override = (_, _) => new ToggleResult(false, ToggleWay.Failed, "Нет доступа к db.json");
            vm.Toggle(vm.Cards[0]);
            Assert.True(vm.Cards[0].Enabled, "не изменилась");
            Assert.True(vm.Toast is { Text: "Нет доступа к db.json", CanUndo: false }, "тост");
        });

        t.Run("отложенное переживает перезапуск менеджера и применяется, когда игры нет", () =>
        {
            var (vm, b) = Make(Mod("A"), Mod("B"));
            b.Running = true;
            vm.Toggle(vm.Cards[0]);
            var saved = vm.PendingChanges;
            Assert.Eq(1, saved.Count);

            var vm2 = new GarageViewModel(b, () => T0);
            vm2.RestorePending(saved);
            Assert.Eq("Выключится, когда закроете игру", vm2.Cards[0].PendingText);
            b.Running = false;
            vm2.OnGameExited();
            Assert.Eq(("a.zip", false), b.Calls.Single());
        });

        t.Run("порог «много включённых» из настроек; 0 — не показывать", () =>
        {
            var (vm, _) = Make(Enumerable.Range(0, 12).Select(i => Mod("M" + i)).ToArray());
            vm.ManyEnabledThreshold = 10;
            vm.Refresh();
            Assert.Eq("Включено 12 модов. Игра может грузиться дольше", vm.ManyEnabledBanner);
            vm.ManyEnabledThreshold = 0;
            vm.Refresh();
            Assert.True(vm.ManyEnabledBanner is null, "выключено");
        });

        t.Run("картинка из фона держится, пока архив тот же; новая версия архива — сброс", () =>
        {
            var (vm, b) = Make(Mod("A"));
            vm.Cards[0].PreviewPath = "/cache/old.jpg";
            vm.Refresh();
            Assert.Eq("/cache/old.jpg", vm.Cards[0].PreviewPath);
            b.Mods[0] = b.Mods[0] with { Mod = b.Mods[0].Mod with { FileName = "a_v2.zip", SizeBytes = 5 } };
            vm.Refresh();
            Assert.True(vm.Cards[0].PreviewPath is null, "старую картинку не показываем");
        });

        t.Run("тост гаснет только свой: старый таймер не гасит новый", () =>
        {
            var (vm, _) = Make(Mod("A"), Mod("B"));
            vm.Toggle(vm.Cards[0]);
            var first = vm.Toast!.Serial;
            vm.Toggle(vm.Cards[1]);
            vm.DismissToast(first);
            Assert.True(vm.Toast is not null, "новый на месте");
            vm.DismissToast(vm.Toast!.Serial);
            Assert.True(vm.Toast is null && !vm.UndoCommand.CanExecute(null), "погас");
        });

        t.Group("Гараж поверх настоящей папки и db.json (0.39.4)");

        t.Run("5 модов друга: BMW выключен флажком, тип из db.json, включение пишет флажок", () =>
        {
            using var g = new GameDbTests.Game();
            var cache = new PreviewCache(Path.Combine(g.Root, "app", "previews"));
            var backend = new LibraryGarageBackend(g.Store, g.Inst, cache, () => false);
            var vm = new GarageViewModel(backend, () => T0);
            vm.Refresh();
            Assert.Eq(5, vm.Cards.Count);
            var bmw = vm.Cards.Single(c => c.FileName == "BMW_M2_G87_inf.zip");
            Assert.False(bmw.Enabled, "выключен в игре");
            Assert.Eq(PlaceholderKind.Map, vm.Cards.Single(c => c.FileName == "mudstone_valley.zip").Placeholder);
            Assert.Eq(PlaceholderKind.Script, vm.Cards.Single(c => c.FileName.StartsWith("TrafficReborn")).Placeholder);
            Assert.Eq(4, vm.Filters[1].Count);
            Assert.Eq(4, vm.Filters[3].Count); // mudstone из репозитория: страница известна сразу

            vm.Toggle(bmw);
            Assert.True(bmw.Enabled, "включён");
            Assert.True(BeamModManager.Core.Game.GameModDb.Read(g.Db).Find("BMW_M2_G87_inf.zip")!.Active, "в db.json тоже");
            Assert.True(File.Exists(Path.Combine(g.Mods, "BMW_M2_G87_inf.zip")), "файл на месте");
        });

        t.Run("повреждённая библиотека: один тост при запуске, дальше тихо", () =>
        {
            using var g = new GameDbTests.Game();
            var lib = Path.Combine(g.Root, "app", "library.json");
            Directory.CreateDirectory(Path.GetDirectoryName(lib)!);
            File.WriteAllText(lib, "{ сломано");
            var store = new JsonLibraryStore(lib);
            var vm = new GarageViewModel(new LibraryGarageBackend(store, g.Inst, new PreviewCache(Path.Combine(g.Root, "p")), () => false, "Часть настроек сброшена"), () => T0);
            vm.Refresh();
            Assert.True(vm.Toast is { CanUndo: false } toast && toast.Text.StartsWith("Часть настроек сброшена. Библиотека: "), "тост");
            vm.DismissToast(vm.Toast!.Serial);
            vm.Refresh();
            Assert.True(vm.Toast is null, "второй раз не показываем");
            Assert.Eq(5, vm.Cards.Count);
        });

        t.Run("картинка из кэша попадает на карточку; папки нет — гараж просит указать", () =>
        {
            using var g = new GameDbTests.Game();
            var cache = new PreviewCache(Path.Combine(g.Root, "app", "previews"));
            var zip = Path.Combine(g.Mods, "w164_kenemation_2.7.zip");
            cache.PutImage(ArchiveStamp.Of(zip), new byte[] { 1 });
            var vm = new GarageViewModel(new LibraryGarageBackend(g.Store, g.Inst, cache, () => false), () => T0);
            vm.Refresh();
            Assert.Eq(cache.ImagePath(ArchiveStamp.Of(zip)), vm.Cards.Single(c => c.FileName == "w164_kenemation_2.7.zip").PreviewPath);

            var none = new ModInstaller(ModLayout.InAppFolder(Path.Combine(g.Root, "нет"), Path.Combine(g.Root, "app2")), () => T0, 3);
            var vm2 = new GarageViewModel(new LibraryGarageBackend(new InMemoryLibraryStore(), none, cache, () => false), () => T0);
            vm2.Refresh();
            Assert.Eq(EmptyAction.PickFolder, vm2.Empty!.Action);
        });

        t.Group("Русские подписи");

        t.Run("числа, время и размер", () =>
        {
            Assert.Eq("1 мод|2 мода|5 модов|11 модов|21 мод|112 модов", string.Join("|", new[] { 1, 2, 5, 11, 21, 112 }.Select(n => Ru.Mods(n))));
            Assert.Eq("только что", Ru.Ago(T0, T0.AddSeconds(30)));
            Assert.Eq("1 час назад", Ru.Ago(T0, T0.AddMinutes(61)));
            Assert.Eq("вчера", Ru.Ago(T0, T0.AddHours(30)));
            Assert.Eq("3 дня назад", Ru.Ago(T0, T0.AddDays(3)));
            Assert.Eq("312 МБ", Ru.Size(312L * 1024 * 1024));
            Assert.Eq("1,5 ГБ", Ru.Size(1536L * 1024 * 1024));
        });
    }
}

public static class GarageCardTests
{
    public static void Register(TestRunner t)
    {
        t.Group("Гараж: подпись и плашка, как на доске «Гараж»");

        t.Run("версия · размер; без страницы — «не проверяется»; обновление — переход белым", () =>
        {
            var mods = new[]
            {
                Make("Honda", "1.6", 88, tracked: true),
                Make("Traffic", "3.1", 12, tracked: false),
                Make("Dacia", "1.0", 142, tracked: true, latest: "1.5"),
                Make("Station", "1.0", 12, tracked: true, on: false),
                Make("Pack", null, 5, tracked: false),
            };
            var vm = new GarageViewModel(new Fixed(mods));
            vm.Refresh();
            ModCard C(string n) => vm.Cards.Single(c => c.Name == n);
            Assert.Eq("1.6 · 88 МБ", C("Honda").Subtitle);
            Assert.Eq(ChipKind.None, C("Honda").Chip);
            Assert.Eq("3.1 · не проверяется", C("Traffic").Subtitle);
            Assert.Eq("1.0 → 1.5", C("Dacia").Subtitle);
            Assert.True(C("Dacia").SubtitleStrong, "белым");
            Assert.Eq((ChipKind.Update, "Есть обновление"), (C("Dacia").Chip, C("Dacia").ChipText));
            Assert.Eq((ChipKind.Disabled, "Выключен"), (C("Station").Chip, C("Station").ChipText));
            Assert.Eq("не проверяется", C("Pack").Subtitle);
        });
    }

    private sealed class Fixed(IReadOnlyList<GarageMod> mods) : IGarageBackend
    {
        public GarageSnapshot Load() => new(mods, true, null);
        public bool IsGameRunning() => false;
        public ToggleResult SetEnabled(string fileName, bool enabled) => new(false, ToggleWay.Failed, "нет");
    }

    private static GarageMod Make(string name, string? ver, long mb, bool tracked, string? latest = null, bool on = true)
    {
        var m = new InstalledMod(name, name, name + ".zip", ver, mb * 1024 * 1024, DateTimeOffset.UnixEpoch, on);
        var bnd = tracked ? new ModBinding(name, "modland", "1", new Uri("https://modland.net/x"), true) : null;
        var u = latest is null ? null : new UpdateFinding(name, name, "modland", UpdateStatus.Likely, ver, latest, bnd!.Url);
        return new GarageMod(m, u, bnd, null, false, false, null, null, "vehicle", null);
    }
}
