using BeamModManager.Core.Library;
using BeamModManager.Core.Matching;
using BeamModManager.Core.Models;
using BeamModManager.Core.Search;
using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Search;
using BeamModManager.Ui.Updates;

namespace BeamModManager.Core.Tests;

/// <summary>Раздел «Поиск» без окна: поток ответов, скелеты, пустые состояния, цены, «Установленные», дубли.</summary>
public static class SearchScreenTests
{
    private const long MB = 1024 * 1024;

    /// <summary>Сайт, которому можно сказать, когда ответить.</summary>
    private sealed class Site : IModSource
    {
        public TaskCompletionSource<IReadOnlyList<Listing>> Answer = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public Site(string id, bool search = true) { Id = id; SupportsSearch = search; }
        public string Id { get; }
        public string DisplayName => Id;
        public bool SupportsSearch { get; }
        public int Calls;
        public async Task<IReadOnlyList<Listing>> SearchAsync(string query, CancellationToken ct = default)
        {
            Calls++;
            var answer = Answer; // ответ этого вызова: Reset для следующего поиска его не подменяет
            using (ct.Register(() => answer.TrySetCanceled(ct))) return await answer.Task.ConfigureAwait(false);
        }
        public void Reply(params Listing[] items) => Answer.TrySetResult(items);
        public void Fail() => Answer.TrySetException(new SourceException(SourceStatus.Unavailable, "код 503"));
        public void Reset() => Answer = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default) => Task.FromResult<Listing?>(null);
        public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? since, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<IReadOnlyList<Offer>> GetOffersAsync(Listing listing, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Offer>>(Array.Empty<Offer>());
    }

    private static Listing L(string source, string id, string title, PriceStatus price = PriceStatus.Unknown) =>
        new(source, new Uri($"https://{source}.test/{id}"), id, title) { Price = price };

    /// <summary>Промежуточные ответы приходят из фона: копим их и применяем, когда тест скажет (как очередь окна).</summary>
    private sealed class Queue
    {
        private readonly List<Action> _items = new();
        public void Post(Action a) { lock (_items) _items.Add(a); }
        public void Drain()
        {
            List<Action> now;
            lock (_items) { now = _items.ToList(); _items.Clear(); }
            foreach (var a in now) a();
        }
    }

    private sealed class Rig
    {
        public readonly Site Modland = new("modland"), Gaming = new("modsgaming"), St = new("stmods"), Repo = new("beamng", search: false);
        public AppSettings Settings = new() { DisabledSources = Array.Empty<string>() }; // все сайты, в том числе Modland
        public readonly MatchDecisionStore Decisions = new();
        public readonly List<BeamModManager.Ui.Garage.ModCard> Installed = new();
        public readonly Queue Q = new();
        public readonly SearchViewModel Vm;
        public Rig()
        {
            var backend = new LibrarySearchBackend(new IModSource[] { Modland, Gaming, St, Repo }, () => Settings, Decisions);
            Vm = new SearchViewModel(backend, () => Installed, Q.Post);
        }
        /// <summary>Ждём, пока поиск дойдёт до сайтов (они вызываются из фона).</summary>
        public void WaitCalls(int n)
        {
            var until = DateTime.UtcNow.AddSeconds(5);
            while (Modland.Calls + Gaming.Calls + St.Calls < n && DateTime.UtcNow < until) Thread.Sleep(5);
        }
        public void WaitPartial()
        {
            var until = DateTime.UtcNow.AddSeconds(5);
            while (DateTime.UtcNow < until) { Thread.Sleep(10); Q.Drain(); if (Vm.Cards.Count > 0) return; }
        }
    }

