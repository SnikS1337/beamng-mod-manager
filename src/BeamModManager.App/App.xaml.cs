using System.IO;
using System.Net.Http;
using System.Windows;
using System.Diagnostics;
using BeamModManager.App.Demo;
using BeamModManager.Core.AppUpdate;
using BeamModManager.App.Services;
using BeamModManager.Core.Game;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Matching;
using BeamModManager.Core.Previews;
using BeamModManager.Core.Settings;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.AppUpdate;
using BeamModManager.Ui.Previews;
using BeamModManager.Ui.Search;
using BeamModManager.Ui.Settings;
using BeamModManager.Ui.Updates;
using BeamModManager.Core.Net;
using BeamModManager.Core.Sources;

namespace BeamModManager.App;

/// <summary>
/// Запуск. Обычный режим: настройки, библиотека и картинки в %LOCALAPPDATA%\BeamModManager, папка mods — из настроек
/// или найденная сама. Режим --demo &lt;папка&gt; [--shot файл.png] [--select «название»] нужен сборочной машине для снимков экрана.
/// </summary>
public partial class App : Application
{
    private static Mutex? _single;

    /// <summary>
    /// Анимации разметки (наведение, проявление, перестроение сетки) по умолчанию WPF ограничивает 60 кадрами. Поднимаем
    /// потолок до 144: выше экрана WPF всё равно не рисует, а на экране 120–144 Гц они становятся плавнее. От батареи
    /// и в режиме экономии — 60 (FramePolicy, DESIGN.md 8). Задаётся один раз до первой анимации; пружины капель и
    /// панелей подстраиваются и на ходу. Фон, огоньки и искры задают свои 24–30 кадров сами.
    /// </summary>
    static App()
    {
        // Первым делом — защита ядра от зависания разбора чужих страниц (Core/Safety/RegexGuard): общий предел времени
        // регулярных выражений действует, только если задан до самого первого Regex в процессе, а его может создать и WPF.
        System.Runtime.CompilerServices.RuntimeHelpers.RunModuleConstructor(typeof(BeamModManager.Core.Settings.AppSettings).Module.ModuleHandle);
        var saving = BeamModManager.App.Services.Native.PowerSaving();
        BeamModManager.Ui.Look.FramePolicy.Shared.PowerSaving = saving;
        try
        {
            System.Windows.Media.Animation.Timeline.DesiredFrameRateProperty.OverrideMetadata(typeof(System.Windows.Media.Animation.Timeline),
                new FrameworkPropertyMetadata { DefaultValue = saving ? 60 : 144 });
        }
        catch (Exception e) when (e is ArgumentException or InvalidOperationException) { } // уже задано — остаётся 60
    }

    /// <summary>
    /// Откуда брать обновления: выпуски этого же (открытого с 2026-10-17) репозитория. latest.json пишет сборочная машина
    /// (windows.yml, задание release). Программы до 0.2 спрашивают SnikS1337/ModManagerBeamNG — туда ушёл один «мост».
    /// </summary>
    public const string UpdateManifestUrl = "https://github.com/SnikS1337/beamng-mod-manager/releases/latest/download/latest.json";

    /// <summary>Код выхода новой версии, которая не запустилась и сама вернула прежнюю (прежнему процессу делать нечего).</summary>
    private const int RolledBackExitCode = 3;

