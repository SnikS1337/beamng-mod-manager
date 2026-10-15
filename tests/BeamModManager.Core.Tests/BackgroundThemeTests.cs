using BeamModManager.Core.Settings;
using BeamModManager.Ui.Look;

namespace BeamModManager.Core.Tests;

/// <summary>Темы фона: полнота, читаемость текста, сдержанность цвета, расклад пятен.</summary>
public static class BackgroundThemeTests
{
    public static void Register(TestRunner t)
    {
        t.Group("Темы фона");

        t.Run("у каждой темы есть палитра в тёмном и светлом оформлении, наборы цветов полные", () =>
        {
            foreach (var light in new[] { false, true })
                foreach (var theme in Enum.GetValues<BackgroundTheme>())
                {
                    var p = BackgroundThemes.Of(theme, light);
                    Assert.True(p.Theme == theme && p.Light == light, $"{theme}/{light}: своя палитра");
                    Assert.Eq(3, p.Base.Count);
                    Assert.Eq(5, p.Grade.Count);
                    Assert.Eq(3, p.Blobs.Count);
                    Assert.True(p.Base.All(c => c >> 24 == 0xFF), $"{theme}: основа непрозрачная");
                    Assert.True(p.Grade[0] == p.Grade[4], $"{theme}: полоса закольцована, шов не виден");
                    Assert.True(p.Name.Length > 0 && p.Note.Length > 0, $"{theme}: подписи");
                }
            Assert.Eq(6, BackgroundThemes.All.Count);
            Assert.Eq(3, BackgroundThemes.Dark.Select(p => p.Name).Distinct().Count());
        });

        t.Run("текст читается в худшем месте любой палитры (не ниже 6:1)", () =>
        {
            foreach (var p in BackgroundThemes.All)
            {
                var c = BackgroundThemes.Contrast(BackgroundThemes.Worst(p), BackgroundThemes.Rgb(p.Ink));
                Assert.True(c >= 6.0, $"{p.Name} ({(p.Light ? "светлое" : "тёмное")}): контраст {c:F1}");
            }
        });

        t.Run("основа приглушённая: в тёмном тёмная, в светлом светлая, превью модов остаются самым ярким", () =>
        {
            foreach (var p in BackgroundThemes.All)
                foreach (var b in p.Base)
                {
                    var (r, g, bl) = BackgroundThemes.Rgb(b);
                    double max = Math.Max(r, Math.Max(g, bl)) / 255, min = Math.Min(r, Math.Min(g, bl)) / 255;
                    var light = (max + min) / 2;
                    var sat = max == min ? 0 : (max - min) / (1 - Math.Abs(2 * light - 1));
                    if (p.Light) Assert.True(light >= 0.82, $"{p.Name}: светлота {light:F2}");
                    else Assert.True(light <= 0.20, $"{p.Name}: светлота {light:F2}");
                    Assert.True(sat <= 0.62, $"{p.Name}: насыщенность {sat:F2}");
                }
        });

        t.Run("пятна полупрозрачные, в пределах окна, а расклад у тем разный", () =>
        {
            foreach (var p in BackgroundThemes.All)
                foreach (var b in p.Blobs)
                {
                    Assert.True(BackgroundThemes.Alpha(b.Color) <= 0.45, $"{p.Name}: пятно не ярче 45 %");
                    Assert.True(b.X is >= 0 and <= 1 && b.Y is >= 0 and <= 1 && b.R is > 0.1 and < 0.5, $"{p.Name}: расклад {b}");
                }
            var layouts = BackgroundThemes.Dark.Select(p => string.Join(";", p.Blobs.Select(b => $"{b.X},{b.Y}"))).ToList();
            Assert.Eq(3, layouts.Distinct().Count());
            // смена оформления пятна не двигает: только цвет
            foreach (var theme in Enum.GetValues<BackgroundTheme>())
            {
                var d = BackgroundThemes.Of(theme).Blobs; var l = BackgroundThemes.Of(theme, true).Blobs;
                Assert.True(d.Zip(l).All(x => x.First.X == x.Second.X && x.First.Y == x.Second.Y && x.First.R == x.Second.R), $"{theme}: тот же расклад");
            }
        });

        t.Run("край пятна мягкий: непрозрачность убывает от центра к краю до нуля", () =>
        {
            var f = BackgroundThemes.Falloff;
            Assert.True(f[0].At == 0 && f[0].Alpha == 1 && f[^1].At == 1 && f[^1].Alpha == 0, "от 1 в центре до 0 на краю");
            for (var i = 1; i < f.Count; i++) Assert.True(f[i].At > f[i - 1].At && f[i].Alpha < f[i - 1].Alpha, "строго убывает");
        });

        t.Run("неизвестная тема — «Ночь» в нужном оформлении", () =>
        {
            Assert.True(BackgroundThemes.Of((BackgroundTheme)77).Theme == BackgroundTheme.Night, "тёмное");
            Assert.True(BackgroundThemes.Of((BackgroundTheme)77, true) is { Theme: BackgroundTheme.Night, Light: true }, "светлое");
        });
    }
}
