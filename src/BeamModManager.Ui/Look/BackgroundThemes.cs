using BeamModManager.Core.Settings;

namespace BeamModManager.Ui.Look;

/// <summary>Светлое пятно фона: центр (доли ширины и высоты окна), радиус (доля большей стороны окна) и цвет ARGB.</summary>
public sealed record ThemeBlob(double X, double Y, double R, uint Color);

/// <summary>
/// Цвета одной темы фона в одном оформлении. Фон собирается слоями, как на доске «Гараж» (DESIGN.md 3.1), только вместо
/// фото градиент: основа (три точки по диагонали) → плывущая цветная полоса (слой с прозрачностью 0,30) → вуаль сверху
/// вниз (в тёмном оформлении затемняет, в светлом высветляет верх) → три размытых пятна, у каждой темы свой расклад →
/// огоньки.
/// </summary>
public sealed record ThemePalette(
    BackgroundTheme Theme,
    bool Light,
    string Name,
    string Note,
    IReadOnlyList<uint> Base,
    IReadOnlyList<uint> Grade,
    IReadOnlyList<ThemeBlob> Blobs,
    uint Veil,
    double VeilTop,
    double VeilBottom)
{
    /// <summary>Цвет текста прямо на фоне этой палитры (подписи плитки, пустые экраны).</summary>
    public uint Ink => Light ? BackgroundThemes.LightInk : 0xFFFFFFFF;
}

/// <summary>
/// Три темы фона × два оформления (решено 2026-10-15: фото убрано, добавлено светлое оформление).
/// Подбор: в тёмном всё приглушённое, белый текст читается в любой точке; в светлом — пастель, тёмный текст читается в
/// любой точке; самым ярким и насыщенным на экране остаются картинки модов. У каждой темы одна холодная и одна тёплая
/// нота и свой расклад пятен, поэтому смена темы — это перетекание цвета и переезд пятен, без вспышки.
/// Проверяется тестами (BackgroundThemeTests).
/// </summary>
public static class BackgroundThemes
{
    /// <summary>Прозрачность слоя с плывущей полосой.</summary>
    public const double GradeOpacity = 0.30;

    /// <summary>Основной цвет текста в светлом оформлении.</summary>
    public const uint LightInk = 0xFF141B2B;

    /// <summary>
    /// Мягкий край пятна (доля радиуса → доля непрозрачности): плато в центре и долгий спад — пятно читается как
    /// размытый свет, а не как круг.
    /// </summary>
    public static IReadOnlyList<(double At, double Alpha)> Falloff { get; } = new[]
    {
        (0.0, 1.0), (0.3, 0.78), (0.55, 0.45), (0.78, 0.16), (1.0, 0.0),
    };

    // расклады пятен: у каждой темы свой, в обоих оформлениях одинаковый (смена оформления не двигает пятна)
    private static readonly (double X, double Y, double R)[] NightLayout = { (0.06, 0.62, 0.34), (0.66, 0.10, 0.36), (0.40, 0.95, 0.28) };
    private static readonly (double X, double Y, double R)[] DeepLayout = { (0.84, 0.72, 0.36), (0.10, 0.12, 0.32), (0.48, 0.40, 0.24) };
    private static readonly (double X, double Y, double R)[] DuskLayout = { (0.22, 0.26, 0.34), (0.90, 0.18, 0.30), (0.66, 0.92, 0.32) };

    private static ThemeBlob[] Blobs((double X, double Y, double R)[] layout, params uint[] colors) =>
        layout.Select((l, i) => new ThemeBlob(l.X, l.Y, l.R, colors[i])).ToArray();

    private const uint DarkVeil = 0xFF0C1220, LightVeil = 0xFFFFFFFF;

