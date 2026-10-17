using BeamModManager.Ui.Look;

namespace BeamModManager.Core.Tests;

/// <summary>Минимум окна по экрану (WindowFit): на маленьком экране окно не выходит за край.</summary>
public static class WindowFitTests
{
    public static void Register(TestRunner t)
    {
        t.Group("Окно на маленьком экране");

        t.Run("обычный экран: минимум 1000 × 680", () =>
        {
            Assert.Eq((1000.0, 680.0), WindowFit.Minimum(1920, 1040));
            Assert.Eq((1000.0, 680.0), WindowFit.Minimum(1016, 696));
        });

        t.Run("ноутбук 1366 × 768 со 125 % (1093 × 614): ширина 1000, высота по экрану", () =>
            Assert.Eq((1000.0, 598.0), WindowFit.Minimum(1093, 614)));

        t.Run("со 150 % (911 × 512): окно целиком на экране", () =>
        {
            var (w, h) = WindowFit.Minimum(911, 512);
            Assert.True(w <= 911 - 16 && h <= 512 - 16, $"{w} × {h}");
        });

        t.Run("крошечный экран и мусор: не меньше 640 × 480, без NaN", () =>
        {
            Assert.Eq((640.0, 480.0), WindowFit.Minimum(300, 200));
            foreach (var bad in new[] { double.NaN, double.PositiveInfinity, 0, -1 })
                Assert.Eq((1000.0, 680.0), WindowFit.Minimum(bad, bad));
        });

        t.Run("островок помещается и на минимальном окне маленького экрана (895 px)", () =>
        {
            var (w, _) = WindowFit.Minimum(911, 512);
            var p = IslandLayout.Place(w, 116, w - 164, 505, 505, 190, 95);
            Assert.True(p.X >= 116 && p.X + p.Width <= w - 164 + 1e-9, $"{p}");
            Assert.Eq(505.0, p.Width, "островок фильтров не обрезан");
        });
    }
}
