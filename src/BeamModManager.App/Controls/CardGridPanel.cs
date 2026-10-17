using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Media;
using System.Windows.Media.Animation;
using BeamModManager.App.Motion;

namespace BeamModManager.App.Controls;

/// <summary>
/// Сетка карточек: 2–6 колонок, карточка 280–340 px, промежуток 20, пропорции 308 × 236 (DESIGN.md раздел 4).
/// В окне 1440 получается ровно 3 колонки по 308.
/// При смене фильтра и поиска карточки переезжают со старых мест на новые, новые проявляются (DESIGN_MOTION 3.4).
/// Сдвиг — свой TranslateTransform у контейнера карточки; подъём при наведении и выборе живёт внутри карточки и не мешает.
/// <para>
/// Виртуализация: карточки (стекло, тени, картинка) создаются только для видимой части сетки с запасом в экран сверху и
/// снизу, остальные — по мере прокрутки. Высота сетки при этом полная, прокрутка честная. Без этого 300 модов создавали
/// 300 карточек при каждом поиске (секунды), а 5000 — десятки секунд при запуске и сотни мегабайт памяти.
/// </para>
/// </summary>
public sealed class CardGridPanel : VirtualizingPanel
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
    /// <summary>Какие карточки (по номеру в списке) созданы сейчас.</summary>
    private int _first, _last = -1;

    public CardGridPanel()
    {
        Loaded += (_, _) =>
        {
            _scroll = FindScroll(this);
            if (_scroll is not null) _scroll.ScrollChanged += (_, _) => OnScrolled();
            InvalidateMeasure(); // до этого прокрутки не знали: создали карточки на первый экран
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

    /// <summary>Видимая часть сетки в её координатах с запасом <paramref name="pad"/>; без прокрутки — неизвестно.</summary>
    private (double Top, double Bottom)? ViewBand(double pad)
    {
        if (_scroll is null || !IsLoaded) return null;
        try
        {
            var origin = TransformToAncestor(_scroll).Transform(new Point(0, 0)).Y; // где панель внутри видимой области
            return (-origin - pad, -origin + _scroll.ViewportHeight + pad);
        }
        catch (InvalidOperationException) { return null; }
    }

    private double ItemHeight => HeightOf(ItemWidth);

    /// <summary>Высота карточки: фото в пропорции <see cref="PhotoRatio"/> и подпись под ним (<see cref="ExtraHeight"/>).</summary>
    private double HeightOf(double itemW) => Math.Round(itemW * PhotoRatio) + ExtraHeight;

    /// <summary>Номера карточек, которые должны быть созданы: видимые ряды и по экрану сверху и снизу.</summary>
    private (int First, int Last) WantedRange(int count)
    {
        if (count == 0) return (0, -1);
        var rowH = ItemHeight + Gap;
        if (ViewBand(_scroll?.ViewportHeight ?? 900) is not { } band)
            return (0, Math.Min(count - 1, Columns * 8 - 1)); // ещё не знаем, где окно: первые восемь рядов
        var firstRow = Math.Max(0, (int)Math.Floor(band.Top / rowH));
        var lastRow = Math.Max(firstRow, (int)Math.Floor(Math.Max(0, band.Bottom) / rowH));
        return (Math.Min(count - 1, firstRow * Columns), Math.Min(count - 1, (lastRow + 1) * Columns - 1));
    }

    private void OnScrolled()
    {
        // прокрутили за пределы созданного — досоздаём; внутри запаса — только отмечаем, кто на виду
        var count = ItemsControl.GetItemsOwner(this)?.Items.Count ?? 0;
        var (f, l) = WantedRange(count);
        if (f < _first || l > _last) InvalidateMeasure();
        else UpdateInView();
    }

    private void UpdateInView()
    {
        var gen = ItemContainerGenerator;
        if (gen is null) return;
        var band = ViewBand(_scroll?.ViewportHeight ?? 0);
        var itemH = ItemHeight;
        for (var i = 0; i < InternalChildren.Count; i++)
        {
            var index = gen.IndexFromGeneratorPosition(new GeneratorPosition(i, 0));
            var y = index / Columns * (itemH + Gap);
            SetInView(InternalChildren[i], band is not { } b || (y + itemH >= b.Top && y <= b.Bottom));
        }
    }

    /// <summary>Прокрутить так, чтобы карточка с этим номером была видна (её контейнера может ещё не быть).</summary>
    public void BringIndexIntoView(int index, double topReserve)
    {
        if (_scroll is null || index < 0) return;
        try
        {
            var origin = TransformToAncestor(_scroll).Transform(new Point(0, 0)).Y + _scroll.VerticalOffset; // верх сетки в прокрутке
            var y = origin + index / Columns * (ItemHeight + Gap);
            var top = _scroll.VerticalOffset;
            if (y - topReserve < top) _scroll.ScrollToVerticalOffset(y - topReserve);
            else if (y + ItemHeight + Gap > top + _scroll.ViewportHeight) _scroll.ScrollToVerticalOffset(y + ItemHeight + Gap - _scroll.ViewportHeight);
        }
        catch (InvalidOperationException) { }
    }

    public const double Gap = 20, MinItem = 280, MaxItem = 340, Ratio = 236.0 / 308.0;

    /// <summary>Пропорция фото: у гаража фото на всю карточку (<see cref="Ratio"/>), у поиска ниже — под ним подпись и кнопки.</summary>
    public static readonly DependencyProperty PhotoRatioProperty = DependencyProperty.Register(nameof(PhotoRatio), typeof(double), typeof(CardGridPanel),
        new FrameworkPropertyMetadata(Ratio, FrameworkPropertyMetadataOptions.AffectsMeasure));
    public double PhotoRatio { get => (double)GetValue(PhotoRatioProperty); set => SetValue(PhotoRatioProperty, value); }

    /// <summary>Постоянная часть высоты под фото (подпись и кнопки карточки поиска); у гаража 0.</summary>
    public static readonly DependencyProperty ExtraHeightProperty = DependencyProperty.Register(nameof(ExtraHeight), typeof(double), typeof(CardGridPanel),
        new FrameworkPropertyMetadata(0.0, FrameworkPropertyMetadataOptions.AffectsMeasure));
    public double ExtraHeight { get => (double)GetValue(ExtraHeightProperty); set => SetValue(ExtraHeightProperty, value); }

    public int Columns { get; private set; } = 3;
    public double ItemWidth { get; private set; } = 308;

    public event EventHandler? ColumnsChanged;

    protected override Size MeasureOverride(Size available)
    {
        var w = double.IsInfinity(available.Width) ? 3 * 308 + 2 * Gap : available.Width;
        var cols = Math.Clamp((int)Math.Floor((w + Gap) / (MinItem + Gap)), 1, 6);
        var itemW = Math.Min(MaxItem, Math.Floor((w - Gap * (cols - 1)) / cols));
        var itemH = HeightOf(itemW);
        if (cols != Columns) { Columns = cols; ColumnsChanged?.Invoke(this, EventArgs.Empty); }
        ItemWidth = itemW;

        var count = ItemsControl.GetItemsOwner(this)?.Items.Count ?? 0;
        var (first, last) = WantedRange(count);
        Realize(first, last);
        foreach (UIElement c in InternalChildren) c.Measure(new Size(itemW, itemH));
        var rows = (count + cols - 1) / cols;
        return new Size(w, rows == 0 ? 0 : rows * itemH + (rows - 1) * Gap);
    }

    /// <summary>Создать карточки first..last (по номеру в списке) и убрать остальные. Порядок детей совпадает с порядком в списке.</summary>
    private void Realize(int first, int last)
    {
        _ = InternalChildren; // без этого обращения генератор у панели ещё не готов
        var gen = ItemContainerGenerator;
        if (gen is null) return;
        // убрать созданные вне диапазона
        for (var i = InternalChildren.Count - 1; i >= 0; i--)
        {
            var pos = new GeneratorPosition(i, 0);
            var index = gen.IndexFromGeneratorPosition(pos);
            if (index >= first && index <= last) continue;
            gen.Remove(pos, 1);
            RemoveInternalChildRange(i, 1);
        }
        _first = first;
        _last = last;
        if (last < first) return;
        var start = gen.GeneratorPositionFromIndex(first);
        var child = start.Offset == 0 ? start.Index : start.Index + 1;
        using (gen.StartAt(start, GeneratorDirection.Forward, allowStartAtRealizedItem: true))
            for (var index = first; index <= last; index++, child++)
            {
                if (gen.GenerateNext(out var isNew) is not UIElement el) break;
                if (!isNew) continue; // уже на месте
                if (child >= InternalChildren.Count) AddInternalChild(el); else InsertInternalChild(child, el);
                gen.PrepareItemContainer(el);
            }
    }

    /// <summary>Список менялся с прошлой раскладки (а не только прокрутка).</summary>
    private bool _itemsChanged;

    protected override void OnItemsChanged(object sender, ItemsChangedEventArgs args)
    {
        _itemsChanged = true;
        switch (args.Action)
        {
            case System.Collections.Specialized.NotifyCollectionChangedAction.Remove:
            case System.Collections.Specialized.NotifyCollectionChangedAction.Replace:
            case System.Collections.Specialized.NotifyCollectionChangedAction.Move:
                if (args.ItemUICount > 0) RemoveInternalChildRange(args.Position.Index, args.ItemUICount);
                break;
        }
        base.OnItemsChanged(sender, args);
    }

    // ───────────── перестроение (FLIP): где карточка стояла в прошлый раз ─────────────

    /// <summary>Прошлые места карточек, по самому моду: контейнер при смене фильтра может пересоздаться.</summary>
    private Dictionary<object, Point> _places = new();
    private bool _filled;
    private int _lastCols;
    private double _lastItemW;

    // длительности с пружиной (×1,6 к ощущаемой): переезд ≈0,31 с, «доезжание» после смены колонок ≈0,2 с
    // (2026-10-16: «ускорить анимации» — раньше 0,47 и 0,28 с)
    private static readonly TimeSpan MoveTime = TimeSpan.FromSeconds(0.5);
    private static readonly TimeSpan SettleTime = TimeSpan.FromSeconds(0.32);
    private static readonly TimeSpan AppearTime = TimeSpan.FromSeconds(0.4);
    private static readonly TimeSpan RiseTime = TimeSpan.FromSeconds(0.55);
    private const double RiseFrom = 18, AppearFrom = 12;

    private static object KeyOf(UIElement c) => c is FrameworkElement { DataContext: { } d } ? d : c;

    protected override Size ArrangeOverride(Size final)
    {
        var itemW = ItemWidth;
        var itemH = HeightOf(itemW);
        var n = InternalChildren.Count;
        var moving = Animated && !SpringDriver.ReducedMotion;
        var first = !_filled && n > 0;
        var colsChanged = _filled && Columns != _lastCols;
        // ширина карточки поменялась без смены колонок (тянут окно): просто встаём на места
        var resizedOnly = _filled && !colsChanged && Math.Abs(itemW - _lastItemW) > 0.5;
        var rise = first && moving ? GetRise(this) : 0;
        var band = (moving && _filled && !resizedOnly) || rise > 0 ? ViewBand(itemH) : null;
        var places = new Dictionary<object, Point>(n);

        var gen = ItemContainerGenerator;
        for (var i = 0; i < n; i++)
        {
            var c = InternalChildren[i];
            // место — по номеру карточки в списке: созданы не все, а только видимые
            var index = gen?.IndexFromGeneratorPosition(new GeneratorPosition(i, 0)) ?? i;
            if (index < 0) index = i;
            var p = new Point(index % Columns * (itemW + Gap), index / Columns * (itemH + Gap));
            c.Arrange(new Rect(p.X, p.Y, itemW, itemH));
            var key = KeyOf(c);
            var had = _places.TryGetValue(key, out var old);
            places[key] = p;

            if (first)
            {
                if (index < rise && (band is not { } b0 || p.Y < b0.Bottom - itemH)) Rise(c, index);
                continue;
            }
            if (!_filled || !moving || resizedOnly) continue;
            bool Near(double y) => band is not { } b || (y + itemH >= b.Top && y <= b.Bottom);
            if (had)
            {
                if (old == p || !(Near(old.Y) || Near(p.Y))) continue;
                Slide(c, old, p, colsChanged ? SettleTime : MoveTime);
            }
            // новая в списке (фильтр, поиск) — проявляется; просто досозданная при прокрутке — уже стоит на месте
            else if (!colsChanged && _itemsChanged && Near(p.Y)) Appear(c);
        }
        _itemsChanged = false;

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

    /// <summary>Вход окна: карточки всплывают по одной с шагом 30 мс (доска: rise .8s, задержка 60 + 45 × n мс; ускорено).</summary>
    private static void Rise(UIElement c, int i)
    {
        var delay = TimeSpan.FromMilliseconds(40 + 30 * i);
        var t = Anim.Mover(c);
        t.BeginAnimation(TranslateTransform.YProperty, Anim.Hold(RiseFrom, 0, delay, RiseTime, Anim.Lift));
        c.BeginAnimation(OpacityProperty, Anim.Hold(0, 1, delay, RiseTime, Anim.Smooth));
        t.Y = 0; // обычные значения конечные; до задержки анимация держит начало
        c.Opacity = 1;
    }
}
