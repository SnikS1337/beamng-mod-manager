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
using BeamModManager.Ui.Look;
using BeamModManager.Ui.Settings;
using BeamModManager.Ui.Updates;
using BeamModManager.Ui.Previews;

namespace BeamModManager.App;

/// <summary>Что окну нужно снаружи: откуда фон и картинки, как узнать про игру, как выбрать папку.</summary>
public sealed class WindowHost
{
    public required IGarageBackend Backend { get; init; }
    public PreviewLoader? Previews { get; init; }
    /// <summary>«Указать папку»: диалог выбора и проверка; возвращает папку mods или null (отказались, не подходит). null — не умеем (демо).</summary>
    public Func<Window, string?>? ChooseFolder { get; init; }
    /// <summary>Взять папку модов (уже проверенную): записывает её в настройки. true — настройки изменились (моды
    /// перечитаются по их событию); false — та же папка, помощник переключён заново, перечитать должно окно.</summary>
    public Func<string, bool>? UseFolder { get; init; }
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
    /// <summary>Раздел «Обновления»: проверка, действия, привязка страниц. null — проверять нечем (раздел покажет пустоту).</summary>
    public IUpdatesBackend? Updates { get; init; }
    /// <summary>Открыть при запуске (снимки сборочной машины): "settings" — настройки.</summary>
    public string? StartPage { get; init; }
    public SettingsSection StartSection { get; init; }
    /// <summary>Фильтр «Обновлений» при запуске (снимки): Available, Hidden, Missing.</summary>
    public string? StartFilter { get; init; }
}

public partial class MainWindow : Window
{
    private readonly GarageViewModel _vm;
    private readonly UpdatesViewModel _upd;
    /// <summary>Повтор проверки раз в N часов (настройка «Повторять»): раз в минуту смотрим, не пора ли.</summary>
    private readonly DispatcherTimer _autoCheckTimer = new() { Interval = TimeSpan.FromMinutes(1) };
    private readonly WindowHost _host;
    private readonly SpringDriver _springs = new();
    private readonly Spring _navDrop, _filterX, _filterW, _capX, _capScale, _inspX;
    /// <summary>Нажатие на каплю: она чуть сжимается (0,92) и упруго возвращается (0,4 с, упругость 0,3 — «физическое»).</summary>
    private readonly Spring _navPress, _filterPress, _capPress;
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

        // пружины создаются до своих «применить»: те читают скорость и сжатие соседних пружин
        _navPress = new Spring(0.4, 0.3, 1);
        _filterPress = new Spring(0.4, 0.3, 1);
        _capPress = new Spring(0.4, 0.3, 1);
        _navDrop = new Spring(0.5, 0.15, 14);
        _filterX = new Spring(0.5, 0.15, -3);
        _capX = new Spring(0.5, 0.15, -1);
        _capScale = new Spring(0.35, 0.15, 0.6);
        _springs.Add(_navPress, _ => NavDropShape());
        _springs.Add(_filterPress, _ => FilterDropShape());
        _springs.Add(_capPress, _ => CapDropShape());
        _springs.Add(_navDrop, v => { NavDropMove.Y = v; NavDropShape(); });
        _springs.Add(_filterX, v => { FilterDropMove.X = v; FilterDropShape(); });
        _filterW = _springs.Add(new Spring(0.5, 0.15, 60), v => { FilterDrop.Width = Math.Max(20, v); FilterDrop.Sync(); });
        _springs.Add(_capX, v => { CapDropMove.X = v; CapDropShape(); });
        _springs.Add(_capScale, _ => CapDropShape());
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

        // иней выключенных карточек: тот же генератор, что у панелей, размер карточки ×2 (чётко до масштаба 200 %);
        // ~60 мс один раз вместо картинки на 300 КБ в программе
        var frostPx = BeamModManager.Ui.Glass.GlassTextures.Ice(616, 472, 52, 120, 15);
        var frostImg = BitmapSource.Create(616, 472, 96, 96, PixelFormats.Pbgra32, null, frostPx, 616 * 4);
        Resources["CardFrost"] = Frozen(new ImageBrush(Frozen(frostImg)) { Stretch = Stretch.Fill });
        _motion = new BackgroundMotion(this, GradeMove, Lights, Flies);
        _glass = new[] { Nav, TopBar, Inspector, Caption, ToastBox, SettingsPanel, PreviewBar, CheckPanel, BindPanel }.Select(g => (G: g, Tint: g.Tint)).ToArray();
        // «Обновления»: список из тех же карточек гаража, проверка через один координатор
        _upd = new UpdatesViewModel(vm, host.Updates ?? new NoUpdates(), host.Settings, a => Dispatcher.BeginInvoke(a));
        UpdScroller.DataContext = UpdEmptyBox.DataContext = CheckPanel.DataContext = _upd;
        _upd.PropertyChanged += Upd_PropertyChanged;
        foreach (var f in _upd.Filters) f.PropertyChanged += (_, _) => Dispatcher.BeginInvoke(AdaptTopBar, DispatcherPriority.Loaded);
        SettingsPanel.DataContext = host.Settings;
        // «Как это выглядит»: под полоской стекла — тот же фон окна (живой) и картинки модов
        PvBack.Fill = new VisualBrush(Backdrop) { Stretch = Stretch.UniformToFill, AlignmentY = AlignmentY.Top };
        PvCount.SetBinding(TextBlock.TextProperty, new System.Windows.Data.Binding(nameof(GarageViewModel.CountText)) { Source = vm });
        host.Settings.Section = host.StartSection;
        host.Settings.Changed += OnSettingsChanged;
        _folderKey = FolderKey(host.Settings.Current);
        ApplyLook(host.Settings.Current);
        SettingsPanel.SizeChanged += (_, _) => AdaptSettings();
        // в Windows переключили «Эффекты прозрачности» или анимацию: применяем сразу, без перезапуска
        Microsoft.Win32.SystemEvents.UserPreferenceChanged += OnUserPreferenceChanged;
        Closed += (_, _) => Microsoft.Win32.SystemEvents.UserPreferenceChanged -= OnUserPreferenceChanged;

        // вход окна (DESIGN.md 6 «Один вход»): не в снимках сборочной машины и не при выключенных анимациях.
        // Начальное положение ставим сразу, до первого кадра; сами анимации — в OnLoaded, карточки — в CardGridPanel.
        _entrance = host.ShotPath is null && !SpringDriver.ReducedMotion;
        if (_entrance)
        {
            TopBar.Opacity = 0;
            Anim.Mover(TopBar).Y = EnterTop;
            Nav.Opacity = 0;
            Anim.Mover(Nav).X = EnterLeft;
            CardGridPanel.SetRise(Cards, 24);
        }
        UpdatesBadge.IsVisibleChanged += (_, _) => UpdateBadgePulse();

        vm.PropertyChanged += Vm_PropertyChanged;
        _toastTimer.Tick += (_, _) => { _toastTimer.Stop(); if (_vm.Toast is { } t) _vm.DismissToast(t.Serial); };
        _gameTimer.Tick += (_, _) => CheckGame();
        _autoCheckTimer.Tick += (_, _) => _ = _upd.TickAsync();
        _linkTimer.Tick += LinkTimer_Tick;
        _liveTimer.Tick += (_, _) => { _vm.TickLive(); if (!_vm.LiveWaiting) _liveTimer.Stop(); };

