using System.IO;
using System.Windows;
using System.Diagnostics;
using BeamModManager.App.Demo;
using BeamModManager.Core.AppUpdate;
using BeamModManager.App.Services;
using BeamModManager.Core.Game;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Previews;
using BeamModManager.Core.Settings;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.AppUpdate;
using BeamModManager.Ui.Previews;
using BeamModManager.Ui.Settings;

namespace BeamModManager.App;

/// <summary>
/// Запуск. Обычный режим: настройки, библиотека и картинки в %LOCALAPPDATA%\BeamModManager, папка mods — из настроек
/// или найденная сама. Режим --demo &lt;папка&gt; [--shot файл.png] [--select «название»] нужен сборочной машине для снимков экрана.
/// </summary>
public partial class App : Application
{
    private static Mutex? _single;

    /// <summary>
    /// Откуда брать обновления: публичный репозиторий только с готовыми сборками (код остаётся закрытым).
    /// latest.json пишет сборочная машина (windows.yml, задание release).
    /// </summary>
    public const string UpdateManifestUrl = "https://github.com/SnikS1337/ModManagerBeamNG/releases/latest/download/latest.json";

    private readonly CancellationTokenSource _life = new();
    private string? _restartExe;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        string? Arg(string name)
        {
            var i = Array.IndexOf(e.Args, name);
            return i >= 0 && i + 1 < e.Args.Length ? e.Args[i + 1] : null;
        }

        var demo = Arg("--demo");
        var shot = Arg("--shot");

        // после самообновления: прежний процесс ещё закрывается и держит «один экземпляр» — ждём его
        if (int.TryParse(Arg("--after-update"), out var oldPid)) WaitForExit(oldPid);

