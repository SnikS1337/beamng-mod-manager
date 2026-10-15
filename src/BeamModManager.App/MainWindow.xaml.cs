using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using BeamModManager.App.Controls;
using BeamModManager.App.Motion;
using BeamModManager.App.Services;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Previews;

namespace BeamModManager.App;

/// <summary>Что окну нужно снаружи: откуда фон и картинки, как узнать про игру, как выбрать папку.</summary>
public sealed class WindowHost
{
    public required IGarageBackend Backend { get; init; }
    public string? WallPath { get; init; }
    public PreviewLoader? Previews { get; init; }
    /// <summary>«Указать папку»: показывает выбор папки и пересобирает гараж; null — не умеем (демо).</summary>
    public Func<Window, bool>? PickFolder { get; init; }
    public Action<Window>? SaveWindow { get; init; }
    /// <summary>Сделать снимок окна в этот файл и закрыться (сборочная машина).</summary>
    public string? ShotPath { get; init; }
    public string? ShotSelect { get; init; }
    /// <summary>Размер с прошлого запуска (только если он не меньше 1000 × 680, AppSettings).</summary>
    public int StartWidth { get; init; }
    public int StartHeight { get; init; }
    public bool StartMaximized { get; init; }
}

public partial class MainWindow : Window
{
    private readonly GarageViewModel _vm;
    private readonly WindowHost _host;
    private readonly SpringDriver _springs = new();
    private readonly Spring _navDrop, _filterX, _filterW, _capX, _capScale;
    private readonly BackgroundMotion _motion;
    private readonly DispatcherTimer _gameTimer = new() { Interval = TimeSpan.FromSeconds(3) };
    private readonly DispatcherTimer _toastTimer = new() { Interval = TimeSpan.FromSeconds(6) };
    private CardGridPanel? _panel;
    private bool _gameRunning;
    private DateTime _lastRefresh;
    private CancellationTokenSource? _previewCts;

    public MainWindow(GarageViewModel vm, WindowHost host)
    {
        _vm = vm;
        _host = host;
        InitializeComponent();
        DataContext = vm;

        _navDrop = _springs.Add(new Spring(0.5, 0.15, 14), v => NavDropMove.Y = v);
        _filterX = _springs.Add(new Spring(0.5, 0.15, -3), v => FilterDropMove.X = v);
        _filterW = _springs.Add(new Spring(0.5, 0.15, 60), v => FilterDrop.Width = Math.Max(20, v));
        _capX = _springs.Add(new Spring(0.5, 0.15, -1), v => CapDropMove.X = v);
        _capScale = _springs.Add(new Spring(0.35, 0.15, 0.6), v => { CapDropScale.ScaleX = v; CapDropScale.ScaleY = v; });

        if (host.StartWidth >= MinWidth && host.StartHeight >= MinHeight)
        {
            Width = Math.Min(host.StartWidth, SystemParameters.VirtualScreenWidth);
            Height = Math.Min(host.StartHeight, SystemParameters.VirtualScreenHeight);
        }
        if (host.StartMaximized) WindowState = WindowState.Maximized;

        var wall = Backdrop.LoadBlurred(host.WallPath);
        if (wall is not null) { Wall.Source = wall; Wall.Visibility = Visibility.Visible; }
        _motion = new BackgroundMotion(this, WallScale, WallMove, GradeMove, Lights);

        vm.PropertyChanged += Vm_PropertyChanged;
        _toastTimer.Tick += (_, _) => { _toastTimer.Stop(); if (_vm.Toast is { } t) _vm.DismissToast(t.Serial); };
        _gameTimer.Tick += (_, _) => CheckGame();

        SourceInitialized += (_, _) => Native.StyleWindow(this);
        Loaded += OnLoaded;
        Loaded += (_, _) => FitMaximized();
        StateChanged += (_, _) => { FitMaximized(); UpdateMotion(); };
        Activated += (_, _) => { UpdateMotion(); RefreshIfStale(); };
        Deactivated += (_, _) => UpdateMotion();
        IsVisibleChanged += (_, _) => UpdateMotion();
        Closing += OnClosing;
        PreviewKeyDown += OnPreviewKeyDown;
        FilterList.LayoutUpdated += (_, _) => PlaceFilterDrop(instant: !_filterPlaced);
    }

