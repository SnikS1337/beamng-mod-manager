using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Shapes;

namespace BeamModManager.App.Services;

/// <summary>
/// Само движется только фон (DESIGN_MOTION раздел 2): плывущая цветная полоса (36 с), пятна света (плывут по петлям 21–37 с
/// и дышат 6–9 с) и огоньки.
/// Останавливается, когда окно не видно, не в фокусе или запущена игра. Включают и выключают его настройки программы
/// («Анимированный фон», «Огоньки»), а не флажки анимации Windows (2026-10-18).
/// Фон и огоньки — две отдельные истории: в настройках они включаются по отдельности («Анимированный фон», «Огоньки»).
/// </summary>
public sealed class BackgroundMotion
{
    private readonly Part _bg, _flies;
    private readonly FrameworkElement _owner;
    private readonly List<(FrameworkElement Shape, RadialGradientBrush Brush)> _blobs = new();
    private readonly List<Grid> _paints = new();

    /// <summary>Четыре пятна: фигура (место и размер ставит окно по теме) и её кисть (не заморожена: цвет меняется с темой).</summary>
    public IReadOnlyList<(FrameworkElement Shape, RadialGradientBrush Brush)> Blobs => _blobs;

    /// <summary>Доля разрешения, в которой хранится картинка пятна (пятно размытое — больше не нужно).</summary>
    private const double BlobCacheScale = 0.5;
    private const double BlobBlur = 48;

    /// <summary>Одна история со своим состоянием: ещё не начата, идёт, на паузе.</summary>
    private sealed class Part
    {
        public readonly Storyboard Sb = new();
        public bool Started, Paused;
    }

