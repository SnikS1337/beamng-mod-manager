using BeamModManager.Core.Models;
using BeamModManager.Core.Sources.Modland;
using BeamModManager.Core.Sources.ModsFire;
using BeamModManager.Core.Sources.ModsGaming;
using BeamModManager.Core.Sources.StMods;

namespace BeamModManager.Core.Tests;

/// <summary>Тесты на настоящих сохранённых страницах (скрипты, стили и служебные поля вырезаны).</summary>
public static class RealPageTests
{
    private static string Fx(string path)
    {
        var dir = AppContext.BaseDirectory;
        while (dir != null && !Directory.Exists(Path.Combine(dir, "tests", "fixtures"))) dir = Path.GetDirectoryName(dir);
        return File.ReadAllText(Path.Combine(dir!, "tests", "fixtures", path));
    }

    public static void Register(TestRunner t)
    {
        t.Group("Настоящие страницы сайтов");

        t.Run("Modland, список: 7 карточек с категориями и размерами", () =>
        {
            var l = ModlandParser.ParseList(Fx("modland/list-real.html"));
            Assert.Eq(7, l.Count);
            Assert.True(l.All(x => x.Category is { Length: > 0 }), "у всех есть категория");
            Assert.Eq("Motorcycles", l.Last().Category);
            Assert.True(l.Count(x => x.SizeBytes is > 0) >= 6, "размеры читаются");
        });
        t.Run("Modland, платный мод: Patreon-ссылка, версии", () =>
        {
            var b = new Listing("modland", new Uri("https://www.modland.net/x/a.html"), "a", "[PAID] Duskport");
            var d = ModlandParser.ParseDetail(Fx("modland/mod-paid-real.html"), b);
            Assert.Eq(PriceStatus.Paid, d.Price);
            Assert.Eq("0.2.3", d.ModVersion);
            Assert.Eq("0.39.x", d.GameVersion);
            Assert.True(d.Offers.Any(o => o.Kind == OfferKind.PaidLink && o.Host.Contains("patreon")));
        });

        t.Run("ModsGaming, список: 10 карточек с версиями", () =>
        {
            var l = ModsGamingParser.ParseList(Fx("modsgaming/list-real.html"));
            Assert.Eq(10, l.Count);
            Assert.Eq("160435", l[0].SourceId);
            Assert.Eq("5.1", l[0].ModVersion);
            Assert.True(l.All(x => x.GameVersion == "0.39.x"), "версия игры у всех");
        });
        t.Run("ModsGaming, карточка: автор, размер, служебная ссылка", () =>
        {
            var b = ModsGamingParser.ParseList(Fx("modsgaming/list-real.html"))[0];
            var d = ModsGamingParser.ParseDetail(Fx("modsgaming/mod-real.html"), b);
            Assert.Eq("Infiniti", d.Author);
            Assert.Eq((long)(316.40 * 1024 * 1024), d.SizeBytes);
            Assert.Eq("/load/0-0-1-160435-20", d.Offers.Single().Url.AbsolutePath);
        });

        t.Run("STMods, список: 7 карточек, автор и версии", () =>
        {
            var l = StModsParser.ParseList(Fx("stmods/list-real.html"));
            Assert.Eq(7, l.Count);
            Assert.Eq("BMW E34", StModsParser.ParseList(Fx("stmods/list-real.html"))[1].Title.Split(" версия")[0]);
            Assert.Eq("3.3", l[1].ModVersion);
            Assert.Eq("matissator", l[1].Author);
        });
        t.Run("STMods, поиск: только моды BeamNG", () =>
        {
            var l = StModsParser.ParseList(Fx("stmods/search-real.html"));
            Assert.Eq(3, l.Count);
            Assert.True(l.All(x => x.Url.AbsolutePath.StartsWith("/beamng_drive/")));
            Assert.True(l.All(x => x.Title.Contains("Sienna")));
        });
        t.Run("STMods, карточка: размер и хост из кнопки скачивания", () =>
        {
            var b = new Listing("stmods", new Uri("https://stmods.org/beamng_drive/mods/x/"), "x", "BMW E34");
            var d = StModsParser.ParseDetail(Fx("stmods/mod-real.html"), b);
            Assert.Eq("matissator", d.Author);
            Assert.Eq((long)(212.72 * 1024 * 1024), d.SizeBytes);
            Assert.Eq("modsfire", d.Offers.Single().Host);
        });
        t.Run("STMods: размер «1 234,5 МБ» с пробелом и запятой", () =>
            Assert.Eq((long)(1234.5 * 1024 * 1024), StModsParser.ParseSize("1 234,5 МБ")));

        t.Run("ModsFire, первая страница: имя, размер, дата, идентификатор", () =>
        {
            var f = ModsFireParser.ParseFirstStep(Fx("modsfire/first-step-real.html"));
            Assert.True(f is not null, "страница разобрана");
            Assert.Eq("mercedes_g_wagon-1_0.zip", f!.FileName);
            Assert.Eq("zip", f.Extension);
            Assert.Eq((long)(517.85 * 1024 * 1024), f.SizeBytes);
            Assert.Eq(2026, f.UploadedAt?.Year);
            Assert.Eq("9nDwN00BJ04Ef55", f.FileId);
        });
        t.Run("ModsFire: чужая страница даёт null", () => Assert.Null(ModsFireParser.ParseFirstStep("<html>нет</html>")));

        t.Run("в сохранённых страницах нет IP и токенов", () =>
        {
            foreach (var f in Directory.GetFiles(Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "fixtures"), "*-real.html", SearchOption.AllDirectories))
            {
                var s = File.ReadAllText(f);
                Assert.False(s.Contains("client_ip") || s.Contains("csrf-token"), f);
            }
        });
    }
}