    public static void Register(TestRunner t)
    {
        t.Group("Раздел «Поиск»");

        t.Run("до запроса: «Что ищем?», сайты готовы; выключенный сайт подписан, без поиска — не в списке", () =>
        {
            var r = new Rig { Settings = new AppSettings { DisabledSources = new[] { "stmods" } } };
            r.Vm.RefreshSites();
            Assert.Eq("Что ищем?", r.Vm.Empty!.Title);
            Assert.Eq(0, r.Vm.Skeletons);
            Assert.Eq("ищем сразу на 2 сайтах", r.Vm.HeaderNote);
            Assert.Eq("modland:готов|modsgaming:готов|stmods:выключен", string.Join("|", r.Vm.Sites.Select(s => s.Id + ":" + s.State)));
        });

        t.Run("ответы по мере прихода: скелеты, потом карточки ответившего, «ищем…» у остальных, итог", async () =>
        {
            var r = new Rig();
            r.Vm.Query = "sienna";
            var run = r.Vm.RunAsync();
            Assert.True(r.Vm.Searching, "ищем");
            Assert.Eq(SearchViewModel.SkeletonSlots, r.Vm.Skeletons);
            Assert.True(r.Vm.Empty is null, "вместо пустоты — скелеты");
            Assert.Eq("ищем на 3 сайтах…", r.Vm.HeaderNote);
            r.WaitCalls(3);
            r.Modland.Reply(L("modland", "a", "Toyota Sienna 2011-2017 2.2", PriceStatus.Free));
            r.WaitPartial();
            Assert.Eq(1, r.Vm.Cards.Count);
            Assert.Eq(0, r.Vm.Skeletons);
            Assert.Eq("modland:нашёл 1|modsgaming:ищем…|stmods:ищем…", string.Join("|", r.Vm.Sites.Select(s => s.Id + ":" + s.State)));
            r.Gaming.Reply(L("modsgaming", "x", "BMW M3 E30 v1.0"));
            r.St.Fail();
            await run;
            r.Q.Drain(); // поздние промежуточные ответы итог не портят
            Assert.False(r.Vm.Searching, "закончили");
            Assert.Eq(2, r.Vm.Cards.Count);
            Assert.Eq("stmods:не ответил", "stmods:" + r.Vm.Sites.Single(s => s.Id == "stmods").State);
            Assert.True(r.Vm.SitesNote!.StartsWith("STMods не ответил"), "предупреждение: " + r.Vm.SitesNote);
            Assert.Eq("найдено 2", r.Vm.SearchNote);
            Assert.Eq("2 мода", r.Vm.HeaderNote);
            r.Vm.Query = "";
            Assert.True(r.Vm.Cards.Count == 0 && r.Vm.Empty!.Title == "Что ищем?", "стёрли запрос — снова «Что ищем?»");
            Assert.True(r.Vm.Sites.All(x => x.State == "готов") && r.Vm.SitesNote is null, "сайты снова готовы");
        });

        t.Run("новый запрос отменяет прежний: его поздний ответ не показывается", async () =>
        {
            var r = new Rig();
            r.Vm.Query = "audi";
            var first = r.Vm.RunAsync();
            r.WaitCalls(3);
            foreach (var s in new[] { r.Modland, r.Gaming, r.St }) s.Reset();
            r.Vm.Query = "bmw";
            var second = r.Vm.RunAsync();
            await first;
            r.WaitCalls(6);
            r.Modland.Reply(L("modland", "b", "BMW M3 E30 v1.0"));
            r.Gaming.Reply();
            r.St.Reply();
            await second;
            r.Q.Drain();
            Assert.Eq("BMW M3 E30", r.Vm.Cards.Single().Title);
        });

        t.Run("короткий запрос и все сайты выключены — на сайты не ходим и объясняем", async () =>
        {
            var r = new Rig();
            r.Vm.Query = "a";
            await r.Vm.RunAsync();
            Assert.Eq("Слишком короткий запрос", r.Vm.Empty!.Title);
            Assert.Eq(0, r.Modland.Calls);
            r.Vm.Query = "ab";
            Assert.Eq("Что ищем?", r.Vm.Empty!.Title, "запрос поправили — подсказка сменилась");
            r.Settings = new AppSettings { DisabledSources = new[] { "modland", "modsgaming", "stmods" } };
            await r.Vm.RunAsync();
            Assert.Eq(EmptyAction.OpenSiteSettings, r.Vm.Empty!.Action);
            Assert.Eq("Все сайты выключены", r.Vm.Empty.Title);
            Assert.Eq(0, r.Modland.Calls);
        });

        t.Run("ничего не нашли и «сайты не ответили» — разные состояния", async () =>
        {
            var r = new Rig();
            r.Vm.Query = "zzz";
            var run = r.Vm.RunAsync();
            r.WaitCalls(3);
            r.Modland.Reply(); r.Gaming.Reply(); r.St.Reply();
            await run;
            Assert.Eq("Ничего не нашли", r.Vm.Empty!.Title);
            foreach (var s in new[] { r.Modland, r.Gaming, r.St }) s.Reset();
            run = r.Vm.RunAsync();
            r.WaitCalls(6);
            r.Modland.Fail(); r.Gaming.Fail(); r.St.Fail();
            await run;
            Assert.Eq(EmptyAction.Retry, r.Vm.Empty!.Action);
        });

        t.Run("цена и кнопки: платный — «Купить у автора», неизвестная — не бесплатный; «Установить» только с загрузкой", async () =>
        {
            var r = new Rig();
            r.Vm.Query = "car";
            var run = r.Vm.RunAsync();
            r.WaitCalls(3);
            r.Modland.Reply(L("modland", "p", "Porsche 911 GT3 RS v2.0", PriceStatus.Paid) with { Author = "pro_mods" },
                L("modland", "f", "Lada Niva 4x4 v1.1", PriceStatus.Free) with { ModVersion = "1.1", Author = "garage47" });
            r.Gaming.Reply(L("modsgaming", "u", "Volvo FH16 Truck v3"));
            r.St.Reply();
            await run;
            FoundCard C(string t) => r.Vm.Cards.Single(c => c.Title.StartsWith(t));
            Assert.True(C("Porsche") is { Primary: FoundAction.Buy, PrimaryLabel: "Купить у автора", PriceText: "Платный", ShowPage: false }, "платный");
            Assert.True(C("Lada") is { Primary: FoundAction.Page, PriceText: "Бесплатно", Subtitle: "1.1 · garage47", SitesText: "Modland" }, "бесплатный: " + C("Lada").Subtitle);
            Assert.True(C("Volvo") is { Price: PriceKind.Unknown, PriceText: "Цена не указана", Primary: FoundAction.Page }, "неизвестная");
            r.Vm.DownloadsAllowed = true;
            Assert.True(C("Lada") is { Primary: FoundAction.Install, PrimaryLabel: "Установить", ShowPage: true }, "с загрузкой");
            Assert.True(C("Porsche").Primary == FoundAction.Buy, "платный и с загрузкой — у автора");
            Assert.Eq("Бесплатные 1", r.Vm.Filters[1].Label);
            r.Vm.Filter = SearchFilter.Free;
            Assert.Eq("Lada Niva 4x4", r.Vm.Cards.Single().Title);
        });

        t.Run("«Установленные»: по странице, за которой следим, и по названию мода", async () =>
        {
            var r = new Rig();
            var g = new GarageViewModel(new OneMod(), () => DateTimeOffset.UtcNow);
            g.Refresh();
            r.Installed.AddRange(g.AllCards);
            r.Vm.Query = "car";
            var run = r.Vm.RunAsync();
            r.WaitCalls(3);
            r.Modland.Reply(L("modland", "bound", "Some Other Title v1"), L("modland", "n", "Something Else Entirely"));
            r.Gaming.Reply(L("modsgaming", "x", "Dacia Sandero Stepway v2.0"));
            r.St.Reply();
            await run;
            Assert.Eq("Установленные 2", r.Vm.Filters[2].Label);
            r.Vm.Filter = SearchFilter.Installed;
            Assert.Eq(2, r.Vm.Cards.Count);
            Assert.True(r.Vm.Cards.All(c => c.Installed), "плашка");
            r.Installed.Clear();
            r.Vm.InstalledChanged();
            Assert.Eq("Ничего из этого не установлено", r.Vm.Empty!.Title);
        });

        t.Run("«Возможно, тот же мод»: «Это один мод» склеивает и запоминается, «Это разные» убирает плашку", async () =>
        {
            var r = new Rig();
            r.Vm.Query = "sienna";
            var run = r.Vm.RunAsync();
            r.WaitCalls(3);
            r.Modland.Reply(L("modland", "a", "Toyota Sienna 2011-2017 2.2") with { SizeBytes = 61 * MB });
            r.Gaming.Reply(L("modsgaming", "b", "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE"));
            r.St.Reply();
            await run;
            Assert.Eq(2, r.Vm.Cards.Count);
            var maybe = r.Vm.Cards.First(c => c.MaybeText is not null);
            Assert.True(maybe.MaybeText is "Возможно, тот же мод, что на ModsGaming" or "Возможно, тот же мод, что на Modland", maybe.MaybeText);
            r.Vm.SameMod(maybe);
            Assert.Eq(1, r.Vm.Cards.Count);
            Assert.Eq("Modland · ModsGaming", r.Vm.Cards[0].SitesText);
            Assert.Eq(1, r.Decisions.All().Count);

            // тот же поиск ещё раз: решение помнится, карточка сразу одна
            foreach (var s in new[] { r.Modland, r.Gaming, r.St }) s.Reset();
            run = r.Vm.RunAsync();
            r.WaitCalls(6);
            r.Modland.Reply(L("modland", "a", "Toyota Sienna 2011-2017 2.2") with { SizeBytes = 61 * MB });
            r.Gaming.Reply(L("modsgaming", "b", "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE"));
            r.St.Reply();
            await run;
            Assert.Eq(1, r.Vm.Cards.Count);

            var r2 = new Rig();
            r2.Vm.Query = "sienna";
            run = r2.Vm.RunAsync();
            r2.WaitCalls(3);
            r2.Modland.Reply(L("modland", "a", "Toyota Sienna 2011-2017 2.2") with { SizeBytes = 61 * MB });
            r2.Gaming.Reply(L("modsgaming", "b", "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE"));
            r2.St.Reply();
            await run;
            r2.Vm.DifferentMods(r2.Vm.Cards.First(c => c.MaybeText is not null));
            Assert.Eq(2, r2.Vm.Cards.Count);
            Assert.True(r2.Vm.Cards.All(c => c.MaybeText is null), "плашки нет");
        });

        t.Run("картинка из выдачи Modland: только https с сайта", () =>
        {
            var html = File.ReadAllText(GameDbTests.Fixture("../modland/search-real.html"));
            var list = BeamModManager.Core.Sources.Modland.ModlandParser.ParseList(html);
            Assert.True(list.Count > 0 && list.All(l => l.PreviewUrl is { Scheme: "https" } u && u.Host.EndsWith("modland.net")), "у всех картинка с сайта");
        });

        t.Run("поток ответов в ядре: промежуточный ответ на каждый сайт, кроме последнего; итог как без потока", async () =>
        {
            var a = new Site("modland"); var b = new Site("modsgaming");
            var svc = new SearchService(new IModSource[] { a, b });
            var partials = new List<SearchResponse>();
            var task = svc.SearchAsync("bmw", p => { lock (partials) partials.Add(p); });
            var until = DateTime.UtcNow.AddSeconds(5);
            while (a.Calls + b.Calls < 2 && DateTime.UtcNow < until) Thread.Sleep(5);
            b.Reply(L("modsgaming", "1", "BMW M3 E30 v1.0"));
            until = DateTime.UtcNow.AddSeconds(5);
            while (DateTime.UtcNow < until) { lock (partials) if (partials.Count > 0) break; Thread.Sleep(5); }
            a.Reply(L("modland", "2", "Audi A4 B5"));
            var final = await task;
            Assert.Eq(1, partials.Count);
            Assert.True(partials[0].Sources[0].Pending && !partials[0].Sources[1].Pending, "modland ещё ищет");
            Assert.Eq(1, partials[0].Cards.Count);
            Assert.Eq(2, final.Cards.Count);
            Assert.True(final.Sources.All(s => !s.Pending), "в итоге все ответили");
        });
    }

    private sealed class OneMod : IGarageBackend
    {
        public GarageSnapshot Load()
        {
            var bound = new InstalledMod("x", "X Car", "x.zip", "1.0", 1, DateTimeOffset.UtcNow, true);
            var named = new InstalledMod("d", "Dacia Sandero Stepway", "dacia_sandero_stepway_v1.0.zip", "1.0", 1, DateTimeOffset.UtcNow, true);
            return new(new[]
            {
                new GarageMod(bound, null, new ModBinding("x", "modland", "bound", new Uri("https://modland.test/bound"), true), null, false, false, null, null, null, null),
                new GarageMod(named, null, null, null, false, false, null, null, null, null),
            }, true, null);
        }
        public bool IsGameRunning() => false;
        public ToggleResult SetEnabled(string fileName, bool enabled) => new(true, ToggleWay.GameDb, "");
    }
}
