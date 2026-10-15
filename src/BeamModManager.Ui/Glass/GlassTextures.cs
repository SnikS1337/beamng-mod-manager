using System.Numerics;
using System.Runtime.CompilerServices;

namespace BeamModManager.Ui.Glass;

/// <summary>
/// Текстуры стеклянных панелей, считаются в программе под любой размер панели
/// (перенос design/mockup/tools/gen.py и gen_ice.py).
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

    /// <summary>
    /// Иней по краям панели: «пальцы» растут от края внутрь, зерно, редкие прожилки, лёгкая дымка, искры.
    /// Возвращает предумноженный BGRA (Pbgra32). reach — насколько иней заходит внутрь (пиксели),
    /// seed — зерно случайности (одно зерно — одна и та же картинка).
    /// </summary>
    public static byte[] Ice(int width, int height, double radius, double reach, int seed)
    {
        if (width <= 0 || height <= 0) return Array.Empty<byte>();
        int w = width, h = height, n = w * h;
        double r = ClampRadius(w, h, radius);
        double f = PositiveOr(reach);
        var rng = new Rng(seed);

        var d = DistanceField(w, h, r);
        var big = new float[n];
        var mid = new float[n];
        var frost = new float[n];
        // порядок шумов как в gen_ice.py
        Fractal(rng, w, h, 7, 3, 0.6, big);    // мелкие пальцы
        Fractal(rng, w, h, 22, 3, 0.55, mid);  // длина пальцев
        Bands(w, h, (y0, y1) => FrostRows(d, big, mid, frost, w, h, f, y0, y1));

        // дальше big — рабочий буфер для шума, mid — сумма прозрачности
        var buf = big;
        var acc = mid;
        Fractal(rng, w, h, 5, 2, 0.55, buf);   // зернистость внутри инея
        Bands(w, h, (y0, y1) => GrainRows(frost, buf, w, y0, y1));
        Fractal(rng, w, h, 30, 4, 0.5, buf);   // редкие тонкие прожилки
        Bands(w, h, (y0, y1) => VeinRows(d, frost, buf, acc, w, f, y0, y1));
        Fractal(rng, w, h, 2, 1, 0.55, buf);   // зерно по всей поверхности
        Bands(w, h, (y0, y1) => AddScaled(acc, buf, 0.03f, y0 * w, y1 * w));
        Fractal(rng, w, h, 90, 3, 0.55, buf);  // лёгкая дымка
        Bands(w, h, (y0, y1) => MistRows(acc, buf, w, y0, y1));
        Fractal(rng, w, h, 2, 1, 0.55, buf);   // искорки у края
        Bands(w, h, (y0, y1) => SparkleRows(d, acc, buf, w, f, y0, y1));

        // лёгкое размытие (гаусс, сигма 0.5), потом цвет
        GaussBlur05(acc, buf, w, h);
        var px = new byte[n * 4];
        Bands(w, h, (y0, y1) => IceColourRows(d, acc, frost, px, w, y0, y1));
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

    /// <summary>Основа инея: шум берётся в ближайшей точке края, поэтому тянется полосами внутрь.</summary>
    [MethodImpl(Hot)]
    private static void FrostRows(double[] d, float[] big, float[] mid, float[] frost, int w, int h, double f, int y0, int y1)
    {
        double far = f * 1.5; // дальше самого длинного пальца инея нет
        for (int y = y0; y < y1; y++)
        {
            int row = y * w;
            for (int x = 0; x < w; x++)
            {
                double dd = d[row + x];
                if (dd < 0) { frost[row + x] = 1; continue; }
                if (dd >= far) { frost[row + x] = 0; continue; }
                Gradient(d, w, h, x, y, out double gx, out double gy);
                double gn = Math.Sqrt(gx * gx + gy * gy) + 1e-6;
                double bx = Math.Clamp(x - dd * gx / gn, 0, w - 1);
                double by = Math.Clamp(y - dd * gy / gn, 0, h - 1);
                double nb = Bilinear(big, w, h, bx, by);
                double nm = Bilinear(mid, w, h, bx, by);
                double re = f * (0.25 + 1.25 * (0.65 * nm + 0.35 * nb));
                double t = 1 - dd / re;
                frost[row + x] = t <= 0 ? 0f : (float)Math.Pow(t, 1.6);
            }
        }
    }

    [MethodImpl(Hot)]
    private static void GrainRows(float[] frost, float[] noise, int w, int y0, int y1)
    {
        for (int i = y0 * w, e = y1 * w; i < e; i++)
            frost[i] *= (float)(0.7 + 0.3 * Smooth(0.3, 0.7, noise[i]));
    }

    [MethodImpl(Hot)]
    private static void VeinRows(double[] d, float[] frost, float[] noise, float[] acc, int w, double f, int y0, int y1)
    {
        double span = f * 2.2;
        for (int i = y0 * w, e = y1 * w; i < e; i++)
        {
            double a = frost[i] * 0.62;
            double fade = 1 - d[i] / span;
            if (fade > 0)
            {
                double ridge = 1 - Math.Abs(noise[i] - 0.5) * 2;
                a += Smooth(0.95, 0.992, ridge) * Math.Min(fade, 1) * 0.16;
            }
            acc[i] = (float)a;
        }
    }

    [MethodImpl(Hot)]
    private static void MistRows(float[] acc, float[] noise, int w, int y0, int y1)
    {
        for (int i = y0 * w, e = y1 * w; i < e; i++)
            acc[i] += (float)(0.035 * Smooth(0.2, 0.8, noise[i]));
    }

    [MethodImpl(Hot)]
    private static void SparkleRows(double[] d, float[] acc, float[] noise, int w, double f, int y0, int y1)
    {
        double span = f * 1.3;
        for (int i = y0 * w, e = y1 * w; i < e; i++)
        {
            double dd = d[i];
            if (dd < 0) { acc[i] = 0; continue; }
            double a = acc[i];
            double fade = 1 - dd / span;
            if (fade > 0) a += Smooth(0.84, 0.95, noise[i]) * Math.Min(fade, 1) * 0.40;
            acc[i] = (float)Math.Clamp(a, 0, 0.9);
        }
    }

    [MethodImpl(Hot)]
    private static void IceColourRows(double[] d, float[] alpha, float[] frost, byte[] px, int w, int y0, int y1)
    {
        for (int i = y0 * w, e = y1 * w; i < e; i++)
        {
            if (d[i] < 0) continue;
            int a = (int)Math.Clamp(alpha[i] * 255.0, 0, 255);
            if (a == 0) continue;
            // холодный оттенок там, где иней плотнее
            double cold = Clamp01(frost[i] * 0.9);
            int cr = (int)(246 - 26 * cold);
            int cg = (int)(251 - 12 * cold);
            int k = i * 4;
            px[k] = (byte)((255 * a + 127) / 255);
            px[k + 1] = (byte)((cg * a + 127) / 255);
            px[k + 2] = (byte)((cr * a + 127) / 255);
            px[k + 3] = (byte)a;
        }
    }

    [MethodImpl(Hot)]
    private static void AddScaled(float[] dst, float[] src, float k, int from, int to)
    {
        var dd = dst.AsSpan(from, to - from);
        var ss = src.AsSpan(from, to - from);
        int i = 0, vc = Vector<float>.Count;
        if (Vector.IsHardwareAccelerated)
        {
            var vk = new Vector<float>(k);
            for (; i <= dd.Length - vc; i += vc)
                (new Vector<float>(dd.Slice(i)) + vk * new Vector<float>(ss.Slice(i))).CopyTo(dd.Slice(i));
        }
        for (; i < dd.Length; i++) dd[i] += k * ss[i];
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

    private static double Smooth(double e0, double e1, double x)
    {
        double t = Clamp01((x - e0) / (e1 - e0));
        return t * t * (3 - 2 * t);
    }

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

    /// <summary>Билинейная выборка (как scipy map_coordinates, order=1) внутри картинки.</summary>
    private static double Bilinear(float[] a, int w, int h, double x, double y)
    {
        int x0 = (int)x, y0 = (int)y;
        double fx = x - x0, fy = y - y0;
        int x1 = x0 + 1 < w ? x0 + 1 : x0;
        int y1 = y0 + 1 < h ? y0 + 1 : y0;
        double top = a[y0 * w + x0] * (1 - fx) + a[y0 * w + x1] * fx;
        double bot = a[y1 * w + x0] * (1 - fx) + a[y1 * w + x1] * fx;
        return top * (1 - fy) + bot * fy;
    }

    /// <summary>Делит строки на полосы и считает их параллельно; маленькие картинки — в одном потоке.</summary>
    private static void Bands(int w, int h, Action<int, int> body)
    {
        long size = (long)w * h;
        int n = size < 40_000 ? 1 : Math.Min(h, Environment.ProcessorCount * 3);
        if (n <= 1) { body(0, h); return; }
        Parallel.For(0, n, b => body((int)((long)h * b / n), (int)((long)h * (b + 1) / n)));
    }

    // ---------- шум ----------

    private sealed class Octave
    {
        public required float[] Rows;   // сетка, уже растянутая по ширине: (строк сетки) x w
        public required Taps Taps;      // веса по высоте
        public required float Amp;
    }

    /// <summary>
    /// Шум 0..1 в o: сумма сглаженных случайных сеток разной крупности (как fractal в gen_ice.py).
    /// Сетка с шагом s растягивается бикубически, как PIL BICUBIC, на (w + 2s) x (h + 2s), берётся левый верхний угол.
    /// Как и в PIL, после каждого прохода значения округляются и обрезаются до 0..255 —
    /// от этого зависят частота искр и зерна после растяжки на 0..1.
    /// </summary>
    private static void Fractal(Rng rng, int w, int h, int baseSize, int octaves, double gain, float[] o)
    {
        var oct = new Octave[octaves];
        double amp = 1, tot = 0;
        for (int k = 0; k < octaves; k++)
        {
            int s = Math.Max(2, (int)(baseSize / Math.Pow(2, k)));
            int gh = h / s + 3, gw = w / s + 3;
            var grid = new float[gh * gw];
            rng.FillBytes(grid);

            var cx = Coeffs(gw, w + 2 * s, w);
            var rows = new float[gh * w];
            Bands(w, gh, (y0, y1) => StretchRows(grid, gw, cx, rows, w, y0, y1));
            oct[k] = new Octave { Rows = rows, Taps = Coeffs(gh, h + 2 * s, h), Amp = (float)amp };
            tot += amp;
            amp *= gain;
        }

        float mn = float.MaxValue, mx = float.MinValue;
        var gate = new object();
        Bands(w, h, (y0, y1) =>
        {
            StackRows(oct, o, w, y0, y1, out float lo, out float hi);
            lock (gate) { if (lo < mn) mn = lo; if (hi > mx) mx = hi; }
        });

        // растяжка на 0..1: (v / tot - min) / (max - min + 1e-9)
        double inv = 1.0 / tot;
        double span = (mx - mn) * inv + 1e-9;
        float ka = (float)(inv / span), kb = (float)(-mn * inv / span);
        Bands(w, h, (y0, y1) => Affine(o, ka, kb, y0 * w, y1 * w));
    }

    /// <summary>Растяжение строк сетки по ширине.</summary>
    [MethodImpl(Hot)]
    private static void StretchRows(float[] grid, int gw, Taps cx, float[] rows, int w, int y0, int y1)
    {
        int[] start = cx.Start, count = cx.Count;
        float[] wt = cx.F;
        for (int gy = y0; gy < y1; gy++)
        {
            var g = grid.AsSpan(gy * gw, gw);
            var dst = rows.AsSpan(gy * w, w);
            for (int x = 0; x < w; x++)
            {
                int k0 = start[x], kk = x * 5, cnt = count[x];
                float v;
                if (cnt == 4)
                    v = wt[kk] * g[k0] + wt[kk + 1] * g[k0 + 1] + wt[kk + 2] * g[k0 + 2] + wt[kk + 3] * g[k0 + 3];
                else
                {
                    v = 0;
                    for (int k = 0; k < cnt; k++) v += wt[kk + k] * g[k0 + k];
                }
                dst[x] = Quant(v);
            }
        }
    }

    /// <summary>Растяжение по высоте и сложение октав; заодно наименьшее и наибольшее значение полосы.</summary>
    [MethodImpl(Hot)]
    private static void StackRows(Octave[] oct, float[] o, int w, int y0, int y1, out float lo, out float hi)
    {
        var line = new float[w];
        lo = float.MaxValue; hi = float.MinValue;
        int vc = Vector<float>.Count;
        bool simd = Vector.IsHardwareAccelerated;
        var v0 = Vector<float>.Zero;
        var vHalf = new Vector<float>(0.5f);
        var v255 = new Vector<float>(255f);
        for (int y = y0; y < y1; y++)
        {
            var orow = o.AsSpan(y * w, w);
            orow.Clear();
            foreach (var oc in oct)
            {
                var ln = line.AsSpan();
                ln.Clear();
                int k0 = oc.Taps.Start[y], kk = y * 5, cnt = oc.Taps.Count[y];
                for (int k = 0; k < cnt; k++)
                {
                    float wk = oc.Taps.F[kk + k];
                    var src = oc.Rows.AsSpan((k0 + k) * w, w);
                    int i = 0;
                    if (simd)
                    {
                        var vw = new Vector<float>(wk);
                        for (; i <= w - vc; i += vc)
                            (new Vector<float>(ln.Slice(i)) + vw * new Vector<float>(src.Slice(i))).CopyTo(ln.Slice(i));
                    }
                    for (; i < w; i++) ln[i] += wk * src[i];
                }
                // округление и обрезка до 0..255, как в PIL, и в сумму с весом октавы
                float amp = oc.Amp;
                int j = 0;
                if (simd)
                {
                    var va = new Vector<float>(amp);
                    for (; j <= w - vc; j += vc)
                    {
                        var v = Vector.Max(new Vector<float>(ln.Slice(j)) + vHalf, v0);
                        var q = Vector.Min(Vector.ConvertToSingle(Vector.ConvertToInt32(v)), v255);
                        (new Vector<float>(orow.Slice(j)) + va * q).CopyTo(orow.Slice(j));
                    }
                }
                for (; j < w; j++) orow[j] += amp * Quant(ln[j]);
            }
            int x = 0;
            if (simd && w >= vc)
            {
                var vlo = new Vector<float>(lo);
                var vhi = new Vector<float>(hi);
                for (; x <= w - vc; x += vc)
                {
                    var v = new Vector<float>(orow.Slice(x));
                    vlo = Vector.Min(vlo, v);
                    vhi = Vector.Max(vhi, v);
                }
                for (int k = 0; k < vc; k++)
                {
                    if (vlo[k] < lo) lo = vlo[k];
                    if (vhi[k] > hi) hi = vhi[k];
                }
            }
            for (; x < w; x++)
            {
                float v = orow[x];
                if (v < lo) lo = v;
                if (v > hi) hi = v;
            }
        }
    }

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    private static float Quant(float v)
    {
        v += 0.5f;
        if (v <= 0) return 0;
        float q = (int)v;
        return q > 255 ? 255 : q;
    }

    [MethodImpl(Hot)]
    private static void Affine(float[] o, float ka, float kb, int from, int to)
    {
        var s = o.AsSpan(from, to - from);
        int i = 0, vc = Vector<float>.Count;
        if (Vector.IsHardwareAccelerated)
        {
            var va = new Vector<float>(ka);
            var vb = new Vector<float>(kb);
            for (; i <= s.Length - vc; i += vc) (new Vector<float>(s.Slice(i)) * va + vb).CopyTo(s.Slice(i));
        }
        for (; i < s.Length; i++) s[i] = s[i] * ka + kb;
    }

    private sealed class Taps
    {
        public required int[] Start;
        public required int[] Count;
        public required float[] F;
    }

    /// <summary>Веса бикубического растяжения как в PIL (a = -0.5, окно обрезается краем и нормируется).</summary>
    private static Taps Coeffs(int inSize, int outSize, int used)
    {
        double scale = (double)inSize / outSize;
        var t = new Taps { Start = new int[used], Count = new int[used], F = new float[used * 5] };
        Span<double> wv = stackalloc double[5];
        for (int xx = 0; xx < used; xx++)
        {
            double center = (xx + 0.5) * scale;
            int xmin = (int)(center - 2 + 0.5);
            if (xmin < 0) xmin = 0;
            int xmax = (int)(center + 2 + 0.5);
            if (xmax > inSize) xmax = inSize;
            int cnt = Math.Min(xmax - xmin, 5);
            double sum = 0;
            for (int k = 0; k < cnt; k++)
            {
                wv[k] = Cubic(k + xmin - center + 0.5);
                sum += wv[k];
            }
            if (sum == 0) sum = 1;
            for (int k = 0; k < cnt; k++) t.F[xx * 5 + k] = (float)(wv[k] / sum);
            t.Start[xx] = xmin;
            t.Count[xx] = cnt;
        }
        return t;
    }

    private static double Cubic(double x)
    {
        const double a = -0.5;
        if (x < 0) x = -x;
        if (x < 1) return ((a + 2) * x - (a + 3)) * x * x + 1;
        if (x < 2) return (((x - 5) * x + 8) * x - 4) * a;
        return 0;
    }

    // ---------- размытие ----------

    /// <summary>Гаусс с сигмой 0.5 (ядро 5 точек, края отражаются, как scipy gaussian_filter). Результат в src, tmp — рабочий.</summary>
    private static void GaussBlur05(float[] src, float[] tmp, int w, int h)
    {
        double e2 = Math.Exp(-2), e8 = Math.Exp(-8), sum = 1 + 2 * e2 + 2 * e8;
        float k2 = (float)(1 / sum), k1 = (float)(e2 / sum), k0 = (float)(e8 / sum);
        Bands(w, h, (y0, y1) => BlurRowsX(src, tmp, w, k0, k1, k2, y0, y1));
        Bands(w, h, (y0, y1) => BlurRowsY(tmp, src, w, h, k0, k1, k2, y0, y1));
    }

    [MethodImpl(Hot)]
    private static void BlurRowsX(float[] src, float[] dst, int w, float k0, float k1, float k2, int y0, int y1)
    {
        for (int y = y0; y < y1; y++)
        {
            var s = src.AsSpan(y * w, w);
            var d = dst.AsSpan(y * w, w);
            int x = 0;
            if (w >= 5)
            {
                // середина строки без отражений
                int vc = Vector<float>.Count, m = w - 4;
                if (Vector.IsHardwareAccelerated)
                {
                    var a0 = new Vector<float>(k0); var a1 = new Vector<float>(k1); var a2 = new Vector<float>(k2);
                    for (; x <= m - vc; x += vc)
                    {
                        var v = a0 * (new Vector<float>(s.Slice(x)) + new Vector<float>(s.Slice(x + 4)))
                              + a1 * (new Vector<float>(s.Slice(x + 1)) + new Vector<float>(s.Slice(x + 3)))
                              + a2 * new Vector<float>(s.Slice(x + 2));
                        v.CopyTo(d.Slice(x + 2));
                    }
                }
                for (; x < m; x++)
                    d[x + 2] = k0 * (s[x] + s[x + 4]) + k1 * (s[x + 1] + s[x + 3]) + k2 * s[x + 2];
                for (int e = 0; e < 2; e++)
                {
                    BlurPointX(s, d, w, e, k0, k1, k2);
                    BlurPointX(s, d, w, w - 1 - e, k0, k1, k2);
                }
            }
            else
            {
                for (x = 0; x < w; x++) BlurPointX(s, d, w, x, k0, k1, k2);
            }
        }
    }

    private static void BlurPointX(Span<float> s, Span<float> d, int w, int x, float k0, float k1, float k2)
    {
        d[x] = k0 * (s[Reflect(x - 2, w)] + s[Reflect(x + 2, w)])
             + k1 * (s[Reflect(x - 1, w)] + s[Reflect(x + 1, w)])
             + k2 * s[x];
    }

    [MethodImpl(Hot)]
    private static void BlurRowsY(float[] src, float[] dst, int w, int h, float k0, float k1, float k2, int y0, int y1)
    {
        int vc = Vector<float>.Count;
        for (int y = y0; y < y1; y++)
        {
            var r0 = src.AsSpan(Reflect(y - 2, h) * w, w);
            var r1 = src.AsSpan(Reflect(y - 1, h) * w, w);
            var r2 = src.AsSpan(y * w, w);
            var r3 = src.AsSpan(Reflect(y + 1, h) * w, w);
            var r4 = src.AsSpan(Reflect(y + 2, h) * w, w);
            var d = dst.AsSpan(y * w, w);
            int x = 0;
            if (Vector.IsHardwareAccelerated)
            {
                var a0 = new Vector<float>(k0); var a1 = new Vector<float>(k1); var a2 = new Vector<float>(k2);
                for (; x <= w - vc; x += vc)
                {
                    var v = a0 * (new Vector<float>(r0.Slice(x)) + new Vector<float>(r4.Slice(x)))
                          + a1 * (new Vector<float>(r1.Slice(x)) + new Vector<float>(r3.Slice(x)))
                          + a2 * new Vector<float>(r2.Slice(x));
                    v.CopyTo(d.Slice(x));
                }
            }
            for (; x < w; x++) d[x] = k0 * (r0[x] + r4[x]) + k1 * (r1[x] + r3[x]) + k2 * r2[x];
        }
    }

    /// <summary>Отражение за край: d c b a | a b c d | d c b a.</summary>
    private static int Reflect(int i, int n)
    {
        if (n == 1) return 0;
        while (i < 0 || i >= n)
        {
            if (i < 0) i = -i - 1;
            if (i >= n) i = 2 * n - i - 1;
        }
        return i;
    }

    /// <summary>Свой генератор (splitmix64): одинаковый результат на любой версии .NET.</summary>
    private sealed class Rng
    {
        private ulong _s;
        public Rng(int seed) { _s = unchecked((ulong)(long)seed * 0x9E3779B97F4A7C15UL + 0x632BE59BD9B4E019UL); }

        /// <summary>Случайные целые 0..254 (как (random * 255).astype(uint8) в gen_ice.py), записанные как float.</summary>
        [MethodImpl(Hot)]
        public void FillBytes(float[] dst)
        {
            ulong s = _s;
            for (int i = 0; i < dst.Length; i++)
            {
                ulong z = unchecked(s += 0x9E3779B97F4A7C15UL);
                z = unchecked((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9UL);
                z = unchecked((z ^ (z >> 27)) * 0x94D049BB133111EBUL);
                z ^= z >> 31;
                // старшие 32 бита -> 0..254
                dst[i] = (float)(((z >> 32) * 255UL) >> 32);
            }
            _s = s;
        }
    }
}
