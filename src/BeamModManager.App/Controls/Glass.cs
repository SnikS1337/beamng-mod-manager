using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Media.Effects;
using System.Windows.Media.Imaging;
using System.Windows.Shapes;
using System.Windows.Threading;
using BeamModManager.Ui.Glass;

namespace BeamModManager.App.Controls;

/// <summary>Как выглядит стекло (DESIGN.md 3.2, «Три режима внешнего вида»).</summary>
public enum GlassMode
{
    /// <summary>Жидкое: живая копия сцены почти без размытия, преломление у краёв с радужной кромкой, блик по кромке.</summary>
    Liquid,
    /// <summary>Матовое: сильное размытие без преломления, плотная подложка, ровная кромка.</summary>
    Matte,
    /// <summary>Плоское: сплошная заливка, копии сцены нет (в Windows выключены эффекты прозрачности).</summary>
    Flat,
}

/// <summary>
/// Стеклянная панель как на доске «Гараж» (DESIGN.md 3.2): снизу живая копия сцены (<see cref="Source"/>) с лёгким
/// размытием и преломлением у краёв (шейдер <see cref="RefractionEffect"/>), над ней подложка, блик, яркая кромка
/// (свет сверху слева), изморозь по краям и искры льда. Кромку и изморозь рисует <see cref="GlassEdge"/> векторами под
/// любой размер (после смены размера ничего не пересчитывается и не проявляется заново).
/// Сцена не должна содержать само стекло, иначе копия будет копировать саму себя.
/// </summary>
public class Glass : ContentControl
{
    public static readonly DependencyProperty SourceProperty = DependencyProperty.Register(
        nameof(Source), typeof(Visual), typeof(Glass), new PropertyMetadata(null, (d, _) => ((Glass)d).Sync()));

    public static readonly DependencyProperty CornerProperty = DependencyProperty.Register(
        nameof(Corner), typeof(double), typeof(Glass), new FrameworkPropertyMetadata(26.0, FrameworkPropertyMetadataOptions.AffectsRender, (d, _) => ((Glass)d).ShapeChanged()));

    public static readonly DependencyProperty BlurProperty = DependencyProperty.Register(
        nameof(Blur), typeof(double), typeof(Glass), new PropertyMetadata(3.0, (d, _) => ((Glass)d).Sync()));

    public static readonly DependencyProperty TintProperty = DependencyProperty.Register(
        nameof(Tint), typeof(Brush), typeof(Glass), new PropertyMetadata(new SolidColorBrush(Color.FromArgb(0x70, 14, 24, 44)),
            (d, _) => ((Glass)d).LookChanged?.Invoke()));

    public static readonly DependencyProperty ShadowProperty = DependencyProperty.Register(
        nameof(Shadow), typeof(bool), typeof(Glass), new PropertyMetadata(true));

    public static readonly DependencyProperty LensProperty = DependencyProperty.Register(
        nameof(Lens), typeof(double), typeof(Glass), new PropertyMetadata(30.0, (d, _) => { ((Glass)d).ShapeChanged(); ((Glass)d).ApplyMode(); }));

    public static readonly DependencyProperty RefractionProperty = DependencyProperty.Register(
        nameof(Refraction), typeof(double), typeof(Glass), new PropertyMetadata(0.0, (d, _) => ((Glass)d).ApplyMode()));

    public static readonly DependencyProperty BrightnessProperty = DependencyProperty.Register(
        nameof(Brightness), typeof(double), typeof(Glass), new PropertyMetadata(0.78, (d, _) => ((Glass)d).ApplyMode()));

    public static readonly DependencyProperty IceProperty = DependencyProperty.Register(
        nameof(Ice), typeof(double), typeof(Glass), new PropertyMetadata(0.55, (d, _) => ((Glass)d).ApplyIce()));

    public static readonly DependencyProperty IceReachProperty = DependencyProperty.Register(
        nameof(IceReach), typeof(double), typeof(Glass), new PropertyMetadata(34.0, (d, _) => ((Glass)d).ShapeChanged()));

