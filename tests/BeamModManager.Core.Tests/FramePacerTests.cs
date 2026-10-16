using BeamModManager.Ui.Look;

namespace BeamModManager.Core.Tests;

/// <summary>Частота кадров анимаций ответа (FramePolicy, FramePacer): экран, потолок 144, батарея, ровные 60 при нехватке.</summary>
public static class FramePacerTests
{
    /// <summary>Прогоняет n кадров экрана с интервалом dt; возвращает, сколько раз пружины шагнули и сколько секунд всего.</summary>
    private static (int Steps, double Time) Run(FramePacer p, double dt, int n)
    {
        int steps = 0; double time = 0;
        for (var i = 0; i < n; i++)
        {
            var s = p.Frame(dt);
            if (s > 0) { steps++; time += s; }
        }
        return (steps, time);
    }

    public static void Register(TestRunner t)
    {
        t.Group("Частота кадров анимаций");

        t.Run("экран 60 Гц: каждый кадр, время не теряется", () =>
        {
            var p = new FramePacer(new FramePolicy());
            var (steps, time) = Run(p, 1 / 60.0, 120);
            Assert.Eq(120, steps);
            Assert.True(Math.Abs(time - 2) < 1e-9, $"{time}");
        });

        t.Run("экран 144 Гц: каждый кадр (потолок 144)", () =>
        {
            var p = new FramePacer(new FramePolicy());
            var (steps, _) = Run(p, 1 / 144.0, 288);
            Assert.Eq(288, steps);
            Assert.True(Math.Abs(p.ScreenInterval - 1 / 144.0) < 1e-9, "частота экрана узнана по кадрам");
        });

        t.Run("экран 240 Гц: пружины шагают ≈144 раза в секунду, время то же", () =>
        {
            var p = new FramePacer(new FramePolicy());
            var (steps, time) = Run(p, 1 / 240.0, 480); // 2 секунды
            Assert.True(steps is >= 230 and <= 300, $"шагов за 2 с: {steps}");
            Assert.True(Math.Abs(time - 2) < 0.02, $"время пружин {time}");
        });

        t.Run("от батареи: на 144 Гц шагов ≈60 в секунду, время то же", () =>
        {
            var policy = new FramePolicy { PowerSaving = true };
            var p = new FramePacer(policy);
            var (steps, time) = Run(p, 1 / 144.0, 288);
            Assert.True(steps is >= 55 * 2 and <= 75 * 2, $"шагов за 2 с: {steps}");
            Assert.True(Math.Abs(time - 2) < 0.02, $"время пружин {time}");
            Assert.Eq(FramePolicy.SafeHz, policy.TargetHz);
        });

        t.Run("на высокой частоте опаздывают больше четверти кадров — следующие анимации идут на ровных 60", () =>
        {
            var policy = new FramePolicy();
            var p = new FramePacer(policy);
            Run(p, 1 / 144.0, 20); // экран узнан
            for (var i = 0; i < 30; i++) p.Frame(i % 2 == 0 ? 1 / 144.0 : 3 / 144.0); // каждый второй кадр опоздал
            Assert.Eq(FramePolicy.MaxHz, policy.TargetHz, "посреди анимации частота не меняется");
            p.BurstEnded();
            Assert.True(policy.Degraded, "снижено");
            Assert.Eq(FramePolicy.SafeHz, policy.TargetHz);
        });

        t.Run("редкие опоздания и короткие всплески частоту не снижают", () =>
        {
            var policy = new FramePolicy();
            var p = new FramePacer(policy);
            Run(p, 1 / 144.0, 40);
            for (var i = 0; i < 40; i++) p.Frame(i % 10 == 0 ? 3 / 144.0 : 1 / 144.0); // 10 % опоздали
            p.BurstEnded();
            Assert.False(policy.Degraded, "10 % опозданий — терпимо");
            for (var i = 0; i < 5; i++) p.Frame(4 / 144.0); // короткий всплеск, все опоздали
            p.BurstEnded();
            Assert.False(policy.Degraded, "5 кадров — не повод судить");
        });

        t.Run("на экране 60 Гц снижать некуда: опоздания ничего не меняют", () =>
        {
            var policy = new FramePolicy();
            var p = new FramePacer(policy);
            Run(p, 1 / 60.0, 20);
            for (var i = 0; i < 30; i++) p.Frame(2 / 60.0);
            p.BurstEnded();
            Assert.False(policy.Degraded, "60 Гц");
        });

        t.Run("сниженная частота через 2 минуты пробуется снова", () =>
        {
            var now = new DateTime(2026, 10, 17, 12, 0, 0, DateTimeKind.Utc);
            var policy = new FramePolicy(() => now);
            var p = new FramePacer(policy);
            Run(p, 1 / 144.0, 20);
            for (var i = 0; i < 30; i++) p.Frame(3 / 144.0);
            p.BurstEnded();
            Assert.True(policy.Degraded, "снижено");
            now += TimeSpan.FromMinutes(1);
            Assert.Eq(FramePolicy.SafeHz, policy.TargetHz, "через минуту ещё 60");
            now += TimeSpan.FromMinutes(1.1);
            Assert.Eq(FramePolicy.MaxHz, policy.TargetHz, "через две — снова частота экрана");
            Assert.False(policy.Degraded, "сброшено");
        });

        t.Run("окно перенесли на монитор помедленнее: частота экрана узнаётся заново за секунду-две", () =>
        {
            var p = new FramePacer(new FramePolicy());
            Run(p, 1 / 144.0, 100);
            Assert.True(Math.Abs(p.ScreenInterval - 1 / 144.0) < 1e-9, "144");
            Run(p, 1 / 60.0, 100);
            Assert.True(Math.Abs(p.ScreenInterval - 1 / 60.0) < 1e-9, "теперь 60");
        });

        t.Run("мусор во времени кадра (ноль, минус, бесконечность, пауза окна) ничего не ломает", () =>
        {
            var p = new FramePacer(new FramePolicy());
            Assert.Eq(0.0, p.Frame(0));
            Assert.Eq(0.0, p.Frame(-1));
            Assert.Eq(0.0, p.Frame(double.NaN));
            Assert.Eq(0.0, p.Frame(double.PositiveInfinity));
            Assert.True(Math.Abs(p.ScreenInterval - 1 / 60.0) < 1e-9, "без кадров — как 60 Гц");
            Assert.True(p.Frame(2.5) > 0, "долгая пауза окна — шаг есть (пружина сама ограничит его)");
            Assert.True(Math.Abs(p.ScreenInterval - 1 / 60.0) < 1e-9, "пауза не считается частотой экрана");
        });

        t.Run("от батареи политика не снижается и не запоминает опоздания", () =>
        {
            var policy = new FramePolicy { PowerSaving = true };
            var p = new FramePacer(policy);
            Run(p, 1 / 144.0, 20);
            for (var i = 0; i < 30; i++) p.Frame(3 / 144.0);
            p.BurstEnded();
            Assert.False(policy.Degraded, "от батареи и так 60");
            policy.PowerSaving = false;
            Assert.Eq(FramePolicy.MaxHz, policy.TargetHz, "включили в сеть — сразу частота экрана");
        });
    }
}
