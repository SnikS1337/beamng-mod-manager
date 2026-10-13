using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Search;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Updates;

namespace BeamModManager.Core.Tests;

/// <summary>Потерянные страницы модов и починка привязок: промахи, предложения замены, применение, регресс.</summary>
public static class RepairTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);

    private sealed class Site : IModSource
    {
        public readonly Dictionary<string, Listing> Cards = new();   // по адресу; нет записи — страницы нет
        public readonly List<Listing> SearchResults = new();
        public List<Listing> Feed = new();
        public int CardCalls, SearchCalls;
        public Site(string id) { Id = id; DisplayName = id; }
        public string Id { get; }
        public string DisplayName { get; }
        public bool SupportsSearch => true;
        public Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<Listing>>(page == 1 ? Feed : Array.Empty<Listing>());
        public Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default)
        {
            Interlocked.Increment(ref CardCalls);
            return Task.FromResult(Cards.GetValueOrDefault(url.AbsoluteUri));
        }
        public Task<IReadOnlyList<Listing>> SearchAsync(string q, CancellationToken ct = default)
        {
            Interlocked.Increment(ref SearchCalls);
            return Task.FromResult<IReadOnlyList<Listing>>(SearchResults.ToList());
        }
        public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? s, CancellationToken ct = default) => FeedPageAsync(1, ct);
        public Task<IReadOnlyList<Offer>> GetOffersAsync(Listing l, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Offer>>(Array.Empty<Offer>());
    }

    private static string U(string src, string id) => $"https://{src}.test/{id}";
    private static Listing L(string src, string id, string title, string? ver = null) =>
        new(src, new Uri(U(src, id)), id, title) { ModVersion = ver };

    /// <summary>Мод «Audi A3» с привязкой к странице на stmods, проверенной давно.</summary>
    private static InMemoryLibraryStore Lib(string src = "stmods", string id = "audi-a3", int checkedDaysAgo = 3)
    {
        var s = new InMemoryLibraryStore();
        s.UpsertMod(new InstalledMod("audi", "Audi A3", "audi_a3_v1.0.zip", "1.0", 50_000_000, T0.AddDays(-90)));
        s.UpsertBinding(new ModBinding("audi", src, id, new Uri(U(src, id)), true));
        s.SaveCheck(new CheckState("audi", src, T0.AddDays(-checkedDaysAgo), T0.AddDays(-30), "1.0", 50_000_000));
        return s;
    }

    private static Task<UpdateRunReport> Run(InMemoryLibraryStore s, Site site, DateTimeOffset now, bool force = false) =>
        UpdateRunner.RunAsync(s, new IModSource[] { site }, now, new UpdateCheckOptions { Force = force });

    public static void Register(TestRunner t)
    {
        t.Group("Потерянные страницы: счёт промахов");

        t.Run("первый промах: состояние и версия сохранены, время проверки обновлено, ложного обновления нет, привязка ещё не «потеряна»", async () =>
        {
            var s = Lib(); var site = new Site("stmods");
            var r = await Run(s, site, T0);
            var c = s.GetCheck("audi", "stmods")!;
            Assert.Eq(1, c.MissCount);
            Assert.Eq("1.0", c.LastVersion);
            Assert.Eq(T0, c.LastCheckedUtc);
            Assert.Eq(0, r.Findings.Count);
            Assert.Eq(0, BindingRepair.Dead(s).Count);
            Assert.Eq(1, r.Sources[0].MissingMods.Count);
        });

        t.Run("сразу повторное нажатие мёртвую страницу не дёргает (срок проверки), промахи не накручиваются", async () =>
        {
            var s = Lib(); var site = new Site("stmods");
            await Run(s, site, T0);
            var calls = site.CardCalls;
            await Run(s, site, T0.AddMinutes(5));
            await Run(s, site, T0.AddMinutes(10));
            Assert.Eq(calls, site.CardCalls);
            Assert.Eq(1, s.GetCheck("audi", "stmods")!.MissCount);
        });

        t.Run("второй промах после срока: привязка потеряна и видна в списке", async () =>
        {
            var s = Lib(); var site = new Site("stmods");
            await Run(s, site, T0);
            await Run(s, site, T0.AddDays(1));
            var dead = BindingRepair.Dead(s);
            Assert.Eq(1, dead.Count);
            Assert.Eq("audi", dead[0].Mod.Id);
            Assert.Eq(2, dead[0].Misses);
        });

        t.Run("страница вернулась: счётчик обнуляется, привязка снова живая, версия обновилась", async () =>
        {
            var s = Lib(); var site = new Site("stmods");
            await Run(s, site, T0); await Run(s, site, T0.AddDays(1));
            site.Cards[U("stmods", "audi-a3")] = L("stmods", "audi-a3", "Audi A3", "1.2");
            var r = await Run(s, site, T0.AddDays(10));
            Assert.Eq(0, s.GetCheck("audi", "stmods")!.MissCount);
            Assert.Eq(0, BindingRepair.Dead(s).Count);
            Assert.Eq("1.2", r.Findings.Single().LatestVersion);
        });

        t.Run("появилась в ленте: промахи сбрасываются без открытия карточки", async () =>
        {
            var s = Lib(); var site = new Site("stmods");
            await Run(s, site, T0);
            site.Feed = new() { L("stmods", "audi-a3", "Audi A3", "1.0") };
            await Run(s, site, T0.AddDays(1));
            Assert.Eq(0, s.GetCheck("audi", "stmods")!.MissCount);
        });

        t.Run("сайт сломался целиком (нет ни одной из трёх карточек): промахи не считаются, виноват сайт", async () =>
        {
            var s = new InMemoryLibraryStore();
            for (var i = 0; i < 4; i++)
            {
                s.UpsertMod(new InstalledMod($"m{i}", $"m{i}", $"m{i}.zip", "1.0", null, T0.AddDays(-90)));
                s.UpsertBinding(new ModBinding($"m{i}", "stmods", $"m{i}", new Uri(U("stmods", $"m{i}")), true));
                s.SaveCheck(new CheckState($"m{i}", "stmods", T0.AddDays(-3), T0.AddDays(-30), "1.0", null));
            }
            var r = await Run(s, new Site("stmods"), T0);
            Assert.True(r.Sources[0].Note is not null, "заметка о возможной смене вёрстки");
            Assert.True(s.Checks().All(c => c.MissCount == 0), "промахи не записаны");
        });

        t.Run("мёртвую страницу проверяют раз в неделю, принудительная проверка — сразу", async () =>
        {
            var s = Lib(); var site = new Site("stmods");
            await Run(s, site, T0); await Run(s, site, T0.AddDays(1));
            var calls = site.CardCalls;
            await Run(s, site, T0.AddDays(3));
            Assert.Eq(calls, site.CardCalls);
            await Run(s, site, T0.AddDays(9));
            Assert.Eq(calls + 1, site.CardCalls);
            await Run(s, site, T0.AddDays(9).AddMinutes(1), force: true);
            Assert.Eq(calls + 2, site.CardCalls);
        });

        t.Group("Починка привязки: предложения");

        (InMemoryLibraryStore Store, Site Stmods, Site Land, SearchService Search, DeadBinding Dead) Dead()
        {
            var s = Lib(); var st = new Site("stmods"); var land = new Site("modland");
            st.SearchResults.Add(L("stmods", "audi-a3", "Audi A3 v1.0"));                // сама потерянная страница
            st.SearchResults.Add(L("stmods", "audi-a3-new", "Audi A3 v1.1", "1.1"));      // замена
            st.SearchResults.Add(L("stmods", "bmw-m3", "BMW M3 E30"));                     // чужое
            land.SearchResults.Add(L("modland", "audi-a3-2", "Audi A3 v1.1", "1.1"));
            var search = new SearchService(new IModSource[] { st, land });
            Run(s, st, T0).Wait(); Run(s, st, T0.AddDays(1)).Wait();
            return (s, st, land, search, BindingRepair.Dead(s).Single());
        }

        t.Run("поиск замены: потерянная страница и чужие моды не предлагаются, настоящие кандидаты есть", async () =>
        {
            var d = Dead();
            var p = await BindingRepair.ProposeAsync(d.Store, d.Dead, d.Search);
            var ids = p.Candidates.Select(c => c.Listing.SourceId).ToList();
            Assert.True(ids.Contains("audi-a3-new"), "замена найдена");
            Assert.True(ids.Contains("audi-a3-2"), "кандидат с другого сайта");
            Assert.False(ids.Contains("audi-a3"), "саму потерянную страницу не предлагаем");
            Assert.False(ids.Contains("bmw-m3"), "чужой мод не предлагаем");
            Assert.Eq(2, p.Sources.Count);
        });

        t.Run("сайт, где у мода уже есть живая привязка, замены не предлагает", async () =>
        {
            var d = Dead();
            d.Store.UpsertBinding(new ModBinding("audi", "modland", "audi-live", new Uri(U("modland", "audi-live")), true));
            var p = await BindingRepair.ProposeAsync(d.Store, d.Dead, d.Search);
            Assert.False(p.Candidates.Any(c => c.Listing.Source == "modland"), "modland уже привязан");
        });

        t.Run("все сайты молчат или ничего похожего: пустой список без ошибок", async () =>
        {
            var d = Dead();
            d.Stmods.SearchResults.Clear(); d.Land.SearchResults.Clear();
            var p = await BindingRepair.ProposeAsync(d.Store, d.Dead, d.Search);
            Assert.Eq(0, p.Candidates.Count);
        });

        t.Group("Починка привязки: применение");

        t.Run("замена на том же сайте: старое состояние проверок убрано, новая привязка подтверждена, потерянных нет", async () =>
        {
            var d = Dead();
            var chosen = L("stmods", "audi-a3-new", "Audi A3 v1.1", "1.1");
            Assert.True(BindingRepair.Apply(d.Store, d.Dead, chosen), "применено");
            var b = d.Store.Bindings().Single();
            Assert.Eq("audi-a3-new", b.SourceId);
            Assert.True(b.Confirmed, "подтверждена");
            Assert.True(d.Store.GetCheck("audi", "stmods") is null, "прошлые промахи не унаследованы");
            Assert.Eq(0, BindingRepair.Dead(d.Store).Count);
            Assert.Eq(1, d.Store.Mods().Count);
        });

        t.Run("замена на другом сайте: старая привязка убрана, новая создана", () =>
        {
            var d = Dead();
            Assert.True(BindingRepair.Apply(d.Store, d.Dead, L("modland", "audi-a3-2", "Audi A3 v1.1", "1.1")), "применено");
            var b = d.Store.Bindings().Single();
            Assert.Eq("modland", b.Source);
            Assert.Eq(0, BindingRepair.Dead(d.Store).Count);
            return Task.CompletedTask;
        });

        t.Run("устаревший выбор не применяется и ничего не портит: мод удалён, привязка уже изменена, выбрана та же мёртвая страница, чужой живой сайт", () =>
        {
            var chosen = L("stmods", "audi-a3-new", "x");
            var d1 = Dead(); d1.Store.RemoveMod("audi");
            Assert.False(BindingRepair.Apply(d1.Store, d1.Dead, chosen), "мод удалён");
            Assert.Eq(0, d1.Store.Bindings().Count);

            var d2 = Dead();
            Assert.True(BindingRepair.Apply(d2.Store, d2.Dead, chosen), "первый раз");
            Assert.False(BindingRepair.Apply(d2.Store, d2.Dead, L("stmods", "other", "y")), "вторую починку по старым данным отклоняем");
            Assert.Eq("audi-a3-new", d2.Store.Bindings().Single().SourceId);

            var d3 = Dead();
            Assert.False(BindingRepair.Apply(d3.Store, d3.Dead, L("stmods", "audi-a3", "Audi A3")), "та же мёртвая страница");
            Assert.Eq(2, d3.Store.GetCheck("audi", "stmods")!.MissCount);

            var d4 = Dead();
            d4.Store.UpsertBinding(new ModBinding("audi", "modland", "live", new Uri(U("modland", "live")), true));
            Assert.False(BindingRepair.Apply(d4.Store, d4.Dead, L("modland", "audi-a3-2", "z")), "живую привязку на другом сайте не перезаписываем");
            Assert.Eq(2, d4.Store.Bindings().Count);
            return Task.CompletedTask;
        });

        t.Run("параллельные «починить»: успешна одна, привязка одна", () =>
        {
            var d = Dead();
            var results = new bool[8];
            Parallel.For(0, 8, i => results[i] = BindingRepair.Apply(d.Store, d.Dead, L("stmods", $"new{i}", "Audi A3")));
            Assert.Eq(1, results.Count(x => x));
            Assert.Eq(1, d.Store.Bindings().Count);
            return Task.CompletedTask;
        });

        t.Group("Починка привязки: сквозной сценарий");

        t.Run("страница пропала дважды → предложение → выбор → обычная проверка видит новую версию по новому адресу", async () =>
        {
            var d = Dead();
            var p = await BindingRepair.ProposeAsync(d.Store, d.Dead, d.Search);
            var best = p.Candidates.First(c => c.Listing.SourceId == "audi-a3-new");
            Assert.True(BindingRepair.Apply(d.Store, d.Dead, best.Listing), "применено");
            d.Stmods.Cards[U("stmods", "audi-a3-new")] = L("stmods", "audi-a3-new", "Audi A3", "1.3");
            var r = await Run(d.Store, d.Stmods, T0.AddDays(20));
            Assert.Eq("1.3", r.Findings.Single().LatestVersion);
            Assert.Eq(0, r.Sources[0].MissingMods.Count);
            Assert.Eq(0, BindingRepair.Dead(d.Store).Count);
        });

        t.Run("координатор целиком: две нажатые проверки с пропавшей страницей дают потерянную привязку без лишних запросов", async () =>
        {
            var now = T0;
            var s = Lib(); var site = new Site("stmods");
            var coord = new UpdateCoordinator(s, () => new IModSource[] { site }, () => now);
            await coord.CheckAsync();
            now = T0.AddDays(2);
            await coord.CheckAsync();
            Assert.Eq(2, site.CardCalls);
            Assert.Eq(1, BindingRepair.Dead(s).Count);
        });
    }
}
