using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Animation;
using BeamModManager.App.Motion;

namespace BeamModManager.App.Controls;

/// <summary>
/// Значок пункта левой панели в два цвета (решено 2026-10-17): тонкий контур и лёгкая заливка внутри (16 %). У выбранного
/// пункта заливка плавно густеет до своей силы (<see cref="ActiveFill"/>): плитки «Модов» и ручки ползунков «Настроек»
/// становятся сплошными, круг «Обновлений» и линза «Поиска» — заметно плотнее. Так выбор виден и без капли (жидкое
/// стекло выключено). Рисуется сам (две фигуры) в квадрате 22 × 22, не ловит мышь.
/// </summary>
public sealed class NavGlyph : FrameworkElement
{
    public const double Idle = 0.16;

    public static readonly DependencyProperty DataProperty = DependencyProperty.Register(
        nameof(Data), typeof(Geometry), typeof(NavGlyph), new FrameworkPropertyMetadata(null, FrameworkPropertyMetadataOptions.AffectsRender));
    public static readonly DependencyProperty BrushProperty = DependencyProperty.Register(
        nameof(Brush), typeof(Brush), typeof(NavGlyph), new FrameworkPropertyMetadata(Brushes.White, FrameworkPropertyMetadataOptions.AffectsRender));
    public static readonly DependencyProperty ActiveProperty = DependencyProperty.Register(
        nameof(Active), typeof(bool?), typeof(NavGlyph), new PropertyMetadata(false, (d, e) => ((NavGlyph)d).OnActive(e.NewValue as bool? == true)));
    private static readonly DependencyProperty LevelProperty = DependencyProperty.Register(
        "Level", typeof(double), typeof(NavGlyph), new FrameworkPropertyMetadata(Idle, FrameworkPropertyMetadataOptions.AffectsRender));

    /// <summary>Заливка значка (замкнутые фигуры внутри контура). Задаётся на самом пункте: <c>c:NavGlyph.Fill</c>.</summary>
    public static readonly DependencyProperty FillProperty = DependencyProperty.RegisterAttached(
        "Fill", typeof(Geometry), typeof(NavGlyph), new FrameworkPropertyMetadata(null, FrameworkPropertyMetadataOptions.AffectsRender));
    /// <summary>Сила заливки у выбранного пункта (0..1). Задаётся на самом пункте: <c>c:NavGlyph.ActiveFill</c>.</summary>
    public static readonly DependencyProperty ActiveFillProperty = DependencyProperty.RegisterAttached(
        "ActiveFill", typeof(double), typeof(NavGlyph), new FrameworkPropertyMetadata(0.4, FrameworkPropertyMetadataOptions.AffectsRender, (d, _) => { if (d is NavGlyph g) g.OnActive(g.Active == true); }));

    public static Geometry? GetFill(DependencyObject d) => (Geometry?)d.GetValue(FillProperty);
    public static void SetFill(DependencyObject d, Geometry? v) => d.SetValue(FillProperty, v);
    public static double GetActiveFill(DependencyObject d) => (double)d.GetValue(ActiveFillProperty);
    public static void SetActiveFill(DependencyObject d, double v) => d.SetValue(ActiveFillProperty, v);

    public Geometry? Data { get => (Geometry?)GetValue(DataProperty); set => SetValue(DataProperty, value); }
    public Brush Brush { get => (Brush)GetValue(BrushProperty); set => SetValue(BrushProperty, value); }
    public bool? Active { get => (bool?)GetValue(ActiveProperty); set => SetValue(ActiveProperty, value); }
    private Geometry? FillData => GetFill(this);
    private double ActiveFillLevel => GetActiveFill(this);

    public NavGlyph()
    {
        IsHitTestVisible = false;
        Width = Height = 22;
        SnapsToDevicePixels = false;
    }

    private void OnActive(bool active)
    {
        var to = active ? Math.Clamp(ActiveFillLevel, 0, 1) : Idle;
        if (!IsLoaded || SpringDriver.ReducedMotion) { BeginAnimation(LevelProperty, null); SetValue(LevelProperty, to); return; }
        BeginAnimation(LevelProperty, new DoubleAnimation(to, TimeSpan.FromSeconds(0.3)) { EasingFunction = Anim.Smooth });
    }

    private Pen? _pen;
    private Brush? _penBrush;

    protected override void OnRender(DrawingContext dc)
    {
        var brush = Brush;
        if (FillData is { } fill)
        {
            dc.PushOpacity(Math.Clamp((double)GetValue(LevelProperty), 0, 1));
            dc.DrawGeometry(brush, null, fill);
            dc.Pop();
        }
        if (Data is { } data)
        {
            if (!ReferenceEquals(_penBrush, brush) || _pen is null)
            {
                _penBrush = brush;
                _pen = new Pen(brush, 1.7) { StartLineCap = PenLineCap.Round, EndLineCap = PenLineCap.Round, LineJoin = PenLineJoin.Round };
            }
            dc.DrawGeometry(null, _pen, data);
        }
    }
}