        SourceInitialized += (_, _) =>
        {
            Native.StyleWindow(this);
            Native.SetDarkFrame(this, _light != true);
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
        Root.SizeChanged += (_, _) => { ApplyWidthClass(); PlaceBlobs(TimeSpan.Zero); };
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
        if (_entrance) PlayEntrance();
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
        if (_host.StartPage is "updates" or "bind") NavUpdates.IsChecked = true;
        if (Enum.TryParse<UpdatesFilter>(_host.StartFilter, true, out var uf) && Enum.IsDefined(uf)) _upd.Filter = uf;
        if (_host.StartPage == "bind" && _vm.AllCards.FirstOrDefault(c => !c.Data.Tracked) is { } untracked) OpenBind(untracked, null);
        if (_host.ShotPath is null)
        {
            // проверка при запуске (настройка) — когда окно уже на экране и картинки грузятся, не мешая им
            Dispatcher.BeginInvoke(async () =>
            {
                await Task.Delay(TimeSpan.FromSeconds(3));
                if (IsLoaded) await _upd.StartupAsync();
            }, DispatcherPriority.ApplicationIdle);
            _autoCheckTimer.Start();
        }
        if (_host.ShotSelect is { } name && _vm.Cards.FirstOrDefault(c => c.Name == name) is { } sel) _vm.Selected = sel;
        if (_host.ShotPath is not null) Dispatcher.BeginInvoke(DispatcherPriority.ApplicationIdle, TakeShot);
    }

    // ───────────── вход окна (DESIGN.md 6 «Один вход», доска: dropin, leftin) ─────────────

    private readonly bool _entrance;
    private const double EnterTop = -18, EnterLeft = -24;

    /// <summary>
    /// Один раз при открытии: верхняя панель опускается, левая выезжает слева, обе проявляются (0,9 с, пружина без отскока).
    /// Карточки всплывают сами (CardGridPanel, свойство Rise), правая панель выезжает как обычно.
    /// </summary>
    private void PlayEntrance()
    {
        Enter(TopBar, TranslateTransform.YProperty, EnterTop, 0.15);
        Enter(Nav, TranslateTransform.XProperty, EnterLeft, 0.05);
    }

    /// <summary>
    /// До OnLoaded панель стоит в начале входа (конструктор), здесь анимация держит начало до задержки и ведёт к месту.
    /// Обычное значение сразу ставится конечным: если анимацию когда-нибудь снимут, панель останется на месте и видна.
    /// Стекло берёт фон под собой по своему месту только после раскладки, поэтому на каждом кадре сдвига просим её.
    /// </summary>
    private static void Enter(Glass g, DependencyProperty axis, double from, double delay)
    {
        var start = TimeSpan.FromSeconds(delay);
        var time = TimeSpan.FromSeconds(0.9);
        var move = Anim.Hold(from, 0, start, time, Anim.Smooth);
        move.CurrentTimeInvalidated += (_, _) => g.InvalidateArrange();
        var mover = Anim.Mover(g);
        mover.BeginAnimation(axis, move);
        mover.SetValue(axis, 0.0);
        g.BeginAnimation(OpacityProperty, Anim.Hold(0, 1, start, time, Anim.Smooth));
        g.Opacity = 1;
    }

    // ───────────── данные ─────────────