    public static readonly DependencyProperty SparksProperty = DependencyProperty.Register(
        nameof(Sparks), typeof(int), typeof(Glass), new PropertyMetadata(0, (d, _) => ((Glass)d).ShapeChanged()));

    public static readonly DependencyProperty SeedProperty = DependencyProperty.Register(
        nameof(Seed), typeof(int), typeof(Glass), new PropertyMetadata(11, (d, _) => ((Glass)d).ShapeChanged()));

    /// <summary>Что видно сквозь стекло.</summary>
    public Visual? Source { get => (Visual?)GetValue(SourceProperty); set => SetValue(SourceProperty, value); }
    public double Corner { get => (double)GetValue(CornerProperty); set => SetValue(CornerProperty, value); }
    /// <summary>Размытие копии сцены в жидком режиме (на доске 2–5 px: «видно картинку, а не пятно»).</summary>
    public double Blur { get => (double)GetValue(BlurProperty); set => SetValue(BlurProperty, value); }
    public Brush Tint { get => (Brush)GetValue(TintProperty); set => SetValue(TintProperty, value); }
    public bool Shadow { get => (bool)GetValue(ShadowProperty); set => SetValue(ShadowProperty, value); }
    /// <summary>Ширина линзы у края (и мягкого свечения кромки), px.</summary>
    public double Lens { get => (double)GetValue(LensProperty); set => SetValue(LensProperty, value); }
    /// <summary>Сила преломления (scale макета × 1,5); 0 — без преломления.</summary>
    public double Refraction { get => (double)GetValue(RefractionProperty); set => SetValue(RefractionProperty, value); }
    /// <summary>Яркость копии сцены под стеклом (brightness() макета): текст на светлых местах остаётся читаемым.</summary>
    public double Brightness { get => (double)GetValue(BrightnessProperty); set => SetValue(BrightnessProperty, value); }
    /// <summary>Сила инея на этой панели при средней силе в настройках (на доске 0,55; у текстовых панелей слабее).</summary>
    public double Ice { get => (double)GetValue(IceProperty); set => SetValue(IceProperty, value); }
    /// <summary>Как далеко иней заходит от края, px.</summary>
    public double IceReach { get => (double)GetValue(IceReachProperty); set => SetValue(IceReachProperty, value); }
    /// <summary>Сколько искр льда на кромке (доска: 6 слева, 8 сверху, 12 справа).</summary>
    public int Sparks { get => (int)GetValue(SparksProperty); set => SetValue(SparksProperty, value); }
    /// <summary>Зерно узора инея и места искр: у каждой панели своё, узор не меняется от размера к размеру.</summary>
    public int Seed { get => (int)GetValue(SeedProperty); set => SetValue(SeedProperty, value); }

    public CornerRadius CornerRadius => new(Corner);

    // ───────────── общий вид, задаётся окном из настроек для всех панелей сразу ─────────────

    private GlassMode _mode = GlassMode.Liquid;
    private double _iceFactor = 1;
    private bool _sparksOn = true;
    private bool _live;
    private bool _light;

    /// <summary>Текущий режим стекла (капли на панели повторяют его).</summary>
    public GlassMode Mode => _mode;
    /// <summary>Светлое оформление.</summary>
    public bool Light => _light;
    /// <summary>Яркость копии сцены с учётом оформления (её же берёт капля, добавляя свои 12 %).</summary>
    public double EffectiveBrightness => _light ? Math.Min(1.06, Brightness + 0.42) : Brightness;
    /// <summary>Вид стекла изменился (режим, оформление, подложка): капли на панели перерисовываются.</summary>
    public event Action? LookChanged;

    /// <summary>Режим стекла, сила инея (0 — без инея; 0,5 / 1 / 1,8 — слабо, средне, сильно), искры и оформление.</summary>
    public void SetLook(GlassMode mode, double iceFactor, bool sparks, bool light = false)
    {
        var modeChanged = mode != _mode;
        _mode = mode;
        _light = light;
        _iceFactor = iceFactor;
        _sparksOn = sparks;
        if (modeChanged) { _lastBlur = -1; _lastBox = Rect.Empty; Sync(); }
        ApplyMode();
        ApplyIce();
        LookChanged?.Invoke();
    }

