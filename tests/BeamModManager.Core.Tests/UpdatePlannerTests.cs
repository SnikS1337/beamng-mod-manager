using BeamModManager.Core.Sources;
using BeamModManager.Core.Updates;

namespace BeamModManager.Core.Tests;

public static class UpdatePlannerTests
{
    private static readonly DateTimeOffset Now = new(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);
    private static readonly IReadOnlyDictionary<string, SourceStatus> AllOk = new Dictionary<string, SourceStatus>();

    private static List<TrackedBinding> Mods(int n, Func<int, TrackedBinding>? f = null)
    {
        var sources = new[] { "modland", "modsgaming", "stmods" };
        return Enumerable.Range(0, n)
            .Select(i => f?.Invoke(i) ?? new TrackedBinding($"m{i}", sources[i % 3], $"id{i}"))
            .ToList();
    }

    private static IReadOnlyDictionary<string, IReadOnlyDictionary<string, FeedEntry>> Feed(
        params (string Source, FeedEntry Entry)[] items) =>
        items.GroupBy(x => x.Source).ToDictionary(
            g => g.Key,
            g => (IReadOnlyDictionary<string, FeedEntry>)g.ToDictionary(x => x.Entry.SourceId, x => x.Entry));

    public static void Register(TestRunner t)
    {
        t.Group("Планировщик проверки обновлений");

        t.Run("200 модов: запросов к лентам немного", () =>
        {
            var p = UpdateCheckPlanner.PlanFeeds(Mods(200), AllOk, Now);
            Assert.Eq(200, p.Due.Count);
            Assert.True(p.TotalRequests <= 30, $"запросов {p.TotalRequests}");
            Assert.True(p.FeedPages.Values.All(v => v <= 10), "лимит страниц ленты");
        });

        t.Run("200 модов, лента ничего не знает: карточек не больше лимита на сайт", () =>
        {
            var p = UpdateCheckPlanner.PlanFeeds(Mods(200), AllOk, Now);
            var c = UpdateCheckPlanner.PlanCards(p.Due, Feed(), Now);
            Assert.True(c.ToFetch.Values.All(v => v.Count <= 25), "лимит карточек на сайт");
            Assert.True(c.TotalRequests <= 75, $"карточек {c.TotalRequests}");
            Assert.Eq(200, c.TotalRequests + c.Postponed.Count + c.ConfirmedUnchanged.Count);
        });

        t.Run("общий расход на 200 модов: ленты плюс карточки не больше 105 запросов", () =>
        {
            var p = UpdateCheckPlanner.PlanFeeds(Mods(200), AllOk, Now);
            var c = UpdateCheckPlanner.PlanCards(p.Due, Feed(), Now);
            Assert.True(p.TotalRequests + c.TotalRequests <= 105, $"{p.TotalRequests + c.TotalRequests}");
        });

        t.Run("свежая проверка (в пределах TTL): мод пропускается", () =>
        {
            var m = Mods(3, i => new TrackedBinding($"m{i}", "stmods", $"id{i}", LastCheckedUtc: Now.AddHours(-1)));
            Assert.Eq(0, UpdateCheckPlanner.PlanFeeds(m, AllOk, Now).Due.Count);
        });

        t.Run("принудительная проверка игнорирует TTL", () =>
        {
            var m = Mods(3, i => new TrackedBinding($"m{i}", "stmods", $"id{i}", LastCheckedUtc: Now.AddHours(-1)));
            var p = UpdateCheckPlanner.PlanFeeds(m, AllOk, Now, new UpdateCheckOptions { Force = true });
            Assert.Eq(3, p.Due.Count);
        });

        t.Run("источник с проверкой «вы человек?» пропускается, остальные идут", () =>
        {
            var st = new Dictionary<string, SourceStatus> { ["modland"] = SourceStatus.NeedsHumanCheck };
            var p = UpdateCheckPlanner.PlanFeeds(Mods(30), st, Now);
            Assert.False(p.FeedPages.ContainsKey("modland"));
            Assert.True(p.FeedPages.ContainsKey("stmods"));
            Assert.Eq(SourceStatus.NeedsHumanCheck, p.SkippedSources.Single().Status);
            Assert.True(p.Due.All(b => b.Source != "modland"));
        });

        t.Run("недоступный источник не тратит запросов", () =>
        {
            var st = new Dictionary<string, SourceStatus> { ["stmods"] = SourceStatus.Unavailable, ["modland"] = SourceStatus.Unavailable, ["modsgaming"] = SourceStatus.Unavailable };
            var p = UpdateCheckPlanner.PlanFeeds(Mods(30), st, Now);
            Assert.Eq(0, p.TotalRequests);
        });

        t.Run("лента: версия не изменилась, карточка не нужна", () =>
        {
            var b = new TrackedBinding("m", "stmods", "x", Now.AddDays(-2), Now.AddDays(-10), "1.0", 100);
            var c = UpdateCheckPlanner.PlanCards(new[] { b }, Feed(("stmods", new FeedEntry("x", "1.0", 100, null))), Now);
            Assert.Eq(0, c.TotalRequests);
            Assert.Eq(1, c.ConfirmedUnchanged.Count);
        });

        t.Run("лента: версия выросла, карточку открываем", () =>
        {
            var b = new TrackedBinding("m", "stmods", "x", Now.AddDays(-2), Now.AddDays(-10), "1.0", 100);
            var c = UpdateCheckPlanner.PlanCards(new[] { b }, Feed(("stmods", new FeedEntry("x", "1.1", 100, null))), Now);
            Assert.Eq(1, c.ToFetch["stmods"].Count);
        });

        t.Run("лента: изменившиеся идут раньше тех, про кого лента молчит", () =>
        {
            var silent = new TrackedBinding("a", "stmods", "a", Now.AddDays(-30), Now.AddDays(-5), "1", 1);
            var changed = new TrackedBinding("b", "stmods", "b", Now.AddDays(-1), Now.AddDays(-5), "1", 1);
            var c = UpdateCheckPlanner.PlanCards(new[] { silent, changed },
                Feed(("stmods", new FeedEntry("b", "2", 1, null))), Now);
            Assert.Eq("b", c.ToFetch["stmods"][0].ModId);
        });

        t.Run("без прошлых данных лента не доказывает отсутствие изменений", () =>
        {
            var b = new TrackedBinding("m", "stmods", "x");
            var c = UpdateCheckPlanner.PlanCards(new[] { b }, Feed(("stmods", new FeedEntry("x", "1.0", 100, null))), Now);
            Assert.Eq(1, c.TotalRequests);
        });

        t.Run("старый неизменный мод проверяется редко: через 2 дня после проверки не берём", () =>
        {
            var old = new TrackedBinding("m", "stmods", "x", Now.AddDays(-2), Now.AddDays(-400), "1", 1);
            var c = UpdateCheckPlanner.PlanCards(new[] { old }, Feed(), Now, new UpdateCheckOptions { Ttl = TimeSpan.Zero });
            Assert.Eq(0, c.TotalRequests);
            Assert.Eq(1, c.Postponed.Count);
        });

        t.Run("недавно обновлявшийся мод проверяется ежедневно", () =>
        {
            var fresh = new TrackedBinding("m", "stmods", "x", Now.AddDays(-2), Now.AddDays(-3), "1", 1);
            var c = UpdateCheckPlanner.PlanCards(new[] { fresh }, Feed(), Now);
            Assert.Eq(1, c.TotalRequests);
        });

        t.Run("самый запущенный мод берётся первым при нехватке лимита", () =>
        {
            var m = Enumerable.Range(0, 30)
                .Select(i => new TrackedBinding($"m{i}", "stmods", $"id{i}", Now.AddDays(-2 - i), Now.AddDays(-5), "1", 1)).ToList();
            var c = UpdateCheckPlanner.PlanCards(m, Feed(), Now);
            Assert.Eq(25, c.ToFetch["stmods"].Count);
            Assert.Eq("m29", c.ToFetch["stmods"][0].ModId);
            Assert.Eq(5, c.Postponed.Count);
        });

        t.Run("принудительная проверка открывает карточки, не дожидаясь интервала", () =>
        {
            var old = new TrackedBinding("m", "stmods", "x", Now.AddDays(-2), Now.AddDays(-400), "1", 1);
            var c = UpdateCheckPlanner.PlanCards(new[] { old }, Feed(), Now, new UpdateCheckOptions { Force = true });
            Assert.Eq(1, c.TotalRequests);
        });
    }
}
