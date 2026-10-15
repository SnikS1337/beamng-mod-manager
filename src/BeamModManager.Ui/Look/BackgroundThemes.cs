using BeamModManager.Core.Settings;

namespace BeamModManager.Ui.Look;

/// <summary>
/// Цвета одной темы фона (ARGB). Фон собирается слоями, как на доске «Гараж» (DESIGN.md 3.1), только вместо фото
/// тёмный градиент: основа (три точки по диагонали) → плывущая цветная полоса (слой с прозрачностью 0,30) →
/// затемнение сверху вниз (одно на все темы) → три светлых пятна → огоньки.
/// </summary>
public sealed record ThemePalette(
    BackgroundTheme Theme,
    string Name,
    string Note,
    IReadOnlyList<uint> Base,
    IReadOnlyList<uint> Grade,
    IReadOnlyList<uint> Blobs);

/// <summary>
/// Три темы фона (решено 2026-10-15: фото убрано). Подбор: все тёмные и приглушённые, чтобы белый текст читался,
/// а цветные превью модов оставались самым ярким на экране; у каждой одна холодная и одна тёплая нота, поэтому
/// между собой они меняются плавно, без «вспышки». Проверено тестами: контраст белого текста не ниже 7:1 в любой
/// точке фона без стекла.
/// </summary>
public static class BackgroundThemes
{
    /// <summary>Затемнение поверх основы и полосы: от 30 % сверху до 52 % снизу (как на доске).</summary>
    public const uint ShadeColor = 0xFF0C1220;
    public const double ShadeTop = 0.30, ShadeBottom = 0.52;

    /// <summary>Прозрачность слоя с плывущей полосой.</summary>
    public const double GradeOpacity = 0.30;

    public static IReadOnlyList<ThemePalette> All { get; } = new[]
    {
        // Ночь — как на доске: синяя ночь с сиреневым и тёплым отсветом
        new ThemePalette(BackgroundTheme.Night, "Ночь", "синий, сиреневый",
            new uint[] { 0xFF101B33, 0xFF1B2236, 0xFF251C34 },
            new uint[] { 0x9E2F86E0, 0x85845BC8, 0x80F08A4A, 0x852BB8D6, 0x9E2F86E0 },
            new uint[] { 0x5758A9D6, 0x4D8E82D8, 0x42E6A060 }),
        // Глубина — морская: бирюза и глубокий синий, без тёплого
        new ThemePalette(BackgroundTheme.Deep, "Глубина", "бирюзовый, синий",
            new uint[] { 0xFF08202A, 0xFF0C2533, 0xFF0D1B2E },
            new uint[] { 0x9E1F9BB0, 0x8A2F6FD0, 0x8040C8A8, 0x852A8FD8, 0x9E1F9BB0 },
            new uint[] { 0x5238B4C6, 0x4C3F7CD6, 0x3E52C8A2 }),
        // Сумерки — сливовые: орхидея, роза и янтарь у горизонта
        new ThemePalette(BackgroundTheme.Dusk, "Сумерки", "сливовый, янтарный",
            new uint[] { 0xFF1C1331, 0xFF27172F, 0xFF30182A },
            new uint[] { 0x9E8A4FD0, 0x85C2508E, 0x80F0954E, 0x857A5AD8, 0x9E8A4FD0 },
            new uint[] { 0x4EB06ACF, 0x46E0789A, 0x40F2AE62 }),
    };

    public static ThemePalette Of(BackgroundTheme t) => All.FirstOrDefault(p => p.Theme == t) ?? All[0];

    // ───────────── проверка читаемости (используют тесты и подбор цветов) ─────────────

    /// <summary>Наложить цвет с прозрачностью alpha (0..1) на непрозрачный.</summary>
    public static (double R, double G, double B) Over((double R, double G, double B) under, uint argb, double alpha)
    {
        var (r, g, b) = Rgb(argb);
        return (under.R + (r - under.R) * alpha, under.G + (g - under.G) * alpha, under.B + (b - under.B) * alpha);
    }

    public static (double R, double G, double B) Rgb(uint argb) => ((argb >> 16) & 0xFF, (argb >> 8) & 0xFF, argb & 0xFF);

    public static double Alpha(uint argb) => ((argb >> 24) & 0xFF) / 255.0;

    /// <summary>Относительная яркость по WCAG 2.1.</summary>
    public static double Luminance((double R, double G, double B) c)
    {
        static double Lin(double v) { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.Pow((v + 0.055) / 1.055, 2.4); }
        return 0.2126 * Lin(c.R) + 0.7152 * Lin(c.G) + 0.0722 * Lin(c.B);
    }

    /// <summary>Контраст белого текста на цвете.</summary>
    public static double WhiteContrast((double R, double G, double B) c) => 1.05 / (Luminance(c) + 0.05);

    /// <summary>
    /// Самое светлое место фона темы: каждая точка основы, самая яркая точка полосы, светлое (верхнее) затемнение и
    /// центр самого яркого пятна — худший случай для белого текста.
    /// </summary>
    public static (double R, double G, double B) Brightest(ThemePalette p)
    {
        (double R, double G, double B) best = (0, 0, 0);
        foreach (var b in p.Base)
            foreach (var g in p.Grade)
                foreach (var blob in p.Blobs)
                {
                    var c = Over(Rgb(b), g, Alpha(g) * GradeOpacity);
                    c = Over(c, ShadeColor, ShadeTop);
                    c = Over(c, blob, Alpha(blob));
                    if (Luminance(c) > Luminance(best)) best = c;
                }
        return best;
    }
}
