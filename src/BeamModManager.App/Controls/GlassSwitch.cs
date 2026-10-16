using System.Windows;
using System.Windows.Media;
using BeamModManager.App.Motion;

namespace BeamModManager.App.Controls;

/// <summary>
/// Переключатель в духе жидкого стекла iOS 26 (решено 2026-10-16 вместо кружка, который просто перепрыгивал):
/// <list type="bullet">
/// <item>дорожка-капсула; её цвет плавно перетекает от «выключено» к голубому «включено»;</item>
/// <item>бегунок — белая таблетка шире, чем выше;</item>
/// <item>при нажатии бегунок «поднимается»: подрастает и становится прозрачной стеклянной линзой (сквозь неё видна дорожка,
///   кромка-блик, радужный край, блик сверху слева);</item>
/// <item>при переключении бегунок едет пружиной с лёгким перелётом, по ходу вытягивается, как капля, и, встав на место,
///   снова становится белым.</item>
/// </list>
/// Рисуется сам (несколько фигур) и не ловит мышь: нажатие получает сам флажок шаблона, сюда приходят IsOn и Pressed.
/// </summary>
public sealed class GlassSwitch : FrameworkElement
{
    public static readonly DependencyProperty IsOnProperty = DependencyProperty.Register(
        nameof(IsOn), typeof(bool?), typeof(GlassSwitch), new PropertyMetadata(false, (d, e) => ((GlassSwitch)d).OnChanged(e.NewValue as bool? == true)));
    public static readonly DependencyProperty PressedProperty = DependencyProperty.Register(
        nameof(Pressed), typeof(bool), typeof(GlassSwitch), new PropertyMetadata(false, (d, e) => ((GlassSwitch)d).OnPressed((bool)e.NewValue)));

    /// <summary>Цвет выключенной дорожки из палитры (DynamicResource TrackOff): смена оформления перерисовывает переключатель.</summary>
    public static readonly DependencyProperty OffBrushProperty = DependencyProperty.Register(
        nameof(OffBrush), typeof(Brush), typeof(GlassSwitch), new FrameworkPropertyMetadata(null, FrameworkPropertyMetadataOptions.AffectsRender));

    public bool? IsOn { get => (bool?)GetValue(IsOnProperty); set => SetValue(IsOnProperty, value); }
    public Brush? OffBrush { get => (Brush?)GetValue(OffBrushProperty); set => SetValue(OffBrushProperty, value); }
    public bool Pressed { get => (bool)GetValue(PressedProperty); set => SetValue(PressedProperty, value); }

    public const double TrackW = 56, TrackH = 30, ThumbW = 34, ThumbH = 24, Inset = 3;

    private readonly SpringDriver _drv = new();
    // место бегунка 0..1: пружина с лёгким перелётом, ≈0,35 с; «стекло» 0..1 — быстрее и без отскока
    private readonly Spring _pos = new(0.38, 0.22, 0);
    private readonly Spring _glass = new(0.26, 0, 0);
    private bool _landing;

    public GlassSwitch()
    {
        IsHitTestVisible = false;
        Width = TrackW;
        Height = TrackH;
        _drv.Add(_pos, _ => OnFrame());
        _drv.Add(_glass, _ => InvalidateVisual());
    }

    private void OnChanged(bool on)
    {
        // до показа — сразу на место (окно открылось, настройки прочитаны); дальше — с движением
        if (!IsLoaded) { _pos.Snap(on ? 1 : 0); InvalidateVisual(); return; }
        _drv.To(_pos, on ? 1 : 0);
        _landing = true;
        if (!SpringDriver.ReducedMotion) _drv.To(_glass, 1); // в пути — стеклянная линза
        InvalidateVisual();
    }

    private void OnPressed(bool pressed)
    {
        if (pressed) _drv.To(_glass, 1);
        else if (!_landing) _drv.To(_glass, 0);
    }

