using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Updates;
using BeamModManager.Core.Versioning;

namespace BeamModManager.Core.Tests;

public static class LibraryTests
{
    private static readonly DateTimeOffset Now = new(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);

    private static InMemoryLibraryStore Store(string version = "1.0", long size = 100)
    {
        var s = new InMemoryLibraryStore();
        s.UpsertMod(new InstalledMod("e34", "BMW E34", "bmw_e34.zip", version, size, Now.AddDays(-30)));
        s.UpsertBinding(new ModBinding("e34", "stmods", "bmw_e34_by_matissator_for_beamng", new Uri("https://stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng/"), true));
        return s;
    }

    private static Listing Site(string? version, long? size = null) =>
        new("stmods", new Uri("https://stmods.org/x/"), "x", "BMW E34") { ModVersion = version, SizeBytes = size };

    public static void Register(TestRunner t)
    {
        t.Group("Библиотека и проверка обновлений");

        t.Run("привязка к несуществующему моду отклоняется", () =>
        {
            var s = new InMemoryLibraryStore();
            var threw = false;
            try { s.UpsertBinding(new ModBinding("нет", "stmods", "x", new Uri("https://a/"), true)); }
            catch (InvalidOperationException) { threw = true; }
            Assert.True(threw, "ожидалось исключение");
        });

        t.Run("неподтверждённая привязка в проверку не попадает", () =>
        {
            var s = Store();
            s.UpsertBinding(new ModBinding("e34", "stmods", "x", new Uri("https://a/"), false));
            Assert.Eq(0, UpdateCheckService.Tracked(s).Count);
        });

        t.Run("новый мод: планировщик берёт его на проверку", () =>
        {
            var p = UpdateCheckService.PlanFeeds(Store(), Now);
            Assert.Eq(1, p.Due.Count);
            Assert.Eq(3, p.FeedPages["stmods"]);
        });

        t.Run("сайт показал версию новее: обновление найдено (Likely)", () =>
        {
            var s = Store();
            var b = UpdateCheckService.Tracked(s).Single();
            UpdateCheckService.ApplyCard(s, b, Site("1.1", 120), Now);
            var f = UpdateCheckService.Pending(s).Single();
            Assert.Eq(UpdateStatus.Likely, f.Status);
            Assert.Eq("1.1", f.LatestVersion);
            Assert.Eq("BMW E34", f.ModName);
        });

        t.Run("версия та же: обновления нет", () =>
        {
            var s = Store();
            UpdateCheckService.ApplyCard(s, UpdateCheckService.Tracked(s).Single(), Site("1.0", 100), Now);
            Assert.Eq(0, UpdateCheckService.Pending(s).Count);
        });

        t.Run("установка новой версии убирает пункт сама", () =>
        {
            var s = Store();
            UpdateCheckService.ApplyCard(s, UpdateCheckService.Tracked(s).Single(), Site("1.1", 120), Now);
            Assert.Eq(1, UpdateCheckService.Pending(s).Count);
            s.UpsertMod(s.GetMod("e34")! with { Version = "1.1", SizeBytes = 120 });
            Assert.Eq(0, UpdateCheckService.Pending(s).Count);
        });

        t.Run("версия на сайте старее установленной: не обновление", () =>
        {
            var s = Store("2.0");
            UpdateCheckService.ApplyCard(s, UpdateCheckService.Tracked(s).Single(), Site("1.5", 100), Now);
            Assert.Eq(0, UpdateCheckService.Pending(s).Count);
        });

        t.Run("версия не разобрана, размер изменился: возможно (Possible)", () =>
        {
            var s = Store("Update 2!", 100);
            UpdateCheckService.ApplyCard(s, UpdateCheckService.Tracked(s).Single(), Site("Update 3!", 150), Now);
            Assert.Eq(UpdateStatus.Possible, UpdateCheckService.Pending(s).Single().Status);
        });

        t.Run("лента подтвердила «без изменений»: время проверки обновилось, статус прежний", () =>
        {
            var s = Store();
            UpdateCheckService.ApplyCard(s, UpdateCheckService.Tracked(s).Single(), Site("1.1", 120), Now);
            var later = Now.AddHours(8);
            UpdateCheckService.ApplyFeedUnchanged(s, UpdateCheckService.Tracked(s).Single(), later);
            Assert.Eq(later, s.GetCheck("e34", "stmods")!.LastCheckedUtc);
            Assert.Eq(1, UpdateCheckService.Pending(s).Count);
        });

        t.Run("LastChanged двигается только когда сайт изменился", () =>
        {
            var s = Store();
            UpdateCheckService.ApplyCard(s, UpdateCheckService.Tracked(s).Single(), Site("1.1", 120), Now);
            UpdateCheckService.ApplyCard(s, UpdateCheckService.Tracked(s).Single(), Site("1.1", 120), Now.AddDays(2));
            Assert.Eq(Now, s.GetCheck("e34", "stmods")!.LastChangedUtc);
            UpdateCheckService.ApplyCard(s, UpdateCheckService.Tracked(s).Single(), Site("1.2", 130), Now.AddDays(3));
            Assert.Eq(Now.AddDays(3), s.GetCheck("e34", "stmods")!.LastChangedUtc);
        });

        t.Run("свежая проверка: повторно в план не попадает", () =>
        {
            var s = Store();
            UpdateCheckService.ApplyCard(s, UpdateCheckService.Tracked(s).Single(), Site("1.0", 100), Now);
            Assert.Eq(0, UpdateCheckService.PlanFeeds(s, Now.AddHours(1)).Due.Count);
            Assert.Eq(1, UpdateCheckService.PlanFeeds(s, Now.AddHours(7)).Due.Count);
        });

        t.Run("источник со статусом «нужна проверка» исключается из плана", () =>
        {
            var s = Store();
            s.SetSourceStatus("stmods", SourceStatus.NeedsHumanCheck);
            var p = UpdateCheckService.PlanFeeds(s, Now);
            Assert.Eq(0, p.Due.Count);
            Assert.Eq(1, p.SkippedSources.Count);
        });

        t.Run("удаление мода убирает привязки и состояние проверок", () =>
        {
            var s = Store();
            UpdateCheckService.ApplyCard(s, UpdateCheckService.Tracked(s).Single(), Site("1.1"), Now);
            s.RemoveMod("e34");
            Assert.Eq(0, s.Bindings().Count);
            Assert.Eq(0, s.Checks().Count);
        });

        t.Run("два сайта: показывается более уверенное обновление", () =>
        {
            var s = Store("Update 2!", 100);
            s.UpsertBinding(new ModBinding("e34", "modland", "m", new Uri("https://www.modland.net/m.html"), true));
            foreach (var b in UpdateCheckService.Tracked(s))
                UpdateCheckService.ApplyCard(s, b, Site(b.Source == "modland" ? "Update 3!" : "3.0", b.Source == "modland" ? 150 : 100), Now);
            var f = UpdateCheckService.Pending(s).Single();
            Assert.Eq(UpdateStatus.Possible, f.Status); // «3.0» против «Update 2!» не сравнивается, размер тот же
            Assert.Eq("modland", f.Source);
        });
    }
}
