namespace BeamModManager.Ui.Look;

public enum ShapeKind { Pill, Circle, Square }

/// <summary>Фигура под стеклом превью: центр и размер в точках превью, поворот в градусах, заливка от и до (ARGB, непрозрачные).</summary>
public sealed record PreviewShape(ShapeKind Kind, double X, double Y, double W, double H, double Angle, uint From, uint To)
{
    /// <summary>Скругление: капсула и круг — половина меньшей стороны, квадрат — мягкий угол.</summary>
    public double Corner => Kind == ShapeKind.Square ? Math.Min(W, H) * 0.24 : Math.Min(W, H) / 2;
}

/// <summary>
/// «Как это выглядит» в настройках (решено 2026-10-17): под стеклом не картинки модов, а несколько ярких мягких фигур в
/// цветах выбранной темы фона. Стекло на них видно лучше всего, и превью одинаково на любом гараже, даже пустом.
/// Расклад привязан к высоте превью (фигуры не растягиваются), по ширине — доли: на узком и широком превью видно одно и то же.
/// Круг всегда заходит под островок посередине сверху — там преломление и капля.
/// </summary>
public static class PreviewShapes
{
    public static IReadOnlyList<PreviewShape> For(ThemePalette p, double width, double height)
    {
        if (!(width > 0) || !(height > 0) || double.IsInfinity(width) || double.IsInfinity(height)) return Array.Empty<PreviewShape>();
        uint Col(int i) => Opaque(p.Grade[i % p.Grade.Count], p.Light);
        var h = height;
        return new[]
        {
            // капсула слева снизу, чуть наискось
            new PreviewShape(ShapeKind.Pill, width * 0.22, h * 0.66, h * 1.25, h * 0.36, -14, Col(0), Mix(Col(0), Col(1), 0.6)),
            // тёплый круг под островком посередине
            new PreviewShape(ShapeKind.Circle, width * 0.5, h * 0.34, h * 0.62, h * 0.62, 0, Col(2), Mix(Col(2), Col(1), 0.45)),
            // повёрнутый квадрат справа снизу
            new PreviewShape(ShapeKind.Square, width * 0.76, h * 0.70, h * 0.48, h * 0.48, 18, Col(3), Mix(Col(3), Col(0), 0.5)),
            // маленький круг справа сверху
            new PreviewShape(ShapeKind.Circle, width * 0.91, h * 0.22, h * 0.24, h * 0.24, 0, Col(1), Mix(Col(1), Col(2), 0.5)),
        };
    }

    /// <summary>Цвет полосы темы без прозрачности; в светлом оформлении чуть мягче, чтобы не резал глаз на пастели.</summary>
    private static uint Opaque(uint argb, bool light)
    {
        var c = argb | 0xFF000000;
        return light ? Mix(c, 0xFFFFFFFF, 0.18) : c;
    }

    private static uint Mix(uint a, uint b, double t)
    {
        static uint Ch(uint x, int s) => (x >> s) & 0xFF;
        uint M(int s) => (uint)Math.Round(Ch(a, s) + (Ch(b, s) - (double)Ch(a, s)) * t);
        return 0xFF000000 | (M(16) << 16) | (M(8) << 8) | M(0);
    }
}