    public static IReadOnlyList<ThemePalette> Dark { get; } = new[]
    {
        // Ночь — как на доске: синяя ночь с сиреневым и тёплым отсветом
        new ThemePalette(BackgroundTheme.Night, false, "Ночь", "синий, сиреневый",
            new uint[] { 0xFF142140, 0xFF1E2843, 0xFF2B2242 },
            new uint[] { 0x9E2F86E0, 0x85845BC8, 0x80F08A4A, 0x852BB8D6, 0x9E2F86E0 },
            Blobs(NightLayout, 0x6A5FB0E8, 0x5E9888E6, 0x4EF0A868), DarkVeil, 0.26, 0.46),
        // Глубина — морская: бирюза и глубокий синий
        new ThemePalette(BackgroundTheme.Deep, false, "Глубина", "бирюзовый, синий",
            new uint[] { 0xFF0E2A34, 0xFF123042, 0xFF13243B },
            new uint[] { 0x9E1F9BB0, 0x8A2F6FD0, 0x8040C8A8, 0x852A8FD8, 0x9E1F9BB0 },
            Blobs(DeepLayout, 0x5840C4D4, 0x584A8AE6, 0x4C5CD8B0), DarkVeil, 0.26, 0.46),
        // Сумерки — сливовые: орхидея, роза и янтарь у горизонта
        new ThemePalette(BackgroundTheme.Dusk, false, "Сумерки", "сливовый, янтарный",
            new uint[] { 0xFF251A3E, 0xFF321E3C, 0xFF3B2034 },
            new uint[] { 0x9E8A4FD0, 0x85C2508E, 0x80F0954E, 0x857A5AD8, 0x9E8A4FD0 },
            Blobs(DuskLayout, 0x60BE78DE, 0x56EA86A8, 0x4EF6B46C), DarkVeil, 0.26, 0.46),
    };

    public static IReadOnlyList<ThemePalette> Light { get; } = new[]
    {
        new ThemePalette(BackgroundTheme.Night, true, "Ночь", "голубой, сиреневый",
            new uint[] { 0xFFDCE4F4, 0xFFE4E2F4, 0xFFEEE4EE },
            new uint[] { 0x802F86E0, 0x70845BC8, 0x66F08A4A, 0x702BB8D6, 0x802F86E0 },
            Blobs(NightLayout, 0x5C78A8E8, 0x52A08EE6, 0x4CF6B47A), LightVeil, 0.18, 0.05),
        new ThemePalette(BackgroundTheme.Deep, true, "Глубина", "бирюзовый, голубой",
            new uint[] { 0xFFD4ECF0, 0xFFDAE8F4, 0xFFE0EAF4 },
            new uint[] { 0x801F9BB0, 0x702F6FD0, 0x6640C8A8, 0x702A8FD8, 0x801F9BB0 },
            Blobs(DeepLayout, 0x5848C4D2, 0x4E5C94E4, 0x4A64D4AA), LightVeil, 0.18, 0.05),
        new ThemePalette(BackgroundTheme.Dusk, true, "Сумерки", "розовый, персиковый",
            new uint[] { 0xFFECE0F0, 0xFFF2E2EA, 0xFFF6E6E0 },
            new uint[] { 0x808A4FD0, 0x70C2508E, 0x66F0954E, 0x707A5AD8, 0x808A4FD0 },
            Blobs(DuskLayout, 0x54C084DE, 0x4CEC94B0, 0x4CF6BA78), LightVeil, 0.18, 0.05),
    };

    /// <summary>Все палитры: сначала тёмные, потом светлые.</summary>
    public static IReadOnlyList<ThemePalette> All { get; } = Dark.Concat(Light).ToList();

    public static ThemePalette Of(BackgroundTheme t, bool light = false)
    {
        var set = light ? Light : Dark;
        return set.FirstOrDefault(p => p.Theme == t) ?? set[0];
    }

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

    /// <summary>Контраст двух цветов по WCAG 2.1.</summary>
    public static double Contrast((double R, double G, double B) a, (double R, double G, double B) b)
    {
        double la = Luminance(a), lb = Luminance(b);
        return (Math.Max(la, lb) + 0.05) / (Math.Min(la, lb) + 0.05);
    }

    /// <summary>
    /// Худшее место фона для текста палитры: перебор точек основы, полосы, обоих краёв вуали и центров пятен. В тёмном
    /// оформлении худшее — самое светлое место, в светлом — самое тёмное.
    /// </summary>
    public static (double R, double G, double B) Worst(ThemePalette p)
    {
        var ink = Rgb(p.Ink);
        (double R, double G, double B)? worst = null;
        foreach (var b in p.Base)
            foreach (var g in p.Grade)
                foreach (var veil in new[] { p.VeilTop, p.VeilBottom })
                    foreach (var blob in p.Blobs.Select(x => (uint?)x.Color).Append(null))
                    {
                        var c = Over(Rgb(b), g, Alpha(g) * GradeOpacity);
                        c = Over(c, p.Veil, veil);
                        if (blob is uint bc) c = Over(c, bc, Alpha(bc));
                        if (worst is null || Contrast(c, ink) < Contrast(worst.Value, ink)) worst = c;
                    }
        return worst!.Value;
    }
}
