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
        t.Run("Modland, лента: значок «Paid content» даёт Paid, остальные не трогаются", () =>
        {
            var l = ModlandParser.ParseList(Fx("modland/list-real.html"));
            Assert.Eq(1, l.Count(x => x.Price == PriceStatus.Paid));
            Assert.True(l.Single(x => x.Price == PriceStatus.Paid).Title.StartsWith("Beach House"), "тот самый мод");
        });
        t.Run("Modland, значок платного учитывается на странице, даже если ссылок нет", () =>
        {
            var html = "<h1 class=\"post-title mb-0\"><span title=\"Paid content\"></span>Some Mod</h1><p>Latest Version 1.0</p>";
            var d = ModlandParser.ParseDetail(html, new Listing("modland", new Uri("https://www.modland.net/x.html"), "x", "Some Mod"));
            Assert.Eq(PriceStatus.Paid, d.Price);
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

        t.Run("ModsFire, второй шаг: распознан, файл тот же, идентификатор из /d/", () =>
        {
            var first = Fx("modsfire/first-step-real.html");
            var second = Fx("modsfire/second-step-real.html");
            Assert.Eq(ModsFireStep.First, ModsFireParser.DetectStep(first));
            Assert.Eq(ModsFireStep.Second, ModsFireParser.DetectStep(second));
            Assert.Eq(ModsFireStep.Unknown, ModsFireParser.DetectStep("<html></html>"));
            var f = ModsFireParser.ParseFile(second);
            Assert.True(f is not null, "второй шаг разобран");
            Assert.Eq("mercedes_g_wagon-1_0.zip", f!.FileName);
            Assert.Eq((long)(517.85 * 1024 * 1024), f.SizeBytes);
            Assert.Eq("9nDwN00BJ04Ef55", f.FileId);
        });

        t.Run("ModsGaming, поиск: 9 результатов с идентификаторами и датами", () =>
        {
            var l = ModsGamingParser.ParseSearch(Fx("modsgaming/search-real.html"));
            Assert.Eq(9, l.Count);
            var ids = l.Select(x => x.SourceId).ToHashSet();
            foreach (var id in new[] { "133899", "141677", "160200", "142469", "134101", "128066", "148563", "118383", "130575" })
                Assert.True(ids.Contains(id), id);
            Assert.True(l.All(x => !x.Title.Contains('<') && x.Title.Contains("Sienna", StringComparison.OrdinalIgnoreCase)), "подсветка снята");
            var newest = l.Single(x => x.SourceId == "160200");
            Assert.Eq("2.2", newest.ModVersion);
            Assert.Eq("0.39.x", newest.GameVersion);
            Assert.Eq(2026, newest.PublishedAt?.Year);
        });

        t.Run("Modland, поиск: карточки читаются обычным разбором ленты", () =>
        {
            var l = ModlandParser.ParseList(Fx("modland/search-real.html"));
            Assert.Eq(6, l.Count);
            Assert.True(l.All(x => x.Title.Contains("sienna", StringComparison.OrdinalIgnoreCase)), string.Join("|", l.Select(x => x.Title)));
        });

        foreach (var (file, host, title) in new[]
        {
            ("pack-wheels-boosty-real.html", "boosty.to", "Biggest Stylish Wheelpack"),
            ("pack-golf-kofi-real.html", "ko-fi.com", "1997-2004 Volkswagen Golf 4 Pack"),
            ("pack-bmw-kofi-discord-real.html", "ko-fi.com", "BMW 3-Series/M3 Pack (Update 2!)"),
        })
            t.Run($"Modland, пак без метки PAID ({host}): платный по ссылке, файла нет", () =>
            {
                var b = new Listing("modland", new Uri("https://www.modland.net/x/p.html"), "p", title);
                var d = ModlandParser.ParseDetail(Fx("modland/" + file), b);
                Assert.Eq(PriceStatus.Paid, d.Price);
                Assert.True(d.Offers.Any(o => o.Kind == OfferKind.PaidLink && o.Host.Contains(host)), "ссылка на страницу автора");
                Assert.False(d.Offers.Any(o => o.Kind != OfferKind.PaidLink), "скачиваемых ссылок нет");
            });

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
