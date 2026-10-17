using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Effects;
using System.Windows.Shapes;

namespace BeamModManager.App.Controls;

/// <summary>
/// Капля жидкого стекла (как выбор в панелях iOS 26) — под активным разделом левой панели, под выбранным фильтром и на
/// кнопках окна. Вариант «В» (утверждён 2026-10-17, образец «Капли жидкого стекла»): в покое — тихая светлая плашка с
/// радужной кромкой под значком и подписью; в пути (<see cref="Motion"/> → 1) она перетекает в прозрачную линзу:
/// <list type="number">
/// <item>линза: чёткая (без размытия) копия сцены под каплей, чуть увеличенная (×1,1), у краёв преломлённая шейдером
///   с тонкой радужной кромкой, ярче и насыщеннее; поверх — подложка своей панели, чтобы капля была тем же стеклом;</item>
/// <item>едва заметная светлая заливка — только чтобы капля читалась на однотонном фоне;</item>
/// <item>блик по кромке: яркий сверху слева, слабее снизу справа, светлый серп снизу внутри (свет выходит из капли);</item>
/// <item>тонкий контур и радужная кромка по кругу.</item>
/// </list>
/// Пока капля стоит, линзы нет совсем (копия сцены не считается) — это и дешевле.
/// При выключенном жидком стекле (матовое и плоское стекло) капли нет: вместо неё ровная полупрозрачная плашка без линзы,
/// бликов и тени — двигается и тянется так же. Вариант «опасно» (<see cref="Danger"/>, кнопка «Закрыть») плавно
/// перекрашивает каплю или плашку в красный. Капля не перехватывает мышь.
/// </summary>
public sealed class Droplet : FrameworkElement
{
    public static readonly DependencyProperty CornerProperty = DependencyProperty.Register(
        nameof(Corner), typeof(double), typeof(Droplet),
        new FrameworkPropertyMetadata(20.0, FrameworkPropertyMetadataOptions.AffectsRender, (d, _) => ((Droplet)d).ShapeChanged()));

    /// <summary>0 — обычная капля, 1 — красная (под «Закрыть»); промежуточные значения — переход.</summary>
    public static readonly DependencyProperty DangerProperty = DependencyProperty.Register(
        nameof(Danger), typeof(double), typeof(Droplet), new FrameworkPropertyMetadata(0.0, (d, _) => ((Droplet)d).Repaint()));

    /// <summary>Подложка под каплей внутри панели (у фильтров — тёмная дорожка); null — нет.</summary>
    public static readonly DependencyProperty UnderProperty = DependencyProperty.Register(
        nameof(Under), typeof(Brush), typeof(Droplet), new PropertyMetadata(null, (d, _) => ((Droplet)d).UpdateLens()));

    /// <summary>
    /// 0 — капля стоит (плашка), 1 — в пути (линза); промежуточные — переход. Задаёт окно по пружинам капли.
    /// Без жидкого стекла не действует: там всегда плашка.
    /// </summary>
    public static readonly DependencyProperty MotionProperty = DependencyProperty.Register(
        nameof(Motion), typeof(double), typeof(Droplet), new FrameworkPropertyMetadata(0.0, (d, _) => ((Droplet)d).ApplyMotion()));

    public double Corner { get => (double)GetValue(CornerProperty); set => SetValue(CornerProperty, value); }
    public double Motion { get => (double)GetValue(MotionProperty); set => SetValue(MotionProperty, value); }
    public double Danger { get => (double)GetValue(DangerProperty); set => SetValue(DangerProperty, value); }
    public Brush? Under { get => (Brush?)GetValue(UnderProperty); set => SetValue(UnderProperty, value); }

    private readonly VisualCollection _children;
    private readonly Grid _clip, _lens;
    private readonly Rectangle _sample, _tint, _under;
    private readonly Paint _paint;
    private readonly RestPaint _rest;
    private readonly Shadow _shadow;
    private readonly VisualBrush _brush = new()
    {
        ViewboxUnits = BrushMappingMode.Absolute, Stretch = Stretch.Fill,
        AlignmentX = AlignmentX.Left, AlignmentY = AlignmentY.Top, TileMode = TileMode.None,
    };
    private readonly RefractionEffect? _refraction;
    private Glass? _owner;
    private Rect _lastBox = Rect.Empty;

