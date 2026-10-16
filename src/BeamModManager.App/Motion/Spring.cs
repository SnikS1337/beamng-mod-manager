using System.Windows.Media;
using System.Windows.Media.Animation;
using BeamModManager.Ui.Look;

namespace BeamModManager.App.Motion;

/// <summary>
/// Пружина Apple (DESIGN_MOTION раздел 5): длительность и упругость → масса 1, жёсткость (2π/длительность)²,
/// затухание 1 − упругость. Считается кадр за кадром и при прерывании продолжает с текущей скоростью.
/// </summary>
public sealed class Spring
{
    private readonly double _k, _c;

    public Spring(double duration, double bounce, double value = 0)
    {
        _k = Math.Pow(2 * Math.PI / duration, 2);
        var zeta = bounce >= 0 ? 1 - bounce : 1 / (1 + bounce);
        _c = 2 * zeta * Math.Sqrt(_k);
        Value = Target = value;
    }

    public double Value { get; private set; }
    public double Velocity { get; private set; }
    public double Target { get; set; }
    public bool Resting => Value == Target && Velocity == 0;

    public void Snap(double v) { Value = Target = v; Velocity = 0; }

    /// <summary>Шаг на dt секунд; false — пружина успокоилась.</summary>
    public bool Step(double dt)
    {
        if (Resting) return false;
        dt = Math.Min(dt, 0.064); // после паузы окна не «выстреливаем»
        var n = Math.Max(1, (int)Math.Ceiling(dt / 0.004));
        var h = dt / n;
        for (var i = 0; i < n; i++)
        {
            var a = -_k * (Value - Target) - _c * Velocity;
            Velocity += a * h;
            Value += Velocity * h;
        }
        if (Math.Abs(Value - Target) < 0.01 && Math.Abs(Velocity) < 0.05) { Value = Target; Velocity = 0; return false; }
        return true;
    }
}

/// <summary>
/// Ведёт набор пружин по кадрам экрана и отписывается от кадров, когда все успокоились (ничего не рисуем зря).
/// Частота — по <see cref="FramePolicy"/>: как у экрана, но не выше 144; от батареи и при нехватке — ровные 60.
/// </summary>
public sealed class SpringDriver
{
    private readonly List<(Spring S, Action<double> Apply)> _items = new();
    private readonly FramePacer _pacer = new();
    private TimeSpan _last;
    private bool _on;

    /// <summary>В Windows выключены эффекты анимации.</summary>
    public static bool SystemReduced => !System.Windows.SystemParameters.ClientAreaAnimation;

    /// <summary>«Анимации интерфейса» выключены в настройках программы.</summary>
    public static bool AnimationsOff { get; set; }

    /// <summary>Замер кадров на сборочной машине (--frames): анимации идут, даже если в Windows они выключены.</summary>
    public static bool ForceMotion { get; set; }

    /// <summary>Интерфейс не анимируется: капли, панели и карточки встают на место сразу. Фон этим не управляется.</summary>
    public static bool ReducedMotion => (SystemReduced && !ForceMotion) || AnimationsOff;

    public Spring Add(Spring s, Action<double> apply)
    {
        _items.Add((s, apply));
        apply(s.Value);
        return s;
    }

    /// <summary>Задать цель; при «Уменьшить анимацию» в Windows — сразу на место.</summary>
    public void To(Spring s, double target, bool instant = false)
    {
        if (instant || ReducedMotion)
        {
            s.Snap(target);
            foreach (var it in _items) if (ReferenceEquals(it.S, s)) it.Apply(target);
            return;
        }
        s.Target = target;
        if (!_on) { _on = true; _last = TimeSpan.Zero; CompositionTarget.Rendering += OnFrame; }
    }

    private void OnFrame(object? sender, EventArgs e)
    {
        var t = e is RenderingEventArgs r ? r.RenderingTime : TimeSpan.Zero;
        var first = _last == TimeSpan.Zero;
        var dt = first ? 1.0 / 60 : (t - _last).TotalSeconds;
        if (dt <= 0) return; // тот же кадр
        _last = t;
        if (!first)
        {
            dt = _pacer.Frame(dt);
            if (dt <= 0) return; // экран быстрее разрешённой частоты: этот кадр пропускаем
        }
        var busy = false;
        foreach (var (s, apply) in _items)
        {
            if (s.Resting) continue;
            busy |= s.Step(dt);
            apply(s.Value);
        }
        if (!busy) { _on = false; CompositionTarget.Rendering -= OnFrame; _pacer.BurstEnded(); }
    }
}

/// <summary>
/// Та же пружина для обычных анимаций XAML (наведение, подъём карточки). Длительность анимации берётся в 1,6 раза
/// больше «ощущаемой»: к её концу пружина успокаивается полностью, без скачка в последнем кадре.
/// </summary>
public sealed class SpringEase : EasingFunctionBase
{
    public const double Settle = 1.6;

    // EaseIn: кривая берётся как есть (EaseOut в WPF перевернул бы её).
    public SpringEase() => EasingMode = EasingMode.EaseIn;

    public double Bounce { get; set; }

    protected override double EaseInCore(double t)
    {
        if (SpringDriver.ReducedMotion) return t > 0 ? 1 : 0; // анимации выключены: сразу в конец, без движения
        var w = 2 * Math.PI;
        var x = t * Settle;
        var zeta = Bounce >= 0 ? 1 - Bounce : 1 / (1 + Bounce);
        if (zeta >= 1) return 1 - (1 + w * x) * Math.Exp(-w * x);
        var wd = w * Math.Sqrt(1 - zeta * zeta);
        return 1 - Math.Exp(-zeta * w * x) * (Math.Cos(wd * x) + zeta * w / wd * Math.Sin(wd * x));
    }

    protected override System.Windows.Freezable CreateInstanceCore() => new SpringEase { Bounce = Bounce };
}