    private static string AppDir => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "BeamModManager");
    private static string LogPath => Path.Combine(AppDir, "errors.log");
    private static string SkipPath => Path.Combine(AppDir, "update-skip.txt");
    private static Version ThisVersion => typeof(App).Assembly.GetName().Version ?? new Version(0, 0);

    private readonly CancellationTokenSource _life = new();
    /// <summary>Путь exe на момент запуска: после подмены файл под ним уже другой, а путь тот же.</summary>
    private string? _exePath;
    private string? _restartExe;
    private Version? _restartVersion;
    private AppUpdater? _updater;
    /// <summary>Окно показалось: запуск удался. До этого любая ошибка — «не запустилась».</summary>
    private bool _healthy;
    /// <summary>Запущены прежней версией сразу после обновления (--after-update &lt;pid&gt;).</summary>
    private bool _afterUpdate;
    private int _parentPid;
    private Version? _rolledBack;
    /// <summary>Папка с выпусками вместо GitHub: проверка самообновления на сборочной машине (BMM_TEST_UPDATE_FEED).</summary>
    private string? _testFeed;

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
        _exePath = Environment.ProcessPath;
        // после самообновления прежний процесс ждёт, пока новая версия покажет окно (и отпустил «один экземпляр»)
        _afterUpdate = int.TryParse(Arg("--after-update"), out _parentPid);
        _rolledBack = AppVersion.Parse(Arg("--rolled-back"));
        // только проверочные сборки сборочной машины (0.1.1, 0.1.2): настоящие выпуски (номер сборки от 57) переменную не замечают
        if (demo is null && ThisVersion.Build is > 0 and < 50
            && Environment.GetEnvironmentVariable("BMM_TEST_UPDATE_FEED") is { Length: > 0 } feed && Directory.Exists(feed))
            _testFeed = Path.GetFullPath(feed);

        // два менеджера над одной библиотекой и одним db.json мешали бы друг другу
        if (demo is null)
        {
            // непредвиденная ошибка: записать в журнал и сказать, а не исчезнуть молча
            var shown = 0;
            DispatcherUnhandledException += (_, a) =>
            {
                a.Handled = true;
                // окна нет: продолжать нечего, иначе останется невидимый процесс, держащий «один экземпляр».
                // Сразу после обновления любая ошибка до первого кадра — повод вернуть прежнюю версию.
                if (!_healthy && (_afterUpdate || MainWindow is not { IsVisible: true })) { StartupFailed(a.Exception); return; }
                Log(a.Exception);
                // повторяющаяся ошибка не должна засыпать окнами: три сообщения за запуск, дальше только журнал
                if (++shown <= 3 && _testFeed is null)
                    MessageBox.Show($"Что-то пошло не так: {a.Exception.Message}\n\nПодробности записаны в {LogPath}. Моды и библиотека не тронуты.",
                        "ModManager BeamNG", MessageBoxButton.OK, MessageBoxImage.Warning);
            };
            _single = new Mutex(true, @"Local\BeamModManager.Single", out var first);
            if (!first)
            {
                _single.Dispose();
                _single = null;
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
        try { Start(demo, shot, Arg); }
        catch (Exception x) when (demo is null && x is not OutOfMemoryException) { StartupFailed(x); }
    }

    private void Start(string? demo, string? shot, Func<string, string?> Arg)
    {
        if (_afterUpdate && _testFeed is not null && AppVersion.Parse(Environment.GetEnvironmentVariable("BMM_TEST_CRASH_VERSION")) == AppVersion.Normalize(ThisVersion))
        {
            // проверка на сборочной машине: «сломанный выпуск». hard — процесс умирает сам (как при сбое до кода программы)
            if (Environment.GetEnvironmentVariable("BMM_TEST_CRASH_HARD") == "1") Environment.Exit(42);
            throw new InvalidOperationException("проверка: новая версия падает при запуске");
        }
        // версия, которая не запустилась, больше не предлагается (до следующей)
        if (_rolledBack is { } rb) UpdateInstaller.WriteSkipped(SkipPath, rb);

        WindowHost host;
        if (demo is not null)
        {
            var demoGarage = new DemoBackend(demo, int.TryParse(Arg("--many"), out var many) ? many : 0);
            host = new WindowHost
            {
                Backend = demoGarage,
                Updates = new DemoUpdatesBackend(demoGarage),
                Search = new DemoSearchBackend(demo),
                SearchImages = (u, _) => Task.FromResult(u.IsFile && File.Exists(u.LocalPath) ? u.LocalPath : null),
                DownloadsAllowed = true, // как на доске «Гараж»
                ShotPath = shot,
                ShotPerf = Array.IndexOf(Environment.GetCommandLineArgs(), "--perf") >= 0,
                ShotFrames = Array.IndexOf(Environment.GetCommandLineArgs(), "--frames") >= 0,
                StartSearch = Arg("--search"),
                StartScroll = double.TryParse(Arg("--scroll"), System.Globalization.CultureInfo.InvariantCulture, out var scr) ? scr : null,
                ShotSelect = Arg("--select"),
                ShotToggle = Array.IndexOf(Environment.GetCommandLineArgs(), "--toggle") >= 0,
                ShotWidth = double.TryParse(Arg("--width"), System.Globalization.CultureInfo.InvariantCulture, out var sw) ? sw : null,
                ShotHeight = double.TryParse(Arg("--height"), System.Globalization.CultureInfo.InvariantCulture, out var sh) ? sh : null,
                // настройки в демо живут только в памяти: файл пользователя не трогаем
                Settings = new SettingsViewModel(new AppSettings
                {
                    Theme = Enum.TryParse<BackgroundTheme>(Arg("--theme"), true, out var th) && Enum.IsDefined(th) ? th : BackgroundTheme.Night,
                    Mode = Enum.TryParse<UiMode>(Arg("--mode"), true, out var md) && Enum.IsDefined(md) ? md : UiMode.Dark,
                    // --matte: жидкое стекло выключено (снимки матового вида с плашками вместо капель)
                    LiquidGlass = Array.IndexOf(Environment.GetCommandLineArgs(), "--matte") < 0,
                })
                {
                    ModsFolderPath = Path.GetFullPath(Path.Combine(demo, "mods")),
                },
                StartPage = Arg("--page"),
                StartFilter = Arg("--filter"),
                StartSection = Enum.TryParse<SettingsSection>(Arg("--section"), true, out var sec) && Enum.IsDefined(sec) ? sec : SettingsSection.Appearance,
            };
        }
        else host = RealHost(CreateUpdater());

        var vm = new GarageViewModel(host.Backend)
        {
            DownloadsAllowed = host.DownloadsAllowed,
            ManyEnabledThreshold = host.ManyEnabledThreshold,
        };
        _updater = host.Updater;
        if (host.Updater is { } up)
        {
            if (_afterUpdate) host.StartupToast = $"Программа обновлена до {AppVersion.Display(up.Current)}";
            if (_rolledBack is { } bad)
                host.StartupToast = $"Версия {AppVersion.Display(bad)} не запустилась, вернули {AppVersion.Display(up.Current)}. Подождём следующую";
            if (Arg("--update-failed") is { Length: > 0 } why)
                host.StartupToast = $"Обновление не встало: {why}. Осталась версия {AppVersion.Display(up.Current)}";
        }
        host.Banner = new UpdateBanner(host.Updater, a => Dispatcher.BeginInvoke(a), RestartIntoUpdate);
        host.Settings.CanUpdateApp = host.Updater is not null;
        host.Settings.VersionText = host.Updater is { } u
            ? $"Версия {AppVersion.Display(u.Current)}"
            : "Сборка для разработки: сама не обновляется";
        var w = new MainWindow(vm, host);
        MainWindow = w;
        w.ContentRendered += (_, _) => OnHealthy(host);
        w.Show();
    }

    /// <summary>
    /// Окно показалось. Новая версия говорит прежнему процессу «запустилась» (он ждёт, чтобы вернуть себя, если нет),
    /// и только теперь убирает прежний exe и скачанное: до этого из них возвращали бы прежнюю версию.
    /// </summary>
    private void OnHealthy(WindowHost host)
    {
        if (_healthy) return;
        _healthy = true;
        if (_afterUpdate) SignalStarted(_parentPid);
        var cleanup = host.Updater is not null && _exePath is { } exe
            ? Task.Run(() => UpdateInstaller.Cleanup(exe, UpdateDir))
            : Task.CompletedTask;
        if (_testFeed is not null && host.Updater is { } up) _ = SelfTestAsync(up, cleanup);
    }

    /// <summary>
    /// Запуск не удался (ошибка до окна). Сразу после обновления — возвращаем прежнюю версию и запускаем её;
    /// иначе — сообщение и выход, а не невидимый процесс, из-за которого следующий запуск скажет «уже открыт».
    /// </summary>
    private void StartupFailed(Exception x)
    {
        Log(x);
        if (_afterUpdate && _exePath is { } exe && UpdateInstaller.RollbackFromRunning(exe))
        {
            ReleaseSingle();
            try { Process.Start(new ProcessStartInfo(exe, $"--rolled-back {AppVersion.Display(ThisVersion)}") { UseShellExecute = false }); }
            catch (Exception y) when (y is System.ComponentModel.Win32Exception or InvalidOperationException or IOException) { Log(y); }
            Environment.Exit(RolledBackExitCode);
        }
        if (_testFeed is null)
            MessageBox.Show($"Программа не смогла запуститься: {x.Message}\n\nПодробности записаны в {LogPath}. Моды и библиотека не тронуты.",
                "ModManager BeamNG", MessageBoxButton.OK, MessageBoxImage.Error);
        ReleaseSingle();
        Environment.Exit(1);
    }

    private static void Log(object? x)
    {
        try
        {
            Directory.CreateDirectory(AppDir);
            // журнал не растёт бесконечно: больше мегабайта — начинаем заново
            if (File.Exists(LogPath) && new FileInfo(LogPath).Length > 1024 * 1024) File.Delete(LogPath);
            File.AppendAllText(LogPath, $"{DateTimeOffset.Now:u} {AppVersion.Display(ThisVersion)}\n{x}\n\n");
        }
        catch (IOException) { } catch (UnauthorizedAccessException) { }
    }

    private static void ReleaseSingle()
    {
        try { _single?.ReleaseMutex(); } catch (ApplicationException) { }
        _single?.Dispose();
        _single = null;
    }

    private static string StartedEventName(int parentPid) => $@"Local\BeamModManager.Started.{parentPid}";

    private static void SignalStarted(int parentPid)
    {
        try
        {
            if (EventWaitHandle.TryOpenExisting(StartedEventName(parentPid), out var ev))
                using (ev) ev.Set();
        }
        catch (Exception x) when (x is IOException or UnauthorizedAccessException or WaitHandleCannotBeOpenedException) { }
    }

    private static string UpdateDir => Path.Combine(AppDir, "update");

    /// <summary>
    /// Самообновление возможно только у настоящей сборки: один BeamModManager.exe с номером версии от сборочной машины.
    /// При запуске через dotnet run, из папки разработки (версия 1.0.0 по умолчанию) и в демо — нет.
    /// </summary>
    private AppUpdater? CreateUpdater()
    {
        var exe = _exePath;
        var ver = ThisVersion;
        if (exe is null || !string.Equals(Path.GetFileName(exe), UpdateInstaller.ExeName, StringComparison.OrdinalIgnoreCase)) return null;
        if (ver.Major != 0 || ver.Build <= 0) return null; // 1.0.0.0 — сборка без номера (не с машины сборки)
        var http = _testFeed is { } feed
            ? new HttpClient(new LocalFeedHandler(feed))
            : UpdateDownloader.CreateClient($"ModManagerBeamNG/{AppVersion.Display(ver)}");
        return new AppUpdater(new UpdateDownloader(http), new Uri(UpdateManifestUrl), ver, exe, UpdateDir,
            p => AppVersion.Parse(FileVersionInfo.GetVersionInfo(p).FileVersion), skipped: UpdateInstaller.ReadSkipped(SkipPath));
    }

    /// <summary>Новая версия уже на месте: закрываемся как обычно (окно сохраняет своё), на выходе запускаем её.</summary>
    private void RestartIntoUpdate()
    {
        // проверка на сборочной машине: записать до закрытия окна (после него процесс может закончиться раньше)
        if (_testFeed is not null) TestNote($"install {_updater?.State.Stage} {_updater?.State.Error}");
        _restartExe = _exePath;
        _restartVersion = _updater?.State.Manifest?.Version;
        MainWindow?.Close();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _life.Cancel();
        if (_restartExe is { } exe)
        {
            ReleaseSingle();
            // подмена — только сейчас: окно закрыто, настройки и библиотека записаны. Собранная в один файл программа
            // догружает свои части из exe по имени, и после подмены прежний процесс уже не смог бы их прочитать
            if (_updater?.Commit() is { } failed)
            {
                Log("обновление не встало: " + failed);
                // прежняя версия на месте: запускаем её снова, она скажет, что случилось
                try
                {
                    var psi = new ProcessStartInfo(exe) { UseShellExecute = false };
                    psi.ArgumentList.Add("--update-failed");
                    psi.ArgumentList.Add(failed);
                    Process.Start(psi);
                }
                catch (Exception x) when (x is System.ComponentModel.Win32Exception or InvalidOperationException or IOException) { Log(x); }
                base.OnExit(e);
                return;
            }
            using var started = new EventWaitHandle(false, EventResetMode.ManualReset, StartedEventName(Environment.ProcessId));
            Process? child = null;
            try { child = Process.Start(new ProcessStartInfo(exe, $"--after-update {Environment.ProcessId}") { UseShellExecute = false }); }
            catch (Exception x) when (x is System.ComponentModel.Win32Exception or InvalidOperationException or IOException) { Log(x); }
            if (child is null) RollbackAndRestart(exe);
            else using (child) WatchNewVersion(child, started, exe);
        }
        base.OnExit(e);
    }

    /// <summary>
    /// Окно прежней версии уже закрыто; ждём, пока новая покажет своё (до минуты). Если она умерла раньше (сбой, который
    /// она сама не поймала: антивирус, повреждённый файл) — возвращаем прежнюю и запускаем её.
    /// </summary>
    private void WatchNewVersion(Process child, EventWaitHandle started, string exe)
    {
        var clock = Stopwatch.StartNew();
        while (clock.Elapsed < TimeSpan.FromSeconds(60))
        {
            if (started.WaitOne(250)) return;
            bool exited;
            try { exited = child.HasExited; }
            catch (Exception x) when (x is InvalidOperationException or System.ComponentModel.Win32Exception) { return; }
            if (!exited) continue;
            // 0 — закрылась обычным образом; RolledBackExitCode — уже вернула прежнюю сама
            if (child.ExitCode is not (0 or RolledBackExitCode)) RollbackAndRestart(exe);
            return;
        }
    }

    private void RollbackAndRestart(string exe)
    {
        if (!UpdateInstaller.Rollback(exe)) return;
        var args = _restartVersion is { } v ? $"--rolled-back {AppVersion.Display(v)}" : "";
        try { Process.Start(new ProcessStartInfo(exe, args) { UseShellExecute = false }); }
        catch (Exception x) when (x is System.ComponentModel.Win32Exception or InvalidOperationException or IOException) { Log(x); }
    }

    /// <summary>
    /// Проверка самообновления на сборочной машине (BMM_TEST_UPDATE_FEED): записать в log.txt этой папки, какая версия
    /// запустилась и как, проверить обновление и поставить его (плашка сама перезапустит программу); иначе закрыться.
    /// </summary>
    private async Task SelfTestAsync(AppUpdater up, Task cleanup)
    {
        await cleanup;
        void Note(string s) => TestNote(s);
        var exe = _exePath!;
        var mode = _afterUpdate ? "after-update" : _rolledBack is not null ? "rolled-back" : "normal";
        Note($"start {AppVersion.Display(up.Current)} {mode} old={File.Exists(exe + UpdateInstaller.OldSuffix)} new={File.Exists(exe + UpdateInstaller.NewSuffix)} bad={File.Exists(exe + UpdateInstaller.BadSuffix)} skip={(up.Skipped is { } sk ? AppVersion.Display(sk) : "-")}");
        await up.CheckAsync(manual: false, _life.Token);
        var s = up.State;
        Note($"check {s.Stage} {s.Error}");
        if (s.Stage == UpdateStage.Available)
        {
            await up.InstallAsync(_life.Token);
            // готово — плашка перезапускает сама и пишет строку в журнал (RestartIntoUpdate)
            if (up.State.Stage == UpdateStage.ReadyToRestart) return;
            Note($"install {up.State.Stage} {up.State.Error}");
        }
        MainWindow?.Close();
    }

    private void TestNote(string line)
    {
        try { File.AppendAllText(Path.Combine(_testFeed!, "log.txt"), line + "\n"); }
        catch (Exception x) when (x is IOException or UnauthorizedAccessException) { }
    }

    /// <summary>Папки BeamNG.drive в библиотеках Steam (путь к Steam — из реестра). Нет Steam — пусто.</summary>
    private static IReadOnlyList<string> SteamGameDirs()
    {
        try
        {
            if (Microsoft.Win32.Registry.GetValue(@"HKEY_CURRENT_USER\Software\Valve\Steam", "SteamPath", null) is string steam && steam.Length > 0)
                return SteamLibraries.GameDirs(steam.Replace('/', '\\'));
        }
        catch (Exception x) when (x is System.Security.SecurityException or IOException or UnauthorizedAccessException or ArgumentException) { }
        return Array.Empty<string>();
    }

    private static WindowHost RealHost(AppUpdater? updater)
    {
        var appDir = AppDir;
        Directory.CreateDirectory(appDir);
        // копии фото для фона из прошлой версии больше не нужны (фото убрано 2026-10-15)
        try { foreach (var f in Directory.EnumerateFiles(appDir, "background-*.*")) File.Delete(f); }
        catch (Exception x) when (x is IOException or UnauthorizedAccessException) { }
        var settingsStore = new SettingsStore(Path.Combine(appDir, "settings.json"));
        var loaded = settingsStore.Load();
        var settings = loaded.Settings;
        string? notice = loaded.HadProblems ? "Часть настроек сброшена. Старый файл сохранён" : null;

        // папка mods: из настроек; если не задана — ищем сами: перенесённая папка пользователя из startup.ini игры в Steam
        // (её часто уносят на другой диск), затем стандартные места (у не-Steam сборок тоже)
        static FolderCheck Resolve(AppSettings s)
        {
            var r = GameFolders.FromSettings(s);
            if (r.Ok || s.ModsFolder is not null || s.UserFolder is not null) return r;
            // автопоиск обходит папки BeamNG: подпапка без прав не должна ронять окно и сбивать смену настроек
            try
            {
                var found = GameFolders.AutoDetect(SteamGameDirs(), GameFolders.DefaultUserRoots());
                return found is not null ? GameFolders.Resolve(found) : r;
            }
            catch (Exception x) when (x is IOException or UnauthorizedAccessException) { return r; }
        }
        var check = Resolve(settings);
        // папка выбрана вручную, а игра обновилась и берёт моды из папки новой версии — подсказываем
        if (check.Ok && settings.ModsFolder is not null && GameFolders.NewerModsFolder(check.ModsDir!) is { } newer)
            notice ??= $"Игра обновилась: её моды теперь в «{Path.GetFileName(Path.GetDirectoryName(newer))}». Сменить папку — «Настройки → Папки»";

        var store = new JsonLibraryStore(Path.Combine(appDir, "library.json"));
        var cache = new PreviewCache(Path.Combine(appDir, "previews"));
        var switching = new SwitchingBackend();
        var svm = new SettingsViewModel(settings);

        void Use(string? modsDir)
        {
            // без папки: заведомо несуществующий путь, гараж покажет «Не нашли папку с модами»
            var dir = modsDir ?? Path.Combine(appDir, "нет-папки-mods");
            // число копий читается при каждой уборке: «−»/«+» в настройках не пересоздают помощника и не перечитывают папку
            var installer = new ModInstaller(ModLayout.InAppFolder(dir, appDir), () => DateTimeOffset.UtcNow, () => svm.Current.KeepBackups);
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

        var folderKey = (svm.Current.ModsFolder, svm.Current.UserFolder);
        svm.Changed += s =>
        {
            saveTimer.Stop();
            saveTimer.Start();
            // папка поменялась: переключаем помощника до того, как окно перечитает моды (окно подписано позже)
            var key = (s.ModsFolder, s.UserFolder);
            if (key == folderKey) return;
            folderKey = key;
            var r = Resolve(s);
            currentDir = r.Ok ? r.ModsDir : null;
            Use(currentDir);
        };

        // проверка обновлений и привязка страниц: один вежливый загрузчик на все сайты (паузы, кэш, «вы человек?»)
        var fetcher = new PoliteFetcher(new HttpPageFetcher());
        IModSource[] sources = { new ModlandSource(fetcher), new ModsGamingSource(fetcher), new StModsSource(fetcher), new BeamNgRepoSource(fetcher) };
        var updates = new LibraryUpdatesBackend(store, sources, () => svm.Current, null, fetcher.ClearCache, fetcher.ResetHost, fetcher.StatusOf);
        // «Поиск»: те же сайты и загрузчик; решения «Это один мод» / «Это разные» хранятся навсегда
        var search = new LibrarySearchBackend(sources, () => svm.Current, new MatchDecisionStore(Path.Combine(appDir, "matches.json")), fetcher.StatusOf);
        var remote = new RemotePreviews(Path.Combine(appDir, "search-previews"));
        _ = Task.Run(() => remote.Prune(TimeSpan.FromDays(30)));

        var pendingPath = Path.Combine(appDir, "pending.json");
        return new WindowHost
        {
            Backend = switching,
            Updates = updates,
            Search = search,
            SearchImages = remote.FetchAsync,
            Settings = svm,
            Updater = updater,
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
            ChooseFolder = owner =>
            {
                var dlg = new Microsoft.Win32.OpenFolderDialog { Title = "Папка mods или папка пользователя BeamNG" };
                if (dlg.ShowDialog(owner) != true) return null;
                var r = GameFolders.Resolve(dlg.FolderName);
                if (r.Ok) return r.ModsDir;
                MessageBox.Show(owner, r.Message, "Папка не подходит", MessageBoxButton.OK, MessageBoxImage.Information);
                return null;
            },
            UseFolder = dir =>
            {
                var before = svm.Current;
                // настройки сами переключат папку (подписка выше), окно перечитает моды по их событию
                svm.Update(s => s with { ModsFolder = dir, UserFolder = null });
                if (!ReferenceEquals(before, svm.Current)) return true;
                // та же папка (например, диск подключили после запуска): настройки не изменились, переключаем сами
                currentDir = dir;
                Use(currentDir);
                return false;
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
            // через временный файл: выключили свет посреди записи — прежний список цел, а не обрывок, который не прочитается
            var tmp = path + ".tmp";
            File.WriteAllText(tmp, System.Text.Json.JsonSerializer.Serialize(pending));
            File.Move(tmp, path, overwrite: true);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
    }
}
