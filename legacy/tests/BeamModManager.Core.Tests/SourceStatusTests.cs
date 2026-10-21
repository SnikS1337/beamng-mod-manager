using BeamModManager.Core.Sources;
using BeamModManager.Core.Sources.Modland;

namespace BeamModManager.Core.Tests;

public static class SourceStatusTests
{
    private const string Interstitial =
        "<html><head><title>Один момент…</title></head><body><h1>modland.net</h1><p>Проверяем, человек ли вы. Это может занять несколько секунд.</p></body></html>";

    private static string Fx(string path)
    {
        var dir = AppContext.BaseDirectory;
        while (dir != null && !Directory.Exists(Path.Combine(dir, "tests", "fixtures"))) dir = Path.GetDirectoryName(dir);
        return File.ReadAllText(Path.Combine(dir!, "tests", "fixtures", path));
    }

    public static void Register(TestRunner t)
    {
        t.Group("Статус источника и страница проверки");

        t.Run("русская страница проверки распознаётся", () => Assert.True(ChallengePageDetector.IsChallenge(Interstitial)));
        t.Run("английская страница проверки распознаётся", () =>
            Assert.True(ChallengePageDetector.IsChallenge("<html><head><title>Just a moment...</title></head></html>")));
        t.Run("обычные страницы всех сайтов не принимаются за проверку", () =>
        {
            foreach (var f in new[] { "modland/list-real.html", "modland/mod-paid-real.html", "modsgaming/list-real.html",
                         "stmods/list-real.html", "modsfire/first-step-real.html" })
                Assert.False(ChallengePageDetector.IsChallenge(Fx(f)), f);
        });
        t.Run("один адрес challenge-platform не делает страницу проверкой", () =>
            Assert.False(ChallengePageDetector.IsChallenge("<html><title>Мод</title><script src=\"/cdn-cgi/challenge-platform/scripts/precursor/main.js\"></script></html>")));
        t.Run("проверка важнее кода ответа", () =>
            Assert.Eq(SourceStatus.NeedsHumanCheck, ChallengePageDetector.Classify(403, Interstitial, false)));
        t.Run("403 без признаков проверки: недоступен", () =>
            Assert.Eq(SourceStatus.Unavailable, ChallengePageDetector.Classify(403, "<html>Forbidden</html>", false)));
        t.Run("нет ответа: недоступен", () =>
            Assert.Eq(SourceStatus.Unavailable, ChallengePageDetector.Classify(null, null, false)));
        t.Run("200, но страница не похожа на список: недоступен", () =>
            Assert.Eq(SourceStatus.Unavailable, ChallengePageDetector.Classify(200, "<html>пусто</html>", ModlandParser.ParseList("<html>пусто</html>").Count > 0)));
        t.Run("200 и список разобран: работает", () =>
        {
            var html = Fx("modland/list-real.html");
            Assert.Eq(SourceStatus.Ok, ChallengePageDetector.Classify(200, html, ModlandParser.ParseList(html).Count > 0));
        });
    }
}