    /// <summary>Искры мерцают в общем ритме 6 с, только пока движется фон (окно на виду, игра не запущена).</summary>
    public void SetLive(bool live)
    {
        _live = live;
        RunSparks();
    }

    private Border? _clip;
    private Grid? _lens;
    private Rectangle? _sample;
    private GlassEdge? _edge;
    private Canvas? _sparks;
    private Border? _contour, _highlight;
    private readonly VisualBrush _brush = new()
    {
        ViewboxUnits = BrushMappingMode.Absolute,
        Stretch = Stretch.Fill,
        AlignmentX = AlignmentX.Left,
        AlignmentY = AlignmentY.Top,
        TileMode = TileMode.None,
    };
    private readonly BlurEffect _blur = new() { KernelType = KernelType.Gaussian, RenderingBias = RenderingBias.Performance };
    private RefractionEffect? _refraction;
    private Rect _lastBox = Rect.Empty;
    private Rect _lastShape = Rect.Empty;
    private double _lastCorner = -1, _lastBlur = -1;

    public Glass()
    {
        LayoutUpdated += (_, _) => Sync();
        SizeChanged += (_, _) => { Sync(); ShapeChanged(); };
        IsVisibleChanged += (_, _) => RunSparks();
        RenderOptions.SetCachingHint(_brush, CachingHint.Unspecified);
        _sparkTimer = new DispatcherTimer(DispatcherPriority.Background) { Interval = TimeSpan.FromMilliseconds(150) };
        _sparkTimer.Tick += (_, _) => { _sparkTimer.Stop(); RebuildSparks(); };
    }

    public override void OnApplyTemplate()
    {
        base.OnApplyTemplate();
        _clip = GetTemplateChild("PART_Clip") as Border;
        _lens = GetTemplateChild("PART_Lens") as Grid;
        _sample = GetTemplateChild("PART_Sample") as Rectangle;
        _edge = GetTemplateChild("PART_Edge") as GlassEdge;
        _sparks = GetTemplateChild("PART_Sparks") as Canvas;
        _contour = GetTemplateChild("PART_Contour") as Border;
        _highlight = GetTemplateChild("PART_Highlight") as Border;
        if (_sample is not null)
        {
            _sample.Fill = _brush;
            _sample.Effect = _blur;
        }
        _lastBox = Rect.Empty;
        _lastShape = Rect.Empty;
        Sync();
        ApplyMode();
        ApplyIce();
        ShapeChanged();
    }

    // ───────────── копия сцены ─────────────

    /// <summary>Ставит окно копии сцены ровно под панель (с запасом на размытие по краям). Зовётся часто, поэтому
    /// ничего не пересоздаёт, если размер и место не изменились.</summary>
    private void Sync()
    {
        if (_sample is null || _clip is null) return;
        var w = ActualWidth;
        var h = ActualHeight;
        if (w <= 0 || h <= 0) return;

        var r = _mode switch { GlassMode.Matte => 26.0, GlassMode.Flat => 0.0, _ => Math.Max(0, Blur) };
        var shape = new Rect(0, 0, w, h);
        if (shape != _lastShape || Corner != _lastCorner || r != _lastBlur)
        {
            _lastShape = shape;
            _lastCorner = Corner;
            _lastBlur = r;
            _blur.Radius = r;
            _sample.Effect = r > 0 ? _blur : null;
            UpdateMask();
            _sample.Margin = new Thickness(-r);
            _lastBox = Rect.Empty;
            if (_refraction is not null) { _refraction.Size = new Size(w, h); _refraction.Radius = Corner; }
        }

        if (_mode == GlassMode.Flat || Source is not Visual src || !IsVisible) { _brush.Visual = null; _lastBox = Rect.Empty; return; }
        if (!ReferenceEquals(_brush.Visual, src)) _brush.Visual = src;
        Point p;
        try { p = TransformToVisual(src).Transform(new Point(0, 0)); }
        catch (InvalidOperationException) { return; } // ещё не в одном дереве
        var box = new Rect(p.X - r, p.Y - r, w + 2 * r, h + 2 * r);
        if (box == _lastBox) return;
        _lastBox = box;
        _brush.Viewbox = box;
    }

