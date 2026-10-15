using BeamModManager.Core.Settings;
using BeamModManager.Ui.Look;

namespace BeamModManager.Core.Tests;

/// <summary>Темы фона: полнота, читаемость белого текста, сдержанность цвета (фон не спорит с превью модов).</summary>
public static class BackgroundThemeTests
{
    public static void Register(TestRunner t)
    {
        t.Group("Темы фона");

        t.Run("у каждой темы из настроек есть цвета, и все наборы цветов полные", () =>
        {
            foreach (var theme in Enum.GetValues<BackgroundTheme>())
            {
                var p = BackgroundThemes.Of(theme);
                Assert.True(p.Theme == theme, $"{theme}: своя палитра");
                Assert.Eq(3, p.Base.Count);
                Assert.Eq(5, p.Grade.Count);
                Assert.Eq(3, p.Blobs.Count);
                Assert.True(p.Base.All(c => c >> 24 == 0xFF), $"{theme}: основа непрозрачная");
                Assert.True(p.Grade[0] == p.Grade[4], $"{theme}: полоса закольцована, шов не виден");
                Assert.True(p.Name.Length > 0 && p.Note.Length > 0, $"{theme}: подписи");
            }
            Assert.Eq(3, BackgroundThemes.All.Select(p => p.Name).Distinct().Count());
        });

        t.Run("белый текст читается в самом светлом месте любой темы (не ниже 7:1)", () =>
        {
            foreach (var p in BackgroundThemes.All)
            {
                var c = BackgroundThemes.WhiteContrast(BackgroundThemes.Brightest(p));
                Assert.True(c >= 7.0, $"{p.Name}: контраст {c:F1}");
            }
        });

        t.Run("основа тёмная и приглушённая: превью модов остаются самым ярким на экране", () =>
        {
            foreach (var p in BackgroundThemes.All)
                foreach (var b in p.Base)
                {
                    var (r, g, bl) = BackgroundThemes.Rgb(b);
                    double max = Math.Max(r, Math.Max(g, bl)) / 255, min = Math.Min(r, Math.Min(g, bl)) / 255;
                    var light = (max + min) / 2;
                    var sat = max == min ? 0 : (max - min) / (1 - Math.Abs(2 * light - 1));
                    Assert.True(light <= 0.16, $"{p.Name}: светлота {light:F2}");
                    Assert.True(sat <= 0.75, $"{p.Name}: насыщенность {sat:F2}");
                }
        });

        t.Run("пятна света полупрозрачные, не ярче 35 %", () =>
        {
            foreach (var p in BackgroundThemes.All)
                Assert.True(p.Blobs.All(c => BackgroundThemes.Alpha(c) <= 0.35), p.Name);
        });

        t.Run("неизвестная тема — «Ночь»", () =>
            Assert.True(BackgroundThemes.Of((BackgroundTheme)77).Theme == BackgroundTheme.Night, "по умолчанию"));
    }
}