    private bool _filterPlaced;

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        Refresh();
        _gameTimer.Start();
        CheckGame();
        UpdateMotion();
        if (_host.ShotSelect is { } name && _vm.Cards.FirstOrDefault(c => c.Name == name) is { } sel) _vm.Selected = sel;
        if (_host.ShotPath is not null) Dispatcher.BeginInvoke(DispatcherPriority.ApplicationIdle, TakeShot);
    }

    // ───────────── данные ─────────────

    private void Refresh()
    {
        _vm.Refresh();
        _lastRefresh = DateTime.UtcNow;
        LoadPreviews();
    }

    /// <summary>Пользователь вернулся в окно: мог переключить моды в игре или положить новые архивы.</summary>
    private void RefreshIfStale()
    {
        if (DateTime.UtcNow - _lastRefresh > TimeSpan.FromSeconds(2)) Refresh();
    }

    private void LoadPreviews()
    {
        if (_host.Previews is not { } loader) return;
        _previewCts?.Cancel();
        _previewCts = new CancellationTokenSource();
        var todo = _vm.AllCards.Where(c => c.PreviewPath is null && c.Data.ArchivePath is not null)
                               .Select(c => (c.Id, c.Data.ArchivePath!)).ToList();
        if (todo.Count == 0) return;
        var byId = _vm.AllCards.ToDictionary(c => c.Id);
        _ = loader.FillAsync(todo, (id, path) => Dispatcher.BeginInvoke(() =>
        {
            if (byId.TryGetValue(id, out var card)) card.PreviewPath = path;
        }), _previewCts.Token);
    }

    private void Vm_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(GarageViewModel.Toast):
                _toastTimer.Stop();
                if (_vm.Toast is not null) _toastTimer.Start();
                break;
            case nameof(GarageViewModel.Filter):
                PlaceFilterDrop(instant: false);
                break;
            case nameof(GarageViewModel.Selected):
                ScrollToSelected();
                break;
        }
    }

    // ───────────── игра и «ничего не рисуем зря» ─────────────

    private void CheckGame()
    {
        var running = _host.Backend.IsGameRunning();
        var changed = running != _gameRunning;
        _gameRunning = running;
        if (changed) UpdateMotion();
        if (!running && _vm.HasPending) _vm.OnGameExited();
    }

    /// <summary>Фон движется, только когда окно на виду, в фокусе и игра не запущена (DESIGN.md раздел 8, «Бюджет»).</summary>
    private void UpdateMotion() =>
        _motion.SetRunning(IsVisible && WindowState != WindowState.Minimized && IsActive && !_gameRunning && _host.ShotPath is null);

    // ───────────── окно ─────────────

    /// <summary>Развёрнутое окно без рамки Windows вылезает за экран на толщину рамки: убираем это отступом.</summary>
    private void FitMaximized()
    {
        var b = SystemParameters.WindowResizeBorderThickness;
        Root.Margin = WindowState == WindowState.Maximized
            ? new Thickness(b.Left + 4, b.Top + 4, b.Right + 4, b.Bottom + 4)
            : new Thickness(0);
        MaxButton.Tag = FindResource(WindowState == WindowState.Maximized ? "IconRestore" : "IconMax");
        MaxButton.ToolTip = WindowState == WindowState.Maximized ? "Вернуть размер" : "Развернуть";
        System.Windows.Automation.AutomationProperties.SetName(MaxButton, (string)MaxButton.ToolTip);
    }

    private void Min_Click(object sender, RoutedEventArgs e) => SystemCommands.MinimizeWindow(this);

    private void Max_Click(object sender, RoutedEventArgs e)
    {
        if (WindowState == WindowState.Maximized) SystemCommands.RestoreWindow(this);
        else SystemCommands.MaximizeWindow(this);
    }

    private void Close_Click(object sender, RoutedEventArgs e) => SystemCommands.CloseWindow(this);

    /// <summary>Капля перетекает под кнопку, на которую навели; под «Закрыть» она красная.</summary>
    private void CapButton_MouseEnter(object sender, MouseEventArgs e)
    {
        var i = sender == MinButton ? 0 : sender == MaxButton ? 1 : 2;
        var appearing = CapDrop.Opacity < 0.5;
        _springs.To(_capX, -1 + 44 * i, instant: appearing);
        _springs.To(_capScale, 1);
        Fade(CapDrop, 1, 0.2);
        CapDrop.Background = i == 2 ? RedDrop : (Brush)FindResource("DropFill");
        CapDrop.BorderBrush = i == 2 ? RedDropRim : (Brush)FindResource("DropRim");
    }

    private void Caption_MouseLeave(object sender, MouseEventArgs e)
    {
        _springs.To(_capScale, 0.6);
        Fade(CapDrop, 0, 0.2);
    }

    private static readonly Brush RedDrop = Frozen(new LinearGradientBrush(new GradientStopCollection
    {
        new(Color.FromArgb(0x9E, 0xFF, 0x82, 0x76), 0), new(Color.FromArgb(0x85, 0xD6, 0x3A, 0x2E), 0.52), new(Color.FromArgb(0x94, 0xFF, 0x78, 0x6C), 1),
    }, new Point(0.2, 0), new Point(0.8, 1)));
    private static readonly Brush RedDropRim = Frozen(new SolidColorBrush(Color.FromArgb(0x73, 0xFF, 0xBE, 0xB4)));
    private static Brush Frozen(Brush b) { b.Freeze(); return b; }

    private static void Fade(UIElement el, double to, double seconds)
    {
        if (SpringDriver.ReducedMotion) { el.BeginAnimation(OpacityProperty, null); el.Opacity = to; return; }
        el.BeginAnimation(OpacityProperty, new DoubleAnimation(to, TimeSpan.FromSeconds(seconds)));
    }

    private void OnClosing(object? sender, CancelEventArgs e)
    {
        _gameTimer.Stop();
        _previewCts?.Cancel();
        _host.SaveWindow?.Invoke(this);
    }

    // ───────────── левая панель ─────────────

    private void Nav_Checked(object sender, RoutedEventArgs e)
    {
        if (_navDrop is null) return; // InitializeComponent ещё идёт
        var rb = (RadioButton)sender;
        _springs.To(_navDrop, Canvas.GetTop(rb), instant: !IsLoaded);
        var garage = rb == NavGarage;
        SectionTitle.Text = (string)rb.Content;
        Scroller.Visibility = garage ? Visibility.Visible : Visibility.Collapsed;
        EmptyBox.Visibility = garage && _vm.Empty is not null ? Visibility.Visible : Visibility.Collapsed;
        SoonBox.Visibility = garage ? Visibility.Collapsed : Visibility.Visible;
        SoonTitle.Text = (string)rb.Content;
        if (garage) EmptyBox.SetBinding(VisibilityProperty, new System.Windows.Data.Binding(nameof(GarageViewModel.Empty)) { Converter = (System.Windows.Data.IValueConverter)FindResource("NullToCollapsed") });
    }

    // ───────────── фильтры и поиск ─────────────

    private void Filter_Click(object sender, RoutedEventArgs e)
    {
        if (((FrameworkElement)sender).DataContext is FilterItem f) _vm.Filter = f.Filter;
    }

    /// <summary>Капля фильтра ложится под активный пункт; пункты меняют ширину (числа), поэтому меряем по факту.</summary>
    private void PlaceFilterDrop(bool instant)
    {
        var item = _vm.Filters.FirstOrDefault(f => f.IsActive);
        if (item is null || FilterList.ItemContainerGenerator.ContainerFromItem(item) is not FrameworkElement el || el.ActualWidth <= 0) return;
        var x = el.TransformToAncestor(FilterList).Transform(new Point(0, 0)).X;
        var targetX = x - 3;
        var targetW = el.ActualWidth + 6;
        if (Math.Abs(_filterX.Target - targetX) < 0.5 && Math.Abs(_filterW.Target - targetW) < 0.5 && _filterPlaced) return;
        _springs.To(_filterX, targetX, instant);
        _springs.To(_filterW, targetW, instant);
        _filterPlaced = true;
    }

    private void SearchBox_TextChanged(object sender, TextChangedEventArgs e) =>
        SearchHint.Visibility = SearchBox.Text.Length == 0 ? Visibility.Visible : Visibility.Collapsed;

    // ───────────── сетка и правая панель ─────────────

    private void CardPanel_Loaded(object sender, RoutedEventArgs e) => _panel = (CardGridPanel)sender;

    private void Card_Click(object sender, RoutedEventArgs e)
    {
        if (((FrameworkElement)sender).DataContext is ModCard c) _vm.Selected = c;
    }

    private void ScrollToSelected()
    {
        if (_vm.Selected is not { } c) return;
        if (Cards.ItemContainerGenerator.ContainerFromItem(c) is FrameworkElement el)
            el.BringIntoView(new Rect(0, -84, el.ActualWidth, el.ActualHeight + 84 + 20)); // не прятать под верхнюю панель
    }

    private void Toggle_Click(object sender, RoutedEventArgs e) => _vm.ToggleSelected();
    private void Undo_Click(object sender, RoutedEventArgs e) => _vm.Undo();
    private void DismissBanner_Click(object sender, RoutedEventArgs e) => _vm.DismissBanner();

    private void OpenPage_Click(object sender, RoutedEventArgs e)
    {
        if (_vm.Details?.PageUrl is not { } url || url.Scheme is not ("https" or "http")) return;
        try { Process.Start(new ProcessStartInfo(url.AbsoluteUri) { UseShellExecute = true }); }
        catch (Exception ex) when (ex is Win32Exception or InvalidOperationException) { }
    }

    private void MainAction_Click(object sender, RoutedEventArgs e)
    {
        switch (_vm.Details?.Action)
        {
            case MainAction.Enable: _vm.ToggleSelected(); break;
            case MainAction.Update:
            case MainAction.FindReplacement:
                // загрузка и поиск — следующий этап; пока ведём на страницу мода
                OpenPage_Click(sender, e);
                break;
        }
    }

    private void EmptyAction_Click(object sender, RoutedEventArgs e)
    {
        switch (_vm.Empty?.Action)
        {
            case EmptyAction.PickFolder:
                if (_host.PickFolder?.Invoke(this) == true) Refresh();
                break;
            case EmptyAction.ShowAll: _vm.Filter = GarageFilter.All; break;
            case EmptyAction.OpenSearch:
            case EmptyAction.SearchSites: NavSearch.IsChecked = true; break;
            case EmptyAction.CheckNow: NavUpdates.IsChecked = true; break;
        }
    }

    // ───────────── клавиатура (DESIGN_FLOWS раздел 7) ─────────────

    private void OnPreviewKeyDown(object sender, KeyEventArgs e)
    {
        var ctrl = (Keyboard.Modifiers & ModifierKeys.Control) != 0;
        var typing = Keyboard.FocusedElement is TextBox;
        if (ctrl && e.Key == Key.F) { NavGarage.IsChecked = true; SearchBox.Focus(); SearchBox.SelectAll(); e.Handled = true; return; }
        if (ctrl && e.Key == Key.D1) { NavGarage.IsChecked = true; e.Handled = true; return; }
        if (ctrl && e.Key == Key.D2) { NavUpdates.IsChecked = true; e.Handled = true; return; }
        if (ctrl && e.Key == Key.D3) { NavSearch.IsChecked = true; e.Handled = true; return; }
        if (ctrl && e.Key == Key.OemComma) { NavSettings.IsChecked = true; e.Handled = true; return; }
        if (e.Key == Key.Escape)
        {
            if (typing && SearchBox.Text.Length > 0) SearchBox.Clear();
            else if (typing) Keyboard.ClearFocus();
            else _vm.Selected = null;
            e.Handled = true;
            return;
        }
        if (typing || NavGarage.IsChecked != true) return;
        var cols = _panel?.Columns ?? 3;
        switch (e.Key)
        {
            case Key.Left: _vm.MoveSelection(-1, 0, cols); break;
            case Key.Right: _vm.MoveSelection(1, 0, cols); break;
            case Key.Up: _vm.MoveSelection(0, -1, cols); break;
            case Key.Down: _vm.MoveSelection(0, 1, cols); break;
            case Key.Space: _vm.ToggleSelected(); break;
            default: return;
        }
        e.Handled = true;
    }

    // ───────────── снимок для сборочной машины ─────────────

    private async void TakeShot()
    {
        await Task.Delay(1500); // картинки и пружины успевают встать на место
        var path = _host.ShotPath!;
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
            var dpi = VisualTreeHelper.GetDpi(this);
            var rt = new RenderTargetBitmap((int)Math.Ceiling(Root.ActualWidth * dpi.DpiScaleX), (int)Math.Ceiling(Root.ActualHeight * dpi.DpiScaleY),
                96 * dpi.DpiScaleX, 96 * dpi.DpiScaleY, PixelFormats.Pbgra32);
            rt.Render(Root);
            var enc = new PngBitmapEncoder();
            enc.Frames.Add(BitmapFrame.Create(rt));
            using var fs = File.Create(path);
            enc.Save(fs);
        }
        finally
        {
            Close();
        }
    }
}
