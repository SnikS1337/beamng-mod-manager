namespace BeamModManager.Ui.Look;

/// <summary>Как показан заголовок раздела рядом с островком.</summary>
public enum TitleFit
{
    /// <summary>Крупный заголовок; при прокрутке сжимается в капсулу.</summary>
    Full,
    /// <summary>Крупный не помещается слева от островка: заголовок сразу капсулой.</summary>
    Compact,
    /// <summary>Не помещается и капсула: заголовка нет (раздел и так виден по капле слева).</summary>
    Hidden,
}

/// <summary>Где стоит островок (по горизонтали, в координатах окна) и как показан заголовок.</summary>
public readonly record struct IslandPlacement(double X, double Width, TitleFit Title);

/// <summary>
/// Раскладка верха (DESIGN.md 4, «Островки», решено 2026-10-17): островок — точно посередине окна, как Dynamic Island,
/// где бы ни была правая панель. Слева от него заголовок раздела, справа кнопки окна.
/// <para>Окно сужается — уступает заголовок, а не островок: крупный → капсула → нет заголовка. Только если островку
/// не хватает места до кнопок окна (на минимальной ширине 1000 этого не бывает), он сдвигается от них влево.</para>
/// <para>Как показать заголовок, решается по конечной ширине островка (<c>target</c>), а не по ширине в пути: пока
/// островок перетекает в поиск и обратно, заголовок не мигает.</para>
/// </summary>
public static class IslandLayout
{
    /// <summary>Поле между заголовком и островком.</summary>
    public const double Gap = 16;

    /// <summary>Поиск раскрыт: капсула 320–440 px, примерно половина ряда.</summary>
    public static double SearchWidth(double rowWidth)
    {
        rowWidth = Clean(rowWidth);
        return Math.Max(0, Math.Min(rowWidth, Math.Clamp(rowWidth * 0.5, 320, 440)));
    }

    /// <param name="windowWidth">Ширина окна.</param>
    /// <param name="left">Левый край ряда (за левой панелью): здесь начинается заголовок.</param>
    /// <param name="right">Правый край ряда (перед кнопками окна).</param>
    /// <param name="width">Ширина островка сейчас (в пути — промежуточная).</param>
    /// <param name="target">Ширина, к которой островок перетекает.</param>
    /// <param name="bigTitle">Ширина крупного заголовка со строкой под ним.</param>
    /// <param name="capsuleTitle">Ширина капсулы заголовка.</param>
    public static IslandPlacement Place(double windowWidth, double left, double right, double width, double target,
        double bigTitle, double capsuleTitle)
    {
        windowWidth = Clean(windowWidth);
        left = Clean(left);
        right = Math.Max(left, Clean(right));
        var room = right - left;
        width = Math.Min(Clean(width), room);
        target = Math.Min(Clean(target), room);

        var x = Center(windowWidth, left, right, width);
        var at = Center(windowWidth, left, right, target);
        var fit = at >= left + Clean(bigTitle) + Gap ? TitleFit.Full
            : at >= left + Clean(capsuleTitle) + Gap ? TitleFit.Compact
            : TitleFit.Hidden;
        return new IslandPlacement(Math.Round(x), width, fit);
    }

    private static double Center(double windowWidth, double left, double right, double width)
    {
        var x = (windowWidth - width) / 2;
        if (x + width > right) x = right - width; // не под кнопками окна
        return Math.Max(left, x);
    }

    private static double Clean(double v) => double.IsFinite(v) && v > 0 ? v : 0;
}
