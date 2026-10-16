using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Effects;
using System.Windows.Shapes;

namespace BeamModManager.App.Controls;

/// <summary>
/// Капля жидкого стекла (доска «Гараж», build_ice.py: .drop / .ndrop / .wdrop) — под активным разделом левой панели,
/// под выбранным фильтром и на кнопках окна. Слои снизу вверх:
/// <list type="number">
/// <item>линза: копия сцены под каплей, преломлённая шейдером у краёв, ярче и насыщеннее (saturate 1.5, brightness 1.12),
///   поверх — подложка своей стеклянной панели: капля — это то же стекло, только «линзой»;</item>
/// <item>заливка: белый градиент под 160° (.24 → .05 → .13);</item>
/// <item>объём: внутренние блики — яркий сверху слева (.85), слабый снизу справа (.45), мягкое свечение по всей кромке
///   изнутри и тонкий светлый контур (.30);</item>
/// <item>радужная кромка по кругу (conic-gradient от 210°: голубая слева сверху, розовая справа снизу), 1,5 px.</item>
/// </list>
/// Тень даёт DropShadowEffect самой капли. Вариант «опасно» (<see cref="Danger"/>, кнопка «Закрыть») плавно перекрашивает
/// заливку и блики в красный. Капля не перехватывает мышь.
/// </summary>
public sealed class Droplet : FrameworkElement
{
    public static readonly DependencyProperty CornerProperty = DependencyProperty.Register(
        nameof(Corner), typeof(double), typeof(Droplet),
        new FrameworkPropertyMetadata(20.0, FrameworkPropertyMetadataOptions.AffectsRender, (d, _) => ((Droplet)d).ShapeChanged()));

    /// <summary>0 — обычная капля, 1 — красная (под «Закрыть»); промежуточные значения — переход.</summary>
    public static readonly DependencyProperty DangerProperty = DependencyProperty.Register(
        nameof(Danger), typeof(double), typeof(Droplet), new FrameworkPropertyMetadata(0.0, (d, _) => ((Droplet)d)._paint.InvalidateVisual()));

    /// <summary>Подложка под каплей внутри панели (у фильтров — тёмная дорожка); null — нет.</summary>
    public static readonly DependencyProperty UnderProperty = DependencyProperty.Register(
        nameof(Under), typeof(Brush), typeof(Droplet), new PropertyMetadata(null, (d, _) => ((Droplet)d).UpdateLens()));

    public double Corner { get => (double)GetValue(CornerProperty); set => SetValue(CornerProperty, value); }
    public double Danger { get => (double)GetValue(DangerProperty); set => SetValue(DangerProperty, value); }
    public Brush? Under { get => (Brush?)GetValue(UnderProperty); set => SetValue(UnderProperty, value); }

    private readonly VisualCollection _children;
    private readonly Grid _clip, _lens;
    private readonly Rectangle _sample, _tint, _under;
    private readonly Paint _paint;
    private readonly VisualBrush _brush = new()
    {
        ViewboxUnits = BrushMappingMode.Absolute, Stretch = Stretch.Fill,
        AlignmentX = AlignmentX.Left, AlignmentY = AlignmentY.Top, TileMode = TileMode.None,
    };
    private readonly RefractionEffect? _refraction;
    private readonly BlurEffect _blur = new() { KernelType = KernelType.Gaussian, RenderingBias = RenderingBias.Performance };
    private Glass? _owner;
    private Rect _lastBox = Rect.Empty;

    public Droplet()
    {
        IsHitTestVisible = false;
        RenderTransformOrigin = new Point(0.5, 0.5);
        _sample = new Rectangle { Fill = _brush, Effect = _blur };
        _tint = new Rectangle();
        _under = new Rectangle();
        // как у панелей (Theme.xaml, c:Glass): форма — снаружи, эффект — на сетке, внутри обрезка ровно по размеру,
        // чтобы шейдер получал картинку размером с каплю, а не с запасом на размытие
        _lens = new Grid { Children = { new System.Windows.Controls.Border { ClipToBounds = true, Child = _sample } } };
        _clip = new Grid { ClipToBounds = true, Children = { _lens } };
        if (RefractionEffect.IsAvailable)
        {
            // линза капли сильнее, чем у панели: scale 44 на доске, ×1,5 как у остального стекла
            _refraction = new RefractionEffect { Strength = 66, Lens = 20, Saturation = 1.5 };
            _lens.Effect = _refraction;
        }
        _paint = new Paint(this);
        _children = new VisualCollection(this) { _clip, _tint, _under, _paint };
        // тень капли: 0 10px 20px rgba(4,8,20,.50)
        Effect = new DropShadowEffect { BlurRadius = 20, ShadowDepth = 8, Direction = 270, Color = Color.FromRgb(4, 8, 20), Opacity = 0.5, RenderingBias = RenderingBias.Performance };
        Loaded += (_, _) => Attach();
        Unloaded += (_, _) => Detach();
        LayoutUpdated += (_, _) => Sync();
        SizeChanged += (_, _) => ShapeChanged();
    }