    private void Refresh()
    {
        _vm.Refresh();
        _lastRefresh = DateTime.UtcNow;
        LoadPreviews();
        // картинки могли прийти из кэша сразу (без загрузки) или смениться с папкой — превью настроек обновляется
        if (SettingsPanel.Visibility == Visibility.Visible) FillLookPreview();
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
            if (SettingsPanel.Visibility == Visibility.Visible && _pvPaths.Count < 3) FillLookPreview();
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
    /// что включено в настройках.
    /// </summary>
    private void UpdateMotion()
    {
        var s = _host.Settings.Current;
        var live = IsVisible && WindowState != WindowState.Minimized && IsActive && !_gameRunning && _host.ShotPath is null;
        _motion.SetRunning(live && s.AnimatedBackground, live && s.Fireflies);
        // искры льда мерцают в том же ритме, что и фон; выключили «Анимированный фон» — стоят и они (DESIGN.md 6)
        foreach (var (g, _) in _glass) g.SetLive(live && s.AnimatedBackground);
        _motionLive = live;
        UpdateBadgePulse();
    }

    // ───────────── бейдж «Обновления»: пульс в ритме 6 с, пока не открывали (DESIGN.md 6, DESIGN_MOTION 3.6) ─────────────

    private bool _motionLive, _updatesSeen, _badgeRunning;
    private Storyboard? _badgePulse;

    private void UpdateBadgePulse()
    {
        var run = _motionLive && !_updatesSeen && !SpringDriver.ReducedMotion && UpdatesBadge.IsVisible;
        if (run == _badgeRunning) return;
        _badgeRunning = run;
        if (run) (_badgePulse ??= MakeBadgePulse()).Begin(this, true);
        else _badgePulse?.Stop(this);
    }

    /// <summary>Как .pulse на доске: на 40–75 % удара от бейджа расходится и гаснет светлое кольцо. Сам бейдж с числом не двигается.</summary>
    private Storyboard MakeBadgePulse()
    {
        var sb = new Storyboard { RepeatBehavior = RepeatBehavior.Forever };
        Timeline.SetDesiredFrameRate(sb, 30);
        sb.Children.Add(Beat(BadgeRing, OpacityProperty, 0, 0.55, 0));
        sb.Children.Add(Beat(BadgeRingScale, ScaleTransform.ScaleXProperty, 1, 1, 1.6));
        sb.Children.Add(Beat(BadgeRingScale, ScaleTransform.ScaleYProperty, 1, 1, 1.6));
        return sb;
    }

    private static DoubleAnimationUsingKeyFrames Beat(DependencyObject target, DependencyProperty prop, double rest, double from, double to)
    {
        var a = new DoubleAnimationUsingKeyFrames { Duration = TimeSpan.FromSeconds(6) };
        a.KeyFrames.Add(new DiscreteDoubleKeyFrame(rest, KeyTime.FromTimeSpan(TimeSpan.Zero)));
        a.KeyFrames.Add(new DiscreteDoubleKeyFrame(from, KeyTime.FromTimeSpan(TimeSpan.FromSeconds(2.4))));
        a.KeyFrames.Add(new EasingDoubleKeyFrame(to, KeyTime.FromTimeSpan(TimeSpan.FromSeconds(4.5)), Anim.Smooth));
        Storyboard.SetTarget(a, target);
        Storyboard.SetTargetProperty(a, new PropertyPath(prop));
        return a;
    }

    // ───────────── внешний вид из настроек (DESIGN.md 5.1) ─────────────

    private readonly (Glass G, Brush Tint)[] _glass;
    private string _folderKey;
    private ThemePalette? _palette;
    private bool? _light;
    private double _frost = -1;

    /// <summary>
    /// Нет видеокарты для WPF (удалённый рабочий стол, виртуальная машина): шейдеры и размытие считает процессор на каждом
    /// кадре фона, поэтому жидкое стекло заменяется матовым (DESIGN.md 3.2). Снимкам сборочной машины нужен настоящий вид.
    /// </summary>
    private bool WeakGraphics() => _host.ShotPath is null && (RenderCapability.Tier >> 16) < 2;

    /// <summary>Матовое стекло (DESIGN.md 3.2): размытие 26 px, плотная подложка; в светлом оформлении — молочно-белая.</summary>
    private static readonly Brush MatteTint = Frozen(new SolidColorBrush(Color.FromArgb(0xA8, 34, 46, 68)));
    private static readonly Brush MatteTintLight = Frozen(new SolidColorBrush(Color.FromArgb(0xD1, 0xEE, 0xF2, 0xF8)));
    /// <summary>Плоское стекло: сплошная заливка #2a3446 (светлое — #eef1f6).</summary>
    private static readonly Brush FlatTint = Frozen(new SolidColorBrush(Color.FromRgb(0x2A, 0x34, 0x46)));
    private static readonly Brush FlatTintLight = Frozen(new SolidColorBrush(Color.FromRgb(0xEE, 0xF1, 0xF6)));
    private readonly Dictionary<Glass, Brush> _lightTints = new();

    /// <summary>
    /// Подложка жидкого стекла в светлом оформлении: та же панель, но белая и чуть плотнее тёмной (на светлом фоне без
    /// этого тёмный текст терял бы контраст): 0,44 → 0,56, 0,50 → 0,62, у тоста 0,70 → 0,82.
    /// </summary>
    private Brush LightTint(Glass g, Brush dark)
    {
        if (_lightTints.TryGetValue(g, out var b)) return b;
        var a = dark is SolidColorBrush sc ? sc.Color.A / 255.0 : 0.44;
        b = Frozen(new SolidColorBrush(Color.FromArgb((byte)Math.Round(Math.Min(0.9, a + 0.12) * 255), 0xF5, 0xF8, 0xFC)));
        _lightTints[g] = b;
        return b;
    }

    // число копий в ключ не входит: установщик читает его сам при каждой уборке, перечитывать папку не нужно
    private static string FolderKey(AppSettings s) => $"{s.ModsFolder}|{s.UserFolder}";

    /// <summary>В Windows переключили светлый режим, прозрачность или анимацию: применяем сразу, без перезапуска.</summary>
    private void OnUserPreferenceChanged(object? sender, Microsoft.Win32.UserPreferenceChangedEventArgs e) =>
        Dispatcher.BeginInvoke(() => { if (IsLoaded) ApplyLook(_host.Settings.Current); });

    private void OnSettingsChanged(AppSettings s)
    {
        ApplyLook(s, _revealFrom);
        _revealFrom = null;
        _vm.ManyEnabledThreshold = s.ActiveModsWarnAbove;
        // папку уже переключила программа (App подписана раньше окна): перечитываем моды
        var key = FolderKey(s);
        if (key != _folderKey) { _folderKey = key; if (IsLoaded) Refresh(); }
    }

    /// <summary>
    /// Применяет внешний вид сразу, без перезапуска: оформление (тёмное/светлое), стекло, иней, тема фона, движение.
    /// <paramref name="origin"/> — откуда раскрывается новое оформление (место нажатия); null — из центра окна.
    /// </summary>
    private void ApplyLook(AppSettings s, Point? origin = null)
    {
        var systemLight = Native.SystemLight();
        _host.Settings.SystemLight = systemLight;
        var light = SettingsViewModel.LightFor(s, systemLight);
        var modeSwitch = _light is not null && _light != light;
        if (_light != light) SwitchPalette(light, origin);

        SpringDriver.AnimationsOff = !s.UiAnimations;
        // ресурс меняем, только если значение другое: замена ресурса обходит всё дерево окна (сотни карточек)
        var frost = s.Frost ? s.FrostStrength switch { 0 => 0.55, 2 => 1.0, _ => 0.95 } : 0.0; // на доске иней на карточке .95
        if (frost != _frost) { _frost = frost; Resources["FrostOpacity"] = frost; }

        // «Уменьшить прозрачность» в Windows сильнее настроек: стекло плоское (DESIGN.md 3.2, «Плоское»)
        var mode = Native.TransparencyOff() ? GlassMode.Flat : s.LiquidGlass && !WeakGraphics() ? GlassMode.Liquid : GlassMode.Matte;
        var ice = s.Frost ? s.FrostStrength switch { 0 => 0.5, 2 => 1.8, _ => 1.0 } : 0.0; // --k на доске
        foreach (var (g, tint) in _glass)
        {
            g.SetLook(mode, ice, s.FrostSparks, light);
            g.Tint = (mode, light) switch
            {
                (GlassMode.Flat, false) => FlatTint,
                (GlassMode.Flat, true) => FlatTintLight,
                (GlassMode.Matte, _) when g == ToastBox => light ? LightTint(g, tint) : tint, // у тоста своя, плотная подложка
                (GlassMode.Matte, false) => MatteTint,
                (GlassMode.Matte, true) => MatteTintLight,
                (_, true) => LightTint(g, tint),
                _ => tint,
            };
        }

        Flies.Visibility = s.Fireflies ? Visibility.Visible : Visibility.Collapsed;
        Flies.Opacity = light ? 0.7 : 1; // белые огоньки на светлом фоне тише, иначе рябят
        var palette = BackgroundThemes.Of(s.Theme, light);
        // смена оформления закрыта раскрытием: под ним фон встаёт сразу; смена темы — плавное перетекание
        if (palette != _palette) ApplyTheme(palette, animate: _palette is not null && IsLoaded && !modeSwitch);
        UpdateMotion();
    }

    // ───────────── тёмное и светлое оформление ─────────────

    /// <summary>Откуда раскрывать следующее оформление: место последнего нажатия в настройках.</summary>
    private Point? _revealFrom;
    private int _revealGen;

    /// <summary>
    /// Меняет словарь цветов (Palette.Dark / Palette.Light): всё, что взято через DynamicResource, перекрашивается сразу.
    /// Чтобы смена не была резкой, сначала снимается кадр прежнего вида; он лежит поверх и «раскрывается» кругом от места
    /// нажатия с мягким краем — новое оформление проступает изнутри наружу за 0,9 с.
    /// </summary>
    private void SwitchPalette(bool light, Point? origin)
    {
        var shot = IsLoaded && _light is not null && !SpringDriver.ReducedMotion && _host.ShotPath is null ? SnapRoot() : null;
        var dicts = Application.Current.Resources.MergedDictionaries;
        var palette = new ResourceDictionary { Source = new Uri($"pack://application:,,,/Palette.{(light ? "Light" : "Dark")}.xaml", UriKind.Absolute) };
        if (dicts.Count > 0 && dicts[0].Source?.OriginalString.Contains("Palette.", StringComparison.Ordinal) == true) dicts[0] = palette;
        else dicts.Insert(0, palette);
        _light = light;
        Native.SetDarkFrame(this, !light);
        if (shot is not null) Reveal(shot, origin ?? new Point(Root.ActualWidth / 2, Root.ActualHeight / 2));
    }

    /// <summary>Кадр всего окна в пикселях экрана (с уже идущим раскрытием, если оно не закончилось).</summary>
    private BitmapSource? SnapRoot()
    {
        var w = Root.ActualWidth; var h = Root.ActualHeight;
        if (w < 1 || h < 1) return null;
        var dpi = VisualTreeHelper.GetDpi(this);
        var rt = new RenderTargetBitmap((int)Math.Ceiling(w * dpi.DpiScaleX), (int)Math.Ceiling(h * dpi.DpiScaleY),
            dpi.PixelsPerInchX, dpi.PixelsPerInchY, PixelFormats.Pbgra32);
        // через кисть, а не Render(Root): у развёрнутого окна у Root есть отступ, и прямой снимок съехал бы на его величину
        var dv = new DrawingVisual();
        using (var dc = dv.RenderOpen())
            dc.DrawRectangle(new VisualBrush(Root) { ViewboxUnits = BrushMappingMode.Absolute, Viewbox = new Rect(0, 0, w, h) }, null, new Rect(0, 0, w, h));
        rt.Render(dv);
        rt.Freeze();
        return rt;
    }

    private void Reveal(BitmapSource old, Point origin)
    {
        var gen = ++_revealGen;
        var w = Root.ActualWidth; var h = Root.ActualHeight;
        // до самого дальнего угла и ещё на ширину мягкого края
        var far = new[] { new Point(0, 0), new Point(w, 0), new Point(0, h), new Point(w, h) }.Max(c => (c - origin).Length);
        const double edge = 0.82; // доля радиуса, где кончается прозрачное «окно» и начинается мягкий край
        var mask = new RadialGradientBrush
        {
            MappingMode = BrushMappingMode.Absolute,
            Center = origin, GradientOrigin = origin, RadiusX = 0, RadiusY = 0,
            GradientStops = { new GradientStop(Colors.Transparent, 0), new GradientStop(Colors.Transparent, edge), new GradientStop(Colors.Black, 1) },
        };
        ModeVeil.Source = old;
        ModeVeil.OpacityMask = mask;
        ModeVeil.Visibility = Visibility.Visible;
        var time = TimeSpan.FromSeconds(1.2); // пружина без отскока: ощущается ≈0,75 с (длительность ×1,6, Motion/Spring.cs)
        var grow = new DoubleAnimation(0, far / edge + 40, time) { EasingFunction = Anim.Smooth };
        grow.Completed += (_, _) =>
        {
            if (gen != _revealGen) return; // уже идёт следующее раскрытие
            ModeVeil.Visibility = Visibility.Collapsed;
            ModeVeil.Source = null;
            ModeVeil.OpacityMask = null;
        };
        mask.BeginAnimation(RadialGradientBrush.RadiusXProperty, grow);
        mask.BeginAnimation(RadialGradientBrush.RadiusYProperty, grow.Clone());
    }

    // ───────────── тема фона ─────────────

    /// <summary>
    /// Тема фона: цвета основы, полосы, вуали и пятен перетекают в новые за 0,8 с по мягкой S-кривой, а пятна плавно
    /// переезжают на свои места, — все слои одновременно и с одной скоростью, без вспышки. Без анимаций — 0,25 с.
    /// </summary>
    private void ApplyTheme(ThemePalette p, bool animate)
    {
        _palette = p;
        var time = !animate ? TimeSpan.Zero : TimeSpan.FromSeconds(SpringDriver.ReducedMotion ? 0.25 : 0.8);
        void To(GradientStop stop, Color c)
        {
            if (time == TimeSpan.Zero) { stop.BeginAnimation(GradientStop.ColorProperty, null); stop.Color = c; return; }
            stop.BeginAnimation(GradientStop.ColorProperty, new ColorAnimation(c, time) { EasingFunction = ThemeEase }, HandoffBehavior.SnapshotAndReplace);
        }
        static Color C(uint argb) => ThemeSwatchConverter.ToColor(argb);
        static Color WithAlpha(uint argb, double a) => Color.FromArgb((byte)Math.Round(Math.Clamp(a, 0, 1) * 255), (byte)(argb >> 16), (byte)(argb >> 8), (byte)argb);

        for (var i = 0; i < Math.Min(BaseBrush.GradientStops.Count, p.Base.Count); i++) To(BaseBrush.GradientStops[i], C(p.Base[i]));
        for (var i = 0; i < Math.Min(Grade.GradientStops.Count, p.Grade.Count); i++) To(Grade.GradientStops[i], C(p.Grade[i]));
        if (VeilBrush.GradientStops.Count >= 2)
        {
            To(VeilBrush.GradientStops[0], WithAlpha(p.Veil, p.VeilTop));
            To(VeilBrush.GradientStops[1], WithAlpha(p.Veil, p.VeilBottom));
        }
        var fall = BackgroundThemes.Falloff;
        for (var i = 0; i < _motion.Blobs.Count && i < p.Blobs.Count; i++)
        {
            var stops = _motion.Blobs[i].Brush.GradientStops;
            var a = BackgroundThemes.Alpha(p.Blobs[i].Color);
            for (var k = 0; k < Math.Min(stops.Count, fall.Count); k++) To(stops[k], WithAlpha(p.Blobs[i].Color, a * fall[k].Alpha));
        }
        PlaceBlobs(time);
    }

    /// <summary>Пятна на места темы под текущий размер окна; при смене темы — переезд за то же время, что и цвет.</summary>
    private void PlaceBlobs(TimeSpan time)
    {
        if (_palette is not { } p) return;
        var w = Math.Max(1, Root.ActualWidth > 0 ? Root.ActualWidth : Width);
        var h = Math.Max(1, Root.ActualHeight > 0 ? Root.ActualHeight : Height);
        void Set(UIElement e, DependencyProperty prop, double v)
        {
            if (time == TimeSpan.Zero) { e.BeginAnimation(prop, null); e.SetValue(prop, v); return; }
            e.BeginAnimation(prop, new DoubleAnimation(v, time) { EasingFunction = ThemeEase }, HandoffBehavior.SnapshotAndReplace);
        }
        for (var i = 0; i < _motion.Blobs.Count && i < p.Blobs.Count; i++)
        {
            var b = p.Blobs[i];
            var size = 2 * b.R * Math.Max(w, h);
            var e = _motion.Blobs[i].Shape;
            Set(e, WidthProperty, size);
            Set(e, HeightProperty, size);
            Set(e, Canvas.LeftProperty, b.X * w - size / 2);
            Set(e, Canvas.TopProperty, b.Y * h - size / 2);
        }
        _motion.LayoutFlies(w, h);
    }

    private static readonly IEasingFunction ThemeEase = Frozen(new SineEase { EasingMode = EasingMode.EaseInOut });
    private static T Frozen<T>(T f) where T : Freezable { f.Freeze(); return f; }

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
        // у «Обновлений» справа всегда панель «Проверка»: место под неё держим и на узком окне
        var beside = (open && !_narrow) || NavUpdates.IsChecked == true; // панель занимает место рядом с сеткой
        var right = beside ? 360 : 16;
        var gridMargin = new Thickness(116, 0, right, 0);
        if (Scroller.Margin != gridMargin)
        {
            Scroller.Margin = UpdScroller.Margin = gridMargin;
            EmptyBox.Margin = SoonBox.Margin = UpdEmptyBox.Margin = new Thickness(116, 84, right, 0);
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
        if (NavGarage.IsChecked != true && !UpdatesPage) { SectionTitle.Visibility = Visibility.Visible; return; } // кроме заголовка там ничего нет
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
        // под «Закрыть» капля краснеет за 0,25 с (доска: transition background .25s); появляясь, сразу нужного цвета
        var danger = i == 2 ? 1.0 : 0.0;
        if (appearing || SpringDriver.ReducedMotion) { CapDrop.BeginAnimation(Droplet.DangerProperty, null); CapDrop.Danger = danger; }
        else CapDrop.BeginAnimation(Droplet.DangerProperty, new DoubleAnimation(danger, TimeSpan.FromSeconds(0.25)) { EasingFunction = Anim.Smooth });
    }

    // ───────────── форма капель: растяжение в пути и сжатие при нажатии (DESIGN_MOTION 3.3) ─────────────

    /// <summary>Насколько капля вытягивается по ходу движения: от скорости пружины, не больше 14 %.</summary>
    private static double StretchOf(double velocity) => Math.Min(0.14, Math.Abs(velocity) / 2600);

    /// <summary>Вытянулась вдоль — сузилась поперёк (объём «сохраняется»), и всё это умножается на сжатие нажатия.</summary>
    private static void Shape(ScaleTransform t, double along, bool vertical, double press, double scale = 1)
    {
        var a = (1 + along) * press * scale;
        var b = (1 - along * 0.45) * press * scale;
        t.ScaleX = vertical ? b : a;
        t.ScaleY = vertical ? a : b;
    }

    private void NavDropShape()
    {
        Shape(NavDropScale, StretchOf(_navDrop.Velocity), vertical: true, _navPress.Value);
        NavDrop.Sync();
    }

    private void FilterDropShape()
    {
        Shape(FilterDropScale, StretchOf(_filterX.Velocity), vertical: false, _filterPress.Value);
        FilterDrop.Sync();
    }

    private void CapDropShape()
    {
        Shape(CapDropScale, StretchOf(_capX.Velocity), vertical: false, _capPress.Value, _capScale.Value);
        CapDrop.Sync();
    }

    // нажали на пункт — капля сжимается; отпустили или ушли мышью — упруго возвращается и уже потом перетекает
    // при «Уменьшить анимацию» и выключенных анимациях капля не сжимается (DESIGN_MOTION 3.3)
    private void Nav_PreviewMouseDown(object sender, MouseButtonEventArgs e) { if (Squish(e)) _springs.To(_navPress, 0.92); }
    // обработчики XAML должны совпадать с делегатом точно: шаблонные события создаются через Delegate.CreateDelegate
    private void Nav_PreviewMouseUp(object sender, MouseButtonEventArgs e) => _springs.To(_navPress, 1);
    private void Nav_MouseLeave(object sender, MouseEventArgs e) => _springs.To(_navPress, 1);
    private void Filter_PreviewMouseDown(object sender, MouseButtonEventArgs e) { if (Squish(e)) _springs.To(_filterPress, 0.92); }
    private void Filter_PreviewMouseUp(object sender, MouseButtonEventArgs e) => _springs.To(_filterPress, 1);
    private void Filter_MouseLeave(object sender, MouseEventArgs e) => _springs.To(_filterPress, 1);
    private void Caption_PreviewMouseDown(object sender, MouseButtonEventArgs e) { if (!SpringDriver.ReducedMotion) _springs.To(_capPress, 0.92); }
    private void Caption_PreviewMouseUp(object sender, MouseButtonEventArgs e) => _springs.To(_capPress, 1);

    private static bool Squish(MouseButtonEventArgs e) => !SpringDriver.ReducedMotion && e.OriginalSource is DependencyObject d && OverButton(d);

    /// <summary>Нажатие пришлось на кнопку или пункт (а не на пустое место панели).</summary>
    private static bool OverButton(DependencyObject d)
    {
        for (DependencyObject? p = d; p is not null; p = p is Visual || p is System.Windows.Media.Media3D.Visual3D ? VisualTreeHelper.GetParent(p) : LogicalTreeHelper.GetParent(p))
            if (p is System.Windows.Controls.Primitives.ButtonBase) return true;
        return false;
    }

    private void Caption_MouseLeave(object sender, MouseEventArgs e)
    {
        _springs.To(_capPress, 1);
        HideCapDrop();
    }

    private void HideCapDrop(bool instant = false)
    {
        _springs.To(_capScale, 0.6, instant);
        _springs.To(_capPress, 1, instant);
        Fade(CapDrop, 0, instant ? 0 : 0.2);
    }

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
        _autoCheckTimer.Stop();
        _bindCts?.Cancel();
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
        var updates = rb == NavUpdates;
        if (!garage) _vm.Selected = null; // правая панель — про мод из гаража, в других разделах ей не место
        SectionTitle.Text = (string)rb.Content;
        if (!garage) SectionTitle.Visibility = Visibility.Visible; // на узком окне гараж мог спрятать заголовок
        Scroller.Visibility = garage ? Visibility.Visible : Visibility.Collapsed;
        EmptyBox.Visibility = garage && _vm.Empty is not null ? Visibility.Visible : Visibility.Collapsed;
        UpdScroller.Visibility = CheckPanel.Visibility = updates ? Visibility.Visible : Visibility.Collapsed;
        UpdEmptyBox.Visibility = updates && _upd.Empty is not null ? Visibility.Visible : Visibility.Collapsed;
        SoonBox.Visibility = garage || settings || updates ? Visibility.Collapsed : Visibility.Visible;
        SettingsPanel.Visibility = settings ? Visibility.Visible : Visibility.Collapsed;
        SoonTitle.Text = (string)rb.Content;
        // фильтры, поиск и число — у гаража и «Обновлений» свои, панель общая; в других разделах им не место
        var gv = garage || updates ? Visibility.Visible : Visibility.Collapsed;
        FilterBox.Visibility = SearchField.Visibility = CountLabel.Visibility = gv;
        BindTopBar(updates);
        PlaceInspector(instant: !IsLoaded);
        if (garage || updates) Dispatcher.BeginInvoke(AdaptTopBar, DispatcherPriority.Loaded);
        if (garage) EmptyBox.SetBinding(VisibilityProperty, new System.Windows.Data.Binding(nameof(GarageViewModel.Empty)) { Converter = (System.Windows.Data.IValueConverter)FindResource("NullToCollapsed") });
        // раздел «Обновления» открыли: найденное просмотрено, бейдж больше не пульсирует до конца сеанса
        if (rb == NavUpdates && !_updatesSeen) { _updatesSeen = true; UpdateBadgePulse(); }
        if (settings) FillLookPreview();
        // сменился раздел: двигается только содержимое, оно проявляется с лёгким подъёмом; капля уже перетекает
        if (IsLoaded)
        {
            if (garage) { Arrive(Scroller, Scroller); Arrive(EmptyBox, EmptyBox); }
            else if (updates) { Arrive(UpdScroller, UpdBody); Arrive(UpdEmptyBox, UpdEmptyBox); Arrive(CheckPanel, CheckBody); }
            else if (settings) Arrive(SettingsPanel, SettingsBody); // стекло только проявляется: его фон берётся по месту
            else Arrive(SoonBox, SoonBox);
        }
    }