    /// <summary>Шаг пружины места: почти доехал (и не держат мышью) — бегунок снова становится белым.</summary>
    private void OnFrame()
    {
        var target = IsOn == true ? 1 : 0;
        if (_landing && Math.Abs(_pos.Value - target) < 0.08)
        {
            _landing = false;
            if (!Pressed) _drv.To(_glass, 0);
        }
        InvalidateVisual();
    }

    protected override Size MeasureOverride(Size available) => new(TrackW, TrackH);

    protected override void OnRender(DrawingContext dc)
    {
        var p = Math.Clamp(_pos.Value, -0.15, 1.15); // перелёт пружины чуть заходит за края — в пределах дорожки
        var g = Math.Clamp(_glass.Value, 0, 1);
        // ── дорожка ──
        var off = OffBrush is SolidColorBrush tb ? tb.Color : Color.FromArgb(0x38, 255, 255, 255);
        var light = off.R < 0x80; // в светлом оформлении выключенная дорожка — тёмная полупрозрачная
        var on = light ? Color.FromRgb(0x5E, 0x9B, 0xF0) : Color.FromRgb(0xA8, 0xCF, 0xFF);
        var onLow = light ? Color.FromRgb(0x3F, 0x80, 0xE0) : Color.FromRgb(0x8F, 0xBF, 0xFF);
        var k = Math.Clamp(p, 0, 1);
        var track = new Rect(0, 0, TrackW, TrackH);
        var trackFill = new LinearGradientBrush(Mix(off, on, k), Mix(off, onLow, k), 90);
        dc.DrawRoundedRectangle(trackFill, null, track, TrackH / 2, TrackH / 2);
        // дорожка утоплена: тень сверху внутри и тонкий светлый край снизу
        dc.DrawRoundedRectangle(new LinearGradientBrush(Color.FromArgb(light ? (byte)0x14 : (byte)0x30, 0, 0, 10), Color.FromArgb(0, 0, 0, 10), 90),
            null, track, TrackH / 2, TrackH / 2);
        dc.DrawRoundedRectangle(null, new Pen(new LinearGradientBrush(Color.FromArgb(0, 255, 255, 255), Color.FromArgb(light ? (byte)0x40 : (byte)0x26, 255, 255, 255), 90), 1),
            new Rect(0.5, 0.5, TrackW - 1, TrackH - 1), TrackH / 2 - 0.5, TrackH / 2 - 0.5);

        // ── бегунок ──
        var stretch = Math.Min(0.24, Math.Abs(_pos.Velocity) * 0.045); // вытягивается по ходу, как капля
        var grow = 1 + 0.26 * g;                                        // поднятый (стеклянный) — крупнее дорожки
        var tw = ThumbW * grow * (1 + stretch);
        var th = ThumbH * grow * (1 - stretch * 0.35);
        var cx = Inset + ThumbW / 2 + p * (TrackW - ThumbW - 2 * Inset);
        var cy = TrackH / 2;
        var thumb = new Rect(cx - tw / 2, cy - th / 2, tw, th);
        var tr = th / 2;
        var shape = new RectangleGeometry(thumb, tr, tr);

        // тень: мягче и светлее, когда бегунок стеклянный
        for (var i = 4; i >= 1; i--)
        {
            var e = i * 1.4;
            var a = (light ? 0.10 : 0.16) * (1 - (i - 1) / 4.0) * (1 - 0.5 * g);
            dc.DrawRoundedRectangle(new SolidColorBrush(Color.FromArgb((byte)(a * 255), 4, 10, 24)), null,
                new Rect(thumb.X - e / 2, thumb.Y - e / 2 + 2, thumb.Width + e, thumb.Height + e), tr + e / 2, tr + e / 2);
        }

        // заливка: белая таблетка → прозрачное стекло, сквозь которое видна (чуть увеличенная, светлее) дорожка
        var white = 1 - 0.86 * g;
        var under = Mix(off, on, Math.Clamp((cx - TrackH / 2) / (TrackW - TrackH), 0, 1));
        dc.DrawGeometry(new SolidColorBrush(Color.FromArgb((byte)(255 * g * 0.55), Lift(under).R, Lift(under).G, Lift(under).B)), null, shape);
        dc.DrawGeometry(new LinearGradientBrush(Color.FromArgb((byte)(255 * white), 255, 255, 255), Color.FromArgb((byte)(255 * white), 0xEE, 0xF3, 0xFA), 90), null, shape);

        // стекло: блик сверху слева, светлый серп снизу, яркая кромка и радужный край
        if (g > 0.02)
        {
            Spot(dc, shape, new Point(thumb.X + tw * 0.32, thumb.Y + th * 0.28), tw * 0.34, th * 0.34, Color.FromArgb((byte)(255 * 0.55 * g), 255, 255, 255));
            Spot(dc, shape, new Point(thumb.X + tw * 0.56, thumb.Bottom + th * 0.05), tw * 0.42, th * 0.34, Color.FromArgb((byte)(255 * 0.30 * g), 225, 240, 255));
            var rim = new LinearGradientBrush(new GradientStopCollection
            {
                new(Color.FromArgb((byte)(255 * 0.95 * g), 255, 255, 255), 0),
                new(Color.FromArgb((byte)(255 * 0.25 * g), 255, 255, 255), 0.5),
                new(Color.FromArgb((byte)(255 * 0.60 * g), 255, 255, 255), 1),
            }, new Point(0, 0), new Point(1, 1));
            dc.DrawRoundedRectangle(null, new Pen(rim, 1.2), Deflate(thumb, 0.6), tr - 0.6, tr - 0.6);
            var prism = new LinearGradientBrush(new GradientStopCollection
            {
                new(Color.FromArgb((byte)(255 * 0.75 * g), 110, 200, 255), 0),
                new(Color.FromArgb(0, 255, 255, 255), 0.35),
                new(Color.FromArgb(0, 255, 255, 255), 0.65),
                new(Color.FromArgb((byte)(255 * 0.70 * g), 255, 120, 205), 1),
            }, new Point(0, 0.5), new Point(1, 0.5));
            dc.DrawRoundedRectangle(null, new Pen(prism, 1.1), Deflate(thumb, 1.6), tr - 1.6, tr - 1.6);
        }
        // тонкий контур у белого бегунка (на светлой дорожке иначе теряется край)
        dc.DrawRoundedRectangle(null, new Pen(new SolidColorBrush(Color.FromArgb((byte)(255 * 0.10 * (1 - g)), 10, 20, 40)), 0.8),
            Deflate(thumb, 0.4), tr - 0.4, tr - 0.4);
    }