    protected override int VisualChildrenCount => _children.Count;
    protected override Visual GetVisualChild(int index) => _children[index];

    protected override Size MeasureOverride(Size available)
    {
        foreach (UIElement c in _children) c.Measure(available);
        return new Size(0, 0); // размер задаётся снаружи (Width/Height или пружина)
    }

    protected override Size ArrangeOverride(Size final)
    {
        foreach (UIElement c in _children) c.Arrange(new Rect(final));
        return final;
    }

    // ───────────── стекло-хозяин и копия сцены ─────────────

    private void Attach()
    {
        Detach();
        for (DependencyObject? p = VisualTreeHelper.GetParent(this); p is not null; p = VisualTreeHelper.GetParent(p))
            if (p is Glass g) { _owner = g; break; }
        if (_owner is not null) _owner.LookChanged += UpdateLens;
        UpdateLens();
    }

    private void Detach()
    {
        if (_owner is not null) _owner.LookChanged -= UpdateLens;
        _owner = null;
    }

    /// <summary>
    /// Линза есть только у жидкого стекла: у матового и плоского капля — заливка, блики и кромка поверх панели (её копию
    /// сцены панель уже показала под каплей). Подложка панели и её яркость берутся у самой панели.
    /// </summary>
    private void UpdateLens()
    {
        var liquid = _owner is { Mode: GlassMode.Liquid };
        _clip.Visibility = liquid ? Visibility.Visible : Visibility.Collapsed;
        _tint.Visibility = liquid ? Visibility.Visible : Visibility.Collapsed;
        _under.Visibility = liquid && Under is not null ? Visibility.Visible : Visibility.Collapsed;
        if (_owner is not null)
        {
            _tint.Fill = _owner.Tint;
            _blur.Radius = Math.Max(0, _owner.Blur);
            _sample.Effect = _blur.Radius > 0 ? _blur : null;
            if (_refraction is not null) _refraction.Brightness = Math.Min(1.15, _owner.EffectiveBrightness * 1.12);
        }
        _under.Fill = Under;
        _lastBox = Rect.Empty;
        _paint.InvalidateVisual();
        Sync();
    }

    private void ShapeChanged()
    {
        var w = ActualWidth; var h = ActualHeight;
        if (w <= 0 || h <= 0) return;
        var r = Math.Min(Corner, Math.Min(w, h) / 2);
        var clip = new RectangleGeometry(new Rect(0, 0, w, h), r, r);
        clip.Freeze();
        _clip.Clip = clip;
        _tint.RadiusX = _tint.RadiusY = _under.RadiusX = _under.RadiusY = r;
        if (_refraction is not null) { _refraction.Size = new Size(w, h); _refraction.Radius = r; }
        _lastBox = Rect.Empty;
        _paint.InvalidateVisual();
        Sync();
    }

    /// <summary>
    /// Ставит окно копии сцены ровно под каплю. Капля ездит трансформациями (пружины), раскладка при этом не идёт,
    /// поэтому окно зовёт Sync на каждом шаге пружины. Растяжение учитывается: берётся рамка капли на экране.
    /// </summary>
    public void Sync()
    {
        if (_clip.Visibility != Visibility.Visible || _owner?.Source is not Visual src || !IsVisible) { _brush.Visual = null; return; }
        var w = ActualWidth; var h = ActualHeight;
        if (w <= 0 || h <= 0) return;
        if (!ReferenceEquals(_brush.Visual, src)) _brush.Visual = src;
        Rect box;
        try { box = TransformToVisual(src).TransformBounds(new Rect(0, 0, w, h)); }
        catch (InvalidOperationException) { return; } // ещё не в одном дереве со сценой
        var r = _blur.Radius;
        // запас на размытие, как у панелей: края копии не темнеют
        var sx = box.Width / w; var sy = box.Height / h;
        box = new Rect(box.X - r * sx, box.Y - r * sy, box.Width + 2 * r * sx, box.Height + 2 * r * sy);
        if (box == _lastBox) return;
        _lastBox = box;
        _sample.Margin = new Thickness(-r);
        _brush.Viewbox = box;
    }

    // ───────────── рисунок капли ─────────────

    /// <summary>Заливка, блики и кромка — свой элемент поверх линзы (OnRender самой капли рисовал бы под ней).</summary>
    private sealed class Paint : FrameworkElement
    {
        private readonly Droplet _d;
        public Paint(Droplet d) { _d = d; IsHitTestVisible = false; }

