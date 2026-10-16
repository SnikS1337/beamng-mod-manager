using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Shapes;

namespace BeamModManager.App.Services;

/// <summary>
/// Само движется только фон (DESIGN_MOTION раздел 2): плывущая цветная полоса (36 с), пятна света (28 с) и огоньки.
/// Останавливается, когда окно не видно, не в фокусе или запущена игра; при «Уменьшить анимацию» в Windows не запускается.
/// Фон и огоньки — две отдельные истории: в настройках они включаются по отдельности («Анимированный фон», «Огоньки»).
/// </summary>
public sealed class BackgroundMotion
{
    private readonly Part _bg, _flies;
    private readonly FrameworkElement _owner;
    private readonly List<(Ellipse Shape, RadialGradientBrush Brush)> _blobs = new();

    /// <summary>Три пятна: фигура (место и размер ставит окно по теме) и её кисть (не заморожена: цвет меняется с темой).</summary>
    public IReadOnlyList<(Ellipse Shape, RadialGradientBrush Brush)> Blobs => _blobs;

    /// <summary>Одна история со своим состоянием: ещё не начата, идёт, на паузе.</summary>
    private sealed class Part
    {
        public readonly Storyboard Sb = new();
        public bool Started, Paused;
    }

    public BackgroundMotion(FrameworkElement owner, TranslateTransform gradeMove, Canvas lights, Canvas flies)
    {
        _owner = owner;
        _bg = new Part();
        _flies = new Part();
        // фон медленный: 24 кадров в секунду глазу хватает, а стекло пересчитывает копию сцены реже (DESIGN.md 8)
        Timeline.SetDesiredFrameRate(_bg.Sb, 24);
        Timeline.SetDesiredFrameRate(_flies.Sb, 24);
        var ease = new SineEase { EasingMode = EasingMode.EaseInOut };
        var sb = _bg.Sb;
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
            sb.Children.Add(a);
        }

        Anim(gradeMove, TranslateTransform.XProperty, 0, 1, 36, reverse: false);

        // три размытых пятна света: места и цвета задаёт тема (окно, PlaceBlobs); здесь — их движение.
        // Дрейф по 28 с со сдвигом фазы 0 / 9 / 18 с и «дыхание» (масштаб 1 → 1,24) с разными периодами,
        // поэтому пятна никогда не двигаются в такт и узор не повторяется заметно. Раньше (48 с и 140 px) движение
        // было около 3 px в секунду — его не замечали; теперь оно видно, но остаётся спокойным.
        (double Dx, double Dy, double Delay, double Breath)[] paths =
        {
            (280, -170, 0, 19), (-300, 210, 9, 23), (320, -230, 18, 27),
        };
        foreach (var d in paths)
        {
            var brush = new RadialGradientBrush { GradientOrigin = new Point(0.5, 0.5) };
            foreach (var (at, _) in BeamModManager.Ui.Look.BackgroundThemes.Falloff)
                brush.GradientStops.Add(new GradientStop(Colors.Transparent, at));
            var e = new Ellipse
            {
                Fill = brush,
                IsHitTestVisible = false,
                RenderTransformOrigin = new Point(0.5, 0.5),
                // размытие поверх мягкого края: пятно — это свет, а не круг. Кэш хранит уже размытую картинку,
                // и сдвиг с масштабом её не перерисовывают
                Effect = new System.Windows.Media.Effects.BlurEffect { Radius = 48, RenderingBias = System.Windows.Media.Effects.RenderingBias.Performance },
                CacheMode = new BitmapCache(0.5),
            };
            var move = new TranslateTransform();
            var scale = new ScaleTransform(1, 1);
            e.RenderTransform = new TransformGroup { Children = { scale, move } };
            lights.Children.Add(e);
            _blobs.Add((e, brush));
            Anim(move, TranslateTransform.XProperty, 0, d.Dx, 28, delay: d.Delay);
            Anim(move, TranslateTransform.YProperty, 0, d.Dy, 28, delay: d.Delay);
            Anim(scale, ScaleTransform.ScaleXProperty, 1, 1.24, d.Breath, delay: d.Delay);
            Anim(scale, ScaleTransform.ScaleYProperty, 1, 1.24, d.Breath, delay: d.Delay);
        }

        // светлячки: 18 мягких огоньков 3–8 px, дрейф 14–30 с, мерцание 2,5–6,5 с; одно зерно — один и тот же рисунок
        sb = _flies.Sb;
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
            // место — доля окна (по доске 1440 × 960: x 100…1400, y 60…920); окно ставит его в LayoutFlies
            _flyAt.Add((e, (100 + rnd.NextDouble() * 1300) / 1440, (60 + rnd.NextDouble() * 860) / 960));
            flies.Children.Add(e);
            var drift = 14 + rnd.NextDouble() * 16;
            Anim(t, TranslateTransform.XProperty, 0, rnd.Next(-90, 90), drift, delay: rnd.NextDouble() * drift);
            Anim(t, TranslateTransform.YProperty, 0, rnd.Next(-110, 60), drift, delay: rnd.NextDouble() * drift);
            var tw = 2.5 + rnd.NextDouble() * 4;
            Anim(e, UIElement.OpacityProperty, 0.15, 0.95, tw / 2, delay: rnd.NextDouble() * tw);
        }
    }

    private readonly List<(UIElement Fly, double X, double Y)> _flyAt = new();

    /// <summary>Огоньки по всему окну любого размера (а не только в углу 1440 × 960).</summary>
    public void LayoutFlies(double width, double height)
    {
        foreach (var (fly, x, y) in _flyAt)
        {
            Canvas.SetLeft(fly, x * width);
            Canvas.SetTop(fly, y * height);
        }
    }

    /// <summary>Двигаться или стоять, фону и огонькам отдельно. Повторные вызовы с тем же значением ничего не делают.</summary>
    public void SetRunning(bool background, bool flies)
    {
        var reduced = Motion.SpringDriver.SystemReduced;
        Run(_bg, background && !reduced);
        Run(_flies, flies && !reduced);
    }

    private void Run(Part p, bool run)
    {
        if (run && !p.Started) { p.Sb.Begin(_owner, true); p.Started = true; p.Paused = false; return; }
        if (!p.Started) return;
        if (run && p.Paused) { p.Sb.Resume(_owner); p.Paused = false; }
        else if (!run && !p.Paused) { p.Sb.Pause(_owner); p.Paused = true; }
    }
}
