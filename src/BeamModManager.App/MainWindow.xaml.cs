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
using BeamModManager.Core.Settings;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Settings;
using BeamModManager.Ui.Previews;

namespace BeamModManager.App;

/// <summary>Что окну нужно снаружи: откуда фон и картинки, как узнать про игру, как выбрать папку.</summary>
public sealed class WindowHost
{
    public required IGarageBackend Backend { get; init; }
    public string? WallPath { get; init; }
    public PreviewLoader? Previews { get; init; }
    /// <summary>«Указать папку»: показывает выбор папки и записывает её в настройки (моды перечитываются по их событию); null — не умеем (демо).</summary>
    public Func<Window, bool>? PickFolder { get; init; }
    public Action<Window>? SaveWindow { get; init; }
    /// <summary>Сделать снимок окна в этот файл и закрыться (сборочная машина).</summary>
    public string? ShotPath { get; init; }
    public string? ShotSelect { get; init; }
    /// <summary>Размер с прошлого запуска (только если он не меньше 1000 × 680, AppSettings).</summary>
    public int StartWidth { get; init; }
    public int StartHeight { get; init; }
    public bool StartMaximized { get; init; }
    public bool DownloadsAllowed { get; init; }
    public int ManyEnabledThreshold { get; init; } = 30;
    public Func<IReadOnlyDictionary<string, bool>>? LoadPending { get; init; }
    public Action<IReadOnlyDictionary<string, bool>>? SavePending { get; init; }
    /// <summary>Дописать библиотеку на диск при закрытии.</summary>
    public Func<bool>? Flush { get; init; }
    /// <summary>Самообновление; null — недоступно (сборка для разработки, демо).</summary>
    public BeamModManager.Core.AppUpdate.AppUpdater? Updater { get; init; }
    public bool CheckUpdatesOnStart { get; init; }
    /// <summary>Сказать при открытии окна (например, «Программа обновлена до 0.1.30»).</summary>
    public string? StartupToast { get; set; }
    /// <summary>Плашка «Вышла новая версия» (создаёт App).</summary>
    public BeamModManager.Ui.AppUpdate.UpdateBanner? Banner { get; set; }
    public double? ShotWidth { get; init; }
    public double? ShotHeight { get; init; }
    /// <summary>Экран «Настройки»: всё применяется сразу; файл пишет App по событию <see cref="SettingsViewModel.Changed"/>.</summary>
    public required SettingsViewModel Settings { get; init; }
    /// <summary>Открыть при запуске (снимки сборочной машины): "settings" — настройки.</summary>
    public string? StartPage { get; init; }
    /// <summary>«Выбрать фото» для фона: диалог, копия в папку программы, запись в настройки; null — не умеем (демо).</summary>
    public Func<Window, bool>? PickWallpaper { get; init; }
    public SettingsSection StartSection { get; init; }
}

public partial class MainWindow : Window
{
    private readonly GarageViewModel _vm;
    private readonly WindowHost _host;
    private readonly SpringDriver _springs = new();
    private readonly Spring _navDrop, _filterX, _filterW, _capX, _capScale, _inspX;
    private readonly BackgroundMotion _motion;
    private readonly DispatcherTimer _gameTimer = new() { Interval = TimeSpan.FromSeconds(3) };
    private readonly DispatcherTimer _toastTimer = new() { Interval = TimeSpan.FromSeconds(6) };
    /// <summary>Пока изменение передано помощнику в игре, раз в секунду смотрим, применила ли его игра.</summary>
    private readonly DispatcherTimer _liveTimer = new() { Interval = TimeSpan.FromSeconds(1) };
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
        if (host.ShotPath is not null)
        {
            // Экран сборочной машины меньше окна, а больше экрана Windows окно не пускает. Canvas детей не обрезает:
            // содержимое раскладывается в нужном размере и снимается целиком, окно при этом живое.
            Content = null;
            Root.Width = host.ShotWidth ?? 1440;
            Root.Height = host.ShotHeight ?? 960;
            Content = new Canvas { Children = { Root } };
        }

