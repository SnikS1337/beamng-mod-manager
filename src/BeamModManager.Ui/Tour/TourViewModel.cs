using BeamModManager.Ui.Mvvm;

namespace BeamModManager.Ui.Tour;

/// <summary>На что указывает шаг знакомства: окно подсвечивает этот элемент и ставит пузырь рядом.</summary>
public enum TourTarget
{
    /// <summary>Левая панель разделов.</summary>
    Nav,
    /// <summary>Островок с фильтрами и поиском.</summary>
    Filters,
    /// <summary>Первая карточка мода (шага нет, если модов нет).</summary>
    Card,
    /// <summary>Всё окно: пузырь по центру, без подсветки.</summary>
    Window,
}

public sealed record TourStep(TourTarget Target, string Title, string Text);

/// <summary>
/// Знакомство при первом запуске (решено 2026-10-18: пузыри у настоящих элементов окна). Четыре шага: разделы, фильтры,
/// карточка мода, установка перетаскиванием. «Далее» / «Готово», «Пропустить»; показывается один раз (флаг
/// <c>FirstRunDone</c> в настройках ставит окно по <see cref="Done"/>).
/// </summary>
public sealed class TourViewModel : Observable
{
    public static readonly IReadOnlyList<TourStep> All = new[]
    {
        new TourStep(TourTarget.Nav, "Разделы",
            "«Моды» — всё, что стоит в игре. «Обновления» — что вышло новее на сайтах. «Поиск» — новые моды. «Настройки» — вид и папки."),
        new TourStep(TourTarget.Filters, "Фильтры",
            "Включённые, выключенные, ждущие обновления — в один щелчок. Лупа ищет среди ваших модов (Ctrl+F)."),
        new TourStep(TourTarget.Card, "Карточка мода",
            "Щелчок — подробности справа. Пробел включает и выключает мод, правая кнопка открывает меню."),
        new TourStep(TourTarget.Window, "Установка",
            "Перетащите zip, rar или 7z на окно: программа проверит архив и предложит установить. Скачанное она может замечать сама — «Настройки → Загрузка модов»."),
    };

    /// <param name="hasCards">Есть ли моды в сетке: без них шага «Карточка мода» нет.</param>
    public TourViewModel(bool hasCards)
    {
        Steps = All.Where(s => hasCards || s.Target != TourTarget.Card).ToList();
    }

    public IReadOnlyList<TourStep> Steps { get; }

    private int _index;
    public int Index { get => _index; private set { if (Set(ref _index, value)) RaiseStep(); } }

    public TourStep Current => Steps[Index];
    public string Counter => $"{Index + 1} из {Steps.Count}";
    public bool IsLast => Index == Steps.Count - 1;
    public string NextLabel => IsLast ? "Готово" : "Далее";

    /// <summary>Знакомство кончилось (дошли до конца или «Пропустить»).</summary>
    public event Action? Done;

    private bool _finished;
    public bool Finished { get => _finished; private set => Set(ref _finished, value); }

    public void Next()
    {
        if (Finished) return;
        if (IsLast) { Finish(); return; }
        Index++;
    }

    public void Back()
    {
        if (!Finished && Index > 0) Index--;
    }

    public void Skip() => Finish();

    /// <summary>Сразу к шагу (снимки, --tour N): 0 — первый; за краем — последний.</summary>
    public void GoTo(int index) => Index = Math.Clamp(index, 0, Steps.Count - 1);

    private void Finish()
    {
        if (Finished) return;
        Finished = true;
        Done?.Invoke();
    }

    private void RaiseStep()
    {
        Raise(nameof(Current));
        Raise(nameof(Counter));
        Raise(nameof(IsLast));
        Raise(nameof(NextLabel));
    }
}
