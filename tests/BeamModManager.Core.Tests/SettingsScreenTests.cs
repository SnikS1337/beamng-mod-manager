using BeamModManager.Core.Settings;
using BeamModManager.Ui.Settings;

namespace BeamModManager.Core.Tests;

/// <summary>Экран «Настройки» без окна: применяется сразу, мусор не пишется, наборы и зависимые пункты.</summary>
public static class SettingsScreenTests
{
    public static void Register(TestRunner t)
    {
        t.Group("Экран «Настройки»");

        t.Run("переключатель сразу меняет настройки и сообщает об этом один раз", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            var got = new List<AppSettings>();
            vm.Changed += got.Add;
            vm.LiquidGlass = false;
            Assert.Eq(1, got.Count);
            Assert.False(got[0].LiquidGlass, "жидкое стекло выключено");
            Assert.False(vm.Current.LiquidGlass, "и в текущих");
            vm.LiquidGlass = false; // то же значение
            Assert.Eq(1, got.Count, "повтор не пишет файл зря");
        });

        t.Run("значения из окна проходят проверку: порог и копии не выходят за пределы", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            vm.WarnAbove = 5000;
            Assert.Eq(1000, vm.Current.ActiveModsWarnAbove);
            vm.KeepBackups = 0;
            Assert.Eq(1, vm.Current.KeepBackups);
            Assert.False(vm.CanLessBackups, "меньше одной копии нельзя");
            vm.WarnAbove = -10;
            Assert.Eq(0, vm.Current.ActiveModsWarnAbove);
        });

        t.Run("порог «много включённых» шагает по 5 и встаёт на круглое", () =>
        {
            var vm = new SettingsViewModel(new AppSettings { ActiveModsWarnAbove = 30 });
            vm.MoreWarn(); Assert.Eq(35, vm.WarnAbove);
            vm.LessWarn(); vm.LessWarn(); Assert.Eq(25, vm.WarnAbove);
            vm.WarnAbove = 33; vm.MoreWarn(); Assert.Eq(35, vm.WarnAbove);
            vm.WarnAbove = 33; vm.LessWarn(); Assert.Eq(30, vm.WarnAbove);
            vm.WarnAbove = 3; vm.LessWarn(); vm.LessWarn(); vm.LessWarn(); Assert.Eq(0, vm.WarnAbove);
            Assert.Eq("нет", vm.WarnAboveText);
            Assert.False(vm.CanLessWarn, "ниже «нет» некуда");
            vm.MoreWarn(); Assert.Eq(1, vm.WarnAbove);
        });

        t.Run("набор выставляет переключатели; ручная правка делает набор «своим», «Сбросить» возвращает «Красиво»", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            Assert.True(vm.IsBeautiful && !vm.IsFast, "по умолчанию «Красиво»");
            vm.ApplyPreset(AppearancePreset.Fast);
            Assert.True(vm.IsFast && !vm.IsBeautiful, "«Быстро»");
            Assert.False(vm.Current.LiquidGlass, "без жидкого стекла");
            Assert.False(vm.Current.AnimatedBackground, "фон стоит");
            Assert.False(vm.Current.Fireflies, "огоньков нет");
            Assert.True(vm.Current.UiAnimations, "анимации интерфейса остаются");
            vm.Fireflies = true;
            Assert.Null<object>(vm.Preset);
            Assert.False(vm.IsBeautiful || vm.IsFast, "ни одна плитка не отмечена");
            Assert.True(vm.PresetNote.StartsWith("Свой набор"), vm.PresetNote);
            vm.ResetAppearance();
            Assert.True(vm.IsBeautiful, "сброс");
            Assert.True(vm.PresetNote.StartsWith("Набор выставляет"), vm.PresetNote);
        });

        t.Run("плитки наборов: выбор плитки выставляет набор, снятие отметки ничего не делает", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            var n = 0;
            vm.Changed += _ => n++;
            vm.IsBeautiful = false; // WPF снимает старую плитку отдельно — это не изменение
            Assert.Eq(0, n);
            Assert.True(vm.IsBeautiful, "осталось «Красиво»");
            vm.IsFast = true;
            Assert.Eq(1, n);
            Assert.True(vm.IsFast, "«Быстро»");
            vm.IsFast = true; // повтор
            Assert.Eq(1, n, "тот же набор не пишет файл зря");
            vm.IsBeautiful = true;
            Assert.True(vm.Current.LiquidGlass && vm.Current.AnimatedBackground && vm.Current.Fireflies && vm.Current.UiAnimations, "всё вернулось");
        });

        t.Run("набор не трогает папки, проверки и окно", () =>
        {
            var vm = new SettingsViewModel(new AppSettings { ModsFolder = @"D:\mods", CheckAppUpdates = false, WindowWidth = 1300, ActiveModsWarnAbove = 50 });
            vm.ApplyPreset(AppearancePreset.Fast);
            Assert.Eq(@"D:\mods", vm.Current.ModsFolder);
            Assert.False(vm.Current.CheckAppUpdates, "проверка программы");
            Assert.Eq(1300, vm.Current.WindowWidth);
            Assert.Eq(50, vm.Current.ActiveModsWarnAbove);
        });

        t.Run("«Огоньки» — один включатель для точек над фоном и искр на стекле; сохраняется и переводит набор в «свой»", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            AppSettings? saved = null;
            vm.Changed += s => saved = s;
            Assert.True(vm.Fireflies && vm.IsBeautiful, "по умолчанию включены, набор «Красиво»");
            vm.Fireflies = false;
            Assert.False(saved!.Fireflies, "ушло на сохранение");
            Assert.True(vm.Preset is null, "набор стал своим");
            vm.ResetAppearance();
            Assert.True(vm.Fireflies && vm.IsBeautiful, "«Сбросить» вернул огоньки");
        });

        t.Run("«Быстро» с включённым жидким стеклом — уже «свой» набор; выключили снова — опять «Быстро»", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            vm.IsFast = true;
            vm.LiquidGlass = true;
            Assert.True(vm.Preset is null, "свой");
            vm.LiquidGlass = false;
            Assert.True(vm.IsFast, "снова «Быстро»");
            Assert.True(AppearancePresets.Detect(vm.Current with { BackgroundTilt = true }) == AppearancePreset.Fast, "наклон фона набор не меняет");
        });

        t.Run("переключатели-сегменты: снятие одного пункта ничего не меняет, выбор другого меняет", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            var n = 0;
            vm.Changed += _ => n++;
            vm.ModeDark = false; // WPF снимает старый пункт отдельно — это не изменение
            Assert.Eq(0, n);
            vm.Repeat6 = true;
            Assert.Eq(6, vm.Current.AutoCheckEveryHours);
            vm.RepeatOff = true;
            Assert.Eq(0, vm.Current.AutoCheckEveryHours);
            vm.Themes[2].IsSelected = true;
            Assert.True(vm.Current.Theme == BackgroundTheme.Dusk, "плитка «Сумерки»");
            vm.Themes[2].IsSelected = false; // снятие отметки — не изменение
            Assert.True(vm.Current.Theme == BackgroundTheme.Dusk, "осталась");
        });

        t.Run("сайты: выключение и включение пишут список отключённых без повторов", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            var st = vm.Sites.First(s => s.Name == "STMods");
            st.Enabled = false;
            Assert.False(vm.Current.IsSourceEnabled(st.Id), "выключен");
            st.Enabled = false;
            Assert.Eq(1, vm.Current.DisabledSources.Count);
            st.Enabled = true;
            Assert.Eq(0, vm.Current.DisabledSources.Count);
            Assert.True(vm.Sites.All(s => s.Enabled), "все включены");
        });

        t.Run("одинаковый список сайтов из нового экземпляра не считается изменением", () =>
        {
            var vm = new SettingsViewModel(new AppSettings { DisabledSources = new[] { "stmods" } });
            var n = 0;
            vm.Changed += _ => n++;
            vm.Update(s => s with { DisabledSources = new List<string> { "stmods" } });
            Assert.Eq(0, n);
        });

        t.Run("папка: «найти самой» снимает ручную, подпись следует", () =>
        {
            var vm = new SettingsViewModel(new AppSettings { ModsFolder = @"D:\Games\mods" });
            Assert.True(vm.FolderIsManual, "указана вручную");
            vm.UseAutoFolder();
            Assert.False(vm.FolderIsManual, "ищет сама");
            Assert.Null(vm.Current.ModsFolder);
            vm.ModsFolderPath = null;
            Assert.False(vm.HasModsFolder, "не нашли");
            Assert.True(vm.ModsFolderText.StartsWith("Не нашли"), vm.ModsFolderText);
        });

        t.Run("темы фона: три плитки, отмечена одна, набор тему не меняет, тема переживает файл", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            Assert.Eq(3, vm.Themes.Count);
            Assert.True(vm.Themes.Count(t => t.IsSelected) == 1 && vm.Themes[0].IsSelected, "по умолчанию «Ночь»");
            vm.Themes[1].IsSelected = true;
            Assert.True(vm.Themes.Count(t => t.IsSelected) == 1 && vm.Themes[1].IsSelected, "одна отмечена");
            vm.ApplyPreset(AppearancePreset.Fast);
            Assert.True(vm.Current.Theme == BackgroundTheme.Deep, "набор тему не трогает");
            Assert.True(vm.IsFast, "а набор распознаётся");
            var p = Path.Combine(Path.GetTempPath(), "bmm-set-" + Guid.NewGuid().ToString("N") + ".json");
            try
            {
                var st = new SettingsStore(p);
                st.Save(vm.Current);
                Assert.True(st.Load().Settings.Theme == BackgroundTheme.Deep, "из файла");
            }
            finally { File.Delete(p); }
        });

        t.Run("оформление: тёмное, светлое, как в Windows; плитки тем показывают палитру текущего оформления", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            Assert.True(vm.ModeDark && !vm.EffectiveLight, "по умолчанию тёмное");
            Assert.False(vm.Themes[0].Palette.Light, "плитка тёмная");
            var raised = new List<string?>();
            vm.Themes[0].PropertyChanged += (_, e) => raised.Add(e.PropertyName);
            vm.ModeLight = true;
            Assert.True(vm.EffectiveLight && vm.Current.Mode == UiMode.Light, "светлое");
            Assert.True(vm.Themes[0].Palette.Light, "плитка светлая");
            Assert.True(raised.Contains(nameof(ThemeTile.Palette)), "окно узнаёт о смене палитры плитки");
            vm.ModeSystem = true;
            Assert.False(vm.EffectiveLight, "в Windows тёмное");
            vm.SystemLight = true;
            Assert.True(vm.EffectiveLight, "в Windows стало светлое");
            vm.ApplyPreset(AppearancePreset.Fast);
            Assert.True(vm.Current.Mode == UiMode.System, "набор оформление не трогает");
            vm.ModeDark = false; // снятие отметки — не изменение
            Assert.True(vm.Current.Mode == UiMode.System, "осталось");
        });

        t.Run("разделы: заголовок следует за выбором", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            Assert.Eq("Внешний вид", vm.SectionTitle);
            vm.Section = SettingsSection.Sites;
            Assert.Eq("Сайты", vm.SectionTitle);
            Assert.Eq(5, vm.Sections.Count);
        });
    }
}
