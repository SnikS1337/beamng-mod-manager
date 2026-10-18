using BeamModManager.Ui.Look;

namespace BeamModManager.Core.Tests;

/// <summary>Капля в пути одним телом (DropBody, вариант «В», 2026-10-17): оболочка от хвоста до головы, без разрыва.</summary>
public static class DropBodyTests
{
    public static void Register(TestRunner t)
    {
        t.Group("Капля одним телом");

        t.Run("капля стоит: оболочка — сама капля, не линза, не приподнята, не сжата", () =>
        {
            var b = DropBody.Of(76, 76, 64, 0);
            Assert.Eq(76.0, b.Start); Assert.Eq(64.0, b.Length);
            Assert.Eq(1.0, b.Across); Assert.Eq(0.0, b.Motion); Assert.Eq(1.0, b.Lift);
        });

        t.Run("в пути вниз и вверх: одна оболочка от хвоста до головы, ни одной щели между ними", () =>
        {
            foreach (var (lead, trail) in new[] { (144.0, 30.0), (8.0, 190.0), (212.0, 211.5) })
            {
                var b = DropBody.Of(lead, trail, 64, 300);
                Assert.Eq(Math.Min(lead, trail), b.Start);
                Assert.True(Math.Abs(b.Start + b.Length - (Math.Max(lead, trail) + 64)) < 1e-9, "конец оболочки — у дальней капли");
            }
        });

        t.Run("растянулась — поперёк сжата не больше чем на 16 %, линза и подъём не больше предела", () =>
        {
            for (var sep = 0.0; sep <= 400; sep += 7)
            {
                var b = DropBody.Of(sep, 0, 64, sep * 10);
                Assert.True(b.Across <= 1 && b.Across >= 1 - DropBody.MaxSquash - 1e-9, $"{sep}: {b.Across}");
                Assert.True(b.Motion >= 0 && b.Motion <= 1, $"{sep}: {b.Motion}");
                Assert.True(b.Lift >= 1 && b.Lift <= 1 + DropBody.MaxLift + 1e-9, $"{sep}: {b.Lift}");
            }
        });

        t.Run("линза появляется в пути и гаснет, когда капля доехала (скорость и разрыв ноль)", () =>
        {
            Assert.True(DropBody.MotionOf(30, 0) > 0.99, "разрыв 30 — линза");
            Assert.True(DropBody.MotionOf(0, 900) > 0.99, "быстро едет — линза");
            Assert.True(DropBody.MotionOf(1, 30) < 0.2, "почти доехала — почти плашка");
            Assert.Eq(0.0, DropBody.MotionOf(0, 0));
        });

        t.Run("стенка островка: перелёт не выносит каплю за край — она сплющивается об него, дальний край на месте", () =>
        {
            // из крайнего правого в крайний левый: голова перелетела за левую стенку (-3) на 14, хвост ещё справа
            var b = DropBody.Of(-17, 40, 70, -500);
            var lift = b.Lift;
            var w = b.Within(-3, 400, lift);
            var c = w.Start + w.Length / 2;
            Assert.True(Math.Abs(c - w.Length * lift / 2 - -3) < 1e-6, $"левый край на экране — у стенки: {w}");
            var farEnd = b.Start + b.Length / 2 + b.Length * lift / 2;
            Assert.True(Math.Abs(c + w.Length * lift / 2 - farEnd) < 1e-6, "правый край не двигается");
            Assert.True(w.Length < b.Length && w.Across > b.Across && w.Across <= b.Across * (1 + DropBody.MaxBulge) + 1e-9, $"сплющилась: {w}");
            var inside = DropBody.Of(100, 120, 70, 300);
            Assert.Eq(inside, inside.Within(-3, 400, inside.Lift));
            var right = DropBody.Of(350, 300, 70, 500).Within(-3, 400, 1.08);
            var rc = right.Start + right.Length / 2;
            Assert.True(rc + right.Length * 1.08 / 2 <= 400 + 1e-6, "и у правой стенки");
            foreach (var bad in new[] { double.NaN, double.PositiveInfinity, 0 })
            {
                var x = DropBody.Of(-500, -500, 70, 0).Within(-3, 400, bad);
                Assert.True(double.IsFinite(x.Start) && x.Length >= 1, $"{bad}: {x}");
            }
            Assert.Eq(b, b.Within(5, 5, 1)); // стенок нет — как есть
        });

        t.Run("живость: на длинном пути голова заметно перелетает пункт, но немного, и всё стоит за полсекунды с небольшим", () =>
        {
            double lead = 0, lv = 0, trail = 0, tv = 0, over = 0, settled = -1;
            const double target = 300, dt = 1 / 240.0;
            double K(double resp) => Math.Pow(2 * Math.PI / resp, 2);
            double C(double resp, double ratio) => 2 * ratio * 2 * Math.PI / resp;
            for (var i = 1; i <= 240 * 2; i++)
            {
                lv += (-K(0.34) * (lead - target) - C(0.34, 1 - 0.32) * lv) * dt; lead += lv * dt;
                tv += (-K(0.40) * (trail - target) - C(0.40, 1 - 0.15) * tv) * dt; trail += tv * dt;
                over = Math.Max(over, lead - target);
                if (settled < 0 && Math.Abs(lead - target) < 0.5 && Math.Abs(trail - target) < 0.5 && Math.Abs(lv) < 20) settled = i * dt;
            }
            Assert.True(over is > 8 and < 20, $"перелёт {over:0.0} из 300 — заметный, но небольшой");
            Assert.True(settled is > 0 and < 0.65, $"успокоилась за {settled:0.00} с");
        });

        t.Run("мусор (NaN, бесконечность, ноль, минус) не ломает каплю", () =>
        {
            foreach (var bad in new[] { double.NaN, double.PositiveInfinity, double.NegativeInfinity, 0, -50 })
            {
                var b = DropBody.Of(bad, bad, bad, bad);
                Assert.True(double.IsFinite(b.Start) && b.Length >= 1 && double.IsFinite(b.Across) && double.IsFinite(b.Motion) && double.IsFinite(b.Lift), $"{bad}: {b}");
                Assert.True(double.IsFinite(DropBody.MotionOf(bad, bad)));
            }
        });

        t.Run("пружины головы и хвоста от одного пункта к другому: тело непрерывно на каждом кадре и в конце стоит", () =>
        {
            // как в окне (MainWindow.DropHead…): голова 0,34 с с перелётом, хвост 0,40 с, чуть пружинит; шаг 1/144 с
            double lead = 8, lv = 0, trail = 8, tv = 0;
            const double target = 212, dt = 1 / 144.0;
            double K(double resp) => Math.Pow(2 * Math.PI / resp, 2);
            double C(double resp, double ratio) => 2 * ratio * 2 * Math.PI / resp;
            for (var i = 0; i < 144 * 2; i++)
            {
                lv += (-K(0.34) * (lead - target) - C(0.34, 1 - 0.32) * lv) * dt; lead += lv * dt;
                tv += (-K(0.40) * (trail - target) - C(0.40, 1 - 0.15) * tv) * dt; trail += tv * dt;
                var b = DropBody.Of(lead, trail, 64, lv);
                Assert.True(b.Start <= Math.Min(lead, trail) + 1e-9 && b.Start + b.Length >= Math.Max(lead, trail) + 64 - 1e-9, $"кадр {i}");
            }
            var end = DropBody.Of(lead, trail, 64, lv);
            Assert.True(Math.Abs(end.Start - target) < 0.5 && end.Motion < 0.05, $"доехала: {end}");
        });
    }
}