        _navDrop = _springs.Add(new Spring(0.5, 0.15, 14), v => NavDropMove.Y = v);
        _filterX = _springs.Add(new Spring(0.5, 0.15, -3), v => FilterDropMove.X = v);
        _filterW = _springs.Add(new Spring(0.5, 0.15, 60), v => FilterDrop.Width = Math.Max(20, v));
        _capX = _springs.Add(new Spring(0.5, 0.15, -1), v => CapDropMove.X = v);
        _capScale = _springs.Add(new Spring(0.35, 0.15, 0.6), v => { CapDropScale.ScaleX = v; CapDropScale.ScaleY = v; });
        _inspX = _springs.Add(new Spring(0.5, 0, 344), v =>
        {
            InspectorMove.X = v;
            if (v >= 343 && _vm.Selected is null) Inspector.Visibility = Visibility.Collapsed; // уехала: стекло не считаем зря
        });

        // на экране меньше 1440 × 960 окно открывается по размеру экрана минус рамка (DESIGN_FLOWS раздел 2)
        var work = SystemParameters.WorkArea;
        if (Width > work.Width - 16) Width = Math.Max(MinWidth, work.Width - 16);
        if (Height > work.Height - 16) Height = Math.Max(MinHeight, work.Height - 16);

        if (host.StartWidth >= MinWidth && host.StartHeight >= MinHeight)
        {
            Width = Math.Min(host.StartWidth, SystemParameters.VirtualScreenWidth);
            Height = Math.Min(host.StartHeight, SystemParameters.VirtualScreenHeight);
        }
        if (host.StartMaximized) WindowState = WindowState.Maximized;

        _motion = new BackgroundMotion(this, WallScale, WallMove, GradeMove, Lights, Flies);
        _glass = new[] { Nav, TopBar, Inspector, Caption, ToastBox, SettingsPanel }.Select(g => (G: g, Blur: g.Blur, Tint: g.Tint)).ToArray();
        SettingsPanel.DataContext = host.Settings;
        host.Settings.Section = host.StartSection;
        host.Settings.Changed += OnSettingsChanged;
        _folderKey = FolderKey(host.Settings.Current);
        ApplyLook(host.Settings.Current);
        SettingsPanel.SizeChanged += (_, _) => AdaptSettings();

        vm.PropertyChanged += Vm_PropertyChanged;
        _toastTimer.Tick += (_, _) => { _toastTimer.Stop(); if (_vm.Toast is { } t) _vm.DismissToast(t.Serial); };
        _gameTimer.Tick += (_, _) => CheckGame();
        _liveTimer.Tick += (_, _) => { _vm.TickLive(); if (!_vm.LiveWaiting) _liveTimer.Stop(); };

