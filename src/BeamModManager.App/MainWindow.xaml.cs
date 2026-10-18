using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using BeamModManager.App.Controls;
using BeamModManager.App.Motion;
using BeamModManager.App.Services;
using BeamModManager.Core.Settings;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Incoming;
using BeamModManager.Ui.Tour;
using BeamModManager.Ui.Look;
using BeamModManager.Ui.Search;
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
    /// <summary>Снимок: выключить выбранный мод (тост «выключен» с «Отменить»).</summary>
    public bool ShotToggle { get; init; }
    /// <summary>Снимок: окно проверки архива с таким (демо) именем; «…update…», «…several…», «…problem…» — разные случаи.</summary>
    public string? ShotIncoming { get; init; }
    /// <summary>Снимок: плашка «Скачан мод …» для такого (демо) архива.</summary>
    public string? ShotOffer { get; init; }
    /// <summary>Снимок: подсказка «Отпустите — проверим…», как при перетаскивании архива.</summary>
    public bool ShotDropHint { get; init; }
    /// <summary>Снимок: плашка «Интерфейс работает медленно».</summary>
    public bool ShotSlowOffer { get; init; }

    /// <summary>Капли кнопок окна и фильтров посреди пути, линзой (--drop-motion): так видно, что они берут под собой.</summary>
    public bool ShotDropMotion { get; init; }
    /// <summary>Снимок: шаг знакомства (1 — первый).</summary>
    public int? ShotTour { get; init; }
    /// <summary>Вместе со снимком записать замеры (запуск, фильтр, поиск, перечитывание, память) в &lt;снимок&gt;.perf.txt.</summary>
    public bool ShotPerf { get; init; }

    /// <summary>Масштаб снимка (--scale 2): как экран с масштабом 200 % — так видны огрехи, которых нет при 100 % (2026-10-18).</summary>
    public double ShotScale { get; init; } = 1;
    /// <summary>Вместе со снимком прогнать анимации и записать кадры в секунду в &lt;снимок&gt;.frames.txt (--frames).</summary>
    public bool ShotFrames { get; init; }
    /// <summary>Демо: открыть с раскрытым поиском и этим запросом (снимки островка).</summary>
    public string? StartSearch { get; init; }
    /// <summary>Демо: прокрутить список на столько точек (снимки сжатого заголовка).</summary>
    public double? StartScroll { get; init; }
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
    /// <summary>Раздел «Поиск» по сайтам. null — искать негде (раздел скажет, что все сайты выключены).</summary>
    public ISearchBackend? Search { get; init; }
    /// <summary>Картинка карточки поиска по адресу с сайта: путь к уменьшенной копии на диске или null (заглушка).</summary>
    public Func<Uri, CancellationToken, Task<string?>>? SearchImages { get; init; }
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
    private readonly SearchViewModel _find;
    /// <summary>Повтор проверки раз в N часов (настройка «Повторять»): раз в минуту смотрим, не пора ли.</summary>
    private readonly DispatcherTimer _autoCheckTimer = new() { Interval = TimeSpan.FromMinutes(1) };
    private readonly WindowHost _host;
    private readonly SpringDriver _springs = new();
    private readonly Spring _navDrop, _navTrail, _filterX, _filterTrail, _filterW, _capX, _capScale, _inspX;
    /// <summary>Ширина островка сверху (фильтры ↔ поиск) и сжатие заголовка при прокрутке (0 — крупный, 1 — капсула).</summary>
    private readonly Spring _islandW, _titleT, _titleVis;
    /// <summary>Нажатие на каплю: она чуть сжимается (0,92) и упруго возвращается (0,4 с, упругость 0,3 — «физическое»).</summary>
    private readonly Spring _navPress, _filterPress, _capPress;
    private readonly BackgroundMotion _motion;
    /// <summary>Шагов выезда панели мода с её появления (кэш картинкой — не раньше, чем она нарисуется вживую).</summary>
    private int _inspLiveFrames;
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
        // капли быстрые и живые (2026-10-16: «ускорить»): переезд ≈0,34 с с лёгким перелётом, нажатие ≈0,22 с
        _navPress = new Spring(0.22, 0.3, 1);
        _filterPress = new Spring(0.22, 0.3, 1);
        _capPress = new Spring(0.22, 0.3, 1);
        // капля в пути — одно тело (вариант «В», 2026-10-17): «голова» догоняет пункт быстро и с лёгким перелётом,
        // «хвост» — медленнее и без отскока; рисуется оболочка от хвоста до головы, без разрыва на две капли
        // капли левой панели и фильтров (2026-10-18, «не хватает живости»): голова перелетает пункт заметнее (на 300 точек
        // пути ≈14 вместо ≈3), хвост чуть пружинит следом; у крайнего пункта перелёт уходит в сплющивание о стенку.
        // Всё успокаивается за ≈0,5 с — как и было
        _navDrop = new Spring(DropHead, DropHeadBounce, NavDropTop);
        _navTrail = new Spring(DropTail, DropTailBounce, NavDropTop);
        _filterX = new Spring(DropHead, DropHeadBounce, -3);
        _filterTrail = new Spring(DropTail, DropTailBounce, -3);
        _filterW = new Spring(0.30, 0.2, 60);
        // путь всего 44 точки: короткая пружина почти без перелёта, иначе капля отстаёт от курсора (видео с виртуалки, 2026-10-18)
        _capX = new Spring(0.18, 0.06, 2);
        _capScale = new Spring(0.24, 0.18, 0.6);
        _springs.Add(_navPress, _ => NavDropShape());
        _springs.Add(_filterPress, _ => FilterDropShape());
        _springs.Add(_capPress, _ => CapDropShape());
        _springs.Add(_navDrop, _ => NavDropShape());
        _springs.Add(_navTrail, _ => NavDropShape());
        _springs.Add(_filterX, _ => FilterDropShape());
        _springs.Add(_filterTrail, _ => FilterDropShape());
        _springs.Add(_filterW, _ => FilterDropShape());
        // перелёт пружины у крайних кнопок не выносит каплю за капсулу
        _springs.Add(_capX, v => { CapDropMove.X = Math.Clamp(v, 2, 90); CapDropShape(); });
        _springs.Add(_capScale, _ => CapDropShape());
        // панель мода выезжает быстро (≈0,25 с, без отскока). В пути она кэширована картинкой: сдвиг готовой картинки
        // почти ничего не стоит, а без кэша на каждом кадре заново считались стекло, преломление и всё содержимое
        var insp = new Spring(0.34, 0, 344);
        _inspX = _springs.Add(insp, v =>
        {
            // последние доли пикселя пружина тянула бы ещё ≈0,5 с — встаём на место сразу
            if (!insp.Resting && Math.Abs(v - insp.Target) < 0.5 && Math.Abs(insp.Velocity) < 30) { insp.Snap(insp.Target); v = insp.Target; }
            InspectorMove.X = v;
            var moving = !insp.Resting;
            // кэш — только после того, как панель хоть раз нарисовалась вживую: панель, которая только что появилась, в
            // первом кадре ещё без копии сцены, и кэш запоминал её такой на весь выезд — тёмной, в полосах, а «через
            // секунду» (кэш снят) белой (0.1.122, светлое оформление). Пружина шагает перед кадром, поэтому два шага с
            // появления — один кадр уже нарисован
            if (moving && Inspector.CacheMode is null && ++_inspLiveFrames > 2) Inspector.CacheMode = new BitmapCache { SnapsToDevicePixels = true };
            else if (!moving && Inspector.CacheMode is not null) { Inspector.CacheMode = null; Inspector.InvalidateArrange(); }
            if (v >= 343 && _vm.Selected is null) Inspector.Visibility = Visibility.Collapsed; // уехала: стекло не считаем зря
        });

        // тост всплывает снизу с лёгким перелётом и уходит вниз; спрятавшись, убирается совсем (стекло не считаем зря)
        _toastIn = _springs.Add(new Spring(0.42, 0.16, 0), v =>
        {
            ToastBox.Opacity = Math.Clamp(v, 0, 1);
            var m = ToastBox.Margin;
            var bottom = ToastBottom - (1 - v) * ToastRise;
            if (m.Bottom != bottom) ToastBox.Margin = new Thickness(m.Left, 0, m.Right, bottom);
            if (_toastIn is { Target: 0, Resting: true } && _vm.Toast is null) { ToastBox.Visibility = Visibility.Collapsed; ToastBox.DataContext = null; }
        });

        // островок перетекает между фильтрами и поиском пружиной с лёгким перелётом, как капля; по ходу чуть сплющивается
        // (обе пружины созданы до Add: Add сразу зовёт «применить», а оно читает обе)
        _islandW = new Spring(0.36, 0.16, 520);
        // заголовок сжимается в капсулу не рывком за колесом мыши, а мягко догоняет прокрутку
        _titleT = new Spring(0.3, 0, 0);
        // заголовок прячется и возвращается на узком окне мягко, без рывка
        _titleVis = new Spring(0.25, 0, 1);
        _springs.Add(_islandW, v => { TopBar.Width = Math.Max(48, Math.Min(v, Math.Max(48, TopRow.ActualWidth))); IslandShape(); PlaceIsland(); });
        _springs.Add(_titleT, _ => ApplyTitleShrink());
        _springs.Add(_titleVis, _ => ApplyTitleShrink());

        // на экране меньше 1440 × 960 окно открывается по размеру экрана минус рамка (DESIGN_FLOWS раздел 2)
        var work = SystemParameters.WorkArea;
        // маленький экран (ноутбук с масштабом 125–150 %): минимум окна не больше экрана, иначе низ уходит за край
        (MinWidth, MinHeight) = WindowFit.Minimum(work.Width, work.Height);
        if (Width > work.Width - 16) Width = Math.Max(MinWidth, work.Width - 16);
        if (Height > work.Height - 16) Height = Math.Max(MinHeight, work.Height - 16);

        if (host.StartWidth >= MinWidth && host.StartHeight >= MinHeight)
        {
            Width = Math.Min(host.StartWidth, SystemParameters.VirtualScreenWidth);
            Height = Math.Min(host.StartHeight, SystemParameters.VirtualScreenHeight);
        }
        if (host.StartMaximized) WindowState = WindowState.Maximized;

        _motion = new BackgroundMotion(this, GradeMove, Lights, Flies, BackgroundPace.Fps(Native.RefreshRate()));
        // зерно фона: в цвете, точка в точку экрана (DitherEffect), или запасной слой поверх (UpdateGrain, из ApplyLook)
        DpiChanged += (_, _) => SizeDither();
        BackdropPaint.SizeChanged += (_, _) => SizeDither();
        SpringDriver.AnyRunningChanged += _motion.Hold;
        Closed += (_, _) => SpringDriver.AnyRunningChanged -= _motion.Hold;
        _glass = new[] { Nav, TopBar, TitleCapsule, Inspector, Caption, ToastBox, SettingsPanel, PreviewBar, CheckPanel, SitesPanel, BindPanel, IncomingPanel }.Select(g => (G: g, Tint: g.Tint)).ToArray();
        // «Обновления»: список из тех же карточек гаража, проверка через один координатор
        _upd = new UpdatesViewModel(vm, host.Updates ?? new NoUpdates(), host.Settings, a => Dispatcher.BeginInvoke(a));
        UpdScroller.DataContext = UpdEmptyBox.DataContext = CheckPanel.DataContext = _upd;
        // «Поиск»: сайты отвечают из фона, их ответы окно применяет у себя
        _find = new SearchViewModel(host.Search ?? new NoSearch(), () => vm.AllCards, a => Dispatcher.BeginInvoke(a)) { DownloadsAllowed = host.DownloadsAllowed };
        // установка из скачанного и перетащенного архива (2026-10-18)
        _in = new IncomingViewModel(host.Backend, vm, a => Dispatcher.BeginInvoke(a));
        IncomingLayer.DataContext = _in;
        OfferBox.DataContext = _in;
        _in.PropertyChanged += Incoming_PropertyChanged;
        host.Settings.DownloadsFolder = DownloadsWatcher.DownloadsPath();
        _downloads = new DownloadsWatcher(p => Dispatcher.BeginInvoke(() => { if (IsLoaded) _ = _in.OfferAsync(p, IncomingOrigin.Downloaded, _closing.Token); }));
        Closed += (_, _) => _downloads.Dispose();
        Root.SizeChanged += (_, _) => { if (_tour is not null) TourPlace(instant: true); };
        // плашка над сеткой появилась (обновление, «Скачан мод») — карточки сдвинулись: «окно» знакомства за ними
        Root.LayoutUpdated += (_, _) => { if (_tour is not null && !_tourMoving && TourTargetOf(_tour.Current.Target)?.Rect is { } r && !Near(r, _tourHole)) TourPlace(instant: true); };
        // «Интерфейс работает медленно»: итоги анимаций приходят от пружин (поток окна)
        _slow.Suggest += () => { _slowOffered = true; FadePlate(SlowBox, SlowBoxMove, true); };
        FramePolicy.Shared.Judged += _slow.Burst;
        Closed += (_, _) => FramePolicy.Shared.Judged -= _slow.Burst;
        // Preview…: поле поиска само принимает перетаскивание текста и не пустило бы файл дальше к окну
        AllowDrop = true;
        PreviewDragEnter += (s, e) => { _dragArchives = e.Data.GetDataPresent(DataFormats.FileDrop) ? DroppedArchives(e.Data).Count : 0; Window_DragOver(s, e); };
        PreviewDragOver += Window_DragOver;
        PreviewDragLeave += Window_DragLeave;
        PreviewDrop += Window_Drop;
        FindScroller.DataContext = FindEmptyBox.DataContext = SitesPanel.DataContext = _find;
        FindSkeletons.ItemsSource = Enumerable.Range(0, SearchViewModel.SkeletonSlots).ToArray();
        _find.PropertyChanged += Find_PropertyChanged;
        _find.Cards.CollectionChanged += (_, _) => LoadFindImages();
        foreach (var f in _find.Filters) f.PropertyChanged += (_, _) => Dispatcher.BeginInvoke(AdaptTopBar, DispatcherPriority.Loaded);
        vm.Refreshed += _find.InstalledChanged; // поставили или удалили мод — плашки «Установлен» по новому списку
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
        // ноутбук отключили от сети или включили экономию энергии — анимации не быстрее 60 кадров (и обратно)
        Microsoft.Win32.SystemEvents.PowerModeChanged += OnPowerModeChanged;
        Closed += (_, _) => Microsoft.Win32.SystemEvents.PowerModeChanged -= OnPowerModeChanged;

        // вход окна (DESIGN.md 6 «Один вход»): не в снимках сборочной машины и не при выключенных анимациях.
        // Начальное положение ставим сразу, до первого кадра; сами анимации — в OnLoaded, карточки — в CardGridPanel.
        _entrance = host.ShotPath is null && !SpringDriver.ReducedMotion;
        if (_entrance)
        {
            TopRow.Opacity = 0;
            Anim.Mover(TopRow).Y = EnterTop;
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
            // плитки раскладок Windows 11 на «Развернуть» не показываем (решено 2026-10-16: мешают): кнопка — обычная кнопка окна.
            // Пока окно тащат или тянут за край, фон и искры стоят: иначе каждый шаг ждал бы перерисовки всего стекла
            if (HwndSource.FromHwnd(new WindowInteropHelper(this).Handle) is { } src) src.AddHook(SizeMoveHook);
        };
        Root.SizeChanged += (_, _) => { ApplyWidthClass(); PlaceBlobs(TimeSpan.Zero); };
        TopRow.SizeChanged += (_, _) => AdaptTopBar();
        // заголовок сжимается, когда список прокручен (у гаража и «Обновлений» свои списки)
        Scroller.ScrollChanged += (_, _) => TitleFollowScroll();
        UpdScroller.ScrollChanged += (_, _) => TitleFollowScroll();
        FindScroller.ScrollChanged += (_, _) => TitleFollowScroll();
        // числа в фильтрах меняют их ширину: пересчитываем панель, когда они меняются
        foreach (var f in vm.Filters.Append(vm.DeletedFilter)) f.PropertyChanged += (_, _) => Dispatcher.BeginInvoke(AdaptTopBar, DispatcherPriority.Loaded);
        // «Удалённые» появляется и пропадает: островок перемеряется
        vm.Filters.CollectionChanged += (_, _) => Dispatcher.BeginInvoke(AdaptTopBar, DispatcherPriority.Loaded);
        vm.PropertyChanged += (_, a) => { if (a.PropertyName is nameof(GarageViewModel.CountText) or nameof(GarageViewModel.HeaderNote)) Dispatcher.BeginInvoke(AdaptTopBar, DispatcherPriority.Loaded); };
        Loaded += (_, _) => Dispatcher.BeginInvoke(DispatcherPriority.Loaded, AdaptTopBar);
        Loaded += OnLoaded;
        Loaded += (_, _) => FitMaximized();
        // свернули или развернули окно: Windows не сообщает, что мышь ушла с кнопки, и капля осталась бы висеть
        // развёрнутое окно переехало на другой монитор или сменился масштаб — выступ за рабочую область пересчитываем
        SizeChanged += (_, _) => { if (WindowState == WindowState.Maximized) FitMaximized(); };
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
            banner.PropertyChanged += Banner_PropertyChanged;
            Closed += (_, _) => banner.PropertyChanged -= Banner_PropertyChanged;
            ShowUpdateBox(banner.Visible, instant: true);
            UpdateBarView(banner, instant: true);
            if (_host.CheckUpdatesOnStart)
                Dispatcher.BeginInvoke(async () =>
                {
                    await Task.Delay(TimeSpan.FromSeconds(5)); // не мешаем запуску
                    if (IsLoaded) await banner.CheckOnStartAsync(_closing.Token);
                }, DispatcherPriority.ApplicationIdle);
        }
        if (_host.StartPage == "settings") NavSettings.IsChecked = true;
        if (_host.StartPage is "updates" or "bind") NavUpdates.IsChecked = true;
        if (_host.StartPage == "search") NavSearch.IsChecked = true;
        if (Enum.TryParse<UpdatesFilter>(_host.StartFilter, true, out var uf) && Enum.IsDefined(uf)) _upd.Filter = uf;
        else if (_host.StartPage is null && Enum.TryParse<GarageFilter>(_host.StartFilter, true, out var gf) && Enum.IsDefined(gf)) _vm.Filter = gf;
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
        if (_host.ShotToggle && _vm.Selected is not null) _vm.ToggleSelected();
        if (_host.ShotIncoming is { } incoming) _ = _in.OfferAsync(incoming, IncomingOrigin.Dropped);
        if (_host.ShotOffer is { } offer) _ = _in.OfferAsync(offer, IncomingOrigin.Downloaded);
        if (_host.ShotDropHint) ShowDropHint(true);
        if (_host.ShotSlowOffer) FadePlate(SlowBox, SlowBoxMove, true);
        if (_host.ShotDropMotion)
            // капля кнопок окна между «−» и «□», капля фильтров у «Все» — обе в пути (линза), как на видео с виртуалки;
            // после того как пружины встали (снимок через 1,5 с), иначе они вернули бы каплям покой
            _ = Task.Delay(1000).ContinueWith(_ => Dispatcher.BeginInvoke(() =>
            {
                CapDrop.BeginAnimation(OpacityProperty, null);
                CapDrop.Opacity = 1;
                CapDropMove.X = 24;
                CapDrop.Motion = 1;
                CapDrop.Sync();
                FilterDrop.Motion = 1;
                FilterDrop.Sync();
            }), TaskScheduler.Default);
        MaybeStartTour();
        UpdateDownloadsWatch(_host.Settings.Current);
        if (_host.StartSearch is { } query)
        {
            if (SearchPage) { QueryText.Text = query; _ = _find.RunAsync(); }
            else
            {
                if (UpdatesPage) _upd.SearchText = query; else _vm.SearchText = query;
                OpenSearch(focus: false, instant: true);
            }
        }
        if (_host.StartScroll is { } scroll)
            Dispatcher.BeginInvoke(() => (UpdatesPage ? UpdScroller : Scroller).ScrollToVerticalOffset(scroll), DispatcherPriority.Loaded);
        if (_host.ShotPath is not null) Dispatcher.BeginInvoke(DispatcherPriority.ApplicationIdle, TakeShot);
    }

    // ───────────── вход окна (DESIGN.md 6 «Один вход», доска: dropin, leftin) ─────────────

    private readonly bool _entrance;
    private const double EnterTop = -18, EnterLeft = -24;

    /// <summary>
    /// Один раз при открытии: верхняя панель опускается, левая выезжает слева, обе проявляются (0,6 с, пружина без отскока).
    /// Карточки всплывают сами (CardGridPanel, свойство Rise), правая панель выезжает как обычно.
    /// </summary>
    private void PlayEntrance()
    {
        Enter(TopRow, TranslateTransform.YProperty, EnterTop, 0.15);
        Enter(Nav, TranslateTransform.XProperty, EnterLeft, 0.05);
    }

    /// <summary>
    /// До OnLoaded панель стоит в начале входа (конструктор), здесь анимация держит начало до задержки и ведёт к месту.
    /// Обычное значение сразу ставится конечным: если анимацию когда-нибудь снимут, панель останется на месте и видна.
    /// Стекло берёт фон под собой по своему месту только после раскладки, поэтому на каждом кадре сдвига просим её.
    /// </summary>
    private static void Enter(FrameworkElement g, DependencyProperty axis, double from, double delay)
    {
        var start = TimeSpan.FromSeconds(delay);
        var time = TimeSpan.FromSeconds(0.6);
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
        // число модов в заголовке превью настроек могло смениться
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
        // картинка ещё не в кэше или архив ещё не читали ради названия (DESIGN_FLOWS 3.1а)
        var todo = _vm.AllCards.Where(c => (c.PreviewPath is null || !c.Data.ArchiveTitleKnown) && c.Data.ArchivePath is not null)
                               .Select(c => (c.Id, c.Data.ArchivePath!)).ToList();
        if (todo.Count == 0) return;
        var byId = _vm.AllCards.ToDictionary(c => c.Id);
        _ = loader.FillAsync(todo, (id, path) => Dispatcher.BeginInvoke(() =>
        {
            if (byId.TryGetValue(id, out var card)) card.PreviewPath = path;
        }), _previewCts.Token, (id, title) => Dispatcher.BeginInvoke(() => _vm.ArchiveTitleRead(id, title)));
    }

    private void Vm_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(GarageViewModel.Toast):
                _toastTimer.Stop();
                // сколько висит, решает гараж (Toast.Seconds, DESIGN_COMPONENTS 3.7); под мышью не гаснет
                _toastTimer.Interval = TimeSpan.FromSeconds(_vm.Toast?.Seconds ?? 3);
                ShowToast(_vm.Toast);
                if (_vm.Toast is not null && !ToastHeld) { _toastTimer.Start(); ToastCountdown(true); }
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
    private bool _sizeMoving;

    private IntPtr SizeMoveHook(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        const int WM_ENTERSIZEMOVE = 0x0231, WM_EXITSIZEMOVE = 0x0232;
        // развёрнутое окно — по рабочей области монитора, панель задач остаётся видна (2026-10-18)
        if (msg == Native.WM_GETMINMAXINFO) Native.FitMaximizedToWorkArea(hwnd, lParam);
        if (msg is WM_ENTERSIZEMOVE or WM_EXITSIZEMOVE)
        {
            _sizeMoving = msg == WM_ENTERSIZEMOVE;
            UpdateMotion();
        }
        return IntPtr.Zero;
    }

    private void UpdateMotion()
    {
        var s = _host.Settings.Current;
        var live = IsVisible && WindowState != WindowState.Minimized && IsActive && !_gameRunning && !_sizeMoving && _host.ShotPath is null;
        _motion.SetRunning(live && s.AnimatedBackground, live && s.Fireflies);
        // искры на кромках стекла — часть «Огоньков» и мерцают вместе с ними (DESIGN.md 6)
        foreach (var (g, _) in _glass) g.SetLive(live && s.Fireflies);
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
    /// Подложка жидкого стекла в светлом оформлении (2026-10-17: «светлое выглядит матовым»): белая и лёгкая — половина
    /// тёмной (0,44 → 0,22, 0,50 → 0,25), стекло прозрачное. Читаемость тёмного текста держит шейдер: тёмное под стеклом
    /// он подтягивает к белому (Glass.EffectiveLift). Тост и окно привязки — плашки поверх всего, у них плотнее: 0,70 → 0,82.
    /// </summary>
    /// <summary>
    /// Подложка островка в тёмном жидком стекле: 0,25 вместо 0,35 (2026-10-18, «прозрачнее процентов на 10»). Светлая
    /// считается от прежней (Tint в разметке), её не трогали.
    /// </summary>
    private static readonly Brush IslandTintDark = Frozen(new SolidColorBrush(Color.FromArgb(0x40, 0x0E, 0x18, 0x2C)));

    private Brush LightTint(Glass g, Brush dark)
    {
        if (_lightTints.TryGetValue(g, out var b)) return b;
        var a = dark is SolidColorBrush sc ? sc.Color.A / 255.0 : 0.44;
        var light = g == ToastBox || g == BindPanel ? Math.Min(0.9, a + 0.12) : a * 0.5;
        b = Frozen(new SolidColorBrush(Color.FromArgb((byte)Math.Round(light * 255), 0xF5, 0xF8, 0xFC)));
        _lightTints[g] = b;
        return b;
    }

    // число копий в ключ не входит: установщик читает его сам при каждой уборке, перечитывать папку не нужно
    private static string FolderKey(AppSettings s) => $"{s.ModsFolder}|{s.UserFolder}";

    /// <summary>В Windows переключили светлый режим, прозрачность или анимацию: применяем сразу, без перезапуска.</summary>
    private static void OnPowerModeChanged(object? sender, Microsoft.Win32.PowerModeChangedEventArgs e)
    {
        if (e.Mode == Microsoft.Win32.PowerModes.StatusChange) FramePolicy.Shared.PowerSaving = Native.PowerSaving();
    }

    private void OnUserPreferenceChanged(object? sender, Microsoft.Win32.UserPreferenceChangedEventArgs e) =>
        Dispatcher.BeginInvoke(() => { if (IsLoaded) ApplyLook(_host.Settings.Current); });

    private void OnSettingsChanged(AppSettings s)
    {
        UpdateDownloadsWatch(s);
        ApplyLook(s, _revealFrom);
        _revealFrom = null;
        _vm.ManyEnabledThreshold = s.ActiveModsWarnAbove;
        _find.RefreshSites(); // сайты могли включить или выключить
        // папку уже переключила программа (App подписана раньше окна): перечитываем моды
        var key = FolderKey(s);
        if (key != _folderKey) { _folderKey = key; if (IsLoaded) Refresh(); }
    }

    /// <summary>
    /// Применяет внешний вид сразу, без перезапуска: оформление (тёмное/светлое), стекло, тема фона, движение.
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

        // «Уменьшить прозрачность» в Windows сильнее настроек: стекло плоское (DESIGN.md 3.2, «Плоское»)
        var mode = Native.TransparencyOff() ? GlassMode.Flat : s.LiquidGlass && !WeakGraphics() ? GlassMode.Liquid : GlassMode.Matte;
        // предлагать «Быстро» есть смысл только на жидком стекле, один раз и если не отказывались (на снимках — никогда)
        var arm = mode == GlassMode.Liquid && !s.SlowOfferDeclined && !_slowOffered && _host.ShotPath is null;
        if (arm != _slow.Armed) { _slow.Armed = arm; _slow.Reset(); }
        foreach (var (g, tint) in _glass)
        {
            g.SetLook(mode, s.Fireflies, light);
            g.Tint = (mode, light) switch
            {
                (GlassMode.Flat, false) => FlatTint,
                (GlassMode.Flat, true) => FlatTintLight,
                (GlassMode.Matte, _) when g == ToastBox => light ? LightTint(g, tint) : tint, // у тоста своя, плотная подложка
                (GlassMode.Matte, false) => MatteTint,
                (GlassMode.Matte, true) => MatteTintLight,
                (_, true) => LightTint(g, tint),
                _ when g == TopBar || g == PreviewBar => IslandTintDark,
                _ => tint,
            };
        }

        UpdateGrain();
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
    /// Чтобы смена не была резкой, поверх ложится вуаль цвета нового фона и расходится кругом от места нажатия (≈0,6 с).
    /// Раньше под раскрытием лежал снимок прежнего вида, но снимок окна WPF считает процессором вместе со всем стеклом
    /// и тенями — это давало заметную паузу перед сменой.
    /// </summary>
    private void SwitchPalette(bool light, Point? origin)
    {
        var veil = IsLoaded && _light is not null && !SpringDriver.ReducedMotion && _host.ShotPath is null;
        var dicts = Application.Current.Resources.MergedDictionaries;
        var palette = new ResourceDictionary { Source = new Uri($"pack://application:,,,/Palette.{(light ? "Light" : "Dark")}.xaml", UriKind.Absolute) };
        if (dicts.Count > 0 && dicts[0].Source?.OriginalString.Contains("Palette.", StringComparison.Ordinal) == true) dicts[0] = palette;
        else dicts.Insert(0, palette);
        _light = light;
        Native.SetDarkFrame(this, !light);
        if (veil) Reveal(origin ?? new Point(Root.ActualWidth / 2, Root.ActualHeight / 2), light);
    }

    private void Reveal(Point origin, bool light)
    {
        var gen = ++_revealGen;
        var w = Root.ActualWidth; var h = Root.ActualHeight;
        // до самого дальнего угла и ещё на ширину мягкого края
        var far = new[] { new Point(0, 0), new Point(w, 0), new Point(0, h), new Point(w, h) }.Max(c => (c - origin).Length);
        const double edge = 0.7; // доля радиуса, где кончается прозрачное «окно» и начинается мягкий край
        var mask = new RadialGradientBrush
        {
            MappingMode = BrushMappingMode.Absolute,
            Center = origin, GradientOrigin = origin, RadiusX = 0, RadiusY = 0,
            GradientStops = { new GradientStop(Colors.Transparent, 0), new GradientStop(Colors.Transparent, edge), new GradientStop(Colors.Black, 1) },
        };
        ModeVeil.Fill = new SolidColorBrush(light ? Color.FromRgb(0xF2, 0xF5, 0xFA) : Color.FromRgb(0x10, 0x16, 0x26));
        ModeVeil.Opacity = 0.7;
        ModeVeil.OpacityMask = mask;
        ModeVeil.Visibility = Visibility.Visible;
        var time = TimeSpan.FromSeconds(0.6); // пружина без отскока: ощущается ≈0,38 с (длительность ×1,6, Motion/Spring.cs)
        var grow = new DoubleAnimation(0, far / edge + 40, time) { EasingFunction = Anim.Smooth };
        grow.Completed += (_, _) =>
        {
            if (gen != _revealGen) return; // уже идёт следующее раскрытие
            ModeVeil.Visibility = Visibility.Collapsed;
            ModeVeil.OpacityMask = null;
        };
        mask.BeginAnimation(RadialGradientBrush.RadiusXProperty, grow);
        mask.BeginAnimation(RadialGradientBrush.RadiusYProperty, grow.Clone());
        ModeVeil.BeginAnimation(OpacityProperty, new DoubleAnimation(0.7, 0, time) { EasingFunction = Anim.Smooth });
    }

    // ───────────── тема фона ─────────────

    /// <summary>
    /// Тема фона: цвета основы, полосы, вуали и пятен перетекают в новые за 0,6 с по мягкой S-кривой, а пятна плавно
    /// переезжают на свои места, — все слои одновременно и с одной скоростью, без вспышки. Без анимаций — 0,25 с.
    /// </summary>
    private void ApplyTheme(ThemePalette p, bool animate)
    {
        _palette = p;
        var time = !animate ? TimeSpan.Zero : TimeSpan.FromSeconds(SpringDriver.ReducedMotion ? 0.25 : 0.6);
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
        LayoutPreviewShapes(time); // фигуры превью настроек — в цветах новой темы, тем же перетеканием
    }

    private DitherEffect? _dither;

    /// <summary>
    /// Зерно фона (2026-10-18, «видны градации цветов»): с видеокартой — прибавкой к цвету, одинаково вверх и вниз на любом
    /// фоне (DitherEffect); без неё и на слабой графике — прежний слой поверх (Grain): шейдер на весь фон каждый кадр
    /// программная отрисовка не потянет. На снимках — всегда в цвете, чтобы его было видно.
    /// </summary>
    private void UpdateGrain()
    {
        if (DitherEffect.IsAvailable && !WeakGraphics())
        {
            _dither ??= new DitherEffect();
            if (!ReferenceEquals(BackdropPaint.Effect, _dither)) BackdropPaint.Effect = _dither;
            GrainLayer.Fill = null;
            SizeDither();
        }
        else
        {
            BackdropPaint.Effect = null;
            GrainLayer.Fill ??= Grain.Brush();
        }
    }

    private void SizeDither()
    {
        if (_dither is null || BackdropPaint.ActualWidth <= 0) return;
        var dpi = VisualTreeHelper.GetDpi(this);
        _dither.SetPixelSize(BackdropPaint.ActualWidth * dpi.DpiScaleX, BackdropPaint.ActualHeight * dpi.DpiScaleY);
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

    /// <summary>Развёрнутое окно занимает рабочую область монитора; если оно всё же выступает за неё, убираем выступ отступом.</summary>
    private void FitMaximized()
    {
        // окно, развёрнутое по рабочей области, может всё же выступать за неё на толщину рамки: содержимое отодвигаем ровно
        // на этот выступ (раньше — всегда на толщину рамки Windows, а окно при этом закрывало панель задач)
        var margin = new Thickness(0);
        if (WindowState == WindowState.Maximized && new WindowInteropHelper(this).Handle is var hwnd && hwnd != IntPtr.Zero)
        {
            var (l, t, r, bt) = Native.Overhang(hwnd);
            var dpi = VisualTreeHelper.GetDpi(this);
            margin = new Thickness(l / dpi.DpiScaleX, t / dpi.DpiScaleY, r / dpi.DpiScaleX, bt / dpi.DpiScaleY);
        }
        Root.Margin = margin;
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
        // у «Обновлений» справа всегда панель «Проверка», у «Поиска» — «Сайты»: место под них держим и на узком окне
        var beside = (open && !_narrow) || NavUpdates.IsChecked == true || NavSearch.IsChecked == true; // панель занимает место рядом с сеткой
        var right = beside ? 360 : 16;
        // прокрутка шире сетки на поля (44 слева, 24 справа — StackPanel внутри): тени карточек у краёв не обрезаются
        var gridMargin = new Thickness(116 - 44, 0, right - 24, 0);
        if (Scroller.Margin != gridMargin)
        {
            Scroller.Margin = UpdScroller.Margin = FindScroller.Margin = gridMargin;
            EmptyBox.Margin = UpdEmptyBox.Margin = FindEmptyBox.Margin = new Thickness(116, 84, right, 0);
            ToastBox.Margin = new Thickness(116, 0, right, ToastBox.Margin.Bottom);
            TopFade.Margin = new Thickness(104, 0, right - 12, 0);
        }
        Dim.Visibility = open && _narrow ? Visibility.Visible : Visibility.Collapsed;
        // рядом с сеткой карточки под панель не заходят: сквозь неё виден только фон, и наведение на карточки её стекло
        // не перерисовывает; на узком окне панель лежит поверх карточек — видна вся сцена
        Visual source = _narrow ? Scene : Backdrop;
        if (!ReferenceEquals(Inspector.Source, source)) Inspector.Source = source;
        if (open && Inspector.Visibility != Visibility.Visible) { Inspector.Visibility = Visibility.Visible; _inspLiveFrames = 0; }
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

    // ───────────── верх: заголовок и островок (решено 2026-10-17, «Островки») ─────────────

    private bool _searchOpen, _islandPlaced;
    private double _filtersW = 520, _searchW = 420;
    private TitleFit _titleFit = TitleFit.Full;

    /// <summary>Гараж, «Обновления» и «Поиск»: у них есть островок с фильтрами; в «Настройках» только заголовок.</summary>
    private bool IslandPage => NavGarage.IsChecked == true || UpdatesPage || SearchPage;

    /// <summary>
    /// Ширина островка: в покое — по фильтрам и кнопке поиска, с раскрытым поиском — капсула 320–440 px по ширине ряда.
    /// Сменилась ширина (числа в фильтрах, раздел, окно) — островок перетекает к новой.
    /// </summary>
    private void AdaptTopBar()
    {
        if (!IslandPage) { PlaceIsland(); return; }
        // поле запроса «Поиска» уже на узком окне: островок с ним и фильтрами помещается между заголовком и кнопками окна
        if (TopRow.ActualWidth > 0) QueryBox.Width = TopRow.ActualWidth < 760 ? 200 : 250;
        FilterLayer.Measure(new Size(double.PositiveInfinity, 48));
        _filtersW = Math.Ceiling(FilterLayer.DesiredSize.Width) + 12 + 2; // поля островка 6 + 6 и кромка
        var row = TopRow.ActualWidth;
        if (row > 0) _searchW = IslandLayout.SearchWidth(row);
        var target = _searchOpen ? _searchW : _filtersW;
        if (Math.Abs(_islandW.Target - target) > 0.5 || !_islandPlaced)
            _springs.To(_islandW, target, instant: !IsLoaded || !_islandPlaced);
        _islandPlaced = IsLoaded;
        PlaceIsland();
    }

    /// <summary>
    /// Островок — точно посередине окна (IslandLayout), где бы ни была правая панель. На узком окне уступает заголовок:
    /// крупный → капсула → нет заголовка (раздел и так виден по капле слева). Вид заголовка зависит от окна и раздела,
    /// но не от того, открыт ли поиск.
    /// </summary>
    private void PlaceIsland()
    {
        var row = TopRow.ActualWidth;
        if (row <= 0) return;
        var w = double.IsNaN(TopBar.Width) ? _filtersW : TopBar.Width;
        var x = (row - w) / 2;
        var fit = TitleFit.Full;
        if (IslandPage)
        {
            var left = TopRow.Margin.Left;
            // заголовку оставляем место под самую широкую форму островка: открыли или закрыли поиск — заголовок тот же
            var room = Math.Max(_filtersW, IslandLayout.SearchWidth(row));
            var p = IslandLayout.Place(Root.ActualWidth, left, left + row, w, room, BigTitleWidth(), CapsuleTitleWidth());
            x = p.X - left;
            fit = p.Title;
        }
        SetTitleFit(fit);
        x = Math.Max(0, Math.Round(x));
        if (TopBar.Margin.Left != x) TopBar.Margin = new Thickness(x, 0, 0, 0);
    }

    private void SetTitleFit(TitleFit fit)
    {
        if (fit == _titleFit) return;
        _titleFit = fit;
        _springs.To(_titleVis, fit == TitleFit.Hidden ? 0 : 1, instant: !IsLoaded);
        TitleFollowScroll(); // капсулой (Compact) — независимо от прокрутки
    }

    /// <summary>Раздел сменился: заголовок сразу в своём виде, без перетекания поверх смены раздела.</summary>
    private void SnapTitle()
    {
        _springs.To(_titleVis, _titleVis.Target, instant: true);
        _springs.To(_titleT, _titleT.Target, instant: true);
    }

    /// <summary>Крупный заголовок со строкой под ним (ширина не зависит от сжатия: решение о месте не мигает).</summary>
    private double BigTitleWidth() =>
        Math.Max(TextWidth(SectionTitle), CountLabel.Visibility == Visibility.Visible ? TextWidth(CountLabel) : 0) + TitleBlock.Margin.Left + 2;

    private double CapsuleTitleWidth() =>
        TextWidth(CapsuleTitle) + (CapsuleCount.Text.Length > 0 ? TextWidth(CapsuleCount) : 0) + 32;

    /// <summary>Островок чуть сплющивается, пока перетекает (как капля в пути), и выпрямляется, встав на место.</summary>
    private void IslandShape()
    {
        var k = Math.Min(0.05, Math.Abs(_islandW.Velocity) / 18000);
        IslandScale.ScaleY = 1 - k;
        IslandScale.ScaleX = 1 + k * 0.3;
    }

    /// <summary>
    /// Заголовок следует за прокруткой своего списка: первые 56 px — от крупного к капсуле. Крупному нет места слева от
    /// островка (Compact) — капсула при любой прокрутке.
    /// </summary>
    private void TitleFollowScroll(bool instant = false)
    {
        var off = NavGarage.IsChecked == true ? Scroller.VerticalOffset : UpdatesPage ? UpdScroller.VerticalOffset : SearchPage ? FindScroller.VerticalOffset : 0;
        var t = _titleFit == TitleFit.Compact ? 1 : Math.Clamp(off / 56, 0, 1);
        if (Math.Abs(_titleT.Target - t) < 0.001 && !instant) return;
        _springs.To(_titleT, t, instant || !IsLoaded);
    }

    /// <summary>
    /// Крупный заголовок гаснет, чуть уменьшаясь и поднимаясь; капсула проявляется, подрастая с 94 %. Пока какой-то из
    /// них не виден совсем, он убран из раскладки (стекло капсулы не считается зря).
    /// </summary>
    private void ApplyTitleShrink()
    {
        var t = Math.Clamp(_titleT.Value, 0, 1);
        var vis = Math.Clamp(_titleVis.Value, 0, 1);
        var big = Math.Clamp(1 - t * 1.6, 0, 1) * vis;
        var cap = Math.Clamp((t - 0.35) / 0.65, 0, 1) * vis;
        TitleBlock.Visibility = big > 0.001 ? Visibility.Visible : Visibility.Collapsed;
        TitleBlock.Opacity = big;
        TitleScale.ScaleX = TitleScale.ScaleY = 1 - 0.12 * t;
        TitleMove.Y = -6 * t;
        TitleCapsule.Visibility = cap > 0.001 ? Visibility.Visible : Visibility.Collapsed;
        TitleCapsule.Opacity = cap;
        CapsuleScale.ScaleX = CapsuleScale.ScaleY = 0.94 + 0.06 * cap;
    }

    // поиск: островок перетекает в одну капсулу и обратно

    private int _searchGen;

    private void SearchButton_Click(object sender, RoutedEventArgs e) => OpenSearch(focus: true);

    private void SearchClose_Click(object sender, RoutedEventArgs e)
    {
        SearchBox.Clear();
        CloseSearch();
    }

    /// <summary>Поле потеряло фокус пустым — островок сам сворачивается в фильтры; с запросом остаётся раскрытым.</summary>
    private void SearchBox_LostKeyboardFocus(object sender, KeyboardFocusChangedEventArgs e)
    {
        if (SearchBox.Text.Length == 0 && _searchOpen && !SearchLayer.IsKeyboardFocusWithin) CloseSearch();
    }

    /// <summary>
    /// Раскрыть поиск: фильтры быстро гаснут, островок пружиной перетекает в капсулу поиска, поле проявляется чуть позже.
    /// Без анимаций — сразу. Стекло при этом то же (жидкое или матовое): меняется только форма.
    /// </summary>
    private void OpenSearch(bool focus, bool instant = false)
    {
        if (!_searchOpen)
        {
            _searchOpen = true;
            var gen = ++_searchGen;
            instant |= SpringDriver.ReducedMotion || !IsLoaded;
            FilterLayer.IsHitTestVisible = false;
            SearchLayer.Visibility = Visibility.Visible;
            Crossfade(FilterLayer, 0, instant ? 0 : 0.12, 0, () => { if (gen == _searchGen) FilterLayer.Visibility = Visibility.Hidden; });
            Crossfade(SearchLayer, 1, instant ? 0 : 0.22, instant ? 0 : 0.07, null);
            AdaptTopBar();
            if (instant) _springs.To(_islandW, _searchW, instant: true);
        }
        // фокус — после раскладки: поле только что стало видимым
        if (focus) Dispatcher.BeginInvoke(() => { if (_searchOpen) { SearchBox.Focus(); SearchBox.SelectAll(); } }, DispatcherPriority.Input);
    }

    /// <summary>Свернуть поиск обратно в фильтры (запрос остаётся у раздела; закрывают его пустым или крестиком).</summary>
    private void CloseSearch(bool instant = false)
    {
        if (!_searchOpen) return;
        _searchOpen = false;
        var gen = ++_searchGen;
        instant |= SpringDriver.ReducedMotion || !IsLoaded;
        if (SearchBox.IsKeyboardFocusWithin) Keyboard.ClearFocus();
        FilterLayer.Visibility = Visibility.Visible;
        FilterLayer.IsHitTestVisible = true;
        Crossfade(SearchLayer, 0, instant ? 0 : 0.1, 0, () => { if (gen == _searchGen) SearchLayer.Visibility = Visibility.Collapsed; });
        Crossfade(FilterLayer, 1, instant ? 0 : 0.22, instant ? 0 : 0.08, null);
        AdaptTopBar();
        if (instant) _springs.To(_islandW, _filtersW, instant: true);
    }

    private static void Crossfade(UIElement el, double to, double seconds, double delay, Action? done)
    {
        if (seconds <= 0)
        {
            el.BeginAnimation(OpacityProperty, null);
            el.Opacity = to;
            done?.Invoke();
            return;
        }
        var a = new DoubleAnimation(to, TimeSpan.FromSeconds(seconds)) { BeginTime = TimeSpan.FromSeconds(delay), EasingFunction = Anim.Smooth };
        if (done is not null) a.Completed += (_, _) => done();
        el.BeginAnimation(OpacityProperty, a);
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
        _springs.To(_capX, 2 + 44 * i, instant: appearing);
        _springs.To(_capScale, 1);
        Fade(CapDrop, 1, 0.2);
        // под «Закрыть» капля краснеет за 0,15 с (было 0,25: красное успевало растечься по соседней кнопке); появляясь — сразу
        var danger = i == 2 ? 1.0 : 0.0;
        if (appearing || SpringDriver.ReducedMotion) { CapDrop.BeginAnimation(Droplet.DangerProperty, null); CapDrop.Danger = danger; }
        else CapDrop.BeginAnimation(Droplet.DangerProperty, new DoubleAnimation(danger, TimeSpan.FromSeconds(0.15)) { EasingFunction = Anim.Smooth });
    }

    // ───────────── форма капель: растяжение в пути и сжатие при нажатии (DESIGN_MOTION 3.3) ─────────────

    /// <summary>Насколько капля вытягивается по ходу движения: от скорости пружины, не больше 22 % (как капля воды в пути).</summary>
    private static double StretchOf(double velocity) => Math.Min(0.22, Math.Abs(velocity) / 1800);

    /// <summary>Вытянулась вдоль — сузилась поперёк (объём «сохраняется»), и всё это умножается на сжатие нажатия.</summary>
    private static void Shape(ScaleTransform t, double along, bool vertical, double press, double scale = 1)
    {
        var a = (1 + along) * press * scale;
        var b = (1 - along * 0.45) * press * scale;
        t.ScaleX = vertical ? b : a;
        t.ScaleY = vertical ? a : b;
    }

    /// <summary>Пружины капель: голова (быстрая, с перелётом) и хвост (медленнее, чуть пружинит).</summary>
    internal const double DropHead = 0.34, DropHeadBounce = 0.32, DropTail = 0.40, DropTailBounce = 0.15;

    /// <summary>Капля левой панели: 68 × 64, отступ 8 от краёв панели (форма 4, утверждена 2026-10-17).</summary>
    private const double NavDropTop = 8, NavDropH = 64;

    private void NavDropShape()
    {
        var b = DropBody.Of(_navDrop.Value, _navTrail.Value, NavDropH, _navDrop.Velocity);
        // как у фильтров: перелёт у крайнего пункта не выносит каплю за его место — она сплющивается о край
        if (Nav.ActualHeight > 0) b = b.Within(NavDropTop, Nav.ActualHeight - NavDropTop, _navPress.Value * b.Lift);
        NavDropMove.Y = b.Start;
        if (Math.Abs(NavDrop.Height - b.Length) > 0.01) NavDrop.Height = b.Length;
        NavDrop.Motion = b.Motion;
        NavDropScale.ScaleX = b.Across * _navPress.Value * b.Lift;
        NavDropScale.ScaleY = _navPress.Value * b.Lift;
        NavDrop.Sync();
    }

    private void FilterDropShape()
    {
        var w = Math.Max(20, _filterW.Value);
        var b = DropBody.Of(_filterX.Value, _filterTrail.Value, w, _filterX.Velocity);
        // стенки островка: капля у крайнего пункта стоит на 3 точки за ним (поле островка), дальше — сплющивается об край
        if (FilterList.ActualWidth > 0) b = b.Within(-3, FilterList.ActualWidth + 3, _filterPress.Value * b.Lift);
        FilterDropMove.X = b.Start;
        if (Math.Abs(FilterDrop.Width - b.Length) > 0.01) FilterDrop.Width = b.Length;
        FilterDrop.Motion = b.Motion;
        FilterDropScale.ScaleX = _filterPress.Value * b.Lift;
        FilterDropScale.ScaleY = b.Across * _filterPress.Value * b.Lift;
        FilterDrop.Sync();
    }

    private void CapDropShape()
    {
        Shape(CapDropScale, StretchOf(_capX.Velocity), vertical: false, _capPress.Value, _capScale.Value);
        CapDrop.Motion = DropBody.MotionOf(0, _capX.Velocity);
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
        if (_tour is not null) DimCaption(true);
    }

    private void Caption_MouseEnter(object sender, MouseEventArgs e)
    {
        if (_tour is not null) DimCaption(false);
    }

    /// <summary>Кнопки окна при знакомстве: тусклые, как всё под затемнением, но живые — навёл, и капсула ожила.</summary>
    private const double CaptionTourOpacity = 0.4;

    private void DimCaption(bool dim)
    {
        var to = dim && !Caption.IsMouseOver ? CaptionTourOpacity : 1;
        if (SpringDriver.ReducedMotion || _host.ShotPath is not null) { Caption.BeginAnimation(OpacityProperty, null); Caption.Opacity = to; return; }
        Caption.BeginAnimation(OpacityProperty, Anim.Go(Caption.Opacity, to, 0.25, Anim.Smooth));
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
        _host.Updates?.Cancel(); // идущая проверка останавливается; найденное уже в библиотеке, Flush ниже допишет его на диск
        _previewCts?.Cancel();
        _host.SaveWindow?.Invoke(this);
        _host.SavePending?.Invoke(_vm.PendingChanges);
        _vm.CommitPending(); // тост «Отменить» уже не нажмут: удалённое забывается до записи библиотеки
        _find.Cancel();
        _findImagesCts?.Cancel();
        _host.Flush?.Invoke();
    }

    // ───────────── левая панель ─────────────

    private void Nav_Checked(object sender, RoutedEventArgs e)
    {
        if (_navDrop is null) return; // InitializeComponent ещё идёт
        var rb = (RadioButton)sender;
        _springs.To(_navDrop, Canvas.GetTop(rb), instant: !IsLoaded);
        _springs.To(_navTrail, Canvas.GetTop(rb), instant: !IsLoaded);
        var garage = rb == NavGarage;
        var settings = rb == NavSettings;
        var updates = rb == NavUpdates;
        var search = rb == NavSearch;
        if (!garage) _vm.Selected = null; // правая панель — про мод из гаража, в других разделах ей не место
        SectionTitle.Text = CapsuleTitle.Text = (string)rb.Content;
        Scroller.Visibility = garage ? Visibility.Visible : Visibility.Collapsed;
        EmptyBox.Visibility = garage && _vm.Empty is not null ? Visibility.Visible : Visibility.Collapsed;
        UpdScroller.Visibility = CheckPanel.Visibility = updates ? Visibility.Visible : Visibility.Collapsed;
        UpdEmptyBox.Visibility = updates && _upd.Empty is not null ? Visibility.Visible : Visibility.Collapsed;
        FindScroller.Visibility = SitesPanel.Visibility = search ? Visibility.Visible : Visibility.Collapsed;
        FindEmptyBox.Visibility = search && _find.Empty is not null ? Visibility.Visible : Visibility.Collapsed;
        SettingsPanel.Visibility = settings ? Visibility.Visible : Visibility.Collapsed;
        // фильтры, поиск и число — у гаража, «Обновлений» и «Поиска» свои, островок общий; в настройках им не место
        var gv = garage || updates || search ? Visibility.Visible : Visibility.Collapsed;
        TopBar.Visibility = CountLabel.Visibility = gv;
        BindTopBar(garage ? TopSource.Garage : updates ? TopSource.Updates : search ? TopSource.Search : TopSource.None);
        // поиск раздела: есть запрос — островок сразу раскрыт, нет — сразу фильтры; заголовок — по прокрутке нового списка.
        // У «Поиска» поле запроса всегда стоит в островке рядом с фильтрами, капсула поиска не раскрывается
        if (garage || updates) { if (SearchBox.Text.Length > 0) OpenSearch(focus: false, instant: true); else CloseSearch(instant: true); }
        else if (search)
        {
            CloseSearch(instant: true);
            _find.RefreshSites();
            // пришли в «Поиск» без запроса — сразу в поле (DESIGN_FLOWS 4: «поле ввода в фокусе»)
            if (IsLoaded && QueryText.Text.Length == 0) Dispatcher.BeginInvoke(() => { if (SearchPage) QueryText.Focus(); }, DispatcherPriority.Input);
        }
        TitleFollowScroll(instant: true);
        PlaceInspector(instant: !IsLoaded);
        PlaceIsland();
        SnapTitle();
        Dispatcher.BeginInvoke(() => { AdaptTopBar(); SnapTitle(); }, DispatcherPriority.Loaded);
        if (garage) EmptyBox.SetBinding(VisibilityProperty, new System.Windows.Data.Binding(nameof(GarageViewModel.Empty)) { Converter = (System.Windows.Data.IValueConverter)FindResource("NullToCollapsed") });
        // раздел «Обновления» открыли: найденное просмотрено, бейдж больше не пульсирует до конца сеанса
        if (rb == NavUpdates && !_updatesSeen) { _updatesSeen = true; UpdateBadgePulse(); }
        if (settings) FillLookPreview();
        // сменился раздел: двигается только содержимое, оно проявляется с лёгким подъёмом; капля уже перетекает
        if (IsLoaded)
        {
            if (garage) { Arrive(Scroller); Arrive(EmptyBox); }
            else if (updates) { Arrive(UpdScroller); Arrive(UpdEmptyBox); Arrive(CheckPanel, rise: false); }
            else if (settings) Arrive(SettingsPanel, rise: false); // стекло только проявляется: его фон берётся по месту
            else if (search) { Arrive(FindScroller); Arrive(FindEmptyBox); Arrive(SitesPanel, rise: false); }
        }
    }

    // ───────────── «Как это выглядит» ─────────────

    /// <summary>
    /// Под стеклом превью — яркие мягкие фигуры в цветах темы фона (решено 2026-10-17 вместо картинок модов): стекло на них
    /// видно лучше всего, и превью одинаково на любом гараже. Расклад — <see cref="PreviewShapes"/>; сменилась тема —
    /// цвета перетекают за то же время, что и фон, сменился размер — фигуры встают на новые места.
    /// </summary>
    private void FillLookPreview()
    {
        Dispatcher.BeginInvoke(PlacePreviewBar, DispatcherPriority.Loaded); // число модов в заголовке могло смениться
        LayoutPreviewShapes(TimeSpan.Zero);
    }

    private void LayoutPreviewShapes(TimeSpan time)
    {
        if (_palette is not { } p) return;
        var shapes = PreviewShapes.For(p, LookPreview.ActualWidth, LookPreview.ActualHeight);
        if (shapes.Count == 0) return;
        if (PvShapes.Children.Count != shapes.Count)
        {
            PvShapes.Children.Clear();
            foreach (var _ in shapes)
                PvShapes.Children.Add(new Border
                {
                    Background = new LinearGradientBrush { StartPoint = new Point(0, 0), EndPoint = new Point(1, 1), GradientStops = { new GradientStop(), new GradientStop { Offset = 1 } } },
                    RenderTransform = new RotateTransform(),
                    RenderTransformOrigin = new Point(0.5, 0.5),
                });
            time = TimeSpan.Zero; // только что созданы: сразу в нужных цветах
        }
        for (var i = 0; i < shapes.Count; i++)
        {
            var sh = shapes[i];
            var b = (Border)PvShapes.Children[i];
            b.Width = sh.W;
            b.Height = sh.H;
            b.CornerRadius = new CornerRadius(sh.Corner);
            Canvas.SetLeft(b, sh.X - sh.W / 2);
            Canvas.SetTop(b, sh.Y - sh.H / 2);
            ((RotateTransform)b.RenderTransform).Angle = sh.Angle;
            var stops = ((LinearGradientBrush)b.Background).GradientStops;
            ShapeColor(stops[0], ThemeSwatchConverter.ToColor(sh.From), time);
            ShapeColor(stops[1], ThemeSwatchConverter.ToColor(sh.To), time);
        }
    }

    private static void ShapeColor(GradientStop stop, Color c, TimeSpan time)
    {
        if (time == TimeSpan.Zero) { stop.BeginAnimation(GradientStop.ColorProperty, null); stop.Color = c; return; }
        stop.BeginAnimation(GradientStop.ColorProperty, new ColorAnimation(c, time) { EasingFunction = ThemeEase }, HandoffBehavior.SnapshotAndReplace);
    }

    private void LookPreview_SizeChanged(object sender, SizeChangedEventArgs e)
    {
        LookPreview.OpacityMask = Glass.RoundMask(e.NewSize.Width, e.NewSize.Height, 22); // маска: края сглажены
        PlacePreviewBar();
        LayoutPreviewShapes(TimeSpan.Zero);
    }

    /// <summary>
    /// Островок превью — посередине, как в окне. Узкое превью, и он задел бы заголовок — встаёт к правому краю
    /// (в превью заголовок не сжимается).
    /// </summary>
    private void PlacePreviewBar()
    {
        var w = LookPreview.ActualWidth;
        if (w <= 0) return;
        PreviewBar.Measure(new Size(double.PositiveInfinity, 48));
        PvTitle.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
        var bar = PreviewBar.DesiredSize.Width;
        var title = PvTitle.DesiredSize.Width;
        const double left = 24, edge = 22;
        var p = IslandLayout.Place(w, left, w - edge, bar, bar, title, title);
        var x = p.Title == TitleFit.Full ? p.X : Math.Max(left, w - edge - bar);
        var m = new Thickness(Math.Round(x), 18, 0, 0);
        if (PreviewBar.Margin != m) PreviewBar.Margin = m;
    }

    private readonly Dictionary<UIElement, int> _arriving = new();
    private int _arriveGen;

    /// <summary>
    /// Новое содержимое раздела: проявление и подъём на 10 px (≈0,2 с, пружина без отскока). На время появления элемент
    /// кэшируется картинкой: прозрачность и сдвиг тогда — наложение готовой картинки, а не перерисовка всех карточек,
    /// стекла и переключателей на каждом кадре (из-за этого вход в «Настройки» казался вечностью). Стеклянные панели не
    /// сдвигаются (<paramref name="rise"/> = false): копия фона под стеклом берётся по месту.
    /// </summary>
    private void Arrive(UIElement el, bool rise = true)
    {
        if (SpringDriver.ReducedMotion || (_host.ShotPath is not null && !_probing)) return;
        const double seconds = 0.36;
        var gen = ++_arriveGen;
        var ours = el.CacheMode is null || _arriving.ContainsKey(el);
        if (ours)
        {
            _arriving[el] = gen;
            el.CacheMode ??= new BitmapCache { SnapsToDevicePixels = true };
        }
        var fade = Anim.Go(0, 1, seconds, Anim.Smooth);
        if (ours)
            fade.Completed += (_, _) =>
            {
                // последнее появление этого элемента закончилось — кэш больше не нужен (иначе текст и стекло «застынут»)
                if (_arriving.TryGetValue(el, out var g) && g == gen) { _arriving.Remove(el); el.CacheMode = null; }
            };
        el.BeginAnimation(OpacityProperty, fade);
        if (rise) Anim.Mover(el).BeginAnimation(TranslateTransform.YProperty, Anim.Go(10, 0, seconds, Anim.Smooth));
    }

    // ───────────── фильтры и поиск ─────────────

    private void Filter_Click(object sender, RoutedEventArgs e)
    {
        switch (((FrameworkElement)sender).DataContext)
        {
            case FilterItem f: _vm.Filter = f.Filter; break;
            case UpdatesFilterItem u: _upd.Filter = u.Filter; break;
            case SearchFilterItem f: _find.Filter = f.Filter; break;
        }
    }

    private enum TopSource { None, Garage, Updates, Search }

    /// <summary>
    /// Островок общий у гаража, «Обновлений» и «Поиска»: фильтры, поле и число модов переключаются на раздел. Капля фильтра
    /// при этом не уезжает издалека, а сразу встаёт под активный пункт нового раздела. У «Поиска» вместо круглой кнопки
    /// поиска — поле запроса по сайтам прямо в островке.
    /// </summary>
    private void BindTopBar(TopSource page)
    {
        if (page == TopSource.None) return;
        object source = page switch { TopSource.Updates => _upd, TopSource.Search => _find, _ => _vm };
        System.Collections.IEnumerable wanted = page switch { TopSource.Updates => _upd.Filters, TopSource.Search => _find.Filters, _ => _vm.Filters };
        if (ReferenceEquals(FilterList.ItemsSource, wanted)) return;
        FilterList.ItemsSource = wanted;
        var search = page == TopSource.Search;
        QueryBox.Visibility = search ? Visibility.Visible : Visibility.Collapsed;
        SearchButton.Visibility = search ? Visibility.Collapsed : Visibility.Visible;
        if (!search)
            SearchBox.SetBinding(TextBox.TextProperty, new System.Windows.Data.Binding("SearchText") { Source = source, UpdateSourceTrigger = System.Windows.Data.UpdateSourceTrigger.PropertyChanged });
        // под заголовком у гаража «15 модов · 3 ждут обновления», у «Обновлений» — «3 доступно», у «Поиска» — «9 модов»
        CountLabel.SetBinding(TextBlock.TextProperty, new System.Windows.Data.Binding(page == TopSource.Updates ? "CountText" : "HeaderNote") { Source = source });
        CapsuleCount.SetBinding(TextBlock.TextProperty, new System.Windows.Data.Binding(search ? "CountText" : "HeaderCount") { Source = source });
        SearchNote.SetBinding(TextBlock.TextProperty, new System.Windows.Data.Binding("SearchNote") { Source = source });
        var updates = page == TopSource.Updates;
        SearchHint.Text = updates ? "Найти среди обновлений" : "Найти мод";
        SearchButton.ToolTip = updates ? "Найти среди обновлений (Ctrl+F)" : "Найти мод (Ctrl+F)";
        System.Windows.Automation.AutomationProperties.SetName(SearchBox, updates ? "Найти среди найденных обновлений" : "Найти среди установленных модов");
        _filterPlaced = false; // следующая раскладка ставит каплю сразу
    }

    private bool UpdatesPage => NavUpdates.IsChecked == true;
    private bool SearchPage => NavSearch.IsChecked == true;

    /// <summary>Капля фильтра ложится под активный пункт; пункты меняют ширину (числа), поэтому меряем по факту.</summary>
    private void PlaceFilterDrop(bool instant)
    {
        object? item = UpdatesPage ? _upd.Filters.FirstOrDefault(f => f.IsActive)
            : SearchPage ? _find.Filters.FirstOrDefault(f => f.IsActive)
            : _vm.Filters.FirstOrDefault(f => f.IsActive);
        if (item is null || FilterList.ItemContainerGenerator.ContainerFromItem(item) is not FrameworkElement el || el.ActualWidth <= 0) return;
        var x = el.TransformToAncestor(FilterList).Transform(new Point(0, 0)).X;
        var targetX = x - 3;
        var targetW = el.ActualWidth + 6;
        if (Math.Abs(_filterX.Target - targetX) < 0.5 && Math.Abs(_filterW.Target - targetW) < 0.5 && _filterPlaced) return;
        _springs.To(_filterX, targetX, instant);
        _springs.To(_filterTrail, targetX, instant);
        _springs.To(_filterW, targetW, instant);
        _filterPlaced = true;
    }

    private void SearchBox_TextChanged(object sender, TextChangedEventArgs e) =>
        SearchHint.Visibility = SearchBox.Text.Length == 0 ? Visibility.Visible : Visibility.Collapsed;

    // ───────────── «Поиск» по сайтам (DESIGN_FLOWS 3.2, DESIGN.md 5.0.1) ─────────────

    private void Find_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(SearchViewModel.Filter):
                PlaceFilterDrop(instant: false);
                break;
            case nameof(SearchViewModel.Empty):
                if (SearchPage) FindEmptyBox.Visibility = _find.Empty is not null ? Visibility.Visible : Visibility.Collapsed;
                break;
            case nameof(SearchViewModel.Skeletons):
                FindSkeletons.Visibility = _find.Skeletons > 0 ? Visibility.Visible : Visibility.Collapsed;
                break;
            case nameof(SearchViewModel.HeaderNote) or nameof(SearchViewModel.CountText):
                if (SearchPage) Dispatcher.BeginInvoke(AdaptTopBar, DispatcherPriority.Loaded);
                break;
        }
    }

    private void FindPanel_Loaded(object sender, RoutedEventArgs e) =>
        ((CardGridPanel)sender).Animated = _host.ShotPath is null; // снимку нужно конечное положение сразу

    private CancellationTokenSource? _findImagesCts;

    /// <summary>Картинки карточек поиска качаются в фоне (не больше четырёх разом), пришедшая встаёт на карточку сразу.</summary>
    private void LoadFindImages()
    {
        if (_host.SearchImages is not { } fetch) return;
        var todo = _find.Cards.Where(c => c.PreviewPath is null && c.PreviewUrl is not null && _findAsked.Add(c.PreviewUrl.AbsoluteUri)).ToList();
        if (todo.Count == 0) return;
        _findImagesCts ??= new CancellationTokenSource();
        var ct = _findImagesCts.Token;
        foreach (var card in todo)
        {
            var url = card.PreviewUrl!;
            _ = Task.Run(async () =>
            {
                var path = await fetch(url, ct).ConfigureAwait(false);
                if (path is null) return;
                await Dispatcher.BeginInvoke(() =>
                {
                    _findImages[url.AbsoluteUri] = path;
                    foreach (var c in _find.Cards.Where(c => c.PreviewUrl == url)) c.PreviewPath = path;
                });
            }, ct);
        }
        // картинка уже скачана для этого адреса (карточка пересобралась после «Это один мод»)
        foreach (var c in _find.Cards.Where(c => c.PreviewPath is null && c.PreviewUrl is { } u && _findImages.ContainsKey(u.AbsoluteUri)))
            c.PreviewPath = _findImages[c.PreviewUrl!.AbsoluteUri];
    }

    private readonly HashSet<string> _findAsked = new();
    private readonly Dictionary<string, string> _findImages = new();

    private void QueryText_TextChanged(object sender, TextChangedEventArgs e)
    {
        var empty = QueryText.Text.Length == 0;
        QueryHint.Visibility = empty ? Visibility.Visible : Visibility.Collapsed;
        QueryClear.Visibility = empty ? Visibility.Collapsed : Visibility.Visible;
        _find.Query = QueryText.Text;
    }

    /// <summary>Enter — на сайты (DESIGN_FLOWS 3.2: «ввод названия, Enter»), а не на каждую букву.</summary>
    private void QueryText_PreviewKeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key != Key.Enter) return;
        _ = _find.RunAsync();
        e.Handled = true;
    }

    private void QueryClear_Click(object sender, RoutedEventArgs e)
    {
        QueryText.Clear();
        QueryText.Focus();
    }

    private void QueryBox_MouseDown(object sender, MouseButtonEventArgs e)
    {
        QueryText.Focus();
        e.Handled = true; // иначе щелчок начал бы перетаскивать окно
    }

    private static FoundCard? FoundOf(object sender) => (sender as FrameworkElement)?.DataContext as FoundCard;

    /// <summary>
    /// «Страница» и «Купить у автора» открывают мод на сайте в браузере. Загрузки в этой сборке нет: «Установить» (появится
    /// с ней) пока ведёт туда же.
    /// </summary>
    private void FoundMain_Click(object sender, RoutedEventArgs e) => OpenUrl(FoundOf(sender)?.PageUrl);
    private void FoundPage_Click(object sender, RoutedEventArgs e) => OpenUrl(FoundOf(sender)?.PageUrl);

    private void FoundSame_Click(object sender, RoutedEventArgs e)
    {
        if (FoundOf(sender) is { } c) _find.SameMod(c);
    }

    private void FoundDifferent_Click(object sender, RoutedEventArgs e)
    {
        if (FoundOf(sender) is { } c) _find.DifferentMods(c);
    }

    private void FindEmpty_Click(object sender, RoutedEventArgs e)
    {
        switch (_find.Empty?.Action)
        {
            case EmptyAction.OpenSiteSettings: OpenSiteSettings(); break;
            case EmptyAction.Retry: _ = _find.RunAsync(); break;
            case EmptyAction.ShowAll: _find.Filter = SearchFilter.All; break;
        }
    }

    private void SitesSettings_Click(object sender, RoutedEventArgs e) => OpenSiteSettings();

    private void ReapplyPreset_Click(object sender, RoutedEventArgs e) => _host.Settings.ReapplyPreset();

    private void OpenSiteSettings()
    {
        _host.Settings.Section = SettingsSection.Sites;
        NavSettings.IsChecked = true;
    }

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
        else _panel?.BringIndexIntoView(_vm.Cards.IndexOf(c), 84); // карточка далеко и ещё не создана (сетка создаёт только видимые)
    }

    /// <summary>Фокус с клавиатуры идёт за выбором: иначе белая рамка осталась бы на прежней карточке.</summary>
    private void FocusSelected()
    {
        if (_vm.Selected is not { } c) return;
        if (CardElement(c) is { } btn) btn.Focus();
        else if (_panel is { } panel && _vm.Cards.IndexOf(c) is var i and >= 0)
        {
            // карточки ещё нет: прокручиваем к ней, сетка её создаст, после раскладки ставим фокус
            panel.BringIndexIntoView(i, 84);
            Dispatcher.BeginInvoke(() =>
            {
                if (ReferenceEquals(_vm.Selected, c) && CardElement(c) is { } b2) b2.Focus();
            }, DispatcherPriority.Loaded);
        }
    }

    private void Toggle_Click(object sender, RoutedEventArgs e) => _vm.ToggleSelected();
    private void Undo_Click(object sender, RoutedEventArgs e) => _vm.Undo();

    /// <summary>
    /// Под мышью тост не гаснет. Фокус у «Отменить» тост больше не держит: кнопка фокус не берёт (иначе пробел, которым
    /// переключают мод, нажимал её, а тост висел, пока фокус внутри), с клавиатуры отмена — Ctrl+Z.
    /// </summary>
    private void UndoByKey()
    {
        if (_vm.Toast is { CanUndo: true }) _vm.Undo();
    }
    private void Delete_Click(object sender, RoutedEventArgs e) => _vm.DeleteSelected();

    private void RestoreVersion_Click(object sender, RoutedEventArgs e)
    {
        if (((FrameworkElement)sender).DataContext is VersionRow row) _vm.RestoreVersion(row);
    }

    // ───────────── тост: под мышью и в фокусе не гаснет ─────────────

    private bool ToastHeld => ToastBox.IsMouseOver;

    /// <summary>Как далеко тост всплыл: 0 — спрятан внизу, 1 — на месте (пружина с лёгким перелётом).</summary>
    private Spring _toastIn = null!;
    /// <summary>Отступ тоста снизу на месте (на узком окне и рядом с панелью меняются только поля слева и справа).</summary>
    private const double ToastBottom = 28, ToastRise = 22;

    /// <summary>
    /// Тост появился, сменился или ушёл (2026-10-18). Появление — всплывает снизу и проявляется пружиной; новый поверх
    /// показанного — тот же тост, меняется только текст (короткий сдвиг и проявление); уход — опускается и гаснет, а
    /// после прячется совсем. Тост — стекло: двигается он полем снизу, а не сдвигом картинки, чтобы копия сцены под ним
    /// шла вместе с ним. Без анимаций — сразу.
    /// </summary>
    /// <summary>Когда сменился текст тоста: частые смены (щёлкают переключатель) идут без подъёма текста, иначе он дрожит.</summary>
    private long _toastChangedAt;

    private void ShowToast(Toast? t)
    {
        if (t is not null)
        {
            var wasShown = ToastBox.Visibility == Visibility.Visible && _toastIn.Target > 0;
            ToastBox.DataContext = t;
            ToastBox.Visibility = Visibility.Visible;
            var now = Environment.TickCount64;
            var calm = now - _toastChangedAt > 500;
            _toastChangedAt = now;
            if (wasShown && calm && !SpringDriver.ReducedMotion)
            {
                ToastBodyMove.BeginAnimation(TranslateTransform.YProperty, Anim.Go(6, 0, 0.32, Anim.Smooth));
                ToastBody.BeginAnimation(OpacityProperty, Anim.Go(0.2, 1, 0.26, Anim.Smooth));
            }
            _springs.To(_toastIn, 1, instant: _host.ShotPath is not null && !_probing);
        }
        else if (ToastBox.Visibility == Visibility.Visible)
        {
            ToastCountdown(false, refill: false);
            _springs.To(_toastIn, 0);
        }
    }

    /// <summary>Полоса «сколько осталось на отмену»: идёт вместе с таймером тоста; под мышью или в фокусе — снова полная.</summary>
    private void ToastCountdown(bool run, bool refill = true)
    {
        if (!run || SpringDriver.ReducedMotion)
        {
            if (refill && !SpringDriver.ReducedMotion)
                ToastLeftScale.BeginAnimation(ScaleTransform.ScaleXProperty, Anim.Go(ToastLeftScale.ScaleX, 1, 0.3, Anim.Smooth));
            else
            {
                // тост уходит: полоса стоит, где кончилась. Просто снять анимацию — значит вернуть исходную 1, и уходящий
                // тост показал бы её полной
                var at = ToastLeftScale.ScaleX;
                ToastLeftScale.BeginAnimation(ScaleTransform.ScaleXProperty, null);
                ToastLeftScale.ScaleX = at;
            }
            if (SpringDriver.ReducedMotion) ToastLeft.Opacity = 0;
            return;
        }
        ToastLeft.Opacity = 0.5;
        var a = new DoubleAnimation(1, 0, _toastTimer.Interval);
        Timeline.SetDesiredFrameRate(a, 30); // полоса тонкая и медленная: 30 кадров хватает
        ToastLeftScale.BeginAnimation(ScaleTransform.ScaleXProperty, a);
    }

    private void Toast_Hold(object sender, MouseEventArgs e) { _toastTimer.Stop(); ToastCountdown(false); }

    private void Toast_Release(object sender, MouseEventArgs e)
    {
        if (_vm.Toast is not null && !ToastHeld) { _toastTimer.Stop(); _toastTimer.Start(); ToastCountdown(true); } // после ухода — полный срок заново
    }


    // ───────────── меню карточки (DESIGN_COMPONENTS 3.9) ─────────────

    private void Card_RightClick(object sender, MouseButtonEventArgs e)
    {
        if (sender is not FrameworkElement el || el.DataContext is not ModCard c) return;
        ShowCardMenu(c, el, atMouse: true);
        e.Handled = true;
    }

    /// <summary>Правая кнопка — меню у курсора; клавиша «Меню» или Shift+F10 — у выбранной карточки.</summary>
    private void ShowCardMenu(ModCard c, FrameworkElement target, bool atMouse)
    {
        var menu = new ContextMenu { Style = (Style)FindResource("CardMenu"), PlacementTarget = target };
        if (atMouse) menu.Placement = System.Windows.Controls.Primitives.PlacementMode.MousePoint;
        else
        {
            menu.Placement = System.Windows.Controls.Primitives.PlacementMode.RelativePoint;
            menu.HorizontalOffset = 24;
            menu.VerticalOffset = 24;
        }
        foreach (var entry in _vm.MenuFor(c))
        {
            if (entry.Danger) menu.Items.Add(new Separator { Style = (Style)FindResource("CardMenuLine") });
            var item = new MenuItem { Header = entry.Label, Style = (Style)FindResource("CardMenuItem") };
            if (entry.Danger) item.SetResourceReference(ForegroundProperty, "Danger");
            var action = entry.Action;
            item.Click += (_, _) => RunMenu(c, action);
            menu.Items.Add(item);
        }
        menu.Closed += (_, _) => { if (target.IsVisible && !atMouse) target.Focus(); };
        menu.IsOpen = true;
    }

    private void RunMenu(ModCard c, MenuAction action)
    {
        switch (action)
        {
            case MenuAction.Open: ChangeSelection(() => _vm.Selected = c); break;
            case MenuAction.Toggle: _vm.Toggle(c); break;
            // загрузки в этой сборке нет: «Обновить» ведёт на страницу с новой версией, как главная кнопка панели
            case MenuAction.OpenPage or MenuAction.Update: OpenUrl(GarageViewModel.PageOf(c)); break;
            case MenuAction.Delete: _vm.Delete(c); break;
            case MenuAction.Restore: _vm.RestoreDeleted(c); break;
        }
    }

    private FrameworkElement? CardElement(ModCard c) =>
        Cards.ItemContainerGenerator.ContainerFromItem(c) is DependencyObject cp && VisualTreeHelper.GetChildrenCount(cp) > 0
            ? VisualTreeHelper.GetChild(cp, 0) as FrameworkElement : null;
    private void DismissBanner_Click(object sender, RoutedEventArgs e) => _vm.DismissBanner();
    private void UpdateAct_Click(object sender, RoutedEventArgs e) => _host.Banner?.Act(_closing.Token);
    private void UpdateLater_Click(object sender, RoutedEventArgs e) => _host.Banner?.Dismiss();
    private void UpdateCancel_Click(object sender, RoutedEventArgs e) => _host.Banner?.Cancel();

    // ───────────── знакомство при первом запуске (2026-10-18) ─────────────

    private TourViewModel? _tour;
    /// <summary>Где сейчас «окно» знакомства и идёт ли перетекание к следующему шагу (тогда за сдвигами не следим).</summary>
    private Rect _tourHole = Rect.Empty;
    private bool _tourMoving;

    private static bool Near(Rect a, Rect b) =>
        !b.IsEmpty && Math.Abs(a.X - b.X) < 1 && Math.Abs(a.Y - b.Y) < 1 && Math.Abs(a.Width - b.Width) < 1 && Math.Abs(a.Height - b.Height) < 1;

    /// <summary>Первый запуск: когда окно вошло и показало моды, — знакомство (если ничего другого поверх нет).</summary>
    private void MaybeStartTour()
    {
        // снимок: после того как островок встал на место (AdaptTopBar идёт с приоритетом Loaded), но до снимка
        if (_host.ShotTour is { } step) { Dispatcher.BeginInvoke(() => StartTour(step - 1), DispatcherPriority.ApplicationIdle); return; }
        if (_host.Settings.Current.FirstRunDone || _host.ShotPath is not null) return;
        _ = Task.Delay(TimeSpan.FromSeconds(1.4)).ContinueWith(_ => Dispatcher.BeginInvoke(() =>
        {
            if (!IsLoaded || _tour is not null || NavGarage.IsChecked != true) return;
            if (IncomingLayer.Visibility == Visibility.Visible || BindLayer.Visibility == Visibility.Visible) return;
            StartTour(0);
        }), TaskScheduler.Default);
    }

    private void StartTour(int step)
    {
        _tour = new TourViewModel(_vm.Cards.Count > 0);
        _tour.GoTo(step);
        _tour.PropertyChanged += (_, e) => { if (e.PropertyName == nameof(TourViewModel.Index)) TourPlace(instant: false); };
        _tour.Done += EndTour;
        TourLayer.DataContext = _tour;
        TourLayer.Visibility = Visibility.Visible;
        TourLayer.UpdateLayout();
        TourPlace(instant: true);
        DimCaption(true);
        if (!SpringDriver.ReducedMotion && _host.ShotPath is null)
            TourLayer.BeginAnimation(OpacityProperty, Anim.Go(0, 1, 0.4, Anim.Smooth));
        Dispatcher.BeginInvoke(() => TourNext.Focus(), DispatcherPriority.Input);
    }

    private void EndTour()
    {
        if (_host.ShotPath is null) _host.Settings.MarkFirstRunDone();
        var quick = SpringDriver.ReducedMotion;
        _tour = null;
        DimCaption(false);
        if (quick) { TourLayer.Visibility = Visibility.Collapsed; TourLayer.DataContext = null; return; }
        var fade = Anim.Go(TourLayer.Opacity, 0, 0.25, Anim.Smooth);
        fade.Completed += (_, _) => { if (_tour is null) { TourLayer.Visibility = Visibility.Collapsed; TourLayer.DataContext = null; TourLayer.BeginAnimation(OpacityProperty, null); TourLayer.Opacity = 1; } };
        TourLayer.BeginAnimation(OpacityProperty, fade);
    }

    private void TourNext_Click(object sender, RoutedEventArgs e) => _tour?.Next();
    private void TourSkip_Click(object sender, RoutedEventArgs e) => _tour?.Skip();

    /// <summary>Рамка элемента шага в координатах окна с полями 8 px и её скругление; null — шаг без элемента (по центру).</summary>
    private (Rect Rect, double Radius)? TourTargetOf(TourTarget t)
    {
        FrameworkElement? el = t switch { TourTarget.Nav => Nav, TourTarget.Filters => TopBar, TourTarget.Card => FindCard(), _ => null };
        if (el is null || !el.IsVisible || el.ActualWidth <= 0) return null;
        Rect r;
        try { r = el.TransformToAncestor(Root).TransformBounds(new Rect(el.RenderSize)); }
        catch (InvalidOperationException) { return null; }
        r.Inflate(8, 8);
        return (r, t switch { TourTarget.Nav => 50, TourTarget.Filters => 32, _ => 30 });
    }

    /// <summary>
    /// Ставит «окно» в затемнении, кольцо и пузырь к элементу шага: у левой панели и карточки пузырь справа (у правого
    /// края — слева), у островка — под ним, у последнего шага — по центру. Между шагами всё перетекает за 0,45 с.
    /// </summary>
    private void TourPlace(bool instant)
    {
        if (_tour is null) return;
        var w = Root.ActualWidth;
        var h = Root.ActualHeight;
        TourAll.Rect = new Rect(0, 0, w, h);
        var target = TourTargetOf(_tour.Current.Target);
        var hole = target?.Rect ?? new Rect(w / 2, h / 2, 0, 0);
        _tourHole = hole;
        TourHole.RadiusX = TourHole.RadiusY = TourRing.RadiusX = TourRing.RadiusY = target?.Radius ?? 0;
        TourRing.Visibility = target is null ? Visibility.Collapsed : Visibility.Visible;

        TourBubble.Measure(new Size(TourBubble.Width, double.PositiveInfinity));
        var bw = TourBubble.Width;
        var bh = TourBubble.DesiredSize.Height;
        double x, y;
        switch (_tour.Current.Target)
        {
            case TourTarget.Filters when target is not null:
                x = hole.Left + hole.Width / 2 - bw / 2; y = hole.Bottom + 14; break;
            case TourTarget.Nav or TourTarget.Card when target is not null:
                x = hole.Right + 14 + bw <= w - 16 ? hole.Right + 14 : hole.Left - 14 - bw;
                y = hole.Top; break;
            default:
                x = (w - bw) / 2; y = (h - bh) / 2; break;
        }
        x = Math.Clamp(x, 16, Math.Max(16, w - bw - 16));
        y = Math.Clamp(y, 16, Math.Max(16, h - bh - 16));

        var quick = instant || SpringDriver.ReducedMotion || _host.ShotPath is not null;
        void Go(DependencyObject o, DependencyProperty p, double to)
        {
            if (quick) { ((IAnimatable)o).BeginAnimation(p, null); o.SetValue(p, to); }
            else ((IAnimatable)o).BeginAnimation(p, new DoubleAnimation(to, TimeSpan.FromSeconds(0.45)) { EasingFunction = Anim.Smooth });
        }
        if (quick) { TourHole.BeginAnimation(RectangleGeometry.RectProperty, null); TourHole.Rect = hole; }
        else
        {
            _tourMoving = true;
            var move = new RectAnimation(hole, TimeSpan.FromSeconds(0.45)) { EasingFunction = Anim.Smooth };
            move.Completed += (_, _) => _tourMoving = false;
            TourHole.BeginAnimation(RectangleGeometry.RectProperty, move);
        }
        Go(TourRing, Canvas.LeftProperty, hole.Left);
        Go(TourRing, Canvas.TopProperty, hole.Top);
        Go(TourRing, WidthProperty, hole.Width);
        Go(TourRing, HeightProperty, hole.Height);
        Go(TourBubble, Canvas.LeftProperty, x);
        Go(TourBubble, Canvas.TopProperty, y);
    }

    // ───────────── «Интерфейс работает медленно» (2026-10-18) ─────────────

    private readonly SlowGlassAdvisor _slow = new();
    private bool _slowOffered;

    private void SlowAccept_Click(object sender, RoutedEventArgs e)
    {
        FadePlate(SlowBox, SlowBoxMove, false);
        _host.Settings.ApplyPreset(AppearancePreset.Fast);
        _vm.Notify("Включён набор «Быстро». Вернуть «Красиво» можно в настройках");
    }

    private void SlowDecline_Click(object sender, RoutedEventArgs e)
    {
        FadePlate(SlowBox, SlowBoxMove, false);
        _host.Settings.DeclineSlowOffer();
    }

    // ───────────── установка из скачанного и перетащенного архива (2026-10-18) ─────────────

    private readonly IncomingViewModel _in;
    private readonly DownloadsWatcher _downloads;
    private IInputElement? _focusBeforeIncoming;

    /// <summary>«Загрузки» и добавленные папки смотрим, только пока включено «Разрешить загрузку модов» (на снимках — никогда).</summary>
    private void UpdateDownloadsWatch(AppSettings s)
    {
        var on = s.AllowDownloads && _host.ShotPath is null && IsLoaded;
        var folders = new List<string>();
        if (on && _host.Settings.DownloadsFolder is { } d) folders.Add(d);
        if (on) folders.AddRange(s.WatchFolders);
        _downloads.SetFolders(folders);
    }

    /// <summary>«Добавить папку» в «Загрузке модов»: обычный выбор папки Windows; уже смотримая или «Загрузки» — объясняем.</summary>
    private void AddWatchFolder_Click(object sender, RoutedEventArgs e)
    {
        var dlg = new Microsoft.Win32.OpenFolderDialog { Title = "Папка, куда вы скачиваете моды" };
        if (dlg.ShowDialog(this) != true) return;
        if (_host.Settings.AddWatchFolder(dlg.FolderName) is { } why) _vm.Notify(why);
    }

    private void RemoveWatchFolder_Click(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { DataContext: string folder }) _host.Settings.RemoveWatchFolder(folder);
    }

    private void Incoming_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(IncomingViewModel.Open): ShowIncoming(_in.Open); break;
            case nameof(IncomingViewModel.Busy): IncomingBusy(_in.Busy); break;
            case nameof(IncomingViewModel.OfferText):
                FadePlate(OfferBox, OfferBoxMove, _in.OfferText is not null);
                // плашка живёт над сеткой «Модов»: в другом разделе её не видно — подскажем, где она
                if (_in.OfferText is { } offer && NavGarage.IsChecked != true) _vm.Notify(offer + " — посмотреть можно в «Модах»");
                break;
        }
    }

    /// <summary>Окно проверки проявляется, содержимое чуть поднимается (как «Указать страницу»); закрылось — фокус назад.</summary>
    private void ShowIncoming(bool open)
    {
        if (open == (IncomingLayer.Visibility == Visibility.Visible)) return;
        if (!open)
        {
            IncomingLayer.Visibility = Visibility.Collapsed;
            IncomingBusy(false);
            if (_focusBeforeIncoming is UIElement { IsVisible: true } f) f.Focus();
            return;
        }
        _focusBeforeIncoming = Keyboard.FocusedElement;
        IncomingLayer.Visibility = Visibility.Visible;
        if (!SpringDriver.ReducedMotion && _host.ShotPath is null)
        {
            IncomingLayer.BeginAnimation(OpacityProperty, Anim.Go(0, 1, 0.35, Anim.Smooth));
            Anim.Mover(IncomingBody).BeginAnimation(TranslateTransform.YProperty, Anim.Go(12, 0, 0.4, Anim.Smooth));
        }
        Dispatcher.BeginInvoke(() => { if (IncomingInstall.IsVisible && IncomingInstall.IsEnabled) IncomingInstall.Focus(); }, DispatcherPriority.Input);
    }

    /// <summary>Пока архив разбирается, по полоске раз в 1,3 с бежит блик (без анимаций — просто полоска).</summary>
    private void IncomingBusy(bool busy)
    {
        if (!busy || SpringDriver.ReducedMotion) { IncomingBusyMove.BeginAnimation(TranslateTransform.XProperty, null); IncomingBusySheen.Opacity = busy ? 0.5 : 0; return; }
        IncomingBusySheen.Opacity = 1;
        var a = new DoubleAnimation(-IncomingBusySheen.Width, IncomingPanel.Width, TimeSpan.FromSeconds(1.3)) { RepeatBehavior = RepeatBehavior.Forever, EasingFunction = Anim.Smooth };
        Timeline.SetDesiredFrameRate(a, 30);
        IncomingBusyMove.BeginAnimation(TranslateTransform.XProperty, a);
    }

    private void IncomingInstall_Click(object sender, RoutedEventArgs e)
    {
        _in.Install();
        LoadPreviews(); // у нового мода сразу картинка и название, а не после возвращения в окно
    }
    private void IncomingCancel_Click(object sender, RoutedEventArgs e) => _in.Close();
    private void IncomingDim_MouseDown(object sender, MouseButtonEventArgs e) => _in.Close();
    private void OfferReview_Click(object sender, RoutedEventArgs e) => _in.Review();
    private void OfferDismiss_Click(object sender, RoutedEventArgs e) => _in.DismissOffer();

    /// <summary>Над окном несут файлы: принимаем, только если среди них есть zip, rar или 7z (остальное — знак «нельзя»).</summary>
    /// <summary>Сколько архивов несут над окном: считается раз при входе (на каждом движении мыши диск не спрашиваем).</summary>
    private int _dragArchives;

    private void Window_DragOver(object sender, DragEventArgs e)
    {
        if (!e.Data.GetDataPresent(DataFormats.FileDrop)) return; // текст в поле поиска — как обычно
        // поверх окна проверки и знакомства не принимаем: сначала закончить то, что открыто
        var ok = _dragArchives > 0 && IncomingLayer.Visibility != Visibility.Visible && TourLayer.Visibility != Visibility.Visible;
        e.Effects = ok ? DragDropEffects.Copy : DragDropEffects.None;
        e.Handled = true;
        ShowDropHint(ok);
    }

    /// <summary>«Ушли» приходит и при переходе между элементами окна: прячем, только когда курсор правда вне окна.</summary>
    private void Window_DragLeave(object sender, DragEventArgs e)
    {
        var p = e.GetPosition(this);
        if (p.X <= 0 || p.Y <= 0 || p.X >= ActualWidth || p.Y >= ActualHeight) ShowDropHint(false);
    }

    private void Window_Drop(object sender, DragEventArgs e)
    {
        if (!e.Data.GetDataPresent(DataFormats.FileDrop)) return;
        ShowDropHint(false);
        var files = DroppedArchives(e.Data);
        e.Handled = true;
        if (files.Count == 0 || IncomingLayer.Visibility == Visibility.Visible || TourLayer.Visibility == Visibility.Visible) return;
        Activate();
        foreach (var f in files) _ = _in.OfferAsync(f, IncomingOrigin.Dropped, _closing.Token); // первый — окном, остальные следом
    }

    private static List<string> DroppedArchives(IDataObject data) =>
        data.GetDataPresent(DataFormats.FileDrop) && data.GetData(DataFormats.FileDrop) is string[] paths
            ? paths.Where(p => File.Exists(p) && BeamModManager.Core.Install.IncomingAnalyzer.IsCandidate(p)).Take(8).ToList()
            : new List<string>();

    private bool _dropShown;

    /// <summary>Подсказка «Отпустите…» проявляется и чуть увеличивается навстречу; ушли с окна — гаснет.</summary>
    private void ShowDropHint(bool show)
    {
        if (show == _dropShown) return;
        _dropShown = show;
        var quick = SpringDriver.ReducedMotion || _host.ShotPath is not null;
        if (show) DropLayer.Visibility = Visibility.Visible;
        if (quick)
        {
            DropLayer.BeginAnimation(OpacityProperty, null);
            DropLayer.Opacity = show ? 1 : 0;
            if (!show) DropLayer.Visibility = Visibility.Collapsed;
            return;
        }
        var fade = Anim.Go(DropLayer.Opacity, show ? 1 : 0, show ? 0.25 : 0.18, Anim.Smooth);
        if (!show) fade.Completed += (_, _) => { if (!_dropShown) DropLayer.Visibility = Visibility.Collapsed; };
        DropLayer.BeginAnimation(OpacityProperty, fade);
        if (show)
        {
            DropHintScale.BeginAnimation(ScaleTransform.ScaleXProperty, Anim.Go(0.94, 1, 0.4, Anim.Lift));
            DropHintScale.BeginAnimation(ScaleTransform.ScaleYProperty, Anim.Go(0.94, 1, 0.4, Anim.Lift));
        }
    }

    private readonly Dictionary<FrameworkElement, bool> _plates = new();

    /// <summary>Плашка над сеткой проявляется и опускается на место; уходит — гаснет (как плашка обновления).</summary>
    private void FadePlate(FrameworkElement box, TranslateTransform move, bool show)
    {
        if (_plates.TryGetValue(box, out var was) && was == show) return;
        _plates[box] = show;
        var quick = SpringDriver.ReducedMotion || (_host.ShotPath is not null && !_probing);
        if (show) box.Visibility = Visibility.Visible;
        if (quick)
        {
            box.BeginAnimation(OpacityProperty, null);
            move.BeginAnimation(TranslateTransform.YProperty, null);
            box.Opacity = show ? 1 : 0;
            move.Y = 0;
            if (!show) box.Visibility = Visibility.Collapsed;
            return;
        }
        if (show)
        {
            box.BeginAnimation(OpacityProperty, Anim.Go(box.Opacity, 1, 0.35, Anim.Smooth));
            move.BeginAnimation(TranslateTransform.YProperty, Anim.Go(-8, 0, 0.5, Anim.Smooth));
            return;
        }
        var fade = Anim.Go(box.Opacity, 0, 0.22, Anim.Smooth);
        fade.Completed += (_, _) => { if (_plates.TryGetValue(box, out var s) && !s) box.Visibility = Visibility.Collapsed; };
        box.BeginAnimation(OpacityProperty, fade);
        move.BeginAnimation(TranslateTransform.YProperty, Anim.Go(0, -6, 0.22, Anim.Smooth));
    }

    // ───────────── плашка обновления: появление, полоса, галочка (2026-10-18) ─────────────

    private bool _updateShown;

    private void Banner_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (sender is not BeamModManager.Ui.AppUpdate.UpdateBanner b) return;
        if (e.PropertyName == nameof(b.Visible)) ShowUpdateBox(b.Visible);
        else if (e.PropertyName is nameof(b.Progress) or nameof(b.Phase)) UpdateBarView(b);
    }

    /// <summary>Плашка проявляется и чуть опускается на место; уходит — гаснет и прячется.</summary>
    private void ShowUpdateBox(bool show, bool instant = false)
    {
        if (show == _updateShown && !instant) return;
        _updateShown = show;
        var quick = instant || SpringDriver.ReducedMotion || (_host.ShotPath is not null && !_probing);
        if (show) UpdateBox.Visibility = Visibility.Visible;
        if (quick)
        {
            UpdateBox.BeginAnimation(OpacityProperty, null);
            UpdateBoxMove.BeginAnimation(TranslateTransform.YProperty, null);
            UpdateBox.Opacity = show ? 1 : 0;
            UpdateBoxMove.Y = 0;
            if (!show) UpdateBox.Visibility = Visibility.Collapsed;
            return;
        }
        if (show)
        {
            UpdateBox.BeginAnimation(OpacityProperty, Anim.Go(UpdateBox.Opacity, 1, 0.35, Anim.Smooth));
            UpdateBoxMove.BeginAnimation(TranslateTransform.YProperty, Anim.Go(-8, 0, 0.5, Anim.Smooth));
            return;
        }
        var fade = Anim.Go(UpdateBox.Opacity, 0, 0.22, Anim.Smooth);
        fade.Completed += (_, _) => { if (!_updateShown) UpdateBox.Visibility = Visibility.Collapsed; };
        UpdateBox.BeginAnimation(OpacityProperty, fade);
        UpdateBoxMove.BeginAnimation(TranslateTransform.YProperty, Anim.Go(0, -6, 0.22, Anim.Smooth));
    }

    /// <summary>
    /// Полоса едет к новому значению за 0,4 с (а не прыгает по процентам); при сбое — янтарная и стоит; при проверке
    /// и распаковке полная, по ней раз в 1,3 с бежит блик; готово — вместо полосы галочка.
    /// </summary>
    private void UpdateBarView(BeamModManager.Ui.AppUpdate.UpdateBanner b, bool instant = false)
    {
        var quick = instant || SpringDriver.ReducedMotion || (_host.ShotPath is not null && !_probing);
        var wasHidden = UpdateBar.Visibility != Visibility.Visible;
        UpdateBar.Visibility = b.Progress is null ? Visibility.Collapsed : Visibility.Visible;
        UpdateBarFill.SetResourceReference(Border.BackgroundProperty, b.Phase == BeamModManager.Ui.AppUpdate.BannerPhase.Failed ? "Warn" : "Ink");
        var to = b.Progress ?? 0;
        if (quick || wasHidden) { UpdateBarScale.BeginAnimation(ScaleTransform.ScaleXProperty, null); UpdateBarScale.ScaleX = to; }
        else if (b.Phase != BeamModManager.Ui.AppUpdate.BannerPhase.Failed)
            UpdateBarScale.BeginAnimation(ScaleTransform.ScaleXProperty, new DoubleAnimation(to, TimeSpan.FromSeconds(0.4)) { EasingFunction = Anim.Smooth });
        else
        {
            // сбой: полоса замирает там, где была на экране
            var at = UpdateBarScale.ScaleX;
            UpdateBarScale.BeginAnimation(ScaleTransform.ScaleXProperty, null);
            UpdateBarScale.ScaleX = at;
        }

        var verifying = b.Phase == BeamModManager.Ui.AppUpdate.BannerPhase.Verifying && !SpringDriver.ReducedMotion;
        if (verifying && UpdateBarSheen.Opacity == 0)
        {
            UpdateBarSheen.Opacity = 1;
            var span = Math.Max(300, UpdateBox.ActualWidth) + UpdateBarSheen.Width;
            var a = new DoubleAnimation(-UpdateBarSheen.Width, span, TimeSpan.FromSeconds(1.3)) { RepeatBehavior = RepeatBehavior.Forever, EasingFunction = Anim.Smooth };
            Timeline.SetDesiredFrameRate(a, 30);
            UpdateBarSheenMove.BeginAnimation(TranslateTransform.XProperty, a);
        }
        else if (!verifying && UpdateBarSheen.Opacity != 0)
        {
            UpdateBarSheen.Opacity = 0;
            UpdateBarSheenMove.BeginAnimation(TranslateTransform.XProperty, null);
        }

        var done = b.Phase == BeamModManager.Ui.AppUpdate.BannerPhase.Ready;
        var doneWas = UpdateDone.Visibility == Visibility.Visible;
        UpdateDone.Visibility = done ? Visibility.Visible : Visibility.Collapsed;
        if (done && !doneWas && !quick)
        {
            UpdateDoneScale.BeginAnimation(ScaleTransform.ScaleXProperty, Anim.Go(0.5, 1, 0.45, Anim.Lift));
            UpdateDoneScale.BeginAnimation(ScaleTransform.ScaleYProperty, Anim.Go(0.5, 1, 0.45, Anim.Lift));
        }
    }

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
        OpenUrl(_vm.Details?.PageUrl);
        // страница обновления, «Загрузки» смотрим: подскажем, что дальше всё само
        if (_vm.Selected is { Data.NeedsUpdate: true } && _host.Settings.Current.AllowDownloads && _vm.Details?.PageUrl is not null)
            _vm.Notify("Скачайте архив на странице — программа заметит его в «Загрузках» и предложит установить");
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
            case MainAction.Restore:
                if (_vm.Selected is { } gone) _vm.RestoreDeleted(gone);
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
            case EmptyAction.OpenSearch: NavSearch.IsChecked = true; break;
            case EmptyAction.SearchSites:
                // «Нет такого мода» среди своих → тот же запрос на сайтах
                var q = _vm.SearchText.Trim();
                NavSearch.IsChecked = true;
                if (q.Length > 0) { QueryText.Text = q; _ = _find.RunAsync(); }
                break;
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
        if (TourLayer.Visibility == Visibility.Visible && _tour is { } tour)
        {
            if (e.Key == Key.Escape) tour.Skip();
            else if (e.Key is Key.Enter or Key.Right) tour.Next();
            else if (e.Key == Key.Left) tour.Back();
            else return;
            e.Handled = true;
            return;
        }
        if (IncomingLayer.Visibility == Visibility.Visible)
        {
            if (e.Key == Key.Escape) { _in.Close(); e.Handled = true; }
            else if (e.Key == Key.Enter && _in.CanInstall && Keyboard.FocusedElement is not Button) { _in.Install(); e.Handled = true; }
            return; // окно поверх: остальные клавиши — его кнопкам
        }
        if (BindLayer.Visibility == Visibility.Visible)
        {
            if (e.Key == Key.Escape) { CloseBind(); e.Handled = true; }
            else if (e.Key == Key.Enter && _bind is { CanApply: true }) { BindApply_Click(this, new RoutedEventArgs()); e.Handled = true; }
            return; // окно поверх: остальные клавиши (и переходы по разделам) — его полям и кнопкам
        }
        if (ctrl && e.Key == Key.F)
        {
            // Ctrl+Shift+F и Ctrl+F в «Поиске» — поле запроса по сайтам; в «Обновлениях» ищем среди найденного, из других
            // разделов — среди модов
            if ((Keyboard.Modifiers & ModifierKeys.Shift) != 0 || SearchPage)
            {
                NavSearch.IsChecked = true;
                Dispatcher.BeginInvoke(() => { QueryText.Focus(); QueryText.SelectAll(); }, DispatcherPriority.Input);
            }
            else
            {
                if (!UpdatesPage) NavGarage.IsChecked = true;
                OpenSearch(focus: true);
            }
            e.Handled = true;
            return;
        }
        // Ctrl+Z — «Отменить» у тоста (кнопка фокус не берёт, DESIGN_FLOWS 7); в поле ввода Ctrl+Z отменяет набор
        if (ctrl && e.Key == Key.Z && !typing && _vm.Toast is { CanUndo: true }) { UndoByKey(); e.Handled = true; return; }
        if (ctrl && e.Key == Key.D1) { NavGarage.IsChecked = true; e.Handled = true; return; }
        if (ctrl && e.Key == Key.D2) { NavUpdates.IsChecked = true; e.Handled = true; return; }
        if (ctrl && e.Key == Key.D3) { NavSearch.IsChecked = true; e.Handled = true; return; }
        if (ctrl && e.Key == Key.OemComma) { NavSettings.IsChecked = true; e.Handled = true; return; }
        // проверить обновления (DESIGN_FLOWS 7): F5 или Ctrl+R, из любого раздела
        if (e.Key == Key.F5 || (ctrl && e.Key == Key.R))
        {
            NavUpdates.IsChecked = true;
            if (!_upd.Checking) _ = _upd.CheckNowAsync();
            e.Handled = true;
            return;
        }
        if (e.Key == Key.Escape && QueryText.IsKeyboardFocused)
        {
            // поле «Поиска»: первый Esc стирает запрос, второй уводит из поля (найденное остаётся)
            if (QueryText.Text.Length > 0) QueryText.Clear(); else Keyboard.ClearFocus();
            e.Handled = true;
            return;
        }
        if (e.Key == Key.Escape)
        {
            // в поиске: первый Esc стирает запрос, второй сворачивает островок обратно в фильтры
            if (SearchBox.IsKeyboardFocused && SearchBox.Text.Length > 0) SearchBox.Clear();
            else if (SearchBox.IsKeyboardFocused) CloseSearch();
            else if (typing) Keyboard.ClearFocus();
            else _vm.Selected = null;
            e.Handled = true;
            return;
        }
        if (typing || NavGarage.IsChecked != true) return;
        // Delete — удалить выбранный (тост «Отменить»); клавиша «Меню» и Shift+F10 — меню выбранной карточки
        if (e.Key == Key.Delete && _vm.Selected is not null)
        {
            _vm.DeleteSelected(); // карточки с фокусом больше нет: стрелки дальше выберут первую
            e.Handled = true;
            return;
        }
        var shiftF10 = e.Key == Key.System && e.SystemKey == Key.F10 && (Keyboard.Modifiers & ModifierKeys.Shift) != 0;
        if ((e.Key == Key.Apps || shiftF10) && _vm.Selected is { } menuCard && CardElement(menuCard) is { } menuEl)
        {
            ShowCardMenu(menuCard, menuEl, atMouse: false);
            e.Handled = true;
            return;
        }
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
            Anim.Mover(BindBody).BeginAnimation(TranslateTransform.YProperty, Anim.Go(12, 0, 0.4, Anim.Smooth));
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

    private async void BindApply_Click(object sender, RoutedEventArgs e)
    {
        if (_bind is not { CanApply: true } vm || _bindCts is not { } cts) return;
        // выбранную подсказку сначала открываем: версия и размер страницы сразу известны, обновление видно без проверки
        var toast = await vm.ApplyAsync(cts.Token);
        if (!ReferenceEquals(_bind, vm)) return; // окно закрыли, пока открывали страницу
        CloseBind();
        if (toast is null) { _vm.Notify("Не получилось: этого мода уже нет в списке"); return; }
        Refresh();
        _vm.Notify(toast);
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
        var startup = DateTime.Now - System.Diagnostics.Process.GetCurrentProcess().StartTime;
        await Task.Delay(1500); // картинки и пружины успевают встать на место
        var path = _host.ShotPath!;
        if (_host.ShotPerf)
        {
            WritePerf(path + ".perf.txt", startup);
            await Task.Delay(800); // после прокрутки туда-обратно картинки видимых карточек успевают встать
        }
        if (_host.ShotFrames) await ProbeFrames(path + ".frames.txt");
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
            Root.UpdateLayout();
            var k = _host.ShotScale;
            var rt = new RenderTargetBitmap((int)Math.Ceiling(Root.ActualWidth * k), (int)Math.Ceiling(Root.ActualHeight * k), 96 * k, 96 * k, PixelFormats.Pbgra32);
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

    /// <summary>Замер большой библиотеки (сборочная машина, --perf): сколько стоят запуск и обычные действия вместе с раскладкой окна.</summary>
    private void WritePerf(string file, TimeSpan startup)
    {
        var sw = System.Diagnostics.Stopwatch.StartNew();
        var lines = new List<string> { $"карточек: {_vm.AllCards.Count}", $"запуск до готовности: {startup.TotalMilliseconds:0} мс" };
        void Lap(string what)
        {
            Root.UpdateLayout();
            Dispatcher.Invoke(() => { }, DispatcherPriority.Render); // кадр нарисован
            lines.Add($"{what}: {sw.ElapsedMilliseconds} мс");
            sw.Restart();
        }
        Lap("раскладка");
        var all = _vm.Filter;
        _vm.Filter = _vm.Filters[1].Filter; Lap("фильтр «Включены»");
        _vm.Filter = all; Lap("фильтр обратно");
        _vm.SearchText = "номер 12"; Lap("поиск");
        _vm.SearchText = ""; Lap("поиск сброшен");
        _vm.Refresh(); Lap("перечитать");
        if (_vm.Cards.Count > 0) { _vm.Toggle(_vm.Cards[0]); Lap("переключить мод"); }
        Scroller.ScrollToEnd(); Lap("прокрутка в конец");
        Scroller.ScrollToHome(); Lap("прокрутка в начало");
        NavUpdates.IsChecked = true; Lap($"раздел «Обновления» ({_upd.Items.Count} карточек)");
        NavGarage.IsChecked = true; Lap("обратно в гараж");
        static long Heap() { GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect(); return GC.GetTotalMemory(true) / 1024 / 1024; }
        var proc = System.Diagnostics.Process.GetCurrentProcess();
        lines.Add($"память: {proc.WorkingSet64 / 1024 / 1024} МБ (пик {proc.PeakWorkingSet64 / 1024 / 1024} МБ), куча {Heap()} МБ");
        if (Array.IndexOf(Environment.GetCommandLineArgs(), "--soak") >= 0)
        {
            // долгая работа: одни и те же действия много раз; память после них не должна расти
            var before = Heap();
            var handles = proc.HandleCount;
            var watch = System.Diagnostics.Stopwatch.StartNew();
            for (var i = 0; i < 300; i++)
            {
                _vm.Filter = _vm.Filters[i % 4].Filter;
                _vm.SearchText = i % 3 == 0 ? "номер " + i % 50 : "";
                if (i % 5 == 0) _vm.Refresh();
                if (i % 7 == 0 && _vm.Cards.Count > 0) _vm.Toggle(_vm.Cards[i % _vm.Cards.Count]);
                if (i % 9 == 0 && _vm.Cards.Count > 0) _vm.Selected = _vm.Cards[i % _vm.Cards.Count];
                if (i % 11 == 0) { NavUpdates.IsChecked = true; Root.UpdateLayout(); NavGarage.IsChecked = true; }
                if (i % 13 == 0) { NavSettings.IsChecked = true; Root.UpdateLayout(); NavGarage.IsChecked = true; }
                Root.UpdateLayout();
                Dispatcher.Invoke(() => { }, DispatcherPriority.Background);
            }
            _vm.Filter = all;
            _vm.SearchText = "";
            Root.UpdateLayout();
            var after = Heap();
            proc.Refresh();
            lines.Add($"300 кругов действий: {watch.ElapsedMilliseconds} мс; куча {before} → {after} МБ; дескрипторы {handles} → {proc.HandleCount}");
        }
        try { File.WriteAllLines(file, lines); } catch (IOException) { }
    }

    // ───────────── кадры в секунду (сборочная машина, --frames) ─────────────

    private bool _probing;

    /// <summary>
    /// Прогоняет анимации, на которые жаловались («будто 15 кадров»): выезд и уезд панели мода, каплю кнопок окна,
    /// наведение на карточку, переход в «Настройки», живой фон. Считает кадры (CompositionTarget.Rendering) и худшую
    /// паузу между ними. На сборочной машине нет видеокарты: числа ниже, чем у человека, важно их сравнение между сборками.
    /// </summary>
    private async Task ProbeFrames(string file)
    {
        var lines = new List<string>
        {
            $"видеокарта WPF: уровень {RenderCapability.Tier >> 16}; эффекты анимации Windows: {(SpringDriver.SystemReduced ? "выключены" : "включены")}",
            $"экран: {Native.RefreshRate()} Гц; фон: {_motion.Fps} кадр/с",
        };
        _probing = true;
        var reducedBefore = SpringDriver.AnimationsOff;
        SpringDriver.AnimationsOff = false;
        SpringDriver.ForceMotion = true;
        try
        {
            // первое появление: сколько окно стоит, пока впервые строит экран настроек и каплю кнопок окна
            long Cold(Action act)
            {
                var sw = System.Diagnostics.Stopwatch.StartNew();
                act();
                Root.UpdateLayout();
                Dispatcher.Invoke(() => { }, DispatcherPriority.Render);
                return sw.ElapsedMilliseconds;
            }
            lines.Add($"первое открытие «Настроек»: {Cold(() => NavSettings.IsChecked = true)} мс");
            NavGarage.IsChecked = true;
            lines.Add($"первая капля кнопок окна: {Cold(() => CapButton_MouseEnter(MinButton, null))} мс");
            HideCapDrop(instant: true);

            async Task Run(string what, Action start, double seconds = 0.9)
            {
                var times = new List<TimeSpan>();
                void OnFrame(object? sender, EventArgs e)
                {
                    if (e is RenderingEventArgs r && (times.Count == 0 || r.RenderingTime != times[^1])) times.Add(r.RenderingTime);
                }
                await Task.Delay(300); // предыдущее успокоилось
                CompositionTarget.Rendering += OnFrame;
                start();
                await Task.Delay(TimeSpan.FromSeconds(seconds));
                CompositionTarget.Rendering -= OnFrame;
                if (times.Count < 2) { lines.Add($"{what}: кадров нет"); return; }
                var span = (times[^1] - times[0]).TotalSeconds;
                var worst = 0.0;
                for (var i = 1; i < times.Count; i++) worst = Math.Max(worst, (times[i] - times[i - 1]).TotalMilliseconds);
                lines.Add($"{what}: {(times.Count - 1) / Math.Max(span, 0.001):0} кадр/с, худшая пауза {worst:0} мс");
            }

            var card = _vm.Cards.FirstOrDefault();
            await Run("тишина", () => { });
            if (card is not null)
            {
                await Run("панель мода выезжает", () => _vm.Selected = card);
                await Run("панель мода уезжает", () => _vm.Selected = null);
            }
            void Later(double seconds, Action a) => _ = Task.Delay(TimeSpan.FromSeconds(seconds)).ContinueWith(_ => Dispatcher.BeginInvoke(a));
            await Run("капля кнопок окна", () =>
            {
                CapButton_MouseEnter(MinButton, null);
                Later(0.3, () => CapButton_MouseEnter(MaxButton, null));
                Later(0.6, () => CapButton_MouseEnter(CloseButton, null));
            });
            HideCapDrop(instant: true);
            if (FindCard() is { } btn && btn.Template.FindName("GlowHover", btn) is UIElement glow)
            {
                // то же, что делает наведение в шаблоне карточки: подъём, свечение и кромка
                var rim = btn.Template.FindName("RimHover", btn) as UIElement;
                var hover = btn.Template.FindName("Hover", btn) as TranslateTransform;
                await Run("наведение на карточку", () =>
                {
                    glow.BeginAnimation(OpacityProperty, Anim.Go(0, 1, 0.6, Anim.Smooth));
                    rim?.BeginAnimation(OpacityProperty, Anim.Go(0, 1, 0.45, Anim.Smooth));
                    hover?.BeginAnimation(TranslateTransform.YProperty, Anim.Go(0, -5, 0.8, Anim.Lift));
                });
                glow.BeginAnimation(OpacityProperty, null);
                rim?.BeginAnimation(OpacityProperty, null);
                hover?.BeginAnimation(TranslateTransform.YProperty, null);
            }
            await Run("тост всплывает", () => _vm.Notify("Проверка кадров"));
            await Run("тост уходит", () => { if (_vm.Toast is { } t) _vm.DismissToast(t.Serial); });
            await Run("поиск раскрывается", () => OpenSearch(focus: true));
            await Run("поиск сворачивается", () => CloseSearch());
            await Run("заголовок сжимается", () => Scroller.ScrollToVerticalOffset(300));
            await Run("заголовок раскрывается", () => Scroller.ScrollToVerticalOffset(0));
            await Run("переход в «Настройки»", () => NavSettings.IsChecked = true);
            await Run("обратно в моды", () => NavGarage.IsChecked = true);
            await Run("живой фон и огоньки", () => _motion.SetRunning(true, true), 1.5);
            _motion.SetRunning(false, false);
        }
        finally
        {
            _probing = false;
            SpringDriver.ForceMotion = false;
            SpringDriver.AnimationsOff = reducedBefore;
        }
        try { File.WriteAllLines(file, lines); } catch (IOException) { }
    }

    /// <summary>Первая карточка гаража на экране (кнопка с шаблоном «Card»).</summary>
    private Button? FindCard()
    {
        Button? found = null;
        void Walk(DependencyObject d)
        {
            if (found is not null) return;
            if (d is Button b && b.Template?.FindName("GlowHover", b) is not null) { found = b; return; }
            for (var i = 0; i < VisualTreeHelper.GetChildrenCount(d); i++) Walk(VisualTreeHelper.GetChild(d, i));
        }
        Walk(Scroller);
        return found;
    }
}
