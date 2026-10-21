using BeamModManager.Core.Models;
using BeamModManager.Core.Sources.Modland;

namespace BeamModManager.Core.Tests;

public static class ModlandTests
{
    private static string Fx(string name)
    {
        var dir = AppContext.BaseDirectory;
        while (dir != null && !Directory.Exists(Path.Combine(dir, "tests", "fixtures"))) dir = Path.GetDirectoryName(dir);
        return File.ReadAllText(Path.Combine(dir!, "tests", "fixtures", "modland", name));
    }

    public static void Register(TestRunner t)
    {
        t.Group("Modland: разбор страниц");
        var list = ModlandParser.ParseList(Fx("list-page.html"));

        t.Run("список: две записи", () => Assert.Eq(2, list.Count));
        t.Run("список: поля бесплатного мода", () =>
        {
            var l = list[0];
            Assert.Eq("Ibishu Miramar Dirt Race [0.9.0]", l.Title);
            Assert.Eq("John Der33", l.Author);
            Assert.Eq("Cars", l.Category);
            Assert.Eq("ibishu-miramar-dirt-race-0.9.0-wV9h", l.SourceId);
            Assert.Eq((long)(1.09 * 1024 * 1024), l.SizeBytes);
        });
        t.Run("список: [PAID] в названии даёт Paid", () =>
        {
            Assert.Eq(PriceStatus.Paid, list[1].Price);
            Assert.Eq("Maps", list[1].Category);
            Assert.Eq("0.2.3", list[1].ModVersion);
        });
        t.Run("список: без положительных признаков цена Unknown", () => Assert.Eq(PriceStatus.Unknown, list[0].Price));

        t.Run("карточка платного: ссылка Patreon, версия и версия игры", () =>
        {
            var d = ModlandParser.ParseDetail(Fx("mod-paid.html"), list[1]);
            Assert.Eq(PriceStatus.Paid, d.Price);
            Assert.Eq(OfferKind.PaidLink, d.Offers.Single().Kind);
            Assert.Eq("0.2.3", d.ModVersion);
            Assert.Eq("0.39.x", d.GameVersion);
        });
        t.Run("карточка бесплатного: платных ссылок нет", () =>
        {
            var d = ModlandParser.ParseDetail(Fx("mod-free.html"), list[0]);
            Assert.False(d.Offers.Any(o => o.Kind == OfferKind.PaidLink));
            Assert.Eq("0.9.0.5.4267", d.GameVersion);
        });
        t.Run("пустой HTML: пустой список", () => Assert.Eq(0, ModlandParser.ParseList("").Count));
    }
}
