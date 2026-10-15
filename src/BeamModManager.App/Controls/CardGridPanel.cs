using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Animation;
using BeamModManager.App.Motion;

namespace BeamModManager.App.Controls;

/// <summary>
/// Сетка карточек: 2–6 колонок, карточка 280–340 px, промежуток 20, пропорции 308 × 236 (DESIGN.md раздел 4).
/// В окне 1440 получается ровно 3 колонки по 308.
/// При смене фильтра и поиска карточки переезжают со старых мест на новые, новые проявляются (DESIGN_MOTION 3.4).
/// Сдвиг — свой TranslateTransform у контейнера карточки; подъём при наведении и выборе живёт внутри карточки и не мешает.
/// </summary>
public sealed class CardGridPanel : Panel
{
    /// <summary>
    /// Карточка рядом с видимой частью сетки (с запасом в экран сверху и снизу). Наследуется вниз по дереву карточки:
    /// картинку держат только такие карточки, иначе при сотнях модов в памяти лежали бы сотни декодированных фото.
    /// </summary>
    public static readonly DependencyProperty InViewProperty = DependencyProperty.RegisterAttached(
        "InView", typeof(bool), typeof(CardGridPanel), new FrameworkPropertyMetadata(false, FrameworkPropertyMetadataOptions.Inherits));

    public static bool GetInView(DependencyObject d) => (bool)d.GetValue(InViewProperty);
    public static void SetInView(DependencyObject d, bool v) => d.SetValue(InViewProperty, v);

    /// <summary>
    /// Сколько первых видимых карточек всплывает по одной при первом заполнении сетки (вход окна, DESIGN.md 6 «Один вход»);
    /// 0 — без входа. Наследуется: окно ставит его на список карточек ещё до того, как сетка создана.
    /// </summary>
    public static readonly DependencyProperty RiseProperty = DependencyProperty.RegisterAttached(
        "Rise", typeof(int), typeof(CardGridPanel), new FrameworkPropertyMetadata(0, FrameworkPropertyMetadataOptions.Inherits));

    public static int GetRise(DependencyObject d) => (int)d.GetValue(RiseProperty);
    public static void SetRise(DependencyObject d, int v) => d.SetValue(RiseProperty, v);

    private ScrollViewer? _scroll;

    public CardGridPanel()
    {
        Loaded += (_, _) =>
        {
            _scroll = FindScroll(this);
            if (_scroll is not null) _scroll.ScrollChanged += (_, _) => UpdateInView();
            UpdateInView();
        };
    }

    /// <summary>Двигать карточки при перестроении; в снимках сборочной машины окно выключает.</summary>
    public bool Animated { get; set; } = true;

    private static ScrollViewer? FindScroll(DependencyObject d)
    {
        for (var p = VisualTreeHelper.GetParent(d); p is not null; p = VisualTreeHelper.GetParent(p))
            if (p is ScrollViewer s) return s;
        return null;
    }

    /// <summary>Видимая часть сетки в её координатах с запасом <paramref name="pad"/>; без прокрутки — вся.</summary>
    private (double Top, double Bottom)? ViewBand(double pad)
    {
        if (_scroll is null) return null;
        try
        {
            var origin = TransformToAncestor(_scroll).Transform(new Point(0, 0)).Y; // где панель внутри видимой области
            return (-origin - pad, -origin + _scroll.ViewportHeight + pad);
        }
        catch (InvalidOperationException) { return null; }
    }

    private void UpdateInView()
    {
        if (_scroll is null) { foreach (UIElement c in InternalChildren) SetInView(c, true); return; }
        // с запасом в экран сверху и снизу
        if (ViewBand(_scroll.ViewportHeight) is not { } band) return;
        var itemH = Math.Round(ItemWidth * Ratio);
        for (var i = 0; i < InternalChildren.Count; i++)
        {
            var y = i / Columns * (itemH + Gap);
            SetInView(InternalChildren[i], y + itemH >= band.Top && y <= band.Bottom);
        }
    }

    public const double Gap = 20, MinItem = 280, MaxItem = 340, Ratio = 236.0 / 308.0;

    public int Columns { get; private set; } = 3;
    public double ItemWidth { get; private set; } = 308;

    public event EventHandler? ColumnsChanged;

    protected override Size MeasureOverride(Size available)
    {
        var w = double.IsInfinity(available.Width) ? 3 * 308 + 2 * Gap : available.Width;
        var cols = Math.Clamp((int)Math.Floor((w + Gap) / (MinItem + Gap)), 1, 6);
        var itemW = Math.Min(MaxItem, Math.Floor((w - Gap * (cols - 1)) / cols));
        var itemH = Math.Round(itemW * Ratio);
        if (cols != Columns) { Columns = cols; ColumnsChanged?.Invoke(this, EventArgs.Empty); }
        ItemWidth = itemW;
        foreach (UIElement c in InternalChildren) c.Measure(new Size(itemW, itemH));
        var n = InternalChildren.Count;
        var rows = (n + cols - 1) / cols;
        return new Size(w, rows == 0 ? 0 : rows * itemH + (rows - 1) * Gap);
    }

    // ───────────── перестроение (FLIP): где карточка стояла в прошлый раз ─────────────