        protected override void OnRender(DrawingContext dc)
        {
            var w = ActualWidth; var h = ActualHeight;
            if (w < 2 || h < 2) return;
            var r = Math.Min(_d.Corner, Math.Min(w, h) / 2);
            var danger = Math.Clamp(_d.Danger, 0, 1);
            var light = _d._owner?.Light == true;
            var shape = new RectangleGeometry(new Rect(0, 0, w, h), r, r);

            // заливка под 160°: в светлом оформлении плотнее, иначе на светлом стекле капли не видно
            Color F(double a) => Color.FromArgb(A(a), 255, 255, 255);
            var fill = new LinearGradientBrush(new GradientStopCollection
            {
                new(Mix(F(light ? 0.62 : 0.24), Color.FromArgb(A(0.62), 255, 130, 118), danger), 0),
                new(Mix(F(light ? 0.30 : 0.05), Color.FromArgb(A(0.52), 214, 58, 46), danger), 0.52),
                new(Mix(F(light ? 0.48 : 0.13), Color.FromArgb(A(0.58), 255, 120, 108), danger), 1),
            }, new Point(0.33, 0.03), new Point(0.67, 0.97));
            dc.DrawGeometry(fill, null, shape);

            dc.PushClip(shape);
            // мягкое свечение изнутри по всей кромке (inset 0 0 16px rgba(255,255,255,.10))
            for (var i = 1; i <= 8; i++)
            {
                var k = 1 - i / 9.0;
                Ring(dc, w, h, r, i * 1.6, 1.6, new SolidColorBrush(Color.FromArgb(A(0.035 * k * k), 255, 255, 255)));
            }
            // блик сверху слева (inset 2px 2px 4px -1px .85) и слабый снизу справа (inset -2px -2px 4px -1px .45)
            var hiTop = Mix(Color.FromRgb(255, 255, 255), Color.FromRgb(255, 235, 230), danger);
            var hiBottom = Mix(Color.FromRgb(255, 255, 255), Color.FromRgb(255, 200, 190), danger);
            double[] soft = { 0.62, 0.38, 0.18, 0.06 };
            for (var i = 0; i < soft.Length; i++)
            {
                Ring(dc, w, h, r, 0.9 + i, 1.1, Diagonal(hiTop, 0.85 * soft[i], topLeft: true));
                Ring(dc, w, h, r, 0.9 + i, 1.1, Diagonal(hiBottom, 0.45 * soft[i], topLeft: false));
            }
            dc.Pop();

            // тонкий светлый контур (inset 0 0 0 1px .30; у красной — розовый .45; на светлом стекле — тёмный штрих)
            var rim = light && danger < 0.5 ? Color.FromArgb(A(0.16), 10, 20, 40) : Mix(Color.FromArgb(A(0.30), 255, 255, 255), Color.FromArgb(A(0.45), 255, 190, 180), danger);
            Ring(dc, w, h, r, 0.5, 1, new SolidColorBrush(rim));

            // радужная кромка по кругу, 1,5 px внутри края
            ConicRim(dc, w, h, r);
        }

        /// <summary>Кольцо по скруглённому прямоугольнику, отступив <paramref name="inset"/> внутрь.</summary>
        private static void Ring(DrawingContext dc, double w, double h, double r, double inset, double thickness, Brush brush)
        {
            if (w - 2 * inset <= 0 || h - 2 * inset <= 0) return;
            var ri = Math.Max(0, r - inset);
            dc.DrawRoundedRectangle(null, new Pen(brush, thickness), new Rect(inset, inset, w - 2 * inset, h - 2 * inset), ri, ri);
        }

        /// <summary>Свет по диагонали: от угла к середине, дальше прозрачно (так ложится inset-тень со сдвигом 2 px).</summary>
        private static Brush Diagonal(Color c, double a, bool topLeft)
        {
            var on = Color.FromArgb(A(a), c.R, c.G, c.B);
            var off = Color.FromArgb(0, c.R, c.G, c.B);
            return topLeft
                ? new LinearGradientBrush(new GradientStopCollection { new(on, 0), new(on, 0.18), new(off, 0.55) }, new Point(0, 0), new Point(1, 1))
                : new LinearGradientBrush(new GradientStopCollection { new(off, 0.45), new(on, 0.82), new(on, 1) }, new Point(0, 0), new Point(1, 1));
        }

