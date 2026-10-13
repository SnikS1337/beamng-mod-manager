using BeamModManager.Core.Models;
using BeamModManager.Core.Sources.ModsGaming;
using BeamModManager.Core.Text;

namespace BeamModManager.Core.Tests;

public static class ModsGamingTests
{
    private static string Fx(string name)
    {
        var dir = AppContext.BaseDirectory;
        while (dir != null && !Directory.Exists(Path.Combine(dir, "tests", "fixtures"))) dir = Path.GetDirectoryName(dir);
        return File.ReadAllText(Path.Combine(dir!, "tests", "fixtures", "modsgaming", name));
    }

    public static void Register(TestRunner t)
    {
        t.Group("ModsGaming и STMods: разбор");
        var list = ModsGamingParser.ParseList(Fx("list-page.html"));

        t.Run("список: две записи, id из entryID", () =>
        {
            Assert.Eq(2, list.Count);
            Assert.Eq("160435", list[0].SourceId);
        });
        t.Run("список: версии из заголовка", () =>
        {
            Assert.Eq("5.1", list[0].ModVersion);
            Assert.Eq("0.39.x", list[0].GameVersion);
            Assert.Eq("mashiny", list[0].Category);
        });
        t.Run("цена без доказательств остаётся Unknown", () => Assert.Eq(PriceStatus.Unknown, list[0].Price));
        t.Run("карточка: автор, размер, служебная ссылка скачивания", () =>
        {
            var d = ModsGamingParser.ParseDetail(Fx("mod-page.html"), list[0]);
            Assert.Eq("Infiniti", d.Author);
            Assert.Eq((long)(316.40 * 1024 * 1024), d.SizeBytes);
            Assert.Eq("/load/0-0-1-160435-20", d.Offers.Single().Url.AbsolutePath);
        });
        t.Run("пустой HTML: пусто", () => Assert.Eq(0, ModsGamingParser.ParseList("").Count));

        t.Run("STMods: «версия 3.3 для BeamNG.drive (v0.39.x)»", () =>
        {
            var p = TitleParser.Parse("BMW E34 версия 3.3 для BeamNG.drive (v0.39.x)");
            Assert.Eq("BMW E34", p.Name);
            Assert.Eq("3.3", p.ModVersion);
            Assert.Eq("0.39.x", p.GameVersion);
        });
        t.Run("STMods и ModsGaming дают одинаковое сравнимое название", () =>
            Assert.True(TitleNormalizer.Similarity("BMW M2 (G87) версия 5.1 для BeamNG.drive (v0.39.x)", "МОД BMW M2 G87 v5.1 (0.39.x) ДЛЯ BEAMNG.DRIVE") >= 0.8));
    }
}