    /// <summary>Преломление только в жидком режиме и только если шейдер есть; кромка-блик и блик сверху — тоже.</summary>
    private void ApplyMode()
    {
        if (_lens is null) return;
        var liquid = _mode == GlassMode.Liquid;
        if (liquid && RefractionEffect.IsAvailable)
        {
            _refraction ??= new RefractionEffect { Saturation = 1.4 };
            _refraction.Strength = Refraction;
            _refraction.Lens = Lens;
            _refraction.Radius = Corner;
            // в светлом оформлении копия сцены под стеклом не притемняется, а чуть высветляется: светлое стекло
            _refraction.Brightness = EffectiveBrightness;
            _refraction.Size = new Size(Math.Max(1, ActualWidth), Math.Max(1, ActualHeight));
            if (!ReferenceEquals(_lens.Effect, _refraction)) _lens.Effect = _refraction;
        }
        else _lens.Effect = null;
        UpdateMask();
        _lens.Visibility = _mode == GlassMode.Flat ? Visibility.Collapsed : Visibility.Visible;
        // у матового стекла ровная кромка 1 px и нет блика; у жидкого — яркая кромка по карте и блик сверху слева
        if (_edge is not null) { _edge.Specular = liquid ? 1 : 0; _edge.Lens = Lens; }
        if (_highlight is not null) _highlight.Visibility = liquid ? Visibility.Visible : Visibility.Collapsed;
        if (_contour is not null) _contour.BorderBrush = _light ? ContourLight : liquid ? ContourLiquid : ContourMatte;
    }

    /// <summary>
    /// Скругление копии сцены. У жидкого стекла его делает шейдер (со сглаживанием по краю); у матового — маска.
    /// Не Clip: обрезка в WPF не сглаживается, и углы стекла были «в пикселях».
    /// </summary>
    private void UpdateMask()
    {
        if (_clip is null) return;
        var w = ActualWidth; var h = ActualHeight;
        var shader = _mode == GlassMode.Liquid && _lens?.Effect is RefractionEffect;
        _clip.OpacityMask = shader || w <= 0 || h <= 0 ? null : RoundMask(w, h, Corner);
    }

    private static readonly Brush ContourLiquid = Frozen(new SolidColorBrush(Color.FromArgb(0x1F, 0xFF, 0xFF, 0xFF)));
    private static readonly Brush ContourMatte = Frozen(new SolidColorBrush(Color.FromArgb(0x42, 0xFF, 0xFF, 0xFF)));
    private static readonly Brush ContourLight = Frozen(new SolidColorBrush(Color.FromArgb(0xB3, 0xFF, 0xFF, 0xFF)));

    private void ApplyIce()
    {
        var on = _iceFactor > 0 && Ice > 0;
        if (_edge is not null)
        {
            _edge.Frost = on ? Math.Min(1.4, Ice * _iceFactor) : 0;
            _edge.Reach = IceReach;
        }
        // искры — отдельная настройка: их можно оставить и без изморози (решено 2026-10-16)
        if (_sparks is not null) _sparks.Visibility = _sparksOn && Sparks > 0 ? Visibility.Visible : Visibility.Collapsed;
        RunSparks();
    }

    // ───────────── кромка и искры под размер панели ─────────────

    private readonly DispatcherTimer _sparkTimer;
    private (int W, int H, int Seed, int Sparks, double Corner) _sparkKey;

    /// <summary>
    /// Размер или форма изменились. Кромку и изморозь элемент рисует сам под любой размер; искры стоят в пикселях от
    /// краёв — их места пересчитываются, когда размер успокоится (не на каждом шаге растяжения окна), до этого их не видно.
    /// </summary>
    private void ShapeChanged()
    {
        if (_edge is not null) { _edge.Reach = IceReach; _edge.Lens = Lens; }
        if (_sparks is null || ActualWidth < 2 || ActualHeight < 2) return;
        var key = ((int)Math.Round(ActualWidth), (int)Math.Round(ActualHeight), Seed, Sparks, Corner);
        if (key == _sparkKey) return;
        _sparks.Opacity = 0;
        _sparkTimer.Stop();
        _sparkTimer.Start();
    }

