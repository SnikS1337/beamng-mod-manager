namespace BeamModManager.Ui.Look;

/// <summary>
/// Общая политика частоты кадров анимаций ответа (DESIGN.md 8, «Частота кадров», решено 2026-10-17):
/// <list type="bullet">
/// <item>с частотой экрана, но не выше <see cref="MaxHz"/> (выше разница почти не видна, а стоит дороже);</item>
/// <item>от батареи или в режиме экономии энергии — не выше <see cref="SafeHz"/>;</item>
/// <item>если на высокой частоте кадры не успевают (больше четверти кадров анимации опоздали), следующие анимации идут на
///   ровных <see cref="SafeHz"/>: ровная пониженная частота выглядит лучше скачущей. Переключение только между
///   анимациями, не посреди движения. Через <see cref="RetryAfter"/> высокая частота пробуется снова.</item>
/// </list>
/// Фоновое движение (фон, огоньки, искры) сюда не относится: у него свои 24–30 кадров.
/// </summary>
public sealed class FramePolicy
{
    public const double MaxHz = 144, SafeHz = 60;
    public static readonly TimeSpan RetryAfter = TimeSpan.FromMinutes(2);

    /// <summary>Сколько кадров анимации нужно, чтобы судить об опозданиях (короткие всплески не в счёт).</summary>
    public const int MinFramesToJudge = 12;
    /// <summary>Доля опоздавших кадров, после которой частота снижается.</summary>
    public const double LateShare = 0.25;

    private readonly Func<DateTime> _now;
    private DateTime _degradedAt;

    public FramePolicy(Func<DateTime>? now = null) => _now = now ?? (() => DateTime.UtcNow);

    /// <summary>Общая на всё окно.</summary>
    public static FramePolicy Shared { get; } = new();

    /// <summary>Компьютер работает от батареи или включена экономия энергии.</summary>
    public bool PowerSaving { get; set; }

    /// <summary>Высокая частота не успевала — пока идём на ровных 60.</summary>
    public bool Degraded { get; private set; }

    /// <summary>Наибольшая частота анимаций сейчас.</summary>
    public double TargetHz
    {
        get
        {
            if (Degraded && _now() - _degradedAt >= RetryAfter) Degraded = false; // пробуем снова
            return PowerSaving || Degraded ? SafeHz : MaxHz;
        }
    }

    /// <summary>
    /// Итог одной анимации: сколько кадров показано, сколько опоздало и на каком экране (интервал кадра экрана, с).
    /// На экране 60 Гц снижать некуда — там ничего не меняется.
    /// </summary>
    public void Report(int frames, int late, double screenInterval)
    {
        if (frames < MinFramesToJudge || PowerSaving || Degraded) return;
        if (screenInterval >= 1 / (SafeHz + 15)) return; // экран и так не быстрее ~75 Гц
        if (late > frames * LateShare)
        {
            Degraded = true;
            _degradedAt = _now();
        }
    }
}

/// <summary>
/// Ведёт одну группу пружин по кадрам экрана с учётом <see cref="FramePolicy"/>: узнаёт частоту экрана по самим кадрам
/// (наименьший интервал за последние кадры — окно перенесли на другой монитор, и через секунду частота уже его),
/// пропускает лишние кадры, когда экран быстрее разрешённой частоты, и считает опоздавшие кадры.
/// Без WPF: проверяется тестами.
/// </summary>
public sealed class FramePacer
{
    private const int Window = 90;
    private readonly FramePolicy _policy;
    private readonly double[] _recent = new double[Window];
    private int _count, _pos;
    private double _acc;
    private int _frames, _late;

    public FramePacer(FramePolicy? policy = null) => _policy = policy ?? FramePolicy.Shared;

    /// <summary>Интервал кадра экрана, с (наименьший за последние кадры; до первых кадров — как у 60 Гц).</summary>
    public double ScreenInterval
    {
        get
        {
            if (_count == 0) return 1.0 / 60;
            var m = double.MaxValue;
            for (var i = 0; i < _count; i++) m = Math.Min(m, _recent[i]);
            return m;
        }
    }

    /// <summary>
    /// Очередной кадр экрана, <paramref name="dt"/> — секунд с прошлого. Возвращает, на сколько секунд шагнуть пружинам,
    /// или 0 — этот кадр пропустить (экран быстрее разрешённой частоты).
    /// </summary>
    public double Frame(double dt)
    {
        if (!(dt > 0) || double.IsInfinity(dt)) return 0;
        // в оценку частоты экрана — только правдоподобные интервалы (≥ 300 Гц не бывает, пауза окна — не кадр)
        if (dt >= 1.0 / 300 && dt <= 0.25)
        {
            _recent[_pos] = dt;
            _pos = (_pos + 1) % Window;
            if (_count < Window) _count++;
        }
        var screen = ScreenInterval;
        _frames++;
        if (dt > screen * 1.6) _late++; // пропущен хотя бы один кадр экрана
        var interval = 1 / _policy.TargetHz;
        if (screen >= interval * 0.9) { _acc = 0; return dt; } // экран не быстрее разрешённого — каждый кадр
        _acc += dt;
        if (_acc + screen * 0.5 < interval) return 0;
        var step = _acc;
        _acc = 0;
        return step;
    }

    /// <summary>Анимация закончилась: итог уходит в политику, счёт — заново.</summary>
    public void BurstEnded()
    {
        _policy.Report(_frames, _late, ScreenInterval);
        _frames = _late = 0;
        _acc = 0;
    }
}
