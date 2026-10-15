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
            vm.Frost = false;
            Assert.Eq(1, got.Count);
            Assert.False(got[0].Frost, "иней выключен");
            Assert.False(vm.Current.Frost, "и в текущих");
            vm.Frost = false; // то же значение
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
            vm.BackgroundDim = -10;
            Assert.Eq(0, vm.Current.BackgroundDim);
        });

        t.Run("порог «много включённых» шагает по 5 и встаёт на круглое", () =>
        {
            var vm = new SettingsViewModel(new AppSettings { ActiveModsWarnAbove = 30 });
            vm.MoreWarn(); Assert.Eq(35, vm.WarnAbove);
            vm.LessWarn(); vm.LessWarn(); Assert.Eq(25, vm.WarnAbove);
            vm.WarnAbove = 33; vm.MoreWarn(); Assert.Eq(35, vm.WarnAbove);
            vm.WarnAbove = 33; vm.LessWarn(); Assert.Eq(30, vm.WarnAbove);
            vm.WarnAbove = 3; vm.LessWarn(); vm.LessWarn(); vm.LessWarn(); Assert.Eq(1, vm.WarnAbove);
        });

        t.Run("набор выставляет переключатели; ручная правка делает набор «своим», «Сбросить» возвращает «Лёд»", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            Assert.True(vm.IsIce, "по умолчанию «Лёд»");
            vm.ApplyPreset(AppearancePreset.Light);
            Assert.True(vm.IsLight, "«Лёгкое»");
            Assert.False(vm.Current.UiAnimations, "анимации выключены");
            Assert.False(vm.Current.Fireflies, "огоньков нет");
            vm.Fireflies = true;
            Assert.Null<object>(vm.Preset);
            Assert.True(vm.PresetNote.StartsWith("Свой набор"), vm.PresetNote);
            vm.ResetAppearance();
            Assert.True(vm.IsIce, "сброс");
        });

        t.Run("набор не трогает папки, проверки и окно", () =>
        {
            var vm = new SettingsViewModel(new AppSettings { ModsFolder = @"D:\mods", CheckAppUpdates = false, WindowWidth = 1300, ActiveModsWarnAbove = 50 });
            vm.ApplyPreset(AppearancePreset.Light);
            Assert.Eq(@"D:\mods", vm.Current.ModsFolder);
            Assert.False(vm.Current.CheckAppUpdates, "проверка программы");
            Assert.Eq(1300, vm.Current.WindowWidth);
            Assert.Eq(50, vm.Current.ActiveModsWarnAbove);
        });

        t.Run("зависимые пункты тускнеют, но значения хранятся", () =>
        {
            var vm = new SettingsViewModel(new AppSettings { FrostStrength = 2 });
            vm.Frost = false;
            Assert.False(vm.FrostOptionsEnabled, "сила инея недоступна");
            Assert.Eq(2, vm.FrostStrength, "значение осталось");
            vm.Frost = true;
            Assert.True(vm.FrostStrong, "вернулось «Сильно»");
            vm.BackgroundColor = true;
            Assert.False(vm.PhotoOptionsEnabled, "движение фото недоступно при «Цвет»");
            Assert.True(vm.Current.AnimatedBackground, "но не выключено");
        });

        t.Run("переключатели-сегменты: снятие одного пункта ничего не меняет, выбор другого меняет", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            var n = 0;
            vm.Changed += _ => n++;
            vm.FrostMedium = false; // WPF снимает старый пункт отдельно — это не изменение
            Assert.Eq(0, n);
            vm.FrostWeak = true;
            Assert.Eq(0, vm.FrostStrength);
            vm.Repeat6 = true;
            Assert.Eq(6, vm.Current.AutoCheckEveryHours);
            vm.RepeatOff = true;
            Assert.Eq(0, vm.Current.AutoCheckEveryHours);
            vm.BackgroundBlurred = true;
            Assert.Eq(BackgroundMode.BlurredPhoto, vm.Current.Background);
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

        t.Run("своё фото: есть — подпись про копию, «Убрать» возвращает градиент; набор фото не трогает", () =>
        {
            var vm = new SettingsViewModel(new AppSettings());
            Assert.False(vm.HasBackgroundImage, "по умолчанию фото нет");
            vm.Update(s => s with { BackgroundImage = @"C:\app\background-1.jpg" });
            Assert.True(vm.HasBackgroundImage, "выбрано");
            vm.ApplyPreset(AppearancePreset.Light);
            Assert.Eq(@"C:\app\background-1.jpg", vm.Current.BackgroundImage);
            vm.RemoveBackgroundImage();
            Assert.Null(vm.Current.BackgroundImage);
            Assert.True(vm.BackgroundImageNote.StartsWith("Своего фото нет"), vm.BackgroundImageNote);
        });

        t.Run("своё фото переживает запись и чтение файла настроек", () =>
        {
            var p = Path.Combine(Path.GetTempPath(), "bmm-set-" + Guid.NewGuid().ToString("N") + ".json");
            try
            {
                var st = new SettingsStore(p);
                st.Save(new AppSettings { BackgroundImage = "  " });
                Assert.Null(st.Load().Settings.BackgroundImage);
                st.Save(new AppSettings { BackgroundImage = @"C:\app\background-2.png" });
                Assert.Eq(@"C:\app\background-2.png", st.Load().Settings.BackgroundImage);
            }
            finally { File.Delete(p); }
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