        // два менеджера над одной библиотекой и одним db.json мешали бы друг другу
        if (demo is null)
        {
            // непредвиденная ошибка: записать в журнал и сказать, а не исчезнуть молча
            var shown = 0;
            DispatcherUnhandledException += (_, a) =>
            {
                var log = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "BeamModManager", "errors.log");
                try
                {
                    // журнал не растёт бесконечно: больше мегабайта — начинаем заново
                    if (File.Exists(log) && new FileInfo(log).Length > 1024 * 1024) File.Delete(log);
                    File.AppendAllText(log, $"{DateTimeOffset.Now:u}\n{a.Exception}\n\n");
                }
                catch (IOException) { } catch (UnauthorizedAccessException) { }
                a.Handled = true;
                // повторяющаяся ошибка не должна засыпать окнами: три сообщения за запуск, дальше только журнал
                if (++shown <= 3)
                    MessageBox.Show($"Что-то пошло не так: {a.Exception.Message}\n\nПодробности записаны в {log}. Моды и библиотека не тронуты.",
                        "ModManager BeamNG", MessageBoxButton.OK, MessageBoxImage.Warning);
            };
            _single = new Mutex(true, @"Local\BeamModManager.Single", out var first);
            if (!first)
            {
                MessageBox.Show("Менеджер уже открыт. Окно может быть свёрнуто на панели задач.", "ModManager BeamNG",
                    MessageBoxButton.OK, MessageBoxImage.Information);
                Shutdown();
                return;
            }
        }
        if (shot is not null)
        {
            // на сборочной машине некому смотреть на окно ошибки: пишем её рядом со снимком и выходим
            void Crash(object? ex)
            {
                try { File.WriteAllText(shot + ".error.txt", ex?.ToString() ?? "неизвестная ошибка"); } catch (IOException) { }
                Environment.Exit(2);
            }
            DispatcherUnhandledException += (_, a) => Crash(a.Exception);
            AppDomain.CurrentDomain.UnhandledException += (_, a) => Crash(a.ExceptionObject);
        }
        WindowHost host;
        if (demo is not null)
        {
            host = new WindowHost
            {
                Backend = new DemoBackend(demo),
                DownloadsAllowed = true, // как на доске «Гараж»
                WallPath = Path.Combine(demo, "wall.jpg"),
                ShotPath = shot,
                ShotSelect = Arg("--select"),
                ShotWidth = double.TryParse(Arg("--width"), System.Globalization.CultureInfo.InvariantCulture, out var sw) ? sw : null,
                ShotHeight = double.TryParse(Arg("--height"), System.Globalization.CultureInfo.InvariantCulture, out var sh) ? sh : null,
                // настройки в демо живут только в памяти: файл пользователя не трогаем
                Settings = new SettingsViewModel(new AppSettings()) { ModsFolderPath = Path.GetFullPath(Path.Combine(demo, "mods")) },
                StartPage = Arg("--page"),
                StartSection = Enum.TryParse<SettingsSection>(Arg("--section"), true, out var sec) ? sec : SettingsSection.Appearance,
            };
        }
        else host = RealHost();

        var vm = new GarageViewModel(host.Backend)
        {
            DownloadsAllowed = host.DownloadsAllowed,
            ManyEnabledThreshold = host.ManyEnabledThreshold,
        };
        if (host.Updater is { } up)
        {
            if (Arg("--after-update") is not null) host.StartupToast = $"Программа обновлена до {AppVersion.Display(up.Current)}";
            // прежний exe и скачанное больше не нужны; убираем при каждом запуске — вдруг прошлый перезапуск прервался
            var exe = Environment.ProcessPath!;
            _ = Task.Run(() => UpdateInstaller.Cleanup(exe, UpdateDir));
        }
        host.Banner = new UpdateBanner(host.Updater, a => Dispatcher.BeginInvoke(a), RestartIntoUpdate);
        host.Settings.CanUpdateApp = host.Updater is not null;
        host.Settings.VersionText = host.Updater is { } u
            ? $"Версия {AppVersion.Display(u.Current)}"
            : "Сборка для разработки: сама не обновляется";
        var w = new MainWindow(vm, host);
        MainWindow = w;
        w.Show();
    }

    private static string UpdateDir => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "BeamModManager", "update");

    /// <summary>
    /// Самообновление возможно только у настоящей сборки: один BeamModManager.exe с номером версии от сборочной машины.
    /// При запуске через dotnet run, из папки разработки (версия 1.0.0 по умолчанию) и в демо — нет.
    /// </summary>
    private static AppUpdater? CreateUpdater()
    {
        var exe = Environment.ProcessPath;
        var ver = typeof(App).Assembly.GetName().Version;
        if (exe is null || ver is null || !string.Equals(Path.GetFileName(exe), UpdateInstaller.ExeName, StringComparison.OrdinalIgnoreCase)) return null;
        if (ver.Major != 0 || ver.Build <= 0) return null; // 1.0.0.0 — сборка без номера (не с машины сборки)
        var http = UpdateDownloader.CreateClient($"ModManagerBeamNG/{AppVersion.Display(ver)}");
        return new AppUpdater(new UpdateDownloader(http), new Uri(UpdateManifestUrl), ver, exe, UpdateDir,
            p => AppVersion.Parse(FileVersionInfo.GetVersionInfo(p).FileVersion));
    }

    private static void WaitForExit(int pid)
    {
        try { using var p = Process.GetProcessById(pid); p.WaitForExit(20000); }
        catch (Exception e) when (e is ArgumentException or InvalidOperationException or System.ComponentModel.Win32Exception) { } // уже вышел
    }

    /// <summary>Новая версия уже на месте: закрываемся как обычно (окно сохраняет своё), на выходе запускаем её.</summary>
    private void RestartIntoUpdate()
    {
        _restartExe = Environment.ProcessPath;
        MainWindow?.Close();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _life.Cancel();
        if (_restartExe is { } exe)
        {
            try { _single?.ReleaseMutex(); } catch (ApplicationException) { }
            _single?.Dispose();
            _single = null;
            try { Process.Start(new ProcessStartInfo(exe, $"--after-update {Environment.ProcessId}") { UseShellExecute = false }); }
            catch (Exception x) when (x is System.ComponentModel.Win32Exception or InvalidOperationException or IOException)
            {
                // новая версия не запустилась: возвращаем прежнюю и запускаем её
                if (UpdateInstaller.Rollback(exe))
                    try { Process.Start(new ProcessStartInfo(exe) { UseShellExecute = false }); }
                    catch (Exception y) when (y is System.ComponentModel.Win32Exception or InvalidOperationException or IOException) { }
            }
        }
        base.OnExit(e);
    }

    private static WindowHost RealHost()
    {
        var appDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "BeamModManager");
        Directory.CreateDirectory(appDir);
        var settingsStore = new SettingsStore(Path.Combine(appDir, "settings.json"));
        var loaded = settingsStore.Load();
        var settings = loaded.Settings;
        string? notice = loaded.HadProblems ? "Часть настроек сброшена. Старый файл сохранён" : null;

        // папка mods: из настроек; если не задана — ищем сами в стандартном месте (у не-Steam сборок тоже, startup.ini не нужен)
        var check = GameFolders.FromSettings(settings);
        if (!check.Ok && settings.ModsFolder is null && settings.UserFolder is null)
        {
            var found = GameFolders.FindModsFolders(GameFolders.DefaultUserRoot()).FirstOrDefault();
            if (found is not null) check = GameFolders.Resolve(found);
        }

        var store = new JsonLibraryStore(Path.Combine(appDir, "library.json"));
        var cache = new PreviewCache(Path.Combine(appDir, "previews"));
        var switching = new SwitchingBackend();
        var svm = new SettingsViewModel(settings);

        void Use(string? modsDir)
        {
            // без папки: заведомо несуществующий путь, гараж покажет «Не нашли папку с модами»
            var dir = modsDir ?? Path.Combine(appDir, "нет-папки-mods");
            var installer = new ModInstaller(ModLayout.InAppFolder(dir, appDir), () => DateTimeOffset.UtcNow, svm.Current.KeepBackups);
            switching.Inner = new LibraryGarageBackend(store, installer, cache, GameProcess.IsRunning, notice);
            svm.ModsFolderPath = modsDir;
            notice = null;
        }
        var currentDir = check.Ok ? check.ModsDir : null;
        Use(currentDir);

        // Настройки пишутся не на каждое движение ползунка, а через полсекунды тишины; при закрытии окна — сразу.
        void SaveNow()
        {
            try { settingsStore.Save(svm.Current); }
            catch (Exception x) when (x is IOException or UnauthorizedAccessException) { } // запишем при следующем изменении
        }
        var saveTimer = new System.Windows.Threading.DispatcherTimer { Interval = TimeSpan.FromMilliseconds(500) };
        saveTimer.Tick += (_, _) => { saveTimer.Stop(); SaveNow(); };

        var folderKey = (svm.Current.ModsFolder, svm.Current.UserFolder, svm.Current.KeepBackups);
        var wall = svm.Current.BackgroundImage;
        svm.Changed += s =>
        {
            saveTimer.Stop();
            saveTimer.Start();
            // фото заменили или убрали: прежняя копия больше не нужна (удаляем только свою, из папки программы)
            if (s.BackgroundImage != wall)
            {
                if (wall is not null)
                    try
                    {
                        if (string.Equals(Path.GetDirectoryName(Path.GetFullPath(wall)), Path.GetFullPath(appDir), StringComparison.OrdinalIgnoreCase)) File.Delete(wall);
                    }
                    catch (Exception x) when (x is IOException or UnauthorizedAccessException or ArgumentException or NotSupportedException) { }
                wall = s.BackgroundImage;
            }
            // папка (или число копий для установщика) поменялась: переключаем помощника до того, как окно перечитает моды
            var key = (s.ModsFolder, s.UserFolder, s.KeepBackups);
            if (key == folderKey) return;
            var folderChanged = key.ModsFolder != folderKey.ModsFolder || key.UserFolder != folderKey.UserFolder;
            folderKey = key;
            if (folderChanged)
            {
                var r = GameFolders.FromSettings(s);
                if (!r.Ok && s.ModsFolder is null && s.UserFolder is null)
                {
                    var found = GameFolders.FindModsFolders(GameFolders.DefaultUserRoot()).FirstOrDefault();
                    if (found is not null) r = GameFolders.Resolve(found);
                }
                currentDir = r.Ok ? r.ModsDir : null;
            }
            Use(currentDir);
        };

        var pendingPath = Path.Combine(appDir, "pending.json");
        return new WindowHost
        {
            Backend = switching,
            Settings = svm,
            Updater = CreateUpdater(),
            CheckUpdatesOnStart = settings.CheckAppUpdates,
            // загрузки в этой сборке ещё нет: кнопки «Обновить» не показываем, даже если в настройках разрешено
            DownloadsAllowed = false,
            ManyEnabledThreshold = settings.ActiveModsWarnAbove,
            LoadPending = () => PendingFile.Load(pendingPath),
            SavePending = p => PendingFile.Save(pendingPath, p),
            Flush = () => store.Flush(),
            StartWidth = settings.WindowWidth,
            StartHeight = settings.WindowHeight,
            StartMaximized = settings.WindowMaximized,
            Previews = new PreviewLoader(cache, Shrinker.ToJpeg),
            PickFolder = owner =>
            {
                var dlg = new Microsoft.Win32.OpenFolderDialog { Title = "Папка mods или папка пользователя BeamNG" };
                if (dlg.ShowDialog(owner) != true) return false;
                var r = GameFolders.Resolve(dlg.FolderName);
                if (!r.Ok)
                {
                    MessageBox.Show(owner, r.Message, "Папка не подходит", MessageBoxButton.OK, MessageBoxImage.Information);
                    return false;
                }
                // настройки сами переключат папку и перечитают моды (подписка выше и в окне)
                svm.Update(s => s with { ModsFolder = r.ModsDir, UserFolder = null });
                return true;
            },
            PickWallpaper = owner =>
            {
                var dlg = new Microsoft.Win32.OpenFileDialog { Title = "Фото для фона", Filter = "Картинки (*.jpg, *.png, *.bmp)|*.jpg;*.jpeg;*.png;*.bmp" };
                if (dlg.ShowDialog(owner) != true) return false;
                // картинка должна читаться, иначе фона не будет, а человек не поймёт почему
                if (Services.Backdrop.LoadBlurred(dlg.FileName, 0) is null)
                {
                    MessageBox.Show(owner, "Эту картинку не получилось открыть. Попробуйте JPG или PNG.", "Фото для фона", MessageBoxButton.OK, MessageBoxImage.Information);
                    return false;
                }
                // своя копия: исходник можно удалить или переместить, фон останется; новое имя — чтобы окно увидело замену
                var dest = Path.Combine(appDir, $"background-{DateTime.Now:yyyyMMdd-HHmmss}{Path.GetExtension(dlg.FileName).ToLowerInvariant()}");
                try { File.Copy(dlg.FileName, dest, overwrite: true); }
                catch (Exception x) when (x is IOException or UnauthorizedAccessException)
                {
                    MessageBox.Show(owner, $"Не удалось сохранить копию фото: {x.Message}", "Фото для фона", MessageBoxButton.OK, MessageBoxImage.Warning);
                    return false;
                }
                // выбрали фото — значит, хотят его видеть: из «Цвет» переходим на «Фото»
                svm.Update(s => s with { BackgroundImage = dest, Background = s.Background == BackgroundMode.SolidColor ? BackgroundMode.Photo : s.Background });
                return true;
            },
            SaveWindow = w =>
            {
                var max = w.WindowState == WindowState.Maximized;
                // свёрнутое или развёрнутое окно: обычный размер знает только RestoreBounds
                var b = w.WindowState != WindowState.Normal ? w.RestoreBounds : new Rect(w.Left, w.Top, w.ActualWidth, w.ActualHeight);
                svm.Update(s => s with { WindowMaximized = max, WindowWidth = (int)b.Width, WindowHeight = (int)b.Height });
                saveTimer.Stop();
                SaveNow(); // окно закрывается: пишем всё, что ещё ждало таймера
            },
        };
    }
}

/// <summary>
/// Переключения, отложенные до закрытия игры, переживают закрытие менеджера (DESIGN_FLOWS 3.5): иначе человек
/// выключил мод, закрыл менеджер раньше игры — и мод остался включённым.
/// </summary>
internal static class PendingFile
{
    public static IReadOnlyDictionary<string, bool> Load(string path)
    {
        try
        {
            if (!File.Exists(path)) return new Dictionary<string, bool>();
            var d = System.Text.Json.JsonSerializer.Deserialize<Dictionary<string, bool>>(File.ReadAllText(path));
            return d is null ? new Dictionary<string, bool>() : new Dictionary<string, bool>(d.Where(x => ModInstaller.SafeFileName(x.Key) is not null), StringComparer.OrdinalIgnoreCase);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or System.Text.Json.JsonException) { return new Dictionary<string, bool>(); }
    }

    public static void Save(string path, IReadOnlyDictionary<string, bool> pending)
    {
        try
        {
            if (pending.Count == 0) { if (File.Exists(path)) File.Delete(path); return; }
            File.WriteAllText(path, System.Text.Json.JsonSerializer.Serialize(pending));
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
    }
}
