namespace BeamModManager.Core.Versioning;

/// <summary>
/// Сравнение размеров с допуском. Сайты пишут размер округлённым («316,40 МБ», «1,2 ГБ»), а у установленного файла он точный,
/// поэтому «не равно» само по себе ничего не значит: без допуска каждый мод выглядел бы обновлённым.
/// Допуск: 2% размера (для файлов от гигабайта 5%: одна цифра после запятой в «ГБ») и не меньше 6 КБ у файлов от 100 КБ
/// (округление до сотых мегабайта), 1 КБ у совсем малых.
/// Мелкая правка внутри архива без смены версии и размера останется незамеченной: это цена отсутствия ложных тревог.
/// </summary>
public static class SizeComparer
{
    private const long MinToleranceSmall = 1024;
    private const long MinToleranceMedium = 6 * 1024;
    private const long GiB = 1024L * 1024 * 1024;

    public static bool Differs(long? a, long? b)
    {
        if (a is not { } x || b is not { } y || x <= 0 || y <= 0) return false; // неизвестное не считаем отличием
        var larger = Math.Max(x, y);
        var fraction = larger >= GiB ? 0.05 : 0.02;
        var tolerance = Math.Max(larger >= 100 * 1024 ? MinToleranceMedium : MinToleranceSmall, (long)(larger * fraction));
        return Math.Abs(x - y) > tolerance;
    }
}