    private void RebuildSparks()
    {
        var w = (int)Math.Round(ActualWidth);
        var h = (int)Math.Round(ActualHeight);
        if (w < 2 || h < 2 || _sparks is null) return;
        _sparkKey = (w, h, Seed, Sparks, Corner);
        BuildSparks(w, h);
        _sparks.Opacity = 1;
    }

    private Storyboard? _sparkBoard;
    private bool _sparkRunning;

    /// <summary>
    /// Искры льда (доска build_ice.py): крестики 9–15 px у самой кромки, вспыхивают по очереди в ритме 6 с: долго
    /// невидимы, на 82 % удара раскрываются с лёгким поворотом и гаснут. Места — по зерну панели.
    /// </summary>
    private void BuildSparks(int w, int h)
    {
        if (_sparks is null) return;
        _sparkBoard?.Stop(this);
        _sparkBoard = null;
        _sparkRunning = false;
        _sparkPaused = false;
        _sparks.Children.Clear();
        if (Sparks <= 0) return;
        var rnd = new Random(Seed * 7919 + 13);
        var sb = new Storyboard { RepeatBehavior = RepeatBehavior.Forever };
        Timeline.SetDesiredFrameRate(sb, 30);
        for (var i = 0; i < Sparks; i++)
        {
            double x = rnd.Next(6, Math.Max(7, w - 6)), y = rnd.Next(6, Math.Max(7, h - 6));
            switch (rnd.Next(4))
            {
                case 0: y = rnd.Next(4, 17); break;
                case 1: y = h - rnd.Next(4, 17); break;
                case 2: x = rnd.Next(4, 17); break;
                default: x = w - rnd.Next(4, 17); break;
            }
            // искра только внутри скруглённой формы: в срезанном углу её не видно было бы целиком
            var size = new[] { 9, 11, 13, 15 }[rnd.Next(4)];
            if (GlassTextures.InsideDistance(x, y, w, h, Corner) < size / 2.0 + 1) continue;
            var spark = MakeSpark(size);
            Canvas.SetLeft(spark, x - size / 2.0);
            Canvas.SetTop(spark, y - size / 2.0);
            _sparks.Children.Add(spark);
            var delay = TimeSpan.FromSeconds(-rnd.Next(0, 6));
            sb.Children.Add(Keys(spark, OpacityProperty, delay, (0, 0), (0.70, 0), (0.82, 1), (0.92, 0.25), (1, 0)));
            var st = (TransformGroup)spark.RenderTransform;
            sb.Children.Add(Keys(st.Children[0], ScaleTransform.ScaleXProperty, delay, (0, 0.4), (0.70, 0.4), (0.82, 1), (0.92, 0.7), (1, 0.4)));
            sb.Children.Add(Keys(st.Children[0], ScaleTransform.ScaleYProperty, delay, (0, 0.4), (0.70, 0.4), (0.82, 1), (0.92, 0.7), (1, 0.4)));
            sb.Children.Add(Keys(st.Children[1], RotateTransform.AngleProperty, delay, (0, 0), (0.70, 0), (0.82, 12), (0.92, 20), (1, 0)));
        }
        _sparkBoard = sb;
        RunSparks();
    }

    private static DoubleAnimationUsingKeyFrames Keys(DependencyObject target, DependencyProperty prop, TimeSpan begin, params (double T, double V)[] keys)
    {
        var a = new DoubleAnimationUsingKeyFrames { Duration = TimeSpan.FromSeconds(6), BeginTime = begin, RepeatBehavior = RepeatBehavior.Forever };
        foreach (var (t, v) in keys)
            a.KeyFrames.Add(new EasingDoubleKeyFrame(v, KeyTime.FromTimeSpan(TimeSpan.FromSeconds(6 * t)), SparkEase));
        Storyboard.SetTarget(a, target);
        Storyboard.SetTargetProperty(a, new PropertyPath(prop));
        return a;
    }

    private static readonly IEasingFunction SparkEase = Frozen(new SineEase { EasingMode = EasingMode.EaseInOut });