        // conic-gradient(from 210deg, transparent, rgba(110,200,255,.9) 14%, transparent 30%, transparent 58%,
        //                rgba(255,110,200,.85) 72%, transparent 90%)
        private static readonly (double At, Color C)[] Conic =
        {
            (0.00, Color.FromArgb(0, 255, 255, 255)), (0.14, Color.FromArgb(230, 110, 200, 255)), (0.30, Color.FromArgb(0, 255, 255, 255)),
            (0.58, Color.FromArgb(0, 255, 255, 255)), (0.72, Color.FromArgb(217, 255, 110, 200)), (0.90, Color.FromArgb(0, 255, 255, 255)),
            (1.00, Color.FromArgb(0, 255, 255, 255)),
        };

        private static readonly Dictionary<uint, Pen> Pens = new();

        /// <summary>
        /// Круговой градиент по кромке: WPF его не умеет, поэтому кромка режется на короткие отрезки, и у каждого свой
        /// цвет по углу от центра капли (как у conic-gradient). Перья запоминаются по цвету.
        /// </summary>
        private static void ConicRim(DrawingContext dc, double w, double h, double r)
        {
            const double inset = 0.75, thick = 1.5;
            var pts = Outline(w, h, Math.Max(0, r - inset), inset);
            double cx = w / 2, cy = h / 2;
            for (var i = 0; i < pts.Count; i++)
            {
                var a = pts[i];
                var b = pts[(i + 1) % pts.Count];
                var mx = (a.X + b.X) / 2 - cx; var my = (a.Y + b.Y) / 2 - cy;
                // угол как в CSS: 0° — вверх, по часовой; отсчёт от 210°
                var deg = (Math.Atan2(mx, -my) * 180 / Math.PI + 360) % 360;
                var c = ConicAt(((deg - 210) % 360 + 360) % 360 / 360);
                if (c.A < 8) continue;
                var key = (uint)(c.A << 24 | c.R << 16 | c.G << 8 | c.B);
                if (!Pens.TryGetValue(key, out var pen))
                {
                    pen = new Pen(new SolidColorBrush(c), thick) { StartLineCap = PenLineCap.Round, EndLineCap = PenLineCap.Round };
                    pen.Freeze();
                    if (Pens.Count < 4096) Pens[key] = pen;
                }
                dc.DrawLine(pen, a, b);
            }
        }

        private static Color ConicAt(double t)
        {
            for (var i = 1; i < Conic.Length; i++)
            {
                if (t > Conic[i].At) continue;
                var (a0, c0) = Conic[i - 1];
                var (a1, c1) = Conic[i];
                var k = a1 > a0 ? (t - a0) / (a1 - a0) : 0;
                // перья по цвету: округляем, чтобы их было немного
                return Quant(Mix(c0, c1, k));
            }
            return Conic[^1].C;
        }

        private static Color Quant(Color c) => Color.FromArgb((byte)(c.A / 8 * 8), (byte)(c.R / 8 * 8), (byte)(c.G / 8 * 8), (byte)(c.B / 8 * 8));

        /// <summary>Точки по контуру скруглённого прямоугольника, по часовой стрелке, шаг ≈2 px.</summary>
        private static List<Point> Outline(double w, double h, double r, double inset)
        {
            var pts = new List<Point>();
            double x0 = inset, y0 = inset, x1 = w - inset, y1 = h - inset;
            void Line(Point a, Point b)
            {
                var n = Math.Max(1, (int)((b - a).Length / 2));
                for (var i = 0; i < n; i++) pts.Add(a + (b - a) * (i / (double)n));
            }
            void Arc(double cx, double cy, double start)
            {
                var n = Math.Max(2, (int)(Math.PI * r / 2 / 2));
                for (var i = 0; i < n; i++)
                {
                    var t = (start + 90.0 * i / n) * Math.PI / 180;
                    pts.Add(new Point(cx + r * Math.Cos(t), cy + r * Math.Sin(t)));
                }
            }
            Line(new Point(x0 + r, y0), new Point(x1 - r, y0));
            Arc(x1 - r, y0 + r, -90);
            Line(new Point(x1, y0 + r), new Point(x1, y1 - r));
            Arc(x1 - r, y1 - r, 0);
            Line(new Point(x1 - r, y1), new Point(x0 + r, y1));
            Arc(x0 + r, y1 - r, 90);
            Line(new Point(x0, y1 - r), new Point(x0, y0 + r));
            Arc(x0 + r, y0 + r, 180);
            return pts;
        }

        private static byte A(double a) => (byte)Math.Round(Math.Clamp(a, 0, 1) * 255);

        private static Color Mix(Color a, Color b, double t) => t <= 0 ? a : t >= 1 ? b : Color.FromArgb(
            (byte)(a.A + (b.A - a.A) * t), (byte)(a.R + (b.R - a.R) * t), (byte)(a.G + (b.G - a.G) * t), (byte)(a.B + (b.B - a.B) * t));
    }
}
