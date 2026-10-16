using System.Windows;
using System.Windows.Media;

namespace BeamModManager.App.Controls;

/// <summary>
/// Кромка стеклянной панели, рисуется векторами под любой размер (без картинок, поэтому ей не нужно пересчитываться
/// после смены размера и ничего не «появляется заново» при переключении разделов):
/// <list type="bullet">
/// <item>блик жидкого стекла (<see cref="Specular"/>): тонкая яркая кромка, свет сверху слева, слабый отсвет снизу справа;</item>
/// <item>изморозь (<see cref="Frost"/>, решено 2026-10-16 вместо инея с зерном): мягкое светлое свечение от края внутрь,
///   гуще в углах, светлее сверху слева, тонкая светлая линия у самого края — как у <see cref="BeamModManager.Ui.Glass.GlassTextures.Frost"/>.</item>
/// </list>
/// Всё внутри скруглённой формы и сглажено (заливки фигур, а не обрезка). Элемент неподвижен — окно кэширует его картинкой.
/// </summary>
public sealed class GlassEdge : FrameworkElement
{
    public static readonly DependencyProperty CornerProperty = DependencyProperty.Register(
        nameof(Corner), typeof(double), typeof(GlassEdge), new FrameworkPropertyMetadata(26.0, FrameworkPropertyMetadataOptions.AffectsRender));
    public static readonly DependencyProperty FrostProperty = DependencyProperty.Register(
        nameof(Frost), typeof(double), typeof(GlassEdge), new FrameworkPropertyMetadata(0.0, FrameworkPropertyMetadataOptions.AffectsRender));
    public static readonly DependencyProperty ReachProperty = DependencyProperty.Register(
        nameof(Reach), typeof(double), typeof(GlassEdge), new FrameworkPropertyMetadata(34.0, FrameworkPropertyMetadataOptions.AffectsRender));
    public static readonly DependencyProperty SpecularProperty = DependencyProperty.Register(
        nameof(Specular), typeof(double), typeof(GlassEdge), new FrameworkPropertyMetadata(1.0, FrameworkPropertyMetadataOptions.AffectsRender));
    public static readonly DependencyProperty LensProperty = DependencyProperty.Register(
        nameof(Lens), typeof(double), typeof(GlassEdge), new FrameworkPropertyMetadata(30.0, FrameworkPropertyMetadataOptions.AffectsRender));

    public double Corner { get => (double)GetValue(CornerProperty); set => SetValue(CornerProperty, value); }
    /// <summary>Сила изморози, 0 — нет.</summary>
    public double Frost { get => (double)GetValue(FrostProperty); set => SetValue(FrostProperty, value); }
    /// <summary>Как далеко изморозь заходит от края, px.</summary>
    public double Reach { get => (double)GetValue(ReachProperty); set => SetValue(ReachProperty, value); }
    /// <summary>Сила блика по кромке (жидкое стекло — 1, матовое и плоское — 0).</summary>
    public double Specular { get => (double)GetValue(SpecularProperty); set => SetValue(SpecularProperty, value); }
    /// <summary>Ширина мягкого свечения под бликом, px (как у линзы стекла).</summary>
    public double Lens { get => (double)GetValue(LensProperty); set => SetValue(LensProperty, value); }

    public GlassEdge()
    {
        IsHitTestVisible = false;
        SnapsToDevicePixels = false;
    }

    private static readonly Color Ice = Color.FromRgb(232, 243, 255);

    protected override void OnRender(DrawingContext dc)
    {
        var w = ActualWidth; var h = ActualHeight;
        if (w < 4 || h < 4) return;
        var r = Math.Min(Corner, Math.Min(w, h) / 2);

        var frost = Math.Clamp(Frost, 0, 1.6);
        if (frost > 0.001) DrawFrost(dc, w, h, r, frost);
        var spec = Math.Clamp(Specular, 0, 1);
        if (spec > 0.001) DrawSpecular(dc, w, h, r, spec);
    }