    public Droplet()
    {
        // без размытия: линза капли чёткая, как у Apple, и это дешевле (размытие считалось бы на каждом шаге пружины)
        _sample = new Rectangle { Fill = _brush };
        _tint = new Rectangle();
        _under = new Rectangle();
        // как у панелей (Theme.xaml, c:Glass): форма — снаружи, эффект — на сетке, внутри обрезка ровно по размеру,
        // чтобы шейдер получал картинку размером с каплю, а не с запасом на размытие
        _lens = new Grid { Children = { new System.Windows.Controls.Border { ClipToBounds = true, Child = _sample } } };
        _clip = new Grid { ClipToBounds = true, Children = { _lens } };
        if (RefractionEffect.IsAvailable)
        {
            // линза капли заметно сильнее, чем у панели (2026-10-16: «как капля»): широкая и сильная, насыщенный цвет
            _refraction = new RefractionEffect { Strength = 100, Lens = 26, Saturation = 1.65 };
            _lens.Effect = _refraction;
        }
        // рисунок капли (кромка, блики, радужный край — сотни коротких штрихов) меняется только с формой и цветом, а капля
        // в пути каждый кадр растягивается: кэш картинкой, иначе штрихи перерисовывались бы на каждом кадре
        _paint = new Paint(this) { CacheMode = new BitmapCache { SnapsToDevicePixels = false } };
        // тень — нарисованные слои, а не DropShadowEffect: эффект на капле пересчитывался бы каждый кадр вместе с линзой
        _shadow = new Shadow(this) { CacheMode = new BitmapCache { SnapsToDevicePixels = false } };
        _rest = new RestPaint(this) { CacheMode = new BitmapCache { SnapsToDevicePixels = false } };
        _children = new VisualCollection(this) { _shadow, _rest, _clip, _tint, _under, _paint };
        // свойства самой капли — только после детей: смена IsHitTestVisible уже обходит дочерние элементы
        IsHitTestVisible = false;
        RenderTransformOrigin = new Point(0.5, 0.5);
        Loaded += (_, _) => Attach();
        Unloaded += (_, _) => Detach();
        LayoutUpdated += (_, _) => Sync();
        SizeChanged += (_, _) => ShapeChanged();
    }

    // до конца конструктора WPF может спросить детей (при установке свойств) — их ещё нет
    protected override int VisualChildrenCount => _children?.Count ?? 0;
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

    /// <summary>Капля — только у жидкого стекла; у матового и плоского вместо неё плашка (см. описание класса).</summary>
    public bool Liquid => _owner is null or { Mode: GlassMode.Liquid };

    /// <summary>
    /// Линза и тень есть только у жидкого стекла: у матового и плоского — одна плашка поверх панели (её копию сцены
    /// панель уже показала под ней). Подложка панели и её яркость берутся у самой панели.
    /// </summary>
    private void UpdateLens()
    {
        _shadow.Visibility = Liquid ? Visibility.Visible : Visibility.Collapsed;
        ApplyMotion();
        if (_owner is not null)
        {
            _tint.Fill = _owner.Tint;
            if (_refraction is not null) _refraction.Brightness = Math.Min(1.15, _owner.EffectiveBrightness * 1.12);
        }
        _under.Fill = Under;
        _lastBox = Rect.Empty;
        Repaint();
        Sync();
    }

    private void Repaint()
    {
        _paint?.InvalidateVisual();
        _rest?.InvalidateVisual();
        _shadow?.InvalidateVisual();
    }

    /// <summary>
    /// Плашка ↔ линза. Только прозрачность готовых (кэшированных) слоёв — перерисовки нет. Линза совсем убирается, пока
    /// капля стоит: тогда и копия сцены под ней не считается. Без жидкого стекла — одна плашка (рисует <see cref="_paint"/>).
    /// </summary>
    private void ApplyMotion()
    {
        if (_children is null) return; // ещё в конструкторе
        var liquid = Liquid;
        var m = liquid ? Math.Clamp(Motion, 0, 1) : 0;
        var lens = liquid && m > 0.01;
        var lensVis = lens ? Visibility.Visible : Visibility.Collapsed;
        var wasOff = _clip.Visibility != Visibility.Visible;
        _clip.Visibility = _tint.Visibility = lensVis;
        _under.Visibility = lens && Under is not null ? Visibility.Visible : Visibility.Collapsed;
        _clip.Opacity = _tint.Opacity = _under.Opacity = m;
        // без жидкого стекла _paint — сама плашка и видна всегда; с жидким — блики линзы, вместе с ней
        _paint.Visibility = !liquid || lens ? Visibility.Visible : Visibility.Collapsed;
        _paint.Opacity = liquid ? m : 1;
        _rest.Visibility = liquid && m < 0.99 ? Visibility.Visible : Visibility.Collapsed;
        _rest.Opacity = 1 - m;
        if (lens && wasOff) { _lastBox = Rect.Empty; Sync(); }
    }