    // ───────────── «Как это выглядит» ─────────────

    /// <summary>Картинки, которые сейчас стоят в превью (до трёх); null — превью ещё не собиралось.</summary>
    private List<string> _pvPaths = new();
    private bool _pvBuilt;

    /// <summary>
    /// Картинки превью — первые три мода с картинкой (сначала включённые), как карточки на доске: 250 и 200 px,
    /// скругление 16, низ уходит за край. Модов с картинками нет — тёмные заглушки, стекло всё равно видно.
    /// </summary>
    private void FillLookPreview()
    {
        var paths = _vm.AllCards.Where(c => c.PreviewPath is not null)
                                .OrderByDescending(c => c.Enabled)
                                .Select(c => c.PreviewPath!).Take(3).ToList();
        // те же картинки — ничего не трогаем; сменилась папка или пришли новые — собираем заново
        if (_pvBuilt && paths.SequenceEqual(_pvPaths, StringComparer.OrdinalIgnoreCase)) return;
        _pvBuilt = true;
        _pvPaths = paths;
        PvCards.Children.Clear();
        double[] widths = { 250, 200, 200 };
        for (var i = 0; i < widths.Length; i++)
        {
            var img = i < paths.Count ? ImageCache.Load(paths[i]) : null;
            Brush fill = img is not null
                ? Frozen(new ImageBrush(img) { Stretch = Stretch.UniformToFill })
                : PvPlaceholder;
            PvCards.Children.Add(new Border { Width = widths[i], Height = 140, CornerRadius = new CornerRadius(16), Margin = new Thickness(0, 0, 12, 0), Background = fill });
        }
    }

