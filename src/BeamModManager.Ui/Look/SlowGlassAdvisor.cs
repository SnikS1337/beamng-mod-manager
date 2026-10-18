namespace BeamModManager.Ui.Look;

/// <summary>
/// «Интерфейс работает медленно. Включить набор «Быстро»?» (DESIGN.md 5.1, решено 2026-10-18). Смотрит итоги анимаций
/// (<see cref="FramePolicy.Judged"/>): если из последних <see cref="Window"/> анимаций хотя бы <see cref="Needed"/> потеряли
/// больше <see cref="SlowShare"/> кадров — один раз предлагает. Только пока стоит жидкое стекло и на вопрос ещё не ответили
/// «Оставить»; после предложения молчит до перезапуска. Одна медленная анимация (окно только открылось, Windows занята) не в счёт.
/// </summary>
public sealed class SlowGlassAdvisor
{
    public const int Window = 6, Needed = 4;
    public const double SlowShare = 0.4;

    private readonly Queue<bool> _recent = new();

    /// <summary>Можно ли сейчас предлагать: жидкое стекло, не отказывались, ещё не предлагали.</summary>
    public bool Armed { get; set; }

    /// <summary>Пора предложить (один раз: после этого <see cref="Armed"/> снимается).</summary>
    public event Action? Suggest;

    public void Burst(int frames, int late)
    {
        if (!Armed || frames < FramePolicy.MinFramesToJudge) return;
        _recent.Enqueue(late > frames * SlowShare);
        while (_recent.Count > Window) _recent.Dequeue();
        if (_recent.Count(s => s) < Needed) return;
        Armed = false;
        _recent.Clear();
        Suggest?.Invoke();
    }

    /// <summary>Стекло сменили или ответили: счёт заново.</summary>
    public void Reset() => _recent.Clear();
}
