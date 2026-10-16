using System.Runtime.CompilerServices;

namespace BeamModManager.Ui.Glass;

/// <summary>
/// Текстуры стеклянных панелей, считаются в программе под любой размер панели
/// (перенос design/mockup/tools/gen.py; иней убран 2026-10-16).
/// Все функции чистые: можно звать из разных потоков.
/// Пиксели идут строками сверху вниз, 4 байта на пиксель в порядке B, G, R, A (шаг строки = ширина * 4).
/// </summary>
public static class GlassTextures
{
    private const double Diag = 0.70710678118654752;
    private const MethodImplOptions Hot = MethodImplOptions.AggressiveOptimization;

    /// <summary>
    /// Расстояние от края скруглённого прямоугольника внутрь (снаружи отрицательное).
    /// x, y — номер пикселя, берётся его центр (+0.5).
    /// </summary>
    public static double InsideDistance(double x, double y, int width, int height, double radius)
    {
        double r = ClampRadius(width, height, radius);
        return Inside(x, y, width * 0.5, height * 0.5, r);
    }

    /// <summary>
    /// Блик по краю: белый, свет сверху слева, слабая кромка с другой стороны и мягкое свечение внутрь.
    /// Возвращает предумноженный BGRA (Pbgra32). lens — ширина свечения в пикселях.
    /// </summary>
    public static byte[] Rim(int width, int height, double radius, double lens)
    {
        if (width <= 0 || height <= 0) return Array.Empty<byte>();
        double r = ClampRadius(width, height, radius);
        double b = PositiveOr(lens);
        var d = DistanceField(width, height, r);
        var px = new byte[width * height * 4];
        Bands(width, height, (y0, y1) => RimRows(d, px, width, height, b, y0, y1));
        return px;
    }

    /// <summary>
    /// Карта смещения для преломления: R и G — нормаль внутрь, умноженная на силу (128 = ноль),
    /// B = 128, A = 255, без предумножения. lens — на сколько пикселей от края действует преломление.
    /// </summary>
    public static byte[] Displacement(int width, int height, double radius, double lens)
    {
        if (width <= 0 || height <= 0) return Array.Empty<byte>();
        double r = ClampRadius(width, height, radius);
        double b = PositiveOr(lens);
        var d = DistanceField(width, height, r);
        var px = new byte[width * height * 4];
        Bands(width, height, (y0, y1) => DisplacementRows(d, px, width, height, b, y0, y1));
        return px;
    }

    // ---------- проходы по строкам ----------

    [MethodImpl(Hot)]
    private static void RimRows(double[] d, byte[] px, int w, int h, double b, int y0, int y1)
    {
        for (int y = y0; y < y1; y++)
        {
            int row = y * w;
            for (int x = 0; x < w; x++)
            {
                double dd = d[row + x];
                if (dd < 0) continue;
                Gradient(d, w, h, x, y, out double gx, out double gy);
                double gl = Math.Sqrt(gx * gx + gy * gy);
                // свет сверху слева: косинус между внешней нормалью и направлением на свет
                double light = gl > 0 ? (gx + gy) / gl * Diag : 0;
                double rim = Clamp01(1 - dd / 3.0);
                double glow = Clamp01(1 - dd / b);
                glow = glow * glow * glow;
                double a1 = rim * (0.35 + 0.65 * Clamp01(light));
                double a2 = rim * 0.25 * Clamp01(-light);
                double alpha = Clamp01(a1 * 0.95 + a2 * 0.6 + glow * 0.10);
                byte v = (byte)Math.Round(alpha * 255);
                int i = (row + x) * 4;
                px[i] = v; px[i + 1] = v; px[i + 2] = v; px[i + 3] = v;
            }
        }
    }

    [MethodImpl(Hot)]
    private static void DisplacementRows(double[] d, byte[] px, int w, int h, double b, int y0, int y1)
    {
        for (int y = y0; y < y1; y++)
        {
            int row = y * w;
            for (int x = 0; x < w; x++)
            {
                double vx = 0, vy = 0;
                double dd = d[row + x];
                if (dd >= 0)
                {
                    Gradient(d, w, h, x, y, out double gx, out double gy);
                    double nrm = Math.Sqrt(gx * gx + gy * gy) + 1e-6;
                    double t = Clamp01(1 - dd / b);
                    double mag = t * t;
                    vx = gx / nrm * mag;
                    vy = gy / nrm * mag;
                }
                int i = (row + x) * 4;
                px[i] = 128;
                px[i + 1] = (byte)Math.Round((vy * 0.5 + 0.5) * 255);
                px[i + 2] = (byte)Math.Round((vx * 0.5 + 0.5) * 255);
                px[i + 3] = 255;
            }
        }
    }

    // ---------- общее ----------

    private static double ClampRadius(int w, int h, double r)
    {
        double max = Math.Min(Math.Max(w, 0), Math.Max(h, 0)) * 0.5;
        if (double.IsNaN(r) || r < 0) return 0;
        return Math.Min(r, max);
    }

    private static double PositiveOr(double v) => v > 1e-3 ? v : 1e-3;

    private static double Clamp01(double v) => v < 0 ? 0 : v > 1 ? 1 : v;

    private static double Inside(double x, double y, double hw, double hh, double r)
    {
        double qx = Math.Abs(x + 0.5 - hw) - (hw - r);
        double qy = Math.Abs(y + 0.5 - hh) - (hh - r);
        double ox = Math.Max(qx, 0), oy = Math.Max(qy, 0);
        double outside = Math.Sqrt(ox * ox + oy * oy);
        double inside = Math.Min(Math.Max(qx, qy), 0);
        return -(outside + inside - r);
    }

    private static double[] DistanceField(int w, int h, double r)
    {
        var d = new double[w * h];
        double hw = w * 0.5, hh = h * 0.5;
        Bands(w, h, (y0, y1) =>
        {
            for (int y = y0; y < y1; y++)
                for (int x = 0, row = y * w; x < w; x++) d[row + x] = Inside(x, y, hw, hh, r);
        });
        return d;
    }

    /// <summary>Производная как numpy.gradient: в середине центральная разность, по краям односторонняя.</summary>
    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    private static void Gradient(double[] d, int w, int h, int x, int y, out double gx, out double gy)
    {
        int i = y * w + x;
        if (w < 2) gx = 0;
        else if (x == 0) gx = d[i + 1] - d[i];
        else if (x == w - 1) gx = d[i] - d[i - 1];
        else gx = (d[i + 1] - d[i - 1]) * 0.5;

        if (h < 2) gy = 0;
        else if (y == 0) gy = d[i + w] - d[i];
        else if (y == h - 1) gy = d[i] - d[i - w];
        else gy = (d[i + w] - d[i - w]) * 0.5;
    }

    /// <summary>Делит строки на полосы и считает их параллельно; маленькие картинки — в одном потоке.</summary>
    private static void Bands(int w, int h, Action<int, int> body)
    {
        long size = (long)w * h;
        int n = size < 40_000 ? 1 : Math.Min(h, Environment.ProcessorCount * 3);
        if (n <= 1) { body(0, h); return; }
        Parallel.For(0, n, b => body((int)((long)h * b / n), (int)((long)h * (b + 1) / n)));
    }
}