    /// <summary>Свечение от края внутрь кольцами (плавно, без полос), сгущение в углах и тонкая светлая линия у края.</summary>
    private void DrawFrost(DrawingContext dc, double w, double h, double r, double k)
    {
        var reach = Math.Max(4, Math.Min(Reach, Math.Min(w, h) / 2 - 1));
        var steps = Math.Clamp((int)Math.Ceiling(reach / 2.5), 6, 24);
        var step = reach / steps;
        for (var i = 0; i < steps; i++)
        {
            var inset = (i + 0.5) * step;
            var t = 1 - inset / reach;
            var a = k * 0.5 * Math.Pow(t, 2.2);
            if (a < 0.004) continue;
            Ring(dc, w, h, r, inset, step + 0.6, Lit(a, 1.15, 0.72));
        }
        // в углах изморозь гуще: мягкое пятно от угла, только внутри формы (пересечение фигур — края сглажены)
        var cr = reach * 2.2;
        var shape = new RectangleGeometry(new Rect(0, 0, w, h), r, r);
        foreach (var (cx, cy, lit) in new[] { (0.0, 0.0, 1.15), (w, 0.0, 0.95), (0.0, h, 0.95), (w, h, 0.75) })
        {
            var brush = new RadialGradientBrush
            {
                MappingMode = BrushMappingMode.Absolute,
                Center = new Point(cx, cy), GradientOrigin = new Point(cx, cy), RadiusX = cr, RadiusY = cr,
                GradientStops =
                {
                    new GradientStop(WithA(Ice, k * 0.30 * lit), 0),
                    new GradientStop(WithA(Ice, k * 0.12 * lit), 0.45),
                    new GradientStop(WithA(Ice, 0), 1),
                },
            };
            var region = new CombinedGeometry(GeometryCombineMode.Intersect, shape, new EllipseGeometry(new Point(cx, cy), cr, cr));
            dc.DrawGeometry(brush, null, region);
        }
        // тонкая светлая линия у самого края
        Ring(dc, w, h, r, 1.2, 1.2, Lit(k * 0.30, 1.15, 0.7));
    }

    /// <summary>Блик жидкого стекла: яркая кромка сверху слева, слабая снизу справа, мягкое свечение внутрь.</summary>
    private void DrawSpecular(DrawingContext dc, double w, double h, double r, double k)
    {
        var glow = Math.Max(3, Lens * 0.35);
        for (var i = 0; i < 4; i++)
        {
            var inset = 2 + i * glow / 4;
            Ring(dc, w, h, r, inset, glow / 4 + 0.5, Lit(k * 0.05 * (1 - i / 4.0), 1, 0.5, white: true));
        }
        Ring(dc, w, h, r, 1.4, 1.2, Diagonal(k * 0.40, k * 0.16));
        Ring(dc, w, h, r, 0.6, 1.1, Diagonal(k * 0.90, k * 0.45));
    }

    /// <summary>Кольцо по скруглённому прямоугольнику, отступив <paramref name="inset"/> внутрь (середина штриха).</summary>
    private static void Ring(DrawingContext dc, double w, double h, double r, double inset, double thickness, Brush brush)
    {
        if (w - 2 * inset <= 0 || h - 2 * inset <= 0) return;
        var ri = Math.Max(0, r - inset);
        dc.DrawRoundedRectangle(null, new Pen(brush, thickness), new Rect(inset, inset, w - 2 * inset, h - 2 * inset), ri, ri);
    }

    /// <summary>Цвет изморози с прозрачностью, светлее сверху слева (от <paramref name="tl"/> до <paramref name="br"/>).</summary>
    private static Brush Lit(double a, double tl, double br, bool white = false)
    {
        var c = white ? Colors.White : Ice;
        return new LinearGradientBrush(WithA(c, a * tl), WithA(c, a * br), new Point(0, 0), new Point(1, 1));
    }

    /// <summary>Блик по диагонали: яркий сверху слева, гаснет к середине, слабый отсвет снизу справа.</summary>
    private static Brush Diagonal(double main, double back) => new LinearGradientBrush(new GradientStopCollection
    {
        new(WithA(Colors.White, main), 0),
        new(WithA(Colors.White, main * 0.55), 0.22),
        new(WithA(Colors.White, main * 0.08), 0.5),
        new(WithA(Colors.White, back * 0.3), 0.72),
        new(WithA(Colors.White, back), 1),
    }, new Point(0, 0), new Point(1, 1));

    private static Color WithA(Color c, double a) => Color.FromArgb((byte)Math.Round(Math.Clamp(a, 0, 1) * 255), c.R, c.G, c.B);
}
