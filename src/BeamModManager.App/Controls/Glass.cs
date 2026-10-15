using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Effects;
using System.Windows.Shapes;

namespace BeamModManager.App.Controls;

/// <summary>
/// Стеклянная панель: под ней живая копия сцены (<see cref="Source"/>), размытая, с подложкой, бликом и светлой кромкой.
/// Это режим «Матовое» из DESIGN.md 3.2; преломление по краям (шейдер) добавится поверх этого же элемента.
/// Сцена не должна содержать само стекло, иначе копия будет копировать саму себя.
/// </summary>
public class Glass : ContentControl
{
    public static readonly DependencyProperty SourceProperty = DependencyProperty.Register(
        nameof(Source), typeof(Visual), typeof(Glass), new PropertyMetadata(null, (d, _) => ((Glass)d).Sync()));

    public static readonly DependencyProperty CornerProperty = DependencyProperty.Register(
        nameof(Corner), typeof(double), typeof(Glass), new FrameworkPropertyMetadata(26.0, FrameworkPropertyMetadataOptions.AffectsRender, (d, _) => ((Glass)d).Sync()));

    public static readonly DependencyProperty BlurProperty = DependencyProperty.Register(
        nameof(Blur), typeof(double), typeof(Glass), new PropertyMetadata(14.0, (d, _) => ((Glass)d).Sync()));

    public static readonly DependencyProperty TintProperty = DependencyProperty.Register(
        nameof(Tint), typeof(Brush), typeof(Glass), new PropertyMetadata(new SolidColorBrush(Color.FromArgb(0x70, 14, 24, 44))));

    public static readonly DependencyProperty ShadowProperty = DependencyProperty.Register(
        nameof(Shadow), typeof(bool), typeof(Glass), new PropertyMetadata(true));

    public Visual? Source { get => (Visual?)GetValue(SourceProperty); set => SetValue(SourceProperty, value); }
    public double Corner { get => (double)GetValue(CornerProperty); set => SetValue(CornerProperty, value); }
    public double Blur { get => (double)GetValue(BlurProperty); set => SetValue(BlurProperty, value); }
    public Brush Tint { get => (Brush)GetValue(TintProperty); set => SetValue(TintProperty, value); }
    public bool Shadow { get => (bool)GetValue(ShadowProperty); set => SetValue(ShadowProperty, value); }

    public CornerRadius CornerRadius => new(Corner);

    private Border? _clip;
    private Rectangle? _sample;
    private readonly VisualBrush _brush = new()
    {
        ViewboxUnits = BrushMappingMode.Absolute,
        Stretch = Stretch.Fill,
        AlignmentX = AlignmentX.Left,
        AlignmentY = AlignmentY.Top,
        TileMode = TileMode.None,
    };
    private readonly BlurEffect _blur = new() { KernelType = KernelType.Gaussian, RenderingBias = RenderingBias.Performance };
    private Rect _lastBox = Rect.Empty;
    private Rect _lastShape = Rect.Empty;
    private double _lastCorner = -1, _lastBlur = -1;

    public Glass()
    {
        LayoutUpdated += (_, _) => Sync();
        SizeChanged += (_, _) => Sync();
        RenderOptions.SetCachingHint(_brush, CachingHint.Unspecified);
    }

    public override void OnApplyTemplate()
    {
        base.OnApplyTemplate();
        _clip = GetTemplateChild("PART_Clip") as Border;
        _sample = GetTemplateChild("PART_Sample") as Rectangle;
        if (_sample is not null)
        {
            _sample.Fill = _brush;
            _sample.Effect = _blur;
        }
        _lastBox = Rect.Empty;
        Sync();
    }

    /// <summary>Ставит окно копии сцены ровно под панель (с запасом на размытие по краям). Зовётся часто, поэтому
    /// ничего не пересоздаёт, если размер и место не изменились.</summary>
    private void Sync()
    {
        if (_sample is null || _clip is null) return;
        var w = ActualWidth;
        var h = ActualHeight;
        if (w <= 0 || h <= 0) return;

        var r = Math.Max(0, Blur);
        var shape = new Rect(0, 0, w, h);
        if (shape != _lastShape || Corner != _lastCorner || r != _lastBlur)
        {
            _lastShape = shape;
            _lastCorner = Corner;
            _lastBlur = r;
            _blur.Radius = r;
            var g = new RectangleGeometry(shape, Corner, Corner);
            g.Freeze();
            _clip.Clip = g;
            _sample.Margin = new Thickness(-r);
            _lastBox = Rect.Empty;
        }

        if (Source is not Visual src || !IsVisible) { _brush.Visual = null; _lastBox = Rect.Empty; return; }
        if (!ReferenceEquals(_brush.Visual, src)) _brush.Visual = src;
        Point p;
        try { p = TransformToVisual(src).Transform(new Point(0, 0)); }
        catch (InvalidOperationException) { return; } // ещё не в одном дереве
        var box = new Rect(p.X - r, p.Y - r, w + 2 * r, h + 2 * r);
        if (box == _lastBox) return;
        _lastBox = box;
        _brush.Viewbox = box;
    }
}
