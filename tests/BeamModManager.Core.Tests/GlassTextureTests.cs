using System.Diagnostics;
using BeamModManager.Ui.Glass;

namespace BeamModManager.Core.Tests;

/// <summary>Текстуры стекла: блик по краю, иней, карта смещения.</summary>
public static class GlassTextureTests
{
    private static int A(byte[] px, int w, int x, int y) => px[(y * w + x) * 4 + 3];
    private static int R(byte[] px, int w, int x, int y) => px[(y * w + x) * 4 + 2];
    private static int G(byte[] px, int w, int x, int y) => px[(y * w + x) * 4 + 1];
    private static int B(byte[] px, int w, int x, int y) => px[(y * w + x) * 4];

    public static void Register(TestRunner t)
    {
        t.Group("Текстуры стекла");

        t.Run("размер массива: 4 байта на пиксель, строка = ширина * 4", () =>
        {
            Assert.Eq(84 * 284 * 4, GlassTextures.Rim(84, 284, 36, 30).Length);
            Assert.Eq(84 * 284 * 4, GlassTextures.Ice(84, 284, 36, 34, 1).Length);
            Assert.Eq(84 * 284 * 4, GlassTextures.Displacement(84, 284, 36, 30).Length);
            Assert.Eq(1 * 1 * 4, GlassTextures.Ice(1, 1, 0, 10, 1).Length);
            // строка: левый край строки y = 100 — полупрозрачный блик, сразу за ним идёт следующий пиксель той же строки
            var rim = GlassTextures.Rim(84, 284, 36, 30);
            Assert.True(A(rim, 84, 0, 100) > 0, "левый край строки виден");
            Assert.True(A(rim, 84, 83, 100) > 0, "правый край той же строки виден");
            Assert.Eq(0, A(rim, 84, 42, 142), "середина пустая");
        });

        t.Run("нулевые и отрицательные размеры дают пустой массив", () =>
        {
            Assert.Eq(0, GlassTextures.Rim(0, 10, 4, 10).Length);
            Assert.Eq(0, GlassTextures.Ice(10, 0, 4, 10, 1).Length);
            Assert.Eq(0, GlassTextures.Displacement(-5, 10, 4, 10).Length);
            Assert.Eq(0, GlassTextures.Ice(-1, -1, 4, 10, 1).Length);
        });

        t.Run("странные радиус и ширина не ломают расчёт", () =>
        {
            var a = GlassTextures.Ice(40, 20, 500, 0, 3);    // радиус больше половины, иней нулевой
            var b = GlassTextures.Rim(40, 20, -3, -1);
            var c = GlassTextures.Displacement(40, 20, double.NaN, 0);
            Assert.Eq(40 * 20 * 4, a.Length);
            Assert.Eq(40 * 20 * 4, b.Length);
            Assert.Eq(40 * 20 * 4, c.Length);
            // радиус обрезан до половины меньшей стороны: получается «таблетка», углы пустые
            Assert.Eq(0, A(GlassTextures.Rim(40, 20, 500, 10), 40, 0, 0));
            Assert.True(A(GlassTextures.Rim(40, 20, 500, 10), 40, 0, 10) > 0, "середина левого края закруглена, но видна");
        });

        t.Run("расстояние от края: середина, край, угол снаружи", () =>
        {
            Assert.True(Math.Abs(GlassTextures.InsideDistance(49.5, 29.5, 100, 60, 10) - 30) < 1e-9, "в центре 30");
            Assert.True(Math.Abs(GlassTextures.InsideDistance(0, 30, 100, 60, 10) - 0.5) < 1e-9, "крайний пиксель — полпикселя");
            Assert.True(GlassTextures.InsideDistance(0, 0, 100, 60, 10) < 0, "угол за скруглением снаружи");
            Assert.True(GlassTextures.InsideDistance(-5, 30, 100, 60, 10) < 0, "за краем отрицательно");
        });

        t.Run("за скруглёнными углами всё прозрачно", () =>
        {
            const int w = 120, h = 90; const double r = 30;
            var rim = GlassTextures.Rim(w, h, r, 40);
            var ice = GlassTextures.Ice(w, h, r, 40, 7);
            int outside = 0;
            for (int y = 0; y < h; y++)
                for (int x = 0; x < w; x++)
                {
                    if (GlassTextures.InsideDistance(x, y, w, h, r) >= 0) continue;
                    outside++;
                    int i = (y * w + x) * 4;
                    for (int k = 0; k < 4; k++)
                    {
                        Assert.Eq(0, (int)rim[i + k], $"блик снаружи ({x},{y})");
                        Assert.Eq(0, (int)ice[i + k], $"иней снаружи ({x},{y})");
                    }
                }
            Assert.True(outside > 4 * 100, "снаружи есть пиксели во всех четырёх углах");
        });

        t.Run("блик ярче сверху слева, чем снизу справа", () =>
        {
            const int w = 200, h = 150;
            var rim = GlassTextures.Rim(w, h, 30, 40);
            Assert.True(A(rim, w, 0, h / 2) > A(rim, w, w - 1, h / 2) + 60, "левый край ярче правого");
            Assert.True(A(rim, w, w / 2, 0) > A(rim, w, w / 2, h - 1) + 60, "верхний край ярче нижнего");
            // на диагонали угла: первый пиксель внутри сверху слева и симметричный снизу справа
            int k = 0;
            while (GlassTextures.InsideDistance(k, k, w, h, 30) < 0) k++;
            Assert.True(A(rim, w, k, k) > A(rim, w, w - 1 - k, h - 1 - k) + 60, "угол сверху слева ярче угла снизу справа");
        });

        t.Run("блик глубоко внутри почти невидим", () =>
        {
            const int w = 300, h = 200;
            var rim = GlassTextures.Rim(w, h, 32, 56);
            Assert.True(A(rim, w, w / 2, h / 2) <= 1, "в центре почти ноль");
            Assert.True(A(rim, w, 60, h / 2) <= 3, "в 60 пикселях от края почти ноль");
            Assert.True(A(rim, w, 20, h / 2) > A(rim, w, 40, h / 2), "свечение спадает от края");
        });

        t.Run("предумножение: цвет не больше прозрачности", () =>
        {
            foreach (var px in new[] { GlassTextures.Rim(160, 120, 26, 40), GlassTextures.Ice(160, 120, 26, 60, 4) })
                for (int i = 0; i < px.Length; i += 4)
                {
                    int a = px[i + 3];
                    if (px[i] > a || px[i + 1] > a || px[i + 2] > a)
                        throw new AssertionException($"пиксель {i / 4}: цвет больше прозрачности {a}");
                }
            // у плотного инея синий (255 * a) почти равен прозрачности, красный заметно меньше — холодный оттенок
            var ice = GlassTextures.Ice(160, 120, 26, 60, 4);
            int k = 0;
            for (int i = 0; i < ice.Length; i += 4)
                if (ice[i + 3] > 150) { k = i; break; }
            Assert.True(ice[k + 3] > 150, "есть плотный иней");
            Assert.True(ice[k] >= ice[k + 3] - 1, "синий во всю силу");
            Assert.True(ice[k + 2] < ice[k], "красного меньше, чем синего");
        });

        t.Run("иней: прозрачность не выше 0.9 и спадает к центру", () =>
        {
            const int w = 328, h = 540; const double r = 32, f = 50;
            var ice = GlassTextures.Ice(w, h, r, f, 11);
            double nearSum = 0, farSum = 0; int nearN = 0, farN = 0, max = 0;
            for (int y = 0; y < h; y++)
                for (int x = 0; x < w; x++)
                {
                    int a = A(ice, w, x, y);
                    if (a > max) max = a;
                    double d = GlassTextures.InsideDistance(x, y, w, h, r);
                    if (d >= 0 && d < 6) { nearSum += a; nearN++; }
                    else if (d >= f * 2) { farSum += a; farN++; }
                }
            double near = nearSum / nearN, far = farSum / farN;
            Assert.True(max <= 229, $"максимум {max} не выше 0.9 * 255");
            Assert.True(max >= 180, $"у края есть плотный иней (максимум {max})");
            Assert.True(near > 100, $"у края в среднем густо ({near:F1})");
            Assert.True(far < 20, $"в глубине почти прозрачно ({far:F1})");
            Assert.True(near > far * 6, $"у края {near:F1} много больше, чем в глубине {far:F1}");
        });

        t.Run("иней: дальность растёт с параметром reach", () =>
        {
            const int w = 300, h = 300; const double r = 30;
            double Band(byte[] px)
            {
                double s = 0; int n = 0;
                for (int y = 0; y < h; y++)
                    for (int x = 0; x < w; x++)
                    {
                        double d = GlassTextures.InsideDistance(x, y, w, h, r);
                        if (d >= 30 && d < 45) { s += A(px, w, x, y); n++; }
                    }
                return s / n;
            }
            double shortReach = Band(GlassTextures.Ice(w, h, r, 20, 5));
            double longReach = Band(GlassTextures.Ice(w, h, r, 60, 5));
            Assert.True(longReach > shortReach * 2, $"на глубине 30–45: {longReach:F1} против {shortReach:F1}");
        });

        t.Run("иней: одно зерно — одна картинка, другое зерно — другая", () =>
        {
            var a = GlassTextures.Ice(200, 120, 26, 40, 5);
            var b = GlassTextures.Ice(200, 120, 26, 40, 5);
            var c = GlassTextures.Ice(200, 120, 26, 40, 6);
            Assert.True(a.AsSpan().SequenceEqual(b), "повтор с тем же зерном совпадает");
            int diff = 0;
            for (int i = 3; i < a.Length; i += 4) if (a[i] != c[i]) diff++;
            Assert.True(diff > a.Length / 4 / 10, $"с другим зерном отличается заметно ({diff} пикселей)");
        });

        t.Run("одновременные вызовы из разных потоков дают то же, что по очереди", () =>
        {
            var expected = GlassTextures.Ice(400, 300, 32, 50, 9);
            var got = new byte[6][];
            Parallel.For(0, got.Length, i => got[i] = GlassTextures.Ice(400, 300, 32, 50, 9));
            foreach (var g in got) Assert.True(expected.AsSpan().SequenceEqual(g), "результат не зависит от соседних вызовов");
        });

        t.Run("карта смещения: 128 в глубине, у края смотрит внутрь", () =>
        {
            const int w = 300, h = 200;
            var m = GlassTextures.Displacement(w, h, 32, 56);
            for (int i = 0; i < m.Length; i += 4)
            {
                if (m[i] != 128 || m[i + 3] != 255) throw new AssertionException($"пиксель {i / 4}: B = 128 и A = 255");
            }
            Assert.Eq(128, R(m, w, w / 2, h / 2), "в центре без сдвига по x");
            Assert.Eq(128, G(m, w, w / 2, h / 2), "в центре без сдвига по y");
            Assert.True(R(m, w, 0, h / 2) > 200, "у левого края вправо");
            Assert.True(R(m, w, w - 1, h / 2) < 56, "у правого края влево");
            Assert.True(G(m, w, w / 2, 0) > 200, "у верхнего края вниз");
            Assert.True(G(m, w, w / 2, h - 1) < 56, "у нижнего края вверх");
            Assert.Eq(128, G(m, w, 0, h / 2), "у левого края без сдвига по y");
            Assert.True(R(m, w, 10, h / 2) < R(m, w, 1, h / 2), "сила спадает от края");
            Assert.Eq(128, R(m, w, 0, 0), "за скруглённым углом без сдвига");
            Assert.Eq(128, B(m, w, 0, 0));
        });

        t.Run("иней на большую панель считается быстро", () =>
        {
            GlassTextures.Ice(200, 100, 26, 40, 1); // прогрев
            var sw = Stopwatch.StartNew();
            var px = GlassTextures.Ice(1308, 760, 32, 50, 1);
            sw.Stop();
            Assert.Eq(1308 * 760 * 4, px.Length);
            Assert.True(sw.Elapsed.TotalSeconds < 1.5, $"Ice 1308x760 заняло {sw.Elapsed.TotalMilliseconds:F0} мс");
        });
    }
}