    private static void Spot(DrawingContext dc, Geometry shape, Point c, double rx, double ry, Color color)
    {
        var brush = new RadialGradientBrush(color, Color.FromArgb(0, color.R, color.G, color.B))
        {
            MappingMode = BrushMappingMode.Absolute, Center = c, GradientOrigin = c, RadiusX = rx, RadiusY = ry,
        };
        dc.DrawGeometry(brush, null, new CombinedGeometry(GeometryCombineMode.Intersect, shape, new EllipseGeometry(c, rx, ry)));
    }

    private static Rect Deflate(Rect r, double d) => new(r.X + d, r.Y + d, Math.Max(0, r.Width - 2 * d), Math.Max(0, r.Height - 2 * d));

    /// <summary>Под линзой дорожка чуть светлее и плотнее (линза собирает свет).</summary>
    private static Color Lift(Color c) => Color.FromArgb(255, (byte)Math.Min(255, c.R + 30), (byte)Math.Min(255, c.G + 30), (byte)Math.Min(255, c.B + 30));

    private static Color Mix(Color a, Color b, double t) => Color.FromArgb(
        (byte)(a.A + (b.A - a.A) * t), (byte)(a.R + (b.R - a.R) * t), (byte)(a.G + (b.G - a.G) * t), (byte)(a.B + (b.B - a.B) * t));
}