    /// <summary>Прошлые места карточек, по самому моду: контейнер при смене фильтра может пересоздаться.</summary>
    private Dictionary<object, Point> _places = new();
    private bool _filled;
    private int _lastCols;
    private double _lastItemW;

    // длительности с пружиной (×1,6 к ощущаемой): переезд ≈0,47 с, «доезжание» после смены колонок ≈0,28 с
    private static readonly TimeSpan MoveTime = TimeSpan.FromSeconds(0.75);
    private static readonly TimeSpan SettleTime = TimeSpan.FromSeconds(0.45);
    private static readonly TimeSpan AppearTime = TimeSpan.FromSeconds(0.6);
    private static readonly TimeSpan RiseTime = TimeSpan.FromSeconds(0.8);
    private const double RiseFrom = 18, AppearFrom = 12;

    private static object KeyOf(UIElement c) => c is FrameworkElement { DataContext: { } d } ? d : c;

    protected override Size ArrangeOverride(Size final)
    {
        var itemW = ItemWidth;
        var itemH = Math.Round(itemW * Ratio);
        var n = InternalChildren.Count;
        var moving = Animated && !SpringDriver.ReducedMotion;
        var first = !_filled && n > 0;
        var colsChanged = _filled && Columns != _lastCols;
        // ширина карточки поменялась без смены колонок (тянут окно): просто встаём на места
        var resizedOnly = _filled && !colsChanged && Math.Abs(itemW - _lastItemW) > 0.5;
        var rise = first && moving ? GetRise(this) : 0;
        var band = (moving && _filled && !resizedOnly) || rise > 0 ? ViewBand(itemH) : null;
        var places = new Dictionary<object, Point>(n);

        for (var i = 0; i < n; i++)
        {
            var c = InternalChildren[i];
            var p = new Point(i % Columns * (itemW + Gap), i / Columns * (itemH + Gap));
            c.Arrange(new Rect(p.X, p.Y, itemW, itemH));
            var key = KeyOf(c);
            var had = _places.TryGetValue(key, out var old);
            places[key] = p;

            if (first)
            {
                if (i < rise && (band is not { } b0 || p.Y < b0.Bottom - itemH)) Rise(c, i);
                continue;
            }
            if (!_filled || !moving || resizedOnly) continue;
            bool Near(double y) => band is not { } b || (y + itemH >= b.Top && y <= b.Bottom);
            if (had)
            {
                if (old == p || !(Near(old.Y) || Near(p.Y))) continue;
                Slide(c, old, p, colsChanged ? SettleTime : MoveTime);
            }
            else if (!colsChanged && Near(p.Y)) Appear(c);
        }

        _places = places;
        if (n > 0) _filled = true;
        _lastCols = Columns;
        _lastItemW = itemW;
        Dispatcher.BeginInvoke(UpdateInView, System.Windows.Threading.DispatcherPriority.Loaded);
        return final;
    }

    /// <summary>
    /// Карточка уже стоит на новом месте; сдвигом возвращаем её туда, где её видно сейчас, и пружиной ведём к нулю.
    /// Прерванный переезд продолжается с того места, где карточка была.
    /// </summary>
    private static void Slide(UIElement c, Point old, Point now, TimeSpan time)
    {
        var t = Anim.Mover(c);
        var dx = old.X + t.X - now.X;
        var dy = old.Y + t.Y - now.Y;
        // анимация с явным началом действует сразу (кадра «уже на новом месте» нет); обычное значение — конечное (0),
        // поэтому снятая анимация никогда не оставит карточку сдвинутой
        t.X = 0;
        t.Y = 0;
        t.BeginAnimation(TranslateTransform.XProperty, new DoubleAnimation(dx, 0, time) { EasingFunction = Anim.Smooth });
        t.BeginAnimation(TranslateTransform.YProperty, new DoubleAnimation(dy, 0, time) { EasingFunction = Anim.Smooth });
    }

    /// <summary>Новая карточка (появилась по фильтру или поиску) проявляется с лёгким подъёмом.</summary>
    private static void Appear(UIElement c)
    {
        var t = Anim.Mover(c);
        t.Y = 0;
        c.Opacity = 1; // обычные значения конечные: анимации ниже действуют сразу с начала
        t.BeginAnimation(TranslateTransform.YProperty, new DoubleAnimation(AppearFrom, 0, MoveTime) { EasingFunction = Anim.Smooth });
        c.BeginAnimation(OpacityProperty, new DoubleAnimation(0, 1, AppearTime) { EasingFunction = Anim.Smooth });
    }

    /// <summary>Вход окна: карточки всплывают по одной с шагом 45 мс (доска: rise .8s, задержка 60 + 45 × n мс).</summary>
    private static void Rise(UIElement c, int i)
    {
        var delay = TimeSpan.FromMilliseconds(60 + 45 * i);
        var t = Anim.Mover(c);
        t.BeginAnimation(TranslateTransform.YProperty, Anim.Hold(RiseFrom, 0, delay, RiseTime, Anim.Lift));
        c.BeginAnimation(OpacityProperty, Anim.Hold(0, 1, delay, RiseTime, Anim.Smooth));
        t.Y = 0; // обычные значения конечные; до задержки анимация держит начало
        c.Opacity = 1;
    }
}
