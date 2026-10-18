using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Net;
using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Versioning;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Settings;
using BeamModManager.Ui.Updates;

namespace BeamModManager.Core.Tests;

/// <summary>Раздел «Обновления»: список, действия, панель «Проверка», окно «Указать страницу» и настоящая связка с библиотекой.</summary>
public static class UpdatesScreenTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 16, 12, 0, 0, TimeSpan.Zero);

    private sealed class Garage : IGarageBackend
    {
        public List<GarageMod> Mods = new();
        public bool Folder = true;
        public int Loads;
        public GarageSnapshot Load() { Loads++; return new(Mods.ToList(), Folder, null); }
        public bool IsGameRunning() => false;
        public ToggleResult SetEnabled(string fileName, bool enabled) => new(true, ToggleWay.GameDb, "");
        public void Edit(string id, Func<GarageMod, GarageMod> f) { var i = Mods.FindIndex(m => m.Mod.Id == id); Mods[i] = f(Mods[i]); }
    }

    private sealed class Fake : IUpdatesBackend
    {
        private readonly Garage _g;
        public Fake(Garage g) { _g = g; }
        public List<string> Calls = new();
        public Func<bool, IProgress<UpdateProgress>?, Task<UpdateRunReport>>? OnCheck;
        public UpdateRunReport? Report;
        public DateTimeOffset? Last;
        public (int, int) Cov = (0, 0);
        public List<SiteInfo> SiteList = new() { new("modland", "Modland", true, SourceStatus.Ok, 2, new Uri("https://www.modland.net/")) };
        public Func<bool, Task<IReadOnlyList<BindingSuggestion>>>? Suggest;
        public Func<string, Task<PageCheck>>? Page;
        public List<(string Mod, Listing L, string? Replace, bool Verified)> Bound = new();

        public bool IsRunning { get; set; }
        public UpdateRunReport? LastReport => Report;
        public async Task<UpdateRunReport> CheckAsync(bool force, IProgress<UpdateProgress>? progress = null)
        {
            Calls.Add(force ? "check!" : "check");
            Report = OnCheck is null ? Empty() : await OnCheck(force, progress);
            return Report;
        }
        public void Cancel() => Calls.Add("cancel");
        public IReadOnlyList<SiteInfo> Sites() => SiteList;
        public DateTimeOffset? LastCheckedUtc() => Last;
        public (int Checked, int Total) Coverage(DateTimeOffset since) => Cov;
        public bool Dismiss(string modId)
        {
            Calls.Add("dismiss " + modId);
            _g.Edit(modId, m => m with { Update = m.Update! with { IsDismissed = true } });
            return true;
        }
        public void Restore(string modId)
        {
            Calls.Add("restore " + modId);
            _g.Edit(modId, m => m with { Update = m.Update! with { IsDismissed = false } });
        }
        public DeclineUndo? Decline(string modId)
        {
            Calls.Add("decline " + modId);
            var before = _g.Mods.First(m => m.Mod.Id == modId);
            _g.Edit(modId, m => m with { Binding = null, PageMissing = false, CheckingDeclined = true });
            return new DeclineUndo(modId, new[] { before.Binding! }, Array.Empty<CheckState>(), null);
        }
        public bool UndoDecline(DeclineUndo undo)
        {
            Calls.Add("undo " + undo.ModId);
            _g.Edit(undo.ModId, m => m with { Binding = undo.Bindings[0], PageMissing = true, CheckingDeclined = false });
            return true;
        }
        public void ResolveHumanCheck(string source) => Calls.Add("human " + source);
        public Task<IReadOnlyList<BindingSuggestion>> SuggestAsync(string modId, bool replacing, CancellationToken ct = default) =>
            Suggest?.Invoke(replacing) ?? Task.FromResult<IReadOnlyList<BindingSuggestion>>(Array.Empty<BindingSuggestion>());
        public Task<PageCheck> CheckPageAsync(string text, CancellationToken ct = default) =>
            Page?.Invoke(text) ?? Task.FromResult(new PageCheck(null, "нет"));
        public bool Bind(string modId, Listing listing, string? replaceSource, bool verified)
        {
            Bound.Add((modId, listing, replaceSource, verified));
            return true;
        }
    }

    private static UpdateRunReport Empty(int requests = 1, int fresh = 0) => new(Array.Empty<UpdateFinding>(), Array.Empty<SourceRunReport>(), requests, 0) { FreshSkipped = fresh };

    private static GarageMod Mod(string name, string ver = "1.0", string? latest = null, UpdateStatus st = UpdateStatus.Likely,
        bool dismissed = false, bool missing = false, bool tracked = true, long? siteSize = null)
    {
        var id = name.ToLowerInvariant();
        var m = new InstalledMod(id, name, id + ".zip", ver, 300L * 1024 * 1024, T0.AddDays(-3), true);
        var b = tracked ? new ModBinding(id, "modland", "1", new Uri("https://www.modland.net/" + id + ".html"), true) : null;
        var u = latest is null && st != UpdateStatus.Possible ? null
            : new UpdateFinding(id, name, "modland", st, ver, latest, b!.Url) { IsDismissed = dismissed };
        return new GarageMod(m, u, b, tracked ? T0.AddHours(-1) : null, missing, false, null, null, "vehicle", null)
        {
            SiteSizeBytes = siteSize,
            SiteChangedUtc = latest is null ? null : T0.AddDays(-2),
        };
    }

    private static (UpdatesViewModel Vm, GarageViewModel G, Garage Gb, Fake B, SettingsViewModel S) Make(params GarageMod[] mods)
    {
        var gb = new Garage { Mods = mods.ToList() };
        var g = new GarageViewModel(gb, () => T0);
        var b = new Fake(gb);
        var s = new SettingsViewModel(new AppSettings());
        var vm = new UpdatesViewModel(g, b, s, null, () => T0);
        g.Refresh();
        return (vm, g, gb, b, s);
    }

    private static GarageMod[] Board() => new[]
    {
        Mod("Dacia", "1.0", "1.5", siteSize: 310L * 1024 * 1024),
        Mod("Rally", "2.0", "2.0", UpdateStatus.Possible, siteSize: 330L * 1024 * 1024),
        Mod("Civic", "1.6", "1.7", dismissed: true),
        Mod("Mustang", "1.2", missing: true),
        Mod("Traffic", "3.1", tracked: false),
        Mod("Aero", "1.0", "1.1"),
    };

    public static void Register(TestRunner t)
    {
        t.Group("Обновления: список и действия");

        t.Run("три списка из карточек гаража: доступные (сначала точные), скрытые, пропавшие; числа в фильтрах", () =>
        {
            var (vm, _, _, _, _) = Make(Board());
            Assert.Eq("Доступные 3", vm.Filters[0].Label);
            Assert.Eq("Скрытые 1", vm.Filters[1].Label);
            Assert.Eq("Страница пропала 1", vm.Filters[2].Label);
            Assert.Eq("3 доступно", vm.CountText);
            Assert.Eq("3", vm.HeaderCount, "число для капсулы заголовка");
            Assert.Eq("", vm.SearchNote);
            Assert.Eq("Aero,Dacia,Rally", string.Join(",", vm.Items.Select(i => i.Card.Name)));
            vm.SearchText = "dac";
            Assert.Eq("найдено 1", vm.SearchNote);
            vm.SearchText = "нет такого";
            Assert.Eq("ничего не нашли", vm.SearchNote);
            vm.SearchText = "";
            vm.Filter = UpdatesFilter.Hidden;
            Assert.Eq("Civic", vm.Items.Single().Card.Name);
            vm.Filter = UpdatesFilter.Missing;
            Assert.Eq("Mustang", vm.Items.Single().Card.Name);
            Assert.True(vm.Filters[2].IsActive && !vm.Filters[0].IsActive, "активен один фильтр");
        });

        t.Run("карточка обновления как на доске: «1.0 → 1.5», сайт, когда заметили, размер; без загрузки главная — «Открыть страницу»", () =>
        {
            var (vm, g, _, _, _) = Make(Board());
            var d = vm.Items.First(i => i.Card.Name == "Dacia");
            Assert.Eq(UpdateKind.Update, d.Kind);
            Assert.Eq("1.0", d.OldVersion);
            Assert.Eq("1.5", d.NewVersion);
            Assert.Eq("Версия на сайте выше установленной.", d.Line1);
            Assert.Eq("Modland, новую версию заметили 2 дня назад. Размер 300 МБ → 310 МБ.", d.Line2);
            Assert.Eq(ItemAction.OpenPage, d.Primary);
            Assert.Eq("Пропустить эту версию", d.SecondLabel);
            Assert.True(d.ThirdLabel is null, "третьей кнопки нет");
            Assert.Eq(new Uri("https://www.modland.net/dacia.html"), d.PageUrl);
            g.DownloadsAllowed = true;
            vm.Rebuild();
            Assert.Eq("Обновить", d.PrimaryLabel);
            Assert.Eq(ItemAction.OpenPage, d.Second);
            Assert.Eq(ItemAction.Dismiss, d.Third);
        });

        t.Run("«Возможно, обновлён» и «Страница пропала»: плашка, пояснение словами, свои кнопки; пропавшая — серая", () =>
        {
            var (vm, _, _, _, _) = Make(Board());
            var r = vm.Items.First(i => i.Card.Name == "Rally");
            Assert.Eq("Возможно, обновлён", r.Chip);
            Assert.False(r.HasVersions, "строки версий нет");
            Assert.Eq("Версия та же, но размер на сайте изменился: 300 МБ → 330 МБ.", r.Line1);
            Assert.Eq("Версия не изменилась", r.SecondLabel);
            vm.Filter = UpdatesFilter.Missing;
            var m = vm.Items.Single();
            Assert.Eq("Страница пропала", m.Chip);
            Assert.Eq("Найти замену", m.PrimaryLabel);
            Assert.Eq("Не проверять этот мод", m.SecondLabel);
            Assert.True(m.Gray, "фото серое");
            vm.Filter = UpdatesFilter.Hidden;
            var h = vm.Items.Single();
            Assert.Eq("Вернуть в список", h.PrimaryLabel);
            Assert.True(h.Line1!.Contains("1.7"), "какую версию скрыли");
        });

        t.Run("поиск по названию внутри списка; пусто — «Нет такого мода»", () =>
        {
            var (vm, _, _, _, _) = Make(Board());
            vm.SearchText = "dac";
            Assert.Eq("Dacia", vm.Items.Single().Card.Name);
            vm.SearchText = "zzz";
            Assert.Eq(0, vm.Items.Count);
            Assert.Eq("Нет такого мода", vm.Empty!.Title);
            vm.SearchText = "";
            Assert.Eq(3, vm.Items.Count);
            Assert.True(vm.Empty is null, "пустого состояния нет");
        });

        t.Run("«Пропустить эту версию»: уходит в «Скрытые», тост с версией и «Отменить», отмена возвращает", () =>
        {
            var (vm, g, _, b, _) = Make(Board());
            var d = vm.Items.First(i => i.Card.Name == "Dacia");
            vm.Act(d, ItemAction.Dismiss);
            Assert.True(b.Calls.Contains("dismiss dacia"), "скрыто в библиотеке");
            Assert.False(vm.Items.Any(i => i.Card.Name == "Dacia"), "из доступных ушло");
            Assert.Eq("Скрытые 2", vm.Filters[1].Label);
            Assert.Eq("Версия 1.5 скрыта. Другая версия снова покажет обновление", g.Toast!.Text);
            Assert.True(g.Toast.CanUndo, "с «Отменить»");
            g.Undo();
            Assert.True(b.Calls.Contains("restore dacia"), "вернули");
            Assert.True(vm.Items.Any(i => i.Card.Name == "Dacia"), "снова в списке");
            Assert.Eq(3, g.Filters[2].Count, "и бейдж гаража снова считает его");
        });

        t.Run("«Версия не изменилась» у сомнительного и «Вернуть в список» у скрытого", () =>
        {
            var (vm, g, _, b, _) = Make(Board());
            vm.Act(vm.Items.First(i => i.Card.Name == "Rally"), ItemAction.Dismiss);
            Assert.Eq("Отметили: у «Rally» версия не изменилась", g.Toast!.Text);
            vm.Filter = UpdatesFilter.Hidden;
            vm.Act(vm.Items.First(i => i.Card.Name == "Civic"), ItemAction.Restore);
            Assert.True(b.Calls.Contains("restore civic"), "вернули");
            Assert.False(g.Toast!.CanUndo, "без «Отменить»");
            vm.Filter = UpdatesFilter.Available;
            Assert.True(vm.Items.Any(i => i.Card.Name == "Civic"), "Civic снова доступен");
        });

        t.Run("«Не проверять этот мод»: мод уходит из «Страница пропала» в «Не проверяются» гаража, «Отменить» возвращает", () =>
        {
            var (vm, g, _, b, _) = Make(Board());
            vm.Filter = UpdatesFilter.Missing;
            vm.Act(vm.Items.Single(), ItemAction.Decline);
            Assert.Eq(0, vm.Items.Count);
            Assert.Eq("Все страницы на месте", vm.Empty!.Title);
            Assert.Eq(2, g.Filters[3].Count);
            Assert.True(g.Toast!.Text.StartsWith("Не проверяем «Mustang»"), g.Toast.Text);
            g.Undo();
            Assert.True(b.Calls.Contains("undo mustang"), "отменено");
            Assert.Eq("Mustang", vm.Items.Single().Card.Name);
        });

        t.Run("пустые состояния: не проверяли, всё свежее, ни за чем не следим, нет папки", () =>
        {
            var (vm, _, gb, b, _) = Make(Mod("A", "1.0"));
            Assert.Eq("Ещё не проверяли", vm.Empty!.Title);
            Assert.Eq(EmptyAction.CheckNow, vm.Empty.Action);
            b.Last = T0.AddMinutes(-18);
            vm.Rebuild();
            Assert.Eq("Всё свежее", vm.Empty!.Title);
            Assert.Eq("Проверено 18 минут назад", vm.Empty.Text);
            var (vm2, _, _, _, _) = Make(Mod("A", tracked: false));
            Assert.Eq("Пока ни за одним модом не следим", vm2.Empty!.Title);
            Assert.Eq(EmptyAction.ShowAll, vm2.Empty.Action);
            var (vm3, g3, gb3, _, _) = Make();
            gb3.Folder = false;
            g3.Refresh();
            Assert.Eq(EmptyAction.PickFolder, vm3.Empty!.Action);
        });

        t.Group("Обновления: панель «Проверка»");

        t.Run("проверка: «Остановить» во время, ход «3 из 10», итог словами; гараж перечитан", async () =>
        {
            var (vm, _, gb, b, _) = Make(Board());
            var gate = new TaskCompletionSource<UpdateRunReport>();
            IProgress<UpdateProgress>? prog = null;
            b.OnCheck = (_, p) => { prog = p; return gate.Task; };
            var loads = gb.Loads;
            var run = vm.CheckNowAsync();
            Assert.True(vm.Checking, "идёт");
            Assert.Eq("Остановить", vm.CheckButton);
            Assert.True(vm.Empty is null || vm.Empty.ActionLabel is null, "кнопки «Проверить сейчас» в пустом состоянии нет");
            prog!.Report(new UpdateProgress("modland", "лента", 1, 3));
            Assert.Eq("читаем ленты сайтов", vm.ProgressText);
            prog.Report(new UpdateProgress("modland", "карточки", 3, 10));
            Assert.Eq("3 из 10", vm.ProgressText);
            Assert.Eq(0.3, Math.Round(vm.ProgressValue, 2));
            await vm.CheckNowAsync(); // второе нажатие — «Остановить»
            Assert.True(b.Calls.Contains("cancel"), "отмена передана");
            var f = new UpdateFinding("dacia", "Dacia", "modland", UpdateStatus.Likely, "1.0", "1.5", new Uri("https://x.org/"));
            gate.SetResult(new UpdateRunReport(new[] { f }, new[] { new SourceRunReport("modland", SourceStatus.Ok, 1, 3, 0, Array.Empty<string>(), null) }, 4, 2)
            { NewFindings = new[] { f } });
            await run;
            Assert.False(vm.Checking, "закончилась");
            Assert.Eq("Нашли 1 новую версию.", vm.CheckText);
            Assert.Eq("Проверить сейчас", vm.CheckButton);
            Assert.True(vm.ProgressText is null, "хода больше нет");
            Assert.True(gb.Loads > loads, "гараж перечитан");
            Assert.Eq("готово", vm.Sites.Single().State);
        });

        t.Run("всё проверено недавно: «Проверено … Новое смотрим не чаще раза в 6 часов», следующая кнопка — принудительно", async () =>
        {
            var (vm, _, _, b, _) = Make(Board());
            b.Last = T0.AddMinutes(-6);
            b.OnCheck = (_, _) => Task.FromResult(Empty(requests: 0, fresh: 5));
            await vm.CheckNowAsync();
            Assert.Eq("Проверено 6 минут назад. Новое смотрим не чаще раза в 6 часов.", vm.CheckText);
            Assert.Eq("Всё равно проверить", vm.CheckButton);
            b.OnCheck = (_, _) => Task.FromResult(Empty());
            await vm.CheckNowAsync();
            Assert.Eq("check,check!", string.Join(",", b.Calls.Where(c => c.StartsWith("check"))));
            Assert.Eq("Новых версий нет.", vm.CheckText);
            Assert.Eq("Проверить сейчас", vm.CheckButton);
        });

        t.Run("неполная проверка: сколько из скольких и кто не ответил; остановленная; сбой — строкой", async () =>
        {
            var (vm, _, _, b, _) = Make(Board());
            b.Cov = (3, 5);
            b.OnCheck = (_, _) => Task.FromResult(new UpdateRunReport(Array.Empty<UpdateFinding>(),
                new[] { new SourceRunReport("modland", SourceStatus.Unavailable, 0, 0, 0, Array.Empty<string>(), "код 503") }, 1, 0));
            await vm.CheckNowAsync();
            Assert.Eq("Проверили 3 из 5. Остальные при следующей проверке. Не ответили: Modland.", vm.CheckText);
            Assert.Eq("не отвечает", vm.Sites.Single().State);
            Assert.Eq("код 503", vm.Sites.Single().Note);
            b.OnCheck = (_, _) => Task.FromResult(Empty() with { Cancelled = true });
            await vm.CheckNowAsync();
            Assert.Eq("Остановлено. Найденное сохранено.", vm.CheckText);
            b.OnCheck = (_, _) => throw new InvalidOperationException("нет диска");
            await vm.CheckNowAsync();
            Assert.Eq("Проверка не удалась: нет диска", vm.CheckText);
            Assert.False(vm.Checking, "окно живо, проверка снята");
        });

        t.Run("сайт ответил, но страницы не понял: итог прямо говорит «не смогли прочитать», у сайта — «возможно, сайт изменился»", async () =>
        {
            var (vm, _, _, b, _) = Make(Board());
            b.Cov = (4, 5);
            b.OnCheck = (_, _) => Task.FromResult(new UpdateRunReport(Array.Empty<UpdateFinding>(),
                new[] { new SourceRunReport("modland", SourceStatus.Ok, 1, 3, 0, Array.Empty<string>(), "не прочитали 1 из 3 страниц: возможно, изменилась вёрстка сайта")
                    { LayoutChanged = true, UnreadableMods = new[] { "Dacia" } } }, 4, 2));
            await vm.CheckNowAsync();
            Assert.Eq("Проверили 4 из 5. Остальные при следующей проверке. Не смогли прочитать страницы: Modland. Возможно, сайт изменился.", vm.CheckText);
            Assert.Eq("возможно, сайт изменился", vm.Sites.Single().State);
            Assert.True(vm.Sites.Single().Note!.Contains("1 из 3"), vm.Sites.Single().Note ?? "");
            // обычная неполная проверка без признаков вёрстки — «не всё успели», без слов про изменённый сайт
            b.OnCheck = (_, _) => Task.FromResult(new UpdateRunReport(Array.Empty<UpdateFinding>(),
                new[] { new SourceRunReport("modland", SourceStatus.Ok, 1, 3, 2, Array.Empty<string>(), "не хватило времени, продолжится при следующей проверке") }, 4, 2));
            await vm.CheckNowAsync();
            Assert.Eq("не всё успели", vm.Sites.Single().State);
            Assert.False(vm.CheckText!.Contains("изменился"), vm.CheckText);
        });

        t.Run("сайты: выключен, нет модов, ещё не проверяли; «вы человек?» — «Открыть сайт», потом «Готово»", () =>
        {
            var (vm, _, _, b, _) = Make(Board());
            b.SiteList = new()
            {
                new("modland", "Modland", true, SourceStatus.NeedsHumanCheck, 2, new Uri("https://www.modland.net/")),
                new("stmods", "STMods", false, SourceStatus.Ok, 1, null),
                new("modsgaming", "ModsGaming", true, SourceStatus.Ok, 0, null),
                new("beamng", "Репозиторий BeamNG", true, SourceStatus.Ok, 4, null),
            };
            vm.Rebuild();
            Assert.Eq("не пускает программу|выключен|нет модов|4 мода", string.Join("|", vm.Sites.Select(s => s.State)));
            var ml = vm.Sites[0];
            Assert.True(ml.NeedsHuman && ml.Note!.Contains("Проверить снова"), "кнопка «Проверить снова» и честное объяснение, без «откройте в браузере»");
            b.SiteList[0] = b.SiteList[0] with { Status = SourceStatus.Ok };
            vm.SiteConfirmed(ml);
            Assert.True(b.Calls.Contains("human modland"), "пауза снята");
            Assert.Eq("проверим в следующий раз", ml.State);
            Assert.False(ml.NeedsHuman, "кнопок больше нет");
        });

        t.Run("автопроверка: при запуске по настройке; повтор раз в N часов только когда пора", async () =>
        {
            var (vm, _, _, b, s) = Make(Board());
            s.AutoCheckOnStart = false;
            await vm.StartupAsync();
            Assert.Eq(0, b.Calls.Count(c => c.StartsWith("check")));
            s.AutoCheckOnStart = true;
            await vm.StartupAsync();
            Assert.Eq(1, b.Calls.Count(c => c.StartsWith("check")));
            Assert.Eq("Проверяем сами при запуске", vm.AutoNote);
            await vm.TickAsync();
            Assert.Eq(1, b.Calls.Count(c => c.StartsWith("check")), "повтор выключен");
            s.AutoCheckEveryHours = 3;
            Assert.Eq("Проверяем сами при запуске и раз в 3 часа", vm.AutoNote);
            await vm.TickAsync();
            Assert.Eq(1, b.Calls.Count(c => c.StartsWith("check")), "только что проверяли");
            var (vm2, _, _, b2, s2) = Make(Board());
            s2.AutoCheckEveryHours = 3;
            b2.Last = T0.AddHours(-4);
            await vm2.TickAsync();
            Assert.Eq("check", b2.Calls.Single());
        });

        t.Group("Обновления: окно «Указать страницу»");

        static BindingSuggestion Sug(string title, string src = "stmods") =>
            new(new Listing(src, new Uri($"https://stmods.org/{title.Replace(' ', '_')}/"), title, title) { ModVersion = "2.2", SizeBytes = 88L * 1024 * 1024 },
                new BeamModManager.Core.Matching.MatchResult(0.8, BeamModManager.Core.Matching.MatchVerdict.Maybe, "похоже"));

        t.Run("подсказки: до трёх, с сайтом и версией; не нашли — подсказка вставить ссылку; сбой поиска не роняет окно", async () =>
        {
            var (_, g, _, b, _) = Make(Board());
            b.Suggest = _ => Task.FromResult<IReadOnlyList<BindingSuggestion>>(new[] { Sug("Sienna 1"), Sug("Sienna 2"), Sug("Sienna 3"), Sug("Sienna 4") });
            var p = new PageBindViewModel(b, g.AllCards[0], null);
            await p.LoadAsync();
            Assert.Eq(3, p.Suggestions.Count);
            Assert.Eq("STMods", p.Suggestions[0].Site);
            Assert.Eq("версия 2.2 · 88 МБ", p.Suggestions[0].Details);
            Assert.True(p.SearchNote is null && !p.Searching, "без подписи");
            b.Suggest = _ => Task.FromResult<IReadOnlyList<BindingSuggestion>>(Array.Empty<BindingSuggestion>());
            await p.LoadAsync();
            Assert.Eq("Похожих не нашли. Вставьте ссылку на страницу мода сами", p.SearchNote);
            b.Suggest = _ => throw new HttpRequestException("сеть");
            await p.LoadAsync();
            Assert.Eq("Поиск не удался. Можно вставить ссылку на страницу сами", p.SearchNote);
        });

        t.Run("ссылка: проверяется, устаревший ответ не применяется; подсказка и ссылка взаимоисключающие", async () =>
        {
            var (_, g, _, b, _) = Make(Board());
            b.Suggest = _ => Task.FromResult<IReadOnlyList<BindingSuggestion>>(new[] { Sug("Sienna") });
            var page = new Listing("modland", new Uri("https://www.modland.net/x.html"), "x", "Toyota Sienna") { ModVersion = "2.2" };
            var slow = new TaskCompletionSource<PageCheck>();
            b.Page = text => text.Contains("slow") ? slow.Task : Task.FromResult(new PageCheck(page, null));
            var p = new PageBindViewModel(b, g.AllCards[0], null);
            await p.LoadAsync();
            Assert.False(p.CanApply, "ничего не выбрано");
            p.Select(p.Suggestions[0]);
            Assert.True(p.CanApply, "выбрана подсказка");
            p.LinkText = "slow.example";
            Assert.False(p.Suggestions[0].IsSelected, "ввод ссылки снял выбор подсказки");
            Assert.False(p.CanApply, "ссылка ещё не проверена");
            var pending = p.CheckLinkAsync();
            Assert.Eq("Проверяем ссылку…", p.LinkNote);
            p.LinkText = "modland.net/x.html";
            await p.CheckLinkAsync();
            Assert.Eq("Modland · Toyota Sienna · версия 2.2", p.LinkNote);
            slow.SetResult(new PageCheck(null, "старый ответ"));
            await pending;
            Assert.Eq("Modland · Toyota Sienna · версия 2.2", p.LinkNote);
            Assert.True(p.CanApply && p.LinkOk, "ссылка подошла");
            Assert.Eq("Следим на Modland", await p.ApplyAsync());
            Assert.True(b.Bound.Single() is { Verified: true, Replace: null }, "проверенная запись");
            p.Select(p.Suggestions[0]);
            Assert.Eq("", p.LinkText);
            Assert.Eq("Следим на STMods", await p.ApplyAsync());
            Assert.False(b.Bound[1].Verified, "страница подсказки не открылась (вернулась чужая) — следим без версии");
            var full = new Listing("stmods", p.Suggestions[0].Listing.Url, "Sienna", "Sienna") { ModVersion = "2.3", SizeBytes = 90L * 1024 * 1024 };
            b.Page = _ => Task.FromResult(new PageCheck(full, null));
            p.Select(p.Suggestions[0]);
            await p.ApplyAsync();
            Assert.True(b.Bound[2] is { Verified: true } x && x.L.ModVersion == "2.3", "подсказку открыли: версия известна сразу");
        });

        t.Run("«Найти замену»: заголовок как в дизайне, пропавшая страница заменяется", async () =>
        {
            var (_, g, _, b, _) = Make(Board());
            var replacing = false;
            b.Suggest = r => { replacing = r; return Task.FromResult<IReadOnlyList<BindingSuggestion>>(new[] { Sug("Shelby") }); };
            var p = new PageBindViewModel(b, g.AllCards.First(c => c.Name == "Mustang"), "modland");
            Assert.Eq("Страница пропала. Вот похожие моды", p.Title);
            Assert.Eq("Следить здесь", p.ApplyLabel);
            await p.LoadAsync();
            Assert.True(replacing, "поиск без пропавшей страницы");
            p.Select(p.Suggestions[0]);
            await p.ApplyAsync();
            Assert.Eq("modland", b.Bound.Single().Replace);
        });

        t.Group("Обновления: библиотека и сайты по-настоящему");

        t.Run("ссылка: не ссылка, чужой сайт, выключенный сайт, нет страницы, настоящая карточка (и без https://)", async () =>
        {
            var (be, _, _) = Real();
            Assert.Eq("Это не похоже на ссылку", (await be.CheckPageAsync("просто текст")).Problem);
            Assert.True((await be.CheckPageAsync("https://example.com/mod")).Problem!.StartsWith("Этот сайт программа не знает"), "чужой сайт");
            Assert.Eq("Такой страницы на сайте нет. Проверьте ссылку", (await be.CheckPageAsync("https://stmods.org/beamng_drive/mods/нет/")).Problem);
            var ok = await be.CheckPageAsync("stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng/");
            Assert.True(ok.Ok, ok.Problem ?? "");
            Assert.Eq("3.3", ok.Listing!.ModVersion);
            var (be2, _, _) = Real(new AppSettings { DisabledSources = new[] { "stmods" } });
            Assert.Eq("STMods выключен в настройках. Включите его в «Настройки → Сайты»",
                (await be2.CheckPageAsync("https://stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng/")).Problem);
        });

        t.Run("привязка проверенной страницы: следим, отказ «Не проверять» снят, обновление видно сразу", async () =>
        {
            var (be, store, _) = Real();
            store.UpsertMod(new InstalledMod("bmw", "BMW E34", "bmw_e34.zip", "3.0", null, T0, true));
            BindingActions.Decline(store, "bmw");
            var page = (await be.CheckPageAsync("https://stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng/")).Listing!;
            Assert.True(be.Bind("bmw", page, null, verified: true), "привязано");
            Assert.False(store.IsCheckingDeclined("bmw"), "отказ снят");
            Assert.True(store.Bindings().Single() is { Source: "stmods", Confirmed: true }, "одна подтверждённая");
            var f = UpdateCheckService.Pending(store).Single();
            Assert.Eq("3.3", f.LatestVersion);
            Assert.True(be.Dismiss("bmw"), "скрыта");
            Assert.Eq(0, UpdateCheckService.Pending(store).Count);
            be.Restore("bmw");
            Assert.Eq(1, UpdateCheckService.Pending(store).Count);
            Assert.Eq(1, be.Sites().Single(s => s.Id == "stmods").Tracked);
            Assert.Eq((1, 1), be.Coverage(T0.AddMinutes(-1)));
            Assert.True(be.LastCheckedUtc() is not null, "есть время проверки");
        });

        t.Run("замена пропавшей страницы: прежняя привязка и её промахи уходят", () =>
        {
            var (be, store, _) = Real();
            store.UpsertMod(new InstalledMod("m", "Mustang", "mustang.zip", "1.0", null, T0, true));
            store.UpsertBinding(new ModBinding("m", "modland", "old", new Uri("https://www.modland.net/old.html"), true));
            store.SaveCheck(new CheckState("m", "modland", T0, null, "1.0", null, MissCount: 3));
            var repl = new Listing("stmods", new Uri("https://stmods.org/new/"), "new", "Mustang");
            Assert.True(be.Bind("m", repl, "modland", verified: false), "заменено");
            Assert.True(store.Bindings().Single() is { Source: "stmods", SourceId: "new" }, "осталась новая");
            Assert.True(store.GetCheck("m", "modland") is null, "промахи старой убраны");
            Assert.False(be.Bind("нет-такого", repl, null, false), "мода нет — ничего не делаем");
        });

        t.Run("подсказки ищут по названию из имени файла на включённых сайтах", async () =>
        {
            var (be, store, _) = Real();
            store.UpsertMod(new InstalledMod("s", "Toyota Sienna", "toyota_sienna.zip", null, null, T0, true));
            var found = await be.SuggestAsync("s", replacing: false);
            Assert.Eq(3, found.Count);
            Assert.True(found[0].Listing.Title.StartsWith("2023 Toyota Sienna"), "самое похожее первым: " + found[0].Listing.Title);
            Assert.True(found.All(x => x.Listing.Source == "stmods"), "только включённые и умеющие искать");
        });
    }

    private sealed class Pages : IPageFetcher
    {
        private readonly Dictionary<string, (int, string)> _map;
        public Pages(Dictionary<string, (int, string)> map) { _map = map; }
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct) =>
            Task.FromResult(_map.TryGetValue(url.AbsoluteUri, out var r) ? new RawResponse(r.Item1, r.Item2) : new RawResponse(404, ""));
        public Task<RawResponse> PostFormAsync(Uri url, string form, CancellationToken ct) => GetAsync(url, ct);
    }

    private static string Fx(string path)
    {
        var dir = AppContext.BaseDirectory;
        while (dir != null && !Directory.Exists(Path.Combine(dir, "tests", "fixtures"))) dir = Path.GetDirectoryName(dir);
        return File.ReadAllText(Path.Combine(dir!, "tests", "fixtures", path));
    }

    private static (LibraryUpdatesBackend B, InMemoryLibraryStore Store, PoliteFetcher F) Real(AppSettings? settings = null)
    {
        var pages = new Pages(new()
        {
            ["https://stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng/"] = (200, Fx("stmods/mod-real.html")),
            ["https://stmods.org/search.php?search_request=toyota%20sienna&partition=beamng"] = (200, Fx("stmods/search-real.html")),
        });
        var f = new PoliteFetcher(pages, new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero, MaxRetries = 0 });
        var store = new InMemoryLibraryStore();
        var s = settings ?? new AppSettings { DisabledSources = new[] { "modland", "modsgaming" } };
        var be = new LibraryUpdatesBackend(store, new IModSource[] { new ModlandSource(f), new StModsSource(f), new ModsGamingSource(f), new BeamNgRepoSource(f) },
            () => s, () => T0, f.ClearCache, f.ResetHost, f.StatusOf);
        return (be, store, f);
    }
}