    private static readonly Brush PvPlaceholder = Frozen(new LinearGradientBrush(Color.FromRgb(0x2A, 0x34, 0x46), Color.FromRgb(0x1B, 0x22, 0x32), 45));

    private void LookPreview_SizeChanged(object sender, SizeChangedEventArgs e)
    {
        var clip = new RectangleGeometry(new Rect(e.NewSize), 22, 22);
        clip.Freeze();
        LookPreview.Clip = clip;
    }

    /// <summary>Новое содержимое раздела: проявление и подъём на 10 px (≈0,35 с, пружина без отскока).</summary>
    private void Arrive(UIElement fade, UIElement rise)
    {
        if (SpringDriver.ReducedMotion || _host.ShotPath is not null) return;
        fade.BeginAnimation(OpacityProperty, Anim.Go(0, 1, 0.55, Anim.Smooth));
        Anim.Mover(rise).BeginAnimation(TranslateTransform.YProperty, Anim.Go(10, 0, 0.55, Anim.Smooth));
    }

    // ───────────── фильтры и поиск ─────────────

    private void Filter_Click(object sender, RoutedEventArgs e)
    {
        switch (((FrameworkElement)sender).DataContext)
        {
            case FilterItem f: _vm.Filter = f.Filter; break;
            case UpdatesFilterItem u: _upd.Filter = u.Filter; break;
        }
    }

