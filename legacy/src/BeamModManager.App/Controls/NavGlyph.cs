using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Animation;
using BeamModManager.App.Motion;

namespace BeamModManager.App.Controls;

/// <summary>
/// Значок пункта левой панели: только контур (2026-10-17, после боя: заливки «Обновлений» и «Поиска» выглядели пятнами,
/// а на светлом оформлении все значки — грязно-чёрными). У выбранного пункта линия плавно становится толще
/// (<see cref="Thin"/> → <see cref="Bold"/>), так выбор виден и без капли. Рисуется сам в квадрате 22 × 22, не ловит мышь.
/// </summary>
public sealed class NavGlyph : FrameworkElement
{
    public const double Thin = 1.7, Bold = 2.2;

    public static readonly DependencyProperty DataProperty = DependencyProperty.Register(
        nameof(Data), typeof(Geometry), typeof(NavGlyph), new FrameworkPropertyMetadata(null, FrameworkPropertyMetadataOptions.AffectsRender));
    public static readonly DependencyProperty BrushProperty = DependencyProperty.Register(
        nameof(Brush), typeof(Brush), typeof(NavGlyph), new FrameworkPropertyMetadata(Brushes.White, FrameworkPropertyMetadataOptions.AffectsRender));
    public static readonly DependencyProperty ActiveProperty = DependencyProperty.Register(
        nameof(Active), typeof(bool?), typeof(NavGlyph), new PropertyMetadata(false, (d, e) => ((NavGlyph)d).OnActive(e.NewValue as bool? == true)));
    private static readonly DependencyProperty WeightProperty = DependencyProperty.Register(
        "Weight", typeof(double), typeof(NavGlyph), new FrameworkPropertyMetadata(Thin, FrameworkPropertyMetadataOptions.AffectsRender));

    public Geometry? Data { get => (Geometry?)GetValue(DataProperty); set => SetValue(DataProperty, value); }
    public Brush Brush { get => (Brush)GetValue(BrushProperty); set => SetValue(BrushProperty, value); }
    public bool? Active { get => (bool?)GetValue(ActiveProperty); set => SetValue(ActiveProperty, value); }

    public NavGlyph()
    {
        IsHitTestVisible = false;
        Width = Height = 22;
        SnapsToDevicePixels = false;
    }

    private void OnActive(bool active)
    {
        var to = active ? Bold : Thin;
        if (!IsLoaded || SpringDriver.ReducedMotion) { BeginAnimation(WeightProperty, null); SetValue(WeightProperty, to); return; }
        BeginAnimation(WeightProperty, new DoubleAnimation(to, TimeSpan.FromSeconds(0.3)) { EasingFunction = Anim.Smooth });
    }

    protected override void OnRender(DrawingContext dc)
    {
        if (Data is not { } data) return;
        var pen = new Pen(Brush, (double)GetValue(WeightProperty)) { StartLineCap = PenLineCap.Round, EndLineCap = PenLineCap.Round, LineJoin = PenLineJoin.Round };
        dc.DrawGeometry(null, pen, data);
    }
}