    private void ShapeChanged()
    {
        var w = ActualWidth; var h = ActualHeight;
        if (w <= 0 || h <= 0) return;
        var r = Math.Min(Corner, Math.Min(w, h) / 2);
        // скругление линзы делает шейдер со сглаживанием; без шейдера — маска (не Clip: он не сглаживается)
        _clip.OpacityMask = _refraction is null ? Glass.RoundMask(w, h, r) : null;
        _tint.RadiusX = _tint.RadiusY = _under.RadiusX = _under.RadiusY = r;
        if (_refraction is not null) { _refraction.Size = new Size(w, h); _refraction.Radius = r; }
        _lastBox = Rect.Empty;
        Repaint();
        Sync();
    }

    /// <summary>Увеличение линзы: под каплей видно чуть меньший кусок сцены, растянутый на всю каплю.</summary>
    private const double Magnify = 1.1;

    /// <summary>
    /// Ставит окно копии сцены под каплю. Капля ездит трансформациями (пружины), раскладка при этом не идёт,
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
        // увеличение от центра капли
        var bw = box.Width / Magnify; var bh = box.Height / Magnify;
        box = new Rect(box.X + (box.Width - bw) / 2, box.Y + (box.Height - bh) / 2, bw, bh);
        if (box == _lastBox) return;
        _lastBox = box;
        _brush.Viewbox = box;
    }

    // ───────────── рисунок капли ─────────────

    /// <summary>
    /// Лёгкая тень под каплей (стекло почти прозрачное, тень только отделяет его от панели): стопка расширяющихся
    /// скруглённых фигур с убывающей прозрачностью. Неподвижна относительно капли — кэшируется картинкой; растяжение
    /// капли её только масштабирует. В светлом оформлении ещё слабее.
    /// </summary>
    private sealed class Shadow : FrameworkElement
    {
        private readonly Droplet _d;
        public Shadow(Droplet d) { _d = d; IsHitTestVisible = false; }

        protected override void OnRender(DrawingContext dc)
        {
            var w = ActualWidth; var h = ActualHeight;
            if (w < 2 || h < 2) return;
            var r = Math.Min(_d.Corner, Math.Min(w, h) / 2);
            var k = _d._owner?.Light == true ? 0.5 : 1.0;
            const int steps = 8;
            for (var i = steps; i >= 1; i--)
            {
                var g = i * 1.6; // на сколько тень шире капли
                var a = k * 0.05 * (1 - (i - 1) / (double)steps);
                var rect = new Rect(-g / 2 + 1, 4 - g / 2 + 2, w + g - 2, h + g - 2);
                dc.DrawRoundedRectangle(new SolidColorBrush(Color.FromArgb((byte)Math.Round(a * 255), 4, 8, 20)), null, rect, r + g / 2, r + g / 2);
            }
        }
    }

    /// <summary>
    /// Капля в покое (жидкое стекло): тихая светлая плашка, блик по верхней кромке, тонкий контур и радужная кромка.
    /// Значок и подпись пункта лежат поверх и не искажаются.
    /// </summary>
    private sealed class RestPaint : FrameworkElement
    {
        private readonly Droplet _d;
        public RestPaint(Droplet d) { _d = d; IsHitTestVisible = false; }

        protected override void OnRender(DrawingContext dc)
        {
            var w = ActualWidth; var h = ActualHeight;
            if (w < 2 || h < 2) return;
            var r = Math.Min(_d.Corner, Math.Min(w, h) / 2);
            var danger = Math.Clamp(_d.Danger, 0, 1);
            var light = _d._owner?.Light == true;
            var shape = new RectangleGeometry(new Rect(0, 0, w, h), r, r);
            Color F(double a) => Color.FromArgb(Paint.A(a), 255, 255, 255);
            var fill = new LinearGradientBrush(
                Paint.Mix(F(light ? 0.66 : 0.13), Color.FromArgb(Paint.A(0.62), 226, 72, 60), danger),
                Paint.Mix(F(light ? 0.56 : 0.09), Color.FromArgb(Paint.A(0.58), 214, 58, 46), danger), 90);
            dc.DrawGeometry(fill, null, shape);
            dc.PushClip(shape);
            Paint.Ring(dc, w, h, r, 0.8, 1.0, Paint.Diagonal(Colors.White, light ? 0.9 : 0.5, topLeft: true));
            dc.Pop();
            var rim = light && danger < 0.5 ? Color.FromArgb(Paint.A(0.10), 10, 20, 40) : Color.FromArgb(Paint.A(0.18), 255, 255, 255);
            Paint.Ring(dc, w, h, r, 0.5, 1, new SolidColorBrush(rim));
            Paint.ConicRim(dc, w, h, r, 0.85 * (1 - danger));
        }
    }

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
            if (_d.Liquid) DrawDrop(dc, w, h, r, danger, light, shape);
            else DrawPill(dc, w, h, r, danger, light, shape);
        }

        /// <summary>
        /// Плашка вместо капли (жидкое стекло выключено): ровная полупрозрачная заливка и тонкий контур. Ни линзы, ни
        /// бликов, ни радужной кромки; на тёмном стекле светлая, на светлом — лёгкая тень цвета текста.
        /// </summary>
        private static void DrawPill(DrawingContext dc, double w, double h, double r, double danger, bool light, Geometry shape)
        {
            var baseTop = light ? Color.FromArgb(A(0.10), 16, 28, 52) : Color.FromArgb(A(0.16), 255, 255, 255);
            var baseBottom = light ? Color.FromArgb(A(0.08), 16, 28, 52) : Color.FromArgb(A(0.12), 255, 255, 255);
            var fill = new LinearGradientBrush(
                Mix(baseTop, Color.FromArgb(A(0.62), 226, 72, 60), danger),
                Mix(baseBottom, Color.FromArgb(A(0.58), 214, 58, 46), danger), 90);
            dc.DrawGeometry(fill, null, shape);
            var rim = light ? Color.FromArgb(A(0.06), 16, 28, 52) : Color.FromArgb(A(0.12), 255, 255, 255);
            Ring(dc, w, h, r, 0.5, 1, new SolidColorBrush(Mix(rim, Color.FromArgb(A(0.30), 255, 190, 180), danger)));
        }

        /// <summary>
        /// Капля жидкого стекла поверх линзы: почти без заливки, вся форма — в бликах по кромке (как у Apple: свет
        /// ложится на край, середина прозрачная), светлый серп снизу внутри и слабая радужная кромка.
        /// </summary>
        private static void DrawDrop(DrawingContext dc, double w, double h, double r, double danger, bool light, Geometry shape)
        {
            // едва заметная заливка под 160°: на светлом стекле чуть плотнее, иначе капли не видно совсем
            Color F(double a) => Color.FromArgb(A(a), 255, 255, 255);
            var fill = new LinearGradientBrush(new GradientStopCollection
            {
                new(Mix(F(light ? 0.26 : 0.07), Color.FromArgb(A(0.44), 255, 130, 118), danger), 0),
                new(Mix(F(light ? 0.10 : 0.00), Color.FromArgb(A(0.36), 214, 58, 46), danger), 0.52),
                new(Mix(F(light ? 0.18 : 0.03), Color.FromArgb(A(0.42), 255, 120, 108), danger), 1),
            }, new Point(0.33, 0.03), new Point(0.67, 0.97));
            dc.DrawGeometry(fill, null, shape);

            // блик по кромке: сверху слева яркий, снизу справа слабее; три тонких кольца — свет на самом крае, не пятном
            var hiTop = Mix(Color.FromRgb(255, 255, 255), Color.FromRgb(255, 235, 230), danger);
            var hiBottom = Mix(Color.FromRgb(255, 255, 255), Color.FromRgb(255, 200, 190), danger);
            double[] soft = { 0.70, 0.30, 0.10 };
            dc.PushClip(shape);
            for (var i = 0; i < soft.Length; i++)
            {
                Ring(dc, w, h, r, 0.8 + i * 0.9, 1.0, Diagonal(hiTop, soft[i], topLeft: true));
                Ring(dc, w, h, r, 0.8 + i * 0.9, 1.0, Diagonal(hiBottom, 0.55 * soft[i], topLeft: false));
            }
            dc.Pop();

            // светлый серп снизу внутри — свет, прошедший сквозь каплю (пересечение фигур: край сглажен)
            Spot(dc, shape, new Point(w * 0.56, h * 1.04), w * 0.44, h * 0.26, Color.FromArgb(A(light ? 0.10 : 0.14), 225, 240, 255));

            // тонкий контур: светлый на тёмном стекле, тёмный штрих на светлом
            var rim = light && danger < 0.5 ? Color.FromArgb(A(0.12), 10, 20, 40) : Mix(Color.FromArgb(A(0.22), 255, 255, 255), Color.FromArgb(A(0.40), 255, 190, 180), danger);
            Ring(dc, w, h, r, 0.5, 1, new SolidColorBrush(rim));

            // радужная кромка (дисперсия на краю линзы; утверждена 2026-10-17)
            ConicRim(dc, w, h, r, 1 - danger);
        }

        /// <summary>Мягкое пятно света (радиальный градиент) внутри формы капли.</summary>
        private static void Spot(DrawingContext dc, Geometry shape, Point c, double rx, double ry, Color color)
        {
            var brush = new RadialGradientBrush(color, Color.FromArgb(0, color.R, color.G, color.B))
            {
                MappingMode = BrushMappingMode.Absolute, Center = c, GradientOrigin = c, RadiusX = rx, RadiusY = ry,
            };
            dc.DrawGeometry(brush, null, new CombinedGeometry(GeometryCombineMode.Intersect, shape, new EllipseGeometry(c, rx, ry)));
        }

        /// <summary>Кольцо по скруглённому прямоугольнику, отступив <paramref name="inset"/> внутрь.</summary>
        internal static void Ring(DrawingContext dc, double w, double h, double r, double inset, double thickness, Brush brush)
        {
            if (w - 2 * inset <= 0 || h - 2 * inset <= 0) return;
            var ri = Math.Max(0, r - inset);
            dc.DrawRoundedRectangle(null, new Pen(brush, thickness), new Rect(inset, inset, w - 2 * inset, h - 2 * inset), ri, ri);
        }

        /// <summary>Свет по диагонали: от угла к середине, дальше прозрачно (так ложится inset-тень со сдвигом 2 px).</summary>
        internal static Brush Diagonal(Color c, double a, bool topLeft)
        {
            var on = Color.FromArgb(A(a), c.R, c.G, c.B);
            var off = Color.FromArgb(0, c.R, c.G, c.B);
            return topLeft
                ? new LinearGradientBrush(new GradientStopCollection { new(on, 0), new(on, 0.18), new(off, 0.55) }, new Point(0, 0), new Point(1, 1))
                : new LinearGradientBrush(new GradientStopCollection { new(off, 0.45), new(on, 0.82), new(on, 1) }, new Point(0, 0), new Point(1, 1));
        }

        // радужная кромка по кругу (утверждена 2026-10-17, образец «Капли жидкого стекла»): голубой → сиреневый → розовый →
        // янтарный → салатовый → голубой, как спектр на краю линзы
        private static readonly (double At, Color C)[] Conic =
        {
            (0.00, Color.FromArgb(150, 110, 200, 255)), (0.20, Color.FromArgb(150, 170, 140, 255)), (0.40, Color.FromArgb(150, 255, 110, 200)),
            (0.60, Color.FromArgb(140, 255, 190, 110)), (0.80, Color.FromArgb(130, 160, 235, 150)), (1.00, Color.FromArgb(150, 110, 200, 255)),
        };

        private static readonly Dictionary<uint, Pen> Pens = new();

        /// <summary>
        /// Круговой градиент по кромке: WPF его не умеет, поэтому кромка режется на короткие отрезки, и у каждого свой
        /// цвет по углу от центра капли (как у conic-gradient). Перья запоминаются по цвету.
        /// </summary>
        internal static void ConicRim(DrawingContext dc, double w, double h, double r, double strength = 1)
        {
            if (strength <= 0.02) return;
            const double inset = 0.75, thick = 1.2;
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
                c = Quant(Color.FromArgb((byte)Math.Round(c.A * Math.Clamp(strength, 0, 1)), c.R, c.G, c.B));
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

        internal static byte A(double a) => (byte)Math.Round(Math.Clamp(a, 0, 1) * 255);

        internal static Color Mix(Color a, Color b, double t) => t <= 0 ? a : t >= 1 ? b : Color.FromArgb(
            (byte)(a.A + (b.A - a.A) * t), (byte)(a.R + (b.R - a.R) * t), (byte)(a.G + (b.G - a.G) * t), (byte)(a.B + (b.B - a.B) * t));
    }
}