    /// <summary>
    /// Верхняя панель общая у гаража и «Обновлений»: фильтры, поиск и число модов переключаются на раздел. Капля фильтра
    /// при этом не уезжает издалека, а сразу встаёт под активный пункт нового раздела.
    /// </summary>
    private void BindTopBar(bool updates)
    {
        object source = updates ? _upd : _vm;
        var wanted = updates ? (System.Collections.IEnumerable)_upd.Filters : _vm.Filters;
        if (ReferenceEquals(FilterList.ItemsSource, wanted)) return;
        FilterList.ItemsSource = wanted;
        SearchBox.SetBinding(TextBox.TextProperty, new System.Windows.Data.Binding("SearchText") { Source = source, UpdateSourceTrigger = System.Windows.Data.UpdateSourceTrigger.PropertyChanged });
        CountLabel.SetBinding(TextBlock.TextProperty, new System.Windows.Data.Binding("CountText") { Source = source });
        SearchHint.Text = updates ? "Найти" : "Найти мод";
        System.Windows.Automation.AutomationProperties.SetName(SearchBox, updates ? "Найти среди найденных обновлений" : "Найти среди установленных модов");
        _filterPlaced = false; // следующая раскладка ставит каплю сразу
    }

    private bool UpdatesPage => NavUpdates.IsChecked == true;

