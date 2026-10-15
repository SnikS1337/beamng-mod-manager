using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Shapes;

namespace BeamModManager.App.Services;

/// <summary>
/// Само движется только фон (DESIGN_MOTION раздел 2): «дыхание» фото, плывущий градиент, пятна и огоньки, всё на 48 с.
/// Останавливается, когда окно не видно, не в фокусе или запущена игра; при «Уменьшить анимацию» в Windows не запускается.
/// </summary>
public sealed class BackgroundMotion
{
    private readonly Storyboard _sb = new();
    private readonly FrameworkElement _owner;
    private bool _started, _paused;

    public BackgroundMotion(FrameworkElement owner, ScaleTransform wallScale, TranslateTransform wallMove, TranslateTransform gradeMove, Canvas lights)
    {
        _owner = owner;
        var ease = new SineEase { EasingMode = EasingMode.EaseInOut };
        void Anim(DependencyObject target, DependencyProperty prop, double from, double to, double seconds, bool reverse = true, double delay = 0)
        {
            var a = new DoubleAnimation(from, to, TimeSpan.FromSeconds(seconds))
            {
                AutoReverse = reverse,
                RepeatBehavior = RepeatBehavior.Forever,
                EasingFunction = reverse ? ease : null,
                BeginTime = TimeSpan.FromSeconds(-delay),
            };
            Storyboard.SetTarget(a, target);
            Storyboard.SetTargetProperty(a, new PropertyPath(prop));
            _sb.Children.Add(a);
        }

        Anim(wallScale, ScaleTransform.ScaleXProperty, 1.04, 1.12, 48);
        Anim(wallScale, ScaleTransform.ScaleYProperty, 1.04, 1.12, 48);
        Anim(wallMove, TranslateTransform.XProperty, 0, -22, 48);
        Anim(wallMove, TranslateTransform.YProperty, 0, -14, 48);
        Anim(gradeMove, TranslateTransform.XProperty, 0, 1, 48, reverse: false);

        // три светлых пятна (200°, 250°, 30°) со сдвигом фазы
        (double X, double Y, double R, Color C, double Dx, double Dy, double Delay)[] blobs =
        {
            (-220, 380, 380, Color.FromArgb(0x57, 0x58, 0xA9, 0xD6), 140, -90, 0),
            (760, -300, 430, Color.FromArgb(0x4D, 0x8E, 0x82, 0xD8), -160, 110, 12),
            (380, 560, 320, Color.FromArgb(0x42, 0xE6, 0xA0, 0x60), 180, -120, 24),
        };
        foreach (var b in blobs)
        {
            var e = new Ellipse
            {
                Width = b.R * 2, Height = b.R * 2,
                Fill = new RadialGradientBrush(b.C, Color.FromArgb(0, b.C.R, b.C.G, b.C.B)) { GradientOrigin = new Point(0.5, 0.5) },
                IsHitTestVisible = false,
                CacheMode = new BitmapCache(0.5),
            };
            var t = new TranslateTransform();
            e.RenderTransform = t;
            Canvas.SetLeft(e, b.X);
            Canvas.SetTop(e, b.Y);
            lights.Children.Add(e);
            Anim(t, TranslateTransform.XProperty, 0, b.Dx, 48, delay: b.Delay);
            Anim(t, TranslateTransform.YProperty, 0, b.Dy, 48, delay: b.Delay);
        }

        // светлячки: 18 мягких огоньков 3–8 px, дрейф 14–30 с, мерцание 2,5–6,5 с; одно зерно — один и тот же рисунок
        var rnd = new Random(7);
        for (var i = 0; i < 18; i++)
        {
            var size = 3 + rnd.Next(6);
            var e = new Ellipse
            {
                Width = size * 4, Height = size * 4,
                Fill = new RadialGradientBrush(new GradientStopCollection
                {
                    new(Color.FromArgb(0xFF, 0xFF, 0xFF, 0xFF), 0),
                    new(Color.FromArgb(0xC0, 0xFF, 0xF4, 0xD6), 0.12),
                    new(Color.FromArgb(0x40, 0xFF, 0xEC, 0xBE), 0.28),
                    new(Color.FromArgb(0x00, 0xFF, 0xE2, 0xAA), 0.5),
                }),
                IsHitTestVisible = false,
                Opacity = 0.15,
            };
            var t = new TranslateTransform();
            e.RenderTransform = t;
            Canvas.SetLeft(e, 100 + rnd.NextDouble() * 1300);
            Canvas.SetTop(e, 60 + rnd.NextDouble() * 860);
            lights.Children.Add(e);
            var drift = 14 + rnd.NextDouble() * 16;
            Anim(t, TranslateTransform.XProperty, 0, rnd.Next(-90, 90), drift, delay: rnd.NextDouble() * drift);
            Anim(t, TranslateTransform.YProperty, 0, rnd.Next(-110, 60), drift, delay: rnd.NextDouble() * drift);
            var tw = 2.5 + rnd.NextDouble() * 4;
            Anim(e, UIElement.OpacityProperty, 0.15, 0.95, tw / 2, delay: rnd.NextDouble() * tw);
        }
    }

    /// <summary>Двигаться или стоять. Повторные вызовы с тем же значением ничего не делают.</summary>
    public void SetRunning(bool run)
    {
        if (Motion.SpringDriver.ReducedMotion) run = false;
        if (run && !_started) { _sb.Begin(_owner, true); _started = true; _paused = false; return; }
        if (!_started) return;
        if (run && _paused) { _sb.Resume(_owner); _paused = false; }
        else if (!run && !_paused) { _sb.Pause(_owner); _paused = true; }
    }
}