        SourceInitialized += (_, _) =>
        {
            Native.StyleWindow(this);
            // фото карточек декодируются под масштаб Windows: на 150 % нужно в полтора раза больше пикселей
            ImageCache.DecodeHeight = (int)Math.Ceiling(260 * VisualTreeHelper.GetDpi(this).DpiScaleY);
            // уход с «Развернуть» на соседнюю кнопку: WPF узнаёт о новой кнопке чуть позже — проверяем после него, иначе капля мигнёт
            SnapLayouts.Attach(this, MaxButton, over =>
                {
                    if (over) CapButton_MouseEnter(MaxButton, null);
                    else Dispatcher.BeginInvoke(() => { if (!Caption.IsMouseOver) HideCapDrop(); }, DispatcherPriority.Input);
                },
                () => Max_Click(MaxButton, new RoutedEventArgs()));
        };
        Root.SizeChanged += (_, _) => ApplyWidthClass();
        TopBar.SizeChanged += (_, _) => AdaptTopBar();
        // числа в фильтрах меняют их ширину: пересчитываем панель, когда они меняются
        foreach (var f in vm.Filters) f.PropertyChanged += (_, _) => Dispatcher.BeginInvoke(AdaptTopBar, DispatcherPriority.Loaded);
        vm.PropertyChanged += (_, a) => { if (a.PropertyName == nameof(GarageViewModel.CountText)) Dispatcher.BeginInvoke(AdaptTopBar, DispatcherPriority.Loaded); };
        Loaded += (_, _) => Dispatcher.BeginInvoke(DispatcherPriority.Loaded, AdaptTopBar);
        Loaded += OnLoaded;
        Loaded += (_, _) => FitMaximized();
        // свернули или развернули окно: Windows не сообщает, что мышь ушла с кнопки, и капля осталась бы висеть
        StateChanged += (_, _) => { FitMaximized(); UpdateMotion(); HideCapDrop(instant: true); };
        Activated += (_, _) => { UpdateMotion(); RefreshIfStale(); if (!Caption.IsMouseOver) HideCapDrop(instant: true); };
        Deactivated += (_, _) => UpdateMotion();
        IsVisibleChanged += (_, _) => UpdateMotion();
        Closing += OnClosing;
        PreviewKeyDown += OnPreviewKeyDown;
        FilterList.LayoutUpdated += (_, _) => PlaceFilterDrop(instant: !_filterPlaced);
    }

    private bool _filterPlaced;
    private readonly CancellationTokenSource _closing = new();

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        // переключения, отложенные до закрытия игры в прошлый раз (менеджер закрыли раньше игры)
        if (_host.LoadPending?.Invoke() is { Count: > 0 } pending) _vm.RestorePending(pending);
        Refresh();
        _gameTimer.Start();
        CheckGame();
        UpdateMotion();
        if (_host.StartupToast is { } toast) _vm.Notify(toast);
        if (_host.Banner is { } banner)
        {
            UpdateBox.DataContext = banner;
            SettingsUpdateBox.DataContext = banner;
            if (_host.CheckUpdatesOnStart)
                Dispatcher.BeginInvoke(async () =>
                {
                    await Task.Delay(TimeSpan.FromSeconds(5)); // не мешаем запуску
                    if (IsLoaded) await banner.CheckOnStartAsync(_closing.Token);
                }, DispatcherPriority.ApplicationIdle);
        }
        if (_host.StartPage == "settings") NavSettings.IsChecked = true;
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
                if (_vm.LiveWaiting && !_liveTimer.IsEnabled) _liveTimer.Start();
                break;
            case nameof(GarageViewModel.Filter):
                PlaceFilterDrop(instant: false);
                break;
            case nameof(GarageViewModel.Selected):
                PlaceInspector();
                // сетка могла перестроиться (панель заняла место): докручиваем, когда раскладка готова
                Dispatcher.BeginInvoke(ScrollToSelected, DispatcherPriority.Loaded);
                break;
        }
    }

    // ───────────── игра и «ничего не рисуем зря» ─────────────

    private bool _checking;

    /// <summary>Поиск процесса игры занимает десятки миллисекунд: делаем его в фоне, чтобы анимация не запиналась.</summary>
    private async void CheckGame()
    {
        if (_checking) return;
        _checking = true;
        bool running;
        try { running = await Task.Run(_host.Backend.IsGameRunning); }
        finally { _checking = false; }
        var changed = running != _gameRunning;
        _gameRunning = running;
        if (changed) UpdateMotion();
        if (!running && _vm.HasPending) _vm.OnGameExited();
        else if (running && _vm.HasPending && !_liveTimer.IsEnabled) _vm.TickLive(); // помощник мог появиться позже: передадим ещё раз
    }

    /// <summary>
    /// Фон движется, только когда окно на виду, в фокусе и игра не запущена (DESIGN.md раздел 8, «Бюджет»), и только то,
    /// что включено в настройках. При фоне «Цвет» двигать нечего.
    /// </summary>
    private void UpdateMotion()
    {
        var s = _host.Settings.Current;
        var live = IsVisible && WindowState != WindowState.Minimized && IsActive && !_gameRunning && _host.ShotPath is null;
        var photo = s.Background != BackgroundMode.SolidColor;
        _motion.SetRunning(live && photo && s.AnimatedBackground, live && photo && s.Fireflies);
    }

    // ───────────── внешний вид из настроек (DESIGN.md 5.1) ─────────────

    private readonly (Glass G, double Blur, Brush Tint)[] _glass;
    private readonly Dictionary<string, BitmapSource?> _walls = new();
    private string _folderKey;

    /// <summary>Матовое стекло (DESIGN.md 3.2): размытие 26 px, плотная подложка.</summary>
    private static readonly Brush MatteTint = Frozen(new SolidColorBrush(Color.FromArgb(0xA8, 34, 46, 68)));

    private static string FolderKey(AppSettings s) => $"{s.ModsFolder}|{s.UserFolder}|{s.KeepBackups}";

    private void OnSettingsChanged(AppSettings s)
    {
        ApplyLook(s);
        _vm.ManyEnabledThreshold = s.ActiveModsWarnAbove;
        // папку уже переключила программа (App подписана раньше окна): перечитываем моды
        var key = FolderKey(s);
        if (key != _folderKey) { _folderKey = key; if (IsLoaded) Refresh(); }
    }

    /// <summary>Применяет внешний вид сразу, без перезапуска: стекло, иней, фон, затемнение, движение.</summary>
    private void ApplyLook(AppSettings s)
    {
        SpringDriver.AnimationsOff = !s.UiAnimations;
        Resources["FrostOpacity"] = s.Frost ? s.FrostStrength switch { 0 => 0.45, 2 => 1.0, _ => 0.75 } : 0.0;

        var matte = !s.LiquidGlass;
        foreach (var (g, blur, tint) in _glass)
        {
            g.Blur = matte ? 26 : blur;
            g.Tint = matte ? MatteTint : tint;
        }

        var photo = s.Background != BackgroundMode.SolidColor;
        var wall = photo ? WallImage(s.BackgroundImage ?? _host.WallPath, s.Background == BackgroundMode.BlurredPhoto ? 40 : 16) : null;
        Wall.Source = wall;
        Wall.Visibility = wall is null ? Visibility.Collapsed : Visibility.Visible;
        GradeLayer.Visibility = Lights.Visibility = photo ? Visibility.Visible : Visibility.Collapsed;
        Flies.Visibility = photo && s.Fireflies ? Visibility.Visible : Visibility.Collapsed;

        // 35 % — как на доске (только градиент); меньше — градиент слабее, больше — сверху ровный тёмный слой
        var dim = photo ? s.BackgroundDim : 35;
        Shade.Opacity = Math.Min(1, dim / 35.0);
        ShadeExtra.Opacity = dim > 35 ? (dim - 35) / 65.0 * 0.7 : 0;

        UpdateMotion();
    }

    /// <summary>Фото-фон с нужным размытием; размывается один раз и запоминается (переключать туда-обратно дёшево).</summary>
    private BitmapSource? WallImage(string? path, double blur)
    {
        if (path is null) return null;
        var key = $"{blur}|{path}";
        if (!_walls.TryGetValue(key, out var img))
        {
            if (_walls.Count > 4) _walls.Clear(); // прежние фото больше не нужны
            _walls[key] = img = Services.Backdrop.LoadBlurred(path, blur);
        }
        return img;
    }

    // ───────────── окно ─────────────

    /// <summary>Развёрнутое окно без рамки Windows вылезает за экран на толщину рамки: убираем это отступом.</summary>
    private void FitMaximized()
    {
        var b = SystemParameters.WindowResizeBorderThickness;
        Root.Margin = WindowState == WindowState.Maximized ? b : new Thickness(0);
        MaxButton.Tag = FindResource(WindowState == WindowState.Maximized ? "IconRestore" : "IconMax");
        MaxButton.ToolTip = WindowState == WindowState.Maximized ? "Вернуть размер" : "Развернуть";
        System.Windows.Automation.AutomationProperties.SetName(MaxButton, (string)MaxButton.ToolTip);
    }

    // ───────────── классы ширины (DESIGN_FLOWS раздел 2) ─────────────

    private bool _narrow;

    /// <summary>
    /// Правая панель есть, только когда выбран мод; без выбора её место отдаётся сетке. Шире 1100 она стоит рядом с сеткой,
    /// на 1000–1099 выезжает поверх неё с затемнением (DESIGN_FLOWS раздел 2).
    /// </summary>
    private void ApplyWidthClass()
    {
        _narrow = Root.ActualWidth < 1100;
        PlaceInspector(instant: !IsLoaded);
    }

    private void PlaceInspector(bool instant = false)
    {
        var open = _vm.Selected is not null;
        var beside = open && !_narrow; // панель занимает место рядом с сеткой
        var right = beside ? 360 : 16;
        var gridMargin = new Thickness(116, 0, right, 0);
        if (Scroller.Margin != gridMargin)
        {
            Scroller.Margin = gridMargin;
            EmptyBox.Margin = SoonBox.Margin = new Thickness(116, 84, right, 0);
            ToastBox.Margin = new Thickness(116, 0, right, 28);
            TopFade.Margin = new Thickness(104, 0, right - 12, 0);
            TopBar.Margin = new Thickness(116, 16, beside ? 360 : 132 + 16 + 16, 0);
        }
        Dim.Visibility = open && _narrow ? Visibility.Visible : Visibility.Collapsed;
        if (open) Inspector.Visibility = Visibility.Visible;
        _springs.To(_inspX, open ? 0 : 344, instant);
        if (!open && (instant || SpringDriver.ReducedMotion)) Inspector.Visibility = Visibility.Collapsed;
    }

    private void Dim_MouseDown(object sender, MouseButtonEventArgs e)
    {
        _vm.Selected = null;
        e.Handled = true; // иначе щелчок дойдёт до окна и начнёт его перетаскивать
    }

    /// <summary>
    /// Окно перетаскивается за любое пустое место (DESIGN_FLOWS раздел 2). Кнопки, поле поиска и полоса прокрутки
    /// обрабатывают нажатие сами, сюда оно от них не доходит.
    /// </summary>
    private void Root_MouseLeftButtonDown(object sender, MouseButtonEventArgs e)
    {
        if (_host.ShotPath is not null || e.ClickCount != 1 || e.ButtonState != MouseButtonState.Pressed) return;
        try { DragMove(); } catch (InvalidOperationException) { } // кнопку уже отпустили
    }

    /// <summary>
    /// Верхняя панель сжимается по шагам, когда не хватает места: убирается число модов (оно есть в правой панели),
    /// поиск сужается, затем убирается заголовок (активный пункт левой панели и так говорит, где мы).
    /// </summary>
    private void AdaptTopBar()
    {
        var avail = TopBar.ActualWidth - 34;
        if (avail <= 0) return;
        if (NavGarage.IsChecked != true) { SectionTitle.Visibility = Visibility.Visible; return; } // кроме заголовка там ничего нет
        // ширины меряем по тексту, а не по раскладке: у скрытого элемента она нулевая, и панель прыгала бы
        // между «влезает» и «не влезает» (заголовок то показывается, то прячется)
        FilterBox.Measure(new Size(double.PositiveInfinity, 52));
        var filters = FilterBox.DesiredSize.Width;
        var title = TextWidth(SectionTitle);
        var count = TextWidth(CountLabel) + 14;
        bool Fits(double search, bool withTitle, bool withCount) =>
            filters + 14 + search + (withTitle ? title + 14 : 0) + (withCount ? count : 0) <= avail;

        (double s, bool ti, bool co) = Fits(200, true, true) ? (200, true, true)
            : Fits(200, true, false) ? (200, true, false)
            : Fits(150, true, false) ? (150, true, false)
            : Fits(150, false, false) ? (150, false, false)
            : (Math.Max(110, avail - filters - 14), false, false);
        if (SearchField.Width != s) SearchField.Width = s;
        var tv = ti ? Visibility.Visible : Visibility.Collapsed;
        if (SectionTitle.Visibility != tv) SectionTitle.Visibility = tv;
        var cv = co ? Visibility.Visible : Visibility.Collapsed;
        if (CountLabel.Visibility != cv) CountLabel.Visibility = cv;
        var hint = s >= 150 ? "Найти мод" : "Найти";
        if (SearchHint.Text != hint) SearchHint.Text = hint;
    }

    private double TextWidth(TextBlock tb)
    {
        var ft = new FormattedText(tb.Text ?? "", System.Globalization.CultureInfo.CurrentUICulture, FlowDirection.LeftToRight,
            new Typeface(tb.FontFamily, tb.FontStyle, tb.FontWeight, tb.FontStretch), tb.FontSize, Brushes.White,
            VisualTreeHelper.GetDpi(this).PixelsPerDip);
        return Math.Ceiling(ft.WidthIncludingTrailingWhitespace) + tb.Margin.Left + tb.Margin.Right;
    }

    private void Min_Click(object sender, RoutedEventArgs e)
    {
        HideCapDrop(instant: true);
        SystemCommands.MinimizeWindow(this);
    }

    private void Max_Click(object sender, RoutedEventArgs e)
    {
        HideCapDrop(instant: true);
        if (WindowState == WindowState.Maximized) SystemCommands.RestoreWindow(this);
        else SystemCommands.MaximizeWindow(this);
    }

    private void Close_Click(object sender, RoutedEventArgs e) => SystemCommands.CloseWindow(this);

    /// <summary>Капля перетекает под кнопку, на которую навели; под «Закрыть» она красная.</summary>
    private void CapButton_MouseEnter(object sender, MouseEventArgs? e)
    {
        var i = sender == MinButton ? 0 : sender == MaxButton ? 1 : 2;
        var appearing = CapDrop.Opacity < 0.5;
        _springs.To(_capX, -1 + 44 * i, instant: appearing);
        _springs.To(_capScale, 1);
        Fade(CapDrop, 1, 0.2);
        CapDrop.Background = i == 2 ? RedDrop : (Brush)FindResource("DropFill");
        CapDrop.BorderBrush = i == 2 ? RedDropRim : (Brush)FindResource("DropRim");
    }

    private void Caption_MouseLeave(object sender, MouseEventArgs e) => HideCapDrop();

    private void HideCapDrop(bool instant = false)
    {
        _springs.To(_capScale, 0.6, instant);
        Fade(CapDrop, 0, instant ? 0 : 0.2);
    }

    private static readonly Brush RedDrop = Frozen(new LinearGradientBrush(new GradientStopCollection
    {
        new(Color.FromArgb(0x9E, 0xFF, 0x82, 0x76), 0), new(Color.FromArgb(0x85, 0xD6, 0x3A, 0x2E), 0.52), new(Color.FromArgb(0x94, 0xFF, 0x78, 0x6C), 1),
    }, new Point(0.2, 0), new Point(0.8, 1)));
    private static readonly Brush RedDropRim = Frozen(new SolidColorBrush(Color.FromArgb(0x73, 0xFF, 0xBE, 0xB4)));
    private static Brush Frozen(Brush b) { b.Freeze(); return b; }

    private static void Fade(UIElement el, double to, double seconds)
    {
        if (SpringDriver.ReducedMotion || seconds <= 0) { el.BeginAnimation(OpacityProperty, null); el.Opacity = to; return; }
        el.BeginAnimation(OpacityProperty, new DoubleAnimation(to, TimeSpan.FromSeconds(seconds)));
    }

    private void OnClosing(object? sender, CancelEventArgs e)
    {
        _closing.Cancel(); // проверка или загрузка обновления прерываются без следов
        _gameTimer.Stop();
        _liveTimer.Stop();
        _previewCts?.Cancel();
        _host.SaveWindow?.Invoke(this);
        _host.SavePending?.Invoke(_vm.PendingChanges);
        _host.Flush?.Invoke();
    }

    // ───────────── левая панель ─────────────

    private void Nav_Checked(object sender, RoutedEventArgs e)
    {
        if (_navDrop is null) return; // InitializeComponent ещё идёт
        var rb = (RadioButton)sender;
        _springs.To(_navDrop, Canvas.GetTop(rb), instant: !IsLoaded);
        var garage = rb == NavGarage;
        var settings = rb == NavSettings;
        if (!garage) _vm.Selected = null; // правая панель — про мод из гаража, в других разделах ей не место
        SectionTitle.Text = (string)rb.Content;
        Scroller.Visibility = garage ? Visibility.Visible : Visibility.Collapsed;
        EmptyBox.Visibility = garage && _vm.Empty is not null ? Visibility.Visible : Visibility.Collapsed;
        SoonBox.Visibility = garage || settings ? Visibility.Collapsed : Visibility.Visible;
        SettingsPanel.Visibility = settings ? Visibility.Visible : Visibility.Collapsed;
        SoonTitle.Text = (string)rb.Content;
        // фильтры, поиск и число модов — про список модов; в других разделах им не место
        var gv = garage ? Visibility.Visible : Visibility.Collapsed;
        FilterBox.Visibility = SearchField.Visibility = CountLabel.Visibility = gv;
        if (garage) Dispatcher.BeginInvoke(AdaptTopBar, DispatcherPriority.Loaded);
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
        // щелчок мышью по уже выбранной карточке закрывает правую панель; Enter с клавиатуры — только открывает
        if (((FrameworkElement)sender).DataContext is not ModCard c) return;
        var byKeyboard = InputManager.Current.MostRecentInputDevice is KeyboardDevice;
        _vm.Selected = ReferenceEquals(_vm.Selected, c) && !byKeyboard ? null : c;
    }

    private void ScrollToSelected()
    {
        if (_vm.Selected is not { } c) return;
        if (Cards.ItemContainerGenerator.ContainerFromItem(c) is FrameworkElement el)
            el.BringIntoView(new Rect(0, -84, el.ActualWidth, el.ActualHeight + 84 + 20)); // не прятать под верхнюю панель
    }

    /// <summary>Фокус с клавиатуры идёт за выбором: иначе белая рамка осталась бы на прежней карточке.</summary>
    private void FocusSelected()
    {
        if (_vm.Selected is not { } c) return;
        if (Cards.ItemContainerGenerator.ContainerFromItem(c) is DependencyObject cp && VisualTreeHelper.GetChildrenCount(cp) > 0 &&
            VisualTreeHelper.GetChild(cp, 0) is UIElement btn)
            btn.Focus();
    }

    private void Toggle_Click(object sender, RoutedEventArgs e) => _vm.ToggleSelected();
    private void Undo_Click(object sender, RoutedEventArgs e) => _vm.Undo();
    private void DismissBanner_Click(object sender, RoutedEventArgs e) => _vm.DismissBanner();
    private void UpdateAct_Click(object sender, RoutedEventArgs e) => _host.Banner?.Act(_closing.Token);
    private void UpdateLater_Click(object sender, RoutedEventArgs e) => _host.Banner?.Dismiss();

    // ───────────── настройки ─────────────

    /// <summary>Внешний вид в две колонки, на узком окне в одну (как на доске при 1440 и на 1000–1100).</summary>
    private void AdaptSettings()
    {
        var one = SettingsScroll.ActualWidth > 0 && SettingsScroll.ActualWidth < 780;
        Grid.SetRow(AppearanceRight, one ? 1 : 0);
        Grid.SetColumn(AppearanceRight, one ? 0 : 1);
        Grid.SetColumnSpan(AppearanceLeft, one ? 2 : 1);
        Grid.SetColumnSpan(AppearanceRight, one ? 2 : 1);
        Grid.SetRow(AppearanceFoot, one ? 2 : 1);
        AppearanceLeft.Margin = one ? new Thickness(0) : new Thickness(0, 0, 14, 0);
        AppearanceRight.Margin = one ? new Thickness(0) : new Thickness(14, 0, 0, 0);
        AppearanceRightCol.Width = one ? new GridLength(0) : new GridLength(1, GridUnitType.Star);
    }

    private void ResetLook_Click(object sender, RoutedEventArgs e) => _host.Settings.ResetAppearance();
    private void PickWall_Click(object sender, RoutedEventArgs e) => _host.PickWallpaper?.Invoke(this);
    private void RemoveWall_Click(object sender, RoutedEventArgs e) => _host.Settings.RemoveBackgroundImage();
    private void LessWarn_Click(object sender, RoutedEventArgs e) => _host.Settings.LessWarn();
    private void MoreWarn_Click(object sender, RoutedEventArgs e) => _host.Settings.MoreWarn();
    private void LessBackups_Click(object sender, RoutedEventArgs e) => _host.Settings.LessBackups();
    private void MoreBackups_Click(object sender, RoutedEventArgs e) => _host.Settings.MoreBackups();
    private void AutoFolder_Click(object sender, RoutedEventArgs e) => _host.Settings.UseAutoFolder();
    private void CheckApp_Click(object sender, RoutedEventArgs e) => _host.Banner?.CheckNowAsync(_closing.Token);

    // папку меняют настройки: моды перечитываются по их событию (OnSettingsChanged), здесь только диалог
    private void PickFolder_Click(object sender, RoutedEventArgs e) => _host.PickFolder?.Invoke(this);

    private void OpenFolder_Click(object sender, RoutedEventArgs e)
    {
        if (_host.Settings.ModsFolderPath is not { } dir || !Directory.Exists(dir)) return;
        try { Process.Start(new ProcessStartInfo("explorer.exe", $"\"{dir}\"") { UseShellExecute = false }); }
        catch (Exception ex) when (ex is Win32Exception or InvalidOperationException) { }
    }

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
            case MainAction.OpenPage: OpenPage_Click(sender, e); break;
            // загрузки и поиска по сайтам в этой сборке ещё нет: «Обновить» ведёт на страницу, «Найти замену» — в раздел «Поиск»
            case MainAction.Update: OpenPage_Click(sender, e); break;
            case MainAction.FindReplacement: NavSearch.IsChecked = true; break;
        }
    }

    private void EmptyAction_Click(object sender, RoutedEventArgs e)
    {
        switch (_vm.Empty?.Action)
        {
            case EmptyAction.PickFolder:
                _host.PickFolder?.Invoke(this);
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
        // Alt+Пробел — системное меню окна, как у обычных окон Windows
        if (e.Key == Key.System && e.SystemKey == Key.Space && (Keyboard.Modifiers & ModifierKeys.Alt) != 0)
        {
            SystemCommands.ShowSystemMenu(this, PointToScreen(new Point(16, 16)));
            e.Handled = true;
            return;
        }
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
        // стрелки и пробел — для карточек; у кнопок фильтров, панели и окна они делают своё обычное дело
        if (Keyboard.FocusedElement is System.Windows.Controls.Primitives.ButtonBase btn && btn.DataContext is not ModCard) return;
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
        FocusSelected();
    }

    // ───────────── снимок для сборочной машины ─────────────

    private async void TakeShot()
    {
        await Task.Delay(1500); // картинки и пружины успевают встать на место
        var path = _host.ShotPath!;
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
            Root.UpdateLayout();
            var rt = new RenderTargetBitmap((int)Math.Ceiling(Root.ActualWidth), (int)Math.Ceiling(Root.ActualHeight), 96, 96, PixelFormats.Pbgra32);
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