    /// <summary>Капля фильтра ложится под активный пункт; пункты меняют ширину (числа), поэтому меряем по факту.</summary>
    private void PlaceFilterDrop(bool instant)
    {
        object? item = UpdatesPage ? _upd.Filters.FirstOrDefault(f => f.IsActive) : _vm.Filters.FirstOrDefault(f => f.IsActive);
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

    private void CardPanel_Loaded(object sender, RoutedEventArgs e)
    {
        _panel = (CardGridPanel)sender;
        _panel.Animated = _host.ShotPath is null; // снимку нужно конечное положение сразу
    }

    private void Card_Click(object sender, RoutedEventArgs e)
    {
        // щелчок мышью по уже выбранной карточке закрывает правую панель; Enter с клавиатуры — только открывает
        if (((FrameworkElement)sender).DataContext is not ModCard c) return;
        var byKeyboard = InputManager.Current.MostRecentInputDevice is KeyboardDevice;
        ChangeSelection(() => _vm.Selected = ReferenceEquals(_vm.Selected, c) && !byKeyboard ? null : c);
    }

    // ───────────── смена выбора при открытой панели (DESIGN_MOTION 3.5) ─────────────

    private int _ghostGen;

    /// <summary>
    /// Панель уже открыта и выбран другой мод: она не выезжает заново, прежнее содержимое тает поверх нового.
    /// Снимок прежнего делается до смены, пока на экране ещё оно.
    /// </summary>
    private void ChangeSelection(Action change)
    {
        var before = _vm.Selected;
        var shot = SnapInspector();
        change();
        if (shot is not null && _vm.Selected is not null && !ReferenceEquals(before, _vm.Selected)) CrossFadeInspector(shot);
    }

    /// <summary>Снимок содержимого правой панели в пикселях экрана; null — панель не стоит открытой или анимации выключены.</summary>
    private BitmapSource? SnapInspector()
    {
        if (_host.ShotPath is not null || SpringDriver.ReducedMotion || _vm.Selected is null) return null;
        if (!Inspector.IsVisible || _inspX.Target != 0 || Math.Abs(_inspX.Value) > 1) return null; // выезжает или уезжает
        var w = InspectorBody.ActualWidth;
        var h = InspectorBody.ActualHeight;
        if (w < 1 || h < 1) return null;
        var dpi = VisualTreeHelper.GetDpi(this);
        var rt = new RenderTargetBitmap((int)Math.Ceiling(w * dpi.DpiScaleX), (int)Math.Ceiling(h * dpi.DpiScaleY),
            dpi.PixelsPerInchX, dpi.PixelsPerInchY, PixelFormats.Pbgra32);
        var dv = new DrawingVisual();
        using (var dc = dv.RenderOpen())
            dc.DrawRectangle(new VisualBrush(InspectorBody) { ViewboxUnits = BrushMappingMode.Absolute, Viewbox = new Rect(0, 0, w, h) },
                null, new Rect(0, 0, w, h));
        rt.Render(dv);
        rt.Freeze();
        return rt;
    }

    /// <summary>Прежнее содержимое (снимок) гаснет, новое проявляется: быстро, ≈0,2 с, без сдвига, текст не расплывается.</summary>
    private void CrossFadeInspector(BitmapSource old)
    {
        var gen = ++_ghostGen;
        InspectorGhost.Source = old;
        InspectorGhost.Visibility = Visibility.Visible;
        var fadeOut = Anim.Go(1, 0, 0.3, Anim.Smooth);
        fadeOut.Completed += (_, _) =>
        {
            if (gen != _ghostGen) return; // уже идёт следующая смена
            InspectorGhost.Visibility = Visibility.Collapsed;
            InspectorGhost.Source = null;
        };
        InspectorGhost.BeginAnimation(OpacityProperty, fadeOut);
        InspectorBody.BeginAnimation(OpacityProperty, Anim.Go(0, 1, 0.35, Anim.Smooth));
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
    // новое оформление раскрывается от того места, где нажали (с клавиатуры — из центра окна)
    private void Mode_PreviewMouseDown(object sender, MouseButtonEventArgs e) => _revealFrom = e.GetPosition(Root);
    private void LessWarn_Click(object sender, RoutedEventArgs e) => _host.Settings.LessWarn();
    private void MoreWarn_Click(object sender, RoutedEventArgs e) => _host.Settings.MoreWarn();
    private void LessBackups_Click(object sender, RoutedEventArgs e) => _host.Settings.LessBackups();
    private void MoreBackups_Click(object sender, RoutedEventArgs e) => _host.Settings.MoreBackups();
    private void AutoFolder_Click(object sender, RoutedEventArgs e)
    {
        if (!_host.Settings.FolderIsManual || BlockedByPending() || !ConfirmForget()) return;
        _host.Settings.UseAutoFolder();
    }
    private void CheckApp_Click(object sender, RoutedEventArgs e) => _host.Banner?.CheckNowAsync(_closing.Token);

    private void PickFolder_Click(object sender, RoutedEventArgs e) => PickFolder();

    /// <summary>
    /// Выбор папки модов. Сначала диалог, потом вопрос — только если папка действительно другая. Смена папки
    /// перечитывает моды по событию настроек; выбор той же (диск подключили после запуска) настроек не меняет,
    /// тогда перечитываем здесь — ровно один раз.
    /// </summary>
    private void PickFolder()
    {
        if (_host.ChooseFolder is not { } choose || _host.UseFolder is not { } use || BlockedByPending()) return;
        if (choose(this) is not { } dir) return;
        var same = _host.Settings.ModsFolderPath is { } cur && string.Equals(Path.TrimEndingDirectorySeparator(cur),
            Path.TrimEndingDirectorySeparator(dir), StringComparison.OrdinalIgnoreCase);
        if (!same && !ConfirmForget()) return;
        if (!use(dir)) Refresh();
    }

    /// <summary>
    /// Папку нельзя менять, пока ждут переключения, отложенные до закрытия игры: они записаны по имени файла и
    /// применились бы к модам другой папки.
    /// </summary>
    private bool BlockedByPending()
    {
        if (!_vm.HasPending) return false;
        MessageBox.Show(this, "Сначала закройте игру: есть моды, которые включатся или выключатся после её закрытия.",
            "Папка модов", MessageBoxButton.OK, MessageBoxImage.Information);
        return true;
    }

    /// <summary>Сведения о модах прежней папки (где следим, пропущенные версии) забудутся: спрашиваем, если моды есть.</summary>
    private bool ConfirmForget()
    {
        if (_vm.AllCards.Count == 0) return true;
        return MessageBox.Show(this, "Список модов перечитается из другой папки. Отметки о модах этой папки (где следим за обновлениями, " +
                "пропущенные версии) будут забыты, сами файлы модов не тронутся.\n\nСменить папку?",
            "Папка модов", MessageBoxButton.YesNo, MessageBoxImage.Question, MessageBoxResult.No) == MessageBoxResult.Yes;
    }

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
            case MainAction.FindReplacement:
                if (_vm.Selected is { } dead) OpenBind(dead, dead.Data.Binding?.Source);
                break;
            case MainAction.SetPage:
                if (_vm.Selected is { } sel) OpenBind(sel, null);
                break;
        }
    }

    private void EmptyAction_Click(object sender, RoutedEventArgs e)
    {
        switch (_vm.Empty?.Action)
        {
            case EmptyAction.PickFolder:
                PickFolder();
                break;
            case EmptyAction.ShowAll: _vm.Filter = GarageFilter.All; break;
            case EmptyAction.OpenSearch:
            case EmptyAction.SearchSites: NavSearch.IsChecked = true; break;
            case EmptyAction.CheckNow:
                NavUpdates.IsChecked = true;
                _ = _upd.CheckNowAsync();
                break;
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
        // проверить обновления (DESIGN_FLOWS 7): F5 или Ctrl+R, из любого раздела
        if (BindLayer.Visibility != Visibility.Visible && (e.Key == Key.F5 || (ctrl && e.Key == Key.R)))
        {
            NavUpdates.IsChecked = true;
            if (!_upd.Checking) _ = _upd.CheckNowAsync();
            e.Handled = true;
            return;
        }
        if (BindLayer.Visibility == Visibility.Visible)
        {
            if (e.Key == Key.Escape) { CloseBind(); e.Handled = true; }
            else if (e.Key == Key.Enter && _bind is { CanApply: true }) { BindApply_Click(this, new RoutedEventArgs()); e.Handled = true; }
            return; // окно поверх: остальные клавиши — его полям и кнопкам
        }
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
            case Key.Left: ChangeSelection(() => _vm.MoveSelection(-1, 0, cols)); break;
            case Key.Right: ChangeSelection(() => _vm.MoveSelection(1, 0, cols)); break;
            case Key.Up: ChangeSelection(() => _vm.MoveSelection(0, -1, cols)); break;
            case Key.Down: ChangeSelection(() => _vm.MoveSelection(0, 1, cols)); break;
            case Key.Space: _vm.ToggleSelected(); break;
            default: return;
        }
        e.Handled = true;
        FocusSelected();
    }

    // ───────────── «Обновления» ─────────────