    /// <param name="fps">Кадров в секунду (BackgroundPace.Fps по частоте экрана): фон медленный, 24–36 кадров глазу хватает,
    /// а стекло пересчитывает копию сцены реже (DESIGN.md 8); главное — ровно в кадры экрана.</param>
    public BackgroundMotion(FrameworkElement owner, TranslateTransform gradeMove, Canvas lights, Canvas flies, int fps = 30)
    {
        _owner = owner;
        _bg = new Part();
        _flies = new Part();
        Fps = fps;
        Timeline.SetDesiredFrameRate(_bg.Sb, fps);
        Timeline.SetDesiredFrameRate(_flies.Sb, fps);
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

        // четыре размытых пятна света (2026-10-17: «фон должен дышать»): места и цвета задаёт тема (окно, PlaceBlobs),
        // здесь — движение. Каждое пятно плывёт по своей петле (X и Y с разными периодами, поэтому путь — кривая, а не
        // отрезок туда-обратно), дышит (масштаб 0,8 ↔ 1,22 за 6–9 с в одну сторону, как вдох) и чуть разгорается и гаснет.
        // Периоды у всех разные: пятна никогда не двигаются в такт. Раньше было 1 ↔ 1,24 за 19–27 с — движение не замечали.
        (double Dx, double Dy, double Px, double Py, double Breath, double Glow, double Delay)[] paths =
        {
            (260, -170, 26, 34, 8.0, 11, 0), (-300, 210, 31, 23, 9.0, 13, 7), (300, -220, 29, 37, 7.0, 10, 15), (-180, 150, 21, 27, 6.0, 9, 4),
        };
        foreach (var d in paths)
        {
            var brush = new RadialGradientBrush { GradientOrigin = new Point(0.5, 0.5) };
            foreach (var (at, _) in BeamModManager.Ui.Look.BackgroundThemes.Falloff)
                brush.GradientStops.Add(new GradientStop(Colors.Transparent, at));
            // размытие поверх мягкого края: пятно — это свет, а не круг. Зерно (SetDither) — внутри той же картинки.
            // Кэш хранит уже готовую картинку, и сдвиг, масштаб и прозрачность её не перерисовывают
            var ellipse = new Ellipse
            {
                Fill = brush,
                Effect = new System.Windows.Media.Effects.BlurEffect { Radius = BlobBlur, RenderingBias = System.Windows.Media.Effects.RenderingBias.Performance },
            };
            var paint = new Grid { Children = { ellipse } };
            var e = new Grid
            {
                IsHitTestVisible = false,
                RenderTransformOrigin = new Point(0.5, 0.5),
                CacheMode = new BitmapCache(BlobCacheScale),
                Children = { paint },
            };
            e.SizeChanged += (_, _) => SizeDither(e, paint);
            _paints.Add(paint);
            var move = new TranslateTransform();
            var scale = new ScaleTransform(1, 1);
            e.RenderTransform = new TransformGroup { Children = { scale, move } };
            lights.Children.Add(e);
            _blobs.Add((e, brush));
            Anim(move, TranslateTransform.XProperty, -d.Dx / 2, d.Dx / 2, d.Px, delay: d.Delay);
            Anim(move, TranslateTransform.YProperty, -d.Dy / 2, d.Dy / 2, d.Py, delay: d.Delay + d.Py / 3);
            Anim(scale, ScaleTransform.ScaleXProperty, 0.8, 1.22, d.Breath, delay: d.Delay);
            Anim(scale, ScaleTransform.ScaleYProperty, 0.8, 1.22, d.Breath, delay: d.Delay);
            Anim(e, UIElement.OpacityProperty, 0.72, 1, d.Glow, delay: d.Delay * 0.6);
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

    /// <summary>
    /// Зерно в цвете пятен (Shaders/Dither.fx; 2026-10-18 — против колец-ступенек вокруг пятен). Раньше шейдер стоял на
    /// всём фоне — и считался каждый кадр фона, причём заново под каждым стеклом (копия сцены): на мощном компьютере всё
    /// окно шло рывками и предлагало «Быстро». Теперь зерно — часть картинки пятна в кэше: считается, только когда
    /// меняются размер или цвет пятна, а движение, дыхание и прозрачность его не трогают.
    /// </summary>
    public void SetDither(bool on)
    {
        for (var i = 0; i < _paints.Count; i++)
        {
            var paint = _paints[i];
            if (!on) { paint.Effect = null; continue; }
            if (paint.Effect is not Controls.DitherEffect) paint.Effect = new Controls.DitherEffect();
            SizeDither(_blobs[i].Shape, paint);
        }
    }

    /// <summary>Точка шума — на точку картинки пятна в кэше (вместе с размытием по краям).</summary>
    private static void SizeDither(FrameworkElement blob, Grid paint)
    {
        if (paint.Effect is not Controls.DitherEffect d || blob.ActualWidth <= 0) return;
        var dpi = VisualTreeHelper.GetDpi(blob);
        d.SetPixelSize((blob.ActualWidth + 2 * BlobBlur) * dpi.DpiScaleX * BlobCacheScale,
            (blob.ActualHeight + 2 * BlobBlur) * dpi.DpiScaleY * BlobCacheScale);
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

    /// <summary>Кадров в секунду у фона и огоньков.</summary>
    public int Fps { get; }

    private bool _wantBg, _wantFlies, _held;

    /// <summary>Двигаться или стоять, фону и огонькам отдельно. Повторные вызовы с тем же значением ничего не делают.</summary>
    public void SetRunning(bool background, bool flies)
    {
        _wantBg = background;
        _wantFlies = flies;
        Apply();
    }

    /// <summary>
    /// Придержать фон, пока идёт анимация ответа (капля, островок, переключатель; 2026-10-17, после боя: «анимации дёрганые»).
    /// Каждый кадр фона заново считает копию сцены под всеми стёклами; совпав с кадрами капли, он отнимал у неё время, и
    /// капля шла рывками. Пауза — доли секунды, медленный фон за это время почти не сдвинулся бы; потом он идёт дальше
    /// с того же места.
    /// </summary>
    public void Hold(bool held)
    {
        if (_held == held) return;
        _held = held;
        Apply();
    }

    private void Apply()
    {
        Run(_bg, _wantBg && !_held);
        Run(_flies, _wantFlies && !_held);
    }

    private void Run(Part p, bool run)
    {
        if (run && !p.Started) { p.Sb.Begin(_owner, true); p.Started = true; p.Paused = false; return; }
        if (!p.Started) return;
        if (run && p.Paused) { p.Sb.Resume(_owner); p.Paused = false; }
        else if (!run && !p.Paused) { p.Sb.Pause(_owner); p.Paused = true; }
    }
}
