using System.IO;
using System.Windows;
using BeamModManager.App.Demo;
using BeamModManager.App.Services;
using BeamModManager.Core.Game;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Previews;
using BeamModManager.Core.Settings;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Previews;

namespace BeamModManager.App;

/// <summary>
/// Запуск. Обычный режим: настройки, библиотека и картинки в %LOCALAPPDATA%\BeamModManager, папка mods — из настроек
/// или найденная сама. Режим --demo &lt;папка&gt; [--shot файл.png] [--select «название»] нужен сборочной машине для снимков экрана.
/// </summary>
public partial class App : Application
{
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
                WallPath = Path.Combine(demo, "wall.jpg"),
                ShotPath = shot,
                ShotSelect = Arg("--select"),
                ShotWidth = double.TryParse(Arg("--width"), System.Globalization.CultureInfo.InvariantCulture, out var sw) ? sw : null,
                ShotHeight = double.TryParse(Arg("--height"), System.Globalization.CultureInfo.InvariantCulture, out var sh) ? sh : null,
            };
        }
        else host = RealHost();

        var w = new MainWindow(new GarageViewModel(host.Backend), host);
        MainWindow = w;
        w.Show();
    }

    private static WindowHost RealHost()
    {
        var appDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "BeamModManager");
        Directory.CreateDirectory(appDir);
        var settingsStore = new SettingsStore(Path.Combine(appDir, "settings.json"));
        var settings = settingsStore.Load().Settings;

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

        void Use(string? modsDir)
        {
            // без папки: заведомо несуществующий путь, гараж покажет «Не нашли папку с модами»
            var dir = modsDir ?? Path.Combine(appDir, "нет-папки-mods");
            var installer = new ModInstaller(ModLayout.InAppFolder(dir, appDir), () => DateTimeOffset.UtcNow, settings.KeepBackups);
            switching.Inner = new LibraryGarageBackend(store, installer, cache, GameProcess.IsRunning);
        }
        Use(check.Ok ? check.ModsDir : null);

        return new WindowHost
        {
            Backend = switching,
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
                settings = settings with { ModsFolder = r.ModsDir };
                settingsStore.Save(settings);
                Use(r.ModsDir);
                return true;
            },
            SaveWindow = w =>
            {
                var max = w.WindowState == WindowState.Maximized;
                var b = max ? w.RestoreBounds : new Rect(w.Left, w.Top, w.ActualWidth, w.ActualHeight);
                settings = settings with { WindowMaximized = max, WindowWidth = (int)b.Width, WindowHeight = (int)b.Height };
                try { settingsStore.Save(settings); } catch (IOException) { } catch (UnauthorizedAccessException) { }
            },
        };
    }
}

/// <summary>Гараж поверх бэкенда, который можно заменить (пользователь указал другую папку).</summary>
internal sealed class SwitchingBackend : IGarageBackend
{
    public IGarageBackend Inner { get; set; } = null!;
    public GarageSnapshot Load() => Inner.Load();
    public bool IsGameRunning() => Inner.IsGameRunning();
    public ToggleResult SetEnabled(string fileName, bool enabled) => Inner.SetEnabled(fileName, enabled);
}