    private void Upd_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(UpdatesViewModel.Filter):
                PlaceFilterDrop(instant: false);
                break;
            case nameof(UpdatesViewModel.Empty):
                if (UpdatesPage) UpdEmptyBox.Visibility = _upd.Empty is not null ? Visibility.Visible : Visibility.Collapsed;
                break;
            case nameof(UpdatesViewModel.CountText):
                if (UpdatesPage) Dispatcher.BeginInvoke(AdaptTopBar, DispatcherPriority.Loaded);
                break;
        }
    }

    /// <summary>
    /// Карточка обновления подстраивается под ширину: на доске фото 560 и текст с 372 px; на узкой — фото уходит под текст.
    /// </summary>
    private void UpdCard_SizeChanged(object sender, SizeChangedEventArgs e)
    {
        if (sender is not FrameworkElement root || Named(root, "Photo") is not FrameworkElement photo || Named(root, "Body") is not FrameworkElement body) return;
        var w = e.NewSize.Width;
        if (w <= 0) return;
        var wide = w >= 760;
        photo.Width = wide ? Math.Min(560, w * 0.58) : w;
        photo.Opacity = wide ? 1 : 0.45;
        var left = wide ? Math.Min(372, w * 0.386) : 26;
        if (body.Margin.Left != left) body.Margin = new Thickness(left, 0, 26, 0);
    }

    /// <summary>Элемент шаблона по имени (имена шаблона в своей области: FindName снаружи их не видит).</summary>
    private static FrameworkElement? Named(DependencyObject root, string name)
    {
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
        {
            var c = VisualTreeHelper.GetChild(root, i);
            if (c is FrameworkElement { Name: var n } fe && n == name) return fe;
            if (Named(c, name) is { } deeper) return deeper;
        }
        return null;
    }

    private void UpdAction_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement { DataContext: UpdateItem item, Tag: ItemAction act }) return;
        switch (act)
        {
            // загрузки в программе пока нет: «Обновить» тоже ведёт на страницу мода
            case ItemAction.OpenPage:
            case ItemAction.Update:
                OpenUrl(item.PageUrl);
                break;
            case ItemAction.FindReplacement:
                OpenBind(item.Card, item.Card.Data.Binding?.Source);
                break;
            default:
                _upd.Act(item, act);
                break;
        }
    }

    private void CheckNow_Click(object sender, RoutedEventArgs e) => _ = _upd.CheckNowAsync();

    private void UpdEmpty_Click(object sender, RoutedEventArgs e)
    {
        switch (_upd.Empty?.Action)
        {
            case EmptyAction.CheckNow: _ = _upd.CheckNowAsync(); break;
            case EmptyAction.PickFolder: PickFolder(); break;
            case EmptyAction.ShowAll:
                // «Перейти к модам»: сразу к тем, у кого страница не указана
                NavGarage.IsChecked = true;
                _vm.Filter = GarageFilter.NotTracked;
                break;
        }
    }

    private void SiteOpen_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement { DataContext: SiteRow row }) return;
        OpenUrl(row.Home);
        _upd.SiteOpened(row);
    }

    private void SiteDone_Click(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { DataContext: SiteRow row }) _upd.SiteConfirmed(row);
    }

    private void OpenUrl(Uri? url)
    {
        if (url is null || (url.Scheme != Uri.UriSchemeHttps && url.Scheme != Uri.UriSchemeHttp)) return;
        try { Process.Start(new ProcessStartInfo(url.AbsoluteUri) { UseShellExecute = true }); }
        catch (Exception x) when (x is System.ComponentModel.Win32Exception or InvalidOperationException) { _vm.Notify("Не удалось открыть браузер"); }
    }

    // ───────────── «Указать страницу» и «Найти замену» ─────────────

    private PageBindViewModel? _bind;
    private CancellationTokenSource? _bindCts;
    private readonly DispatcherTimer _linkTimer = new() { Interval = TimeSpan.FromMilliseconds(450) };
    private IInputElement? _focusBeforeBind;

    /// <summary>Окно поверх раздела: подсказки поиска грузятся сразу, ссылка проверяется после паузы в наборе.</summary>
    private void OpenBind(ModCard card, string? replaceSource)
    {
        if (_host.Updates is not { } backend) { _vm.Notify("Указать страницу здесь нельзя"); return; }
        _bindCts?.Cancel();
        _bindCts = new CancellationTokenSource();
        _bind = new PageBindViewModel(backend, card, replaceSource);
        BindLayer.DataContext = _bind;
        BindLinkHint.Visibility = Visibility.Visible;
        _focusBeforeBind = Keyboard.FocusedElement;
        BindLayer.Visibility = Visibility.Visible;
        if (!SpringDriver.ReducedMotion && _host.ShotPath is null)
        {
            BindLayer.BeginAnimation(OpacityProperty, Anim.Go(0, 1, 0.35, Anim.Smooth));
            Anim.Mover(BindBody).BeginAnimation(TranslateTransform.YProperty, Anim.Go(12, 0, 0.55, Anim.Smooth));
        }
        Dispatcher.BeginInvoke(() => BindLink.Focus(), DispatcherPriority.Input);
        var token = _bindCts.Token;
        var vm = _bind;
        _ = vm.LoadAsync(token);
    }

    private void CloseBind()
    {
        _linkTimer.Stop();
        _bindCts?.Cancel();
        _bind = null;
        BindLayer.Visibility = Visibility.Collapsed;
        BindLayer.DataContext = null;
        if (_focusBeforeBind is UIElement { IsVisible: true } f) f.Focus();
    }

    private void BindLink_TextChanged(object sender, TextChangedEventArgs e)
    {
        BindLinkHint.Visibility = BindLink.Text.Length == 0 ? Visibility.Visible : Visibility.Collapsed;
        // проверяем, когда человек перестал печатать или вставил ссылку целиком
        _linkTimer.Stop();
        _linkTimer.Start();
    }

    private void LinkTimer_Tick(object? sender, EventArgs e)
    {
        _linkTimer.Stop();
        if (_bind is { } vm && _bindCts is { } cts) _ = vm.CheckLinkAsync(cts.Token);
    }

    private void BindPick_Click(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { DataContext: PageSuggestion s } && _bind is { } vm) vm.Select(s);
    }

    private void BindApply_Click(object sender, RoutedEventArgs e)
    {
        if (_bind is not { } vm) return;
        var toast = vm.Apply();
        if (toast is null) { _vm.Notify("Не получилось: мода уже нет в библиотеке"); CloseBind(); return; }
        CloseBind();
        Refresh();
        _vm.Notify(toast);
        // новую страницу проверяем сразу (то, что уже проверено, сайты не дёргает)
        _ = _upd.CheckSoonAsync();
    }

    private void BindCancel_Click(object sender, RoutedEventArgs e) => CloseBind();

    private void BindDim_MouseDown(object sender, MouseButtonEventArgs e)
    {
        CloseBind();
        e.Handled = true;
    }

    /// <summary>«Не проверять этот мод» прямо из «Найти замену».</summary>
    private void BindDecline_Click(object sender, RoutedEventArgs e)
    {
        if (_bind is not { } vm) return;
        var card = vm.Card;
        CloseBind();
        if (_upd.Items.FirstOrDefault(i => i.ModId == card.Id) is { } item) _upd.Act(item, ItemAction.Decline);
        else if (_host.Updates is { } be && be.Decline(card.Id) is { } undo)
        {
            Refresh();
            _vm.Notify($"Не проверяем «{card.Name}». Мод в фильтре «Не проверяются»", () => { be.UndoDecline(undo); Refresh(); });
        }
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