    private bool _sparkPaused;

    /// <summary>
    /// Искры мерцают, пока панель на экране и фон живой. Окно неактивно или игра запущена — замирают, где были.
    /// «Уменьшить анимацию» в Windows — стоят неподвижно и видны наполовину (как на доске: opacity .55).
    /// </summary>
    private void RunSparks()
    {
        if (_sparkBoard is null || _sparks is null) return;
        var shown = IsVisible && _sparks.Visibility == Visibility.Visible;
        if (Motion.SpringDriver.SystemReduced)
        {
            if (_sparkRunning) { _sparkBoard.Stop(this); _sparkRunning = false; _sparkPaused = false; }
            foreach (FrameworkElement sp in _sparks.Children)
            {
                sp.Opacity = 0.55;
                var st = (TransformGroup)sp.RenderTransform;
                ((ScaleTransform)st.Children[0]).ScaleX = ((ScaleTransform)st.Children[0]).ScaleY = 1;
            }
            return;
        }
        var run = _live && shown;
        if (run && !_sparkRunning) { _sparkBoard.Begin(this, true); _sparkRunning = true; _sparkPaused = false; }
        else if (run && _sparkPaused) { _sparkBoard.Resume(this); _sparkPaused = false; }
        else if (!run && _sparkRunning && !_sparkPaused) { _sparkBoard.Pause(this); _sparkPaused = true; }
    }

    /// <summary>Искра: два тонких светлых луча крестом и мягкая точка в центре (как .gl на доске).</summary>
    private static FrameworkElement MakeSpark(int size)
    {
        var g = new Grid { Width = size, Height = size, IsHitTestVisible = false, Opacity = 0, RenderTransformOrigin = new Point(0.5, 0.5) };
        g.RenderTransform = new TransformGroup { Children = { new ScaleTransform(0.4, 0.4), new RotateTransform(0) } };
        g.Children.Add(new Rectangle { Height = 1.5, VerticalAlignment = VerticalAlignment.Center, Fill = RayH });
        g.Children.Add(new Rectangle { Width = 1.5, HorizontalAlignment = HorizontalAlignment.Center, Fill = RayV });
        g.Children.Add(new Ellipse { Fill = SparkCore });
        return g;
    }

    private static readonly Brush RayH = Frozen(new LinearGradientBrush(new GradientStopCollection
    {
        new(Color.FromArgb(0, 235, 246, 255), 0), new(Color.FromArgb(0xB3, 235, 246, 255), 0.5), new(Color.FromArgb(0, 235, 246, 255), 1),
    }, new Point(0, 0.5), new Point(1, 0.5)));
    private static readonly Brush RayV = Frozen(new LinearGradientBrush(new GradientStopCollection
    {
        new(Color.FromArgb(0, 235, 246, 255), 0), new(Color.FromArgb(0xB3, 235, 246, 255), 0.5), new(Color.FromArgb(0, 235, 246, 255), 1),
    }, new Point(0.5, 0), new Point(0.5, 1)));
    private static readonly Brush SparkCore = Frozen(new RadialGradientBrush(new GradientStopCollection
    {
        new(Color.FromArgb(0xF2, 255, 255, 255), 0), new(Color.FromArgb(0, 210, 232, 255), 0.55),
    }));

    private static T Frozen<T>(T f) where T : Freezable { f.Freeze(); return f; }

    /// <summary>
    /// Маска скруглённого прямоугольника в координатах элемента. Заливка фигуры сглаживается (в отличие от Clip), поэтому
    /// края стекла и капель ровные на любом масштабе.
    /// </summary>
    internal static Brush RoundMask(double w, double h, double r)
    {
        var drawing = new GeometryDrawing(Brushes.Black, null, new RectangleGeometry(new Rect(0, 0, w, h), r, r));
        return Frozen(new DrawingBrush(drawing)
        {
            Stretch = Stretch.Fill,
            ViewboxUnits = BrushMappingMode.Absolute, Viewbox = new Rect(0, 0, w, h),
            ViewportUnits = BrushMappingMode.Absolute, Viewport = new Rect(0, 0, w, h),
        });
    }
}
