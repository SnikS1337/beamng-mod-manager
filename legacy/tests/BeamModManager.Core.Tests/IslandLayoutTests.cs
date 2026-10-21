using BeamModManager.Ui.Look;

namespace BeamModManager.Core.Tests;

/// <summary>
/// Раскладка верха (IslandLayout): островок точно посередине окна, на узком окне уступает заголовок. Числа близки к
/// настоящим: ряд от 116 (за левой панелью) до ширины окна − 164 (кнопки окна 132 + поля), островок фильтров гаража
/// ≈505, «Обновлений» ≈415, крупный заголовок гаража ≈190, его капсула ≈95.
/// </summary>
public static class IslandLayoutTests
{
    private const double Left = 116, Garage = 505, Updates = 415, Big = 190, Cap = 95;
    private static double Right(double w) => w - 164;

    private static IslandPlacement Place(double window, double island, double big = Big, double cap = Cap, double? target = null) =>
        IslandLayout.Place(window, Left, Right(window), island, target ?? island, big, cap);

    public static void Register(TestRunner t)
    {
        t.Group("Островок посередине");

        t.Run("островок точно посередине окна на любой ширине от 1000 до 3840", () =>
        {
            for (var w = 1000; w <= 3840; w += 10)
            foreach (var island in new[] { Garage, Updates, IslandLayout.SearchWidth(Right(w) - Left) })
            {
                var p = Place(w, island);
                Assert.True(Math.Abs(p.X + p.Width / 2 - w / 2.0) <= 0.5, $"окно {w}, островок {island}: центр {p.X + p.Width / 2}");
            }
        });

        t.Run("на минимальной ширине 1000 островок не заходит под кнопки окна и за левую панель", () =>
        {
            foreach (var island in new[] { Garage, Updates, 560.0 })
            {
                var p = Place(1000, island);
                Assert.True(p.X >= Left, $"{p.X}");
                Assert.True(p.X + p.Width <= Right(1000), $"{p.X + p.Width}");
            }
        });

        t.Run("широкое окно: крупный заголовок", () => Assert.Eq(TitleFit.Full, Place(1440, Garage).Title));

        t.Run("1100: крупный не помещается — заголовок капсулой, островок остаётся посередине", () =>
        {
            var p = Place(1100, Garage);
            Assert.Eq(TitleFit.Compact, p.Title);
            Assert.True(Math.Abs(p.X + p.Width / 2 - 550) <= 0.5, $"{p.X}");
        });

        t.Run("1000, «Обновления»: островок уже, крупный заголовок помещается", () =>
            Assert.Eq(TitleFit.Full, Place(1000, Updates, big: 150, cap: 140).Title));

        t.Run("не помещается и капсула — заголовка нет, островок всё равно посередине", () =>
        {
            var p = Place(1000, Garage, big: 190, cap: 140);
            Assert.Eq(TitleFit.Hidden, p.Title);
            Assert.True(Math.Abs(p.X + p.Width / 2 - 500) <= 0.5, $"{p.X}");
        });

        t.Run("заголовок решается по ширине, под которую оставлено место, а не по ширине в пути: поиск его не меняет", () =>
        {
            // ширина в пути или капсула поиска (440) оставила бы место крупному заголовку, фильтры (505) — уже нет
            Assert.Eq(TitleFit.Full, Place(1100, 440).Title);
            Assert.Eq(TitleFit.Compact, Place(1100, 440, target: Garage).Title);
        });

        t.Run("окно растёт — заголовок только прибавляет: нет → капсула → крупный, без скачков назад", () =>
        {
            var prev = TitleFit.Hidden;
            for (var w = 900; w <= 2000; w++)
            {
                var fit = Place(w, Garage, cap: 120).Title;
                Assert.True(fit <= prev, $"окно {w}: {prev} → {fit}");
                prev = fit;
            }
            Assert.Eq(TitleFit.Full, prev);
        });

        t.Run("уже минимальной ширины: островок отходит от кнопок окна влево, но не за левую панель", () =>
        {
            var p = Place(900, 600);
            Assert.Eq(Right(900) - 600, p.X);
            var q = Place(700, 600);
            Assert.Eq(Left, q.X);
            Assert.True(q.Width <= Right(700) - Left, "островок не шире ряда");
        });

        t.Run("мусор в размерах (ноль, минус, NaN, бесконечность) ничего не ломает", () =>
        {
            foreach (var bad in new[] { 0, -5, double.NaN, double.PositiveInfinity, double.NegativeInfinity })
            {
                var p = IslandLayout.Place(bad, Left, bad, bad, bad, bad, bad);
                Assert.True(double.IsFinite(p.X) && double.IsFinite(p.Width), $"{bad}: {p}");
                var q = IslandLayout.Place(1440, Left, Right(1440), bad, Garage, bad, bad);
                Assert.True(double.IsFinite(q.X) && q.Width >= 0, $"{bad}: {q}");
                Assert.True(double.IsFinite(IslandLayout.SearchWidth(bad)));
            }
        });

        t.Run("ширина поиска: половина ряда, 320–440, но не шире ряда", () =>
        {
            Assert.Eq(360.0, IslandLayout.SearchWidth(720));
            Assert.Eq(440.0, IslandLayout.SearchWidth(1160));
            Assert.Eq(320.0, IslandLayout.SearchWidth(500));
            Assert.Eq(200.0, IslandLayout.SearchWidth(200));
            Assert.Eq(0.0, IslandLayout.SearchWidth(double.NaN));
        });
    }
}
