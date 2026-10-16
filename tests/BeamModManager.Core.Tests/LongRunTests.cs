using System.IO.Compression;
using System.Runtime.CompilerServices;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Previews;
using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Settings;
using BeamModManager.Ui.Updates;

namespace BeamModManager.Core.Tests;

/// <summary>
/// Программа открыта полдня: одни и те же действия сотни раз, проверки по таймеру, окна привязки открываются и
/// закрываются. Ничего не должно копиться: ни память, ни карточки ушедших модов, ни временные файлы.
/// </summary>
public static class LongRunTests
{
    private static readonly byte[] TinyZip = MakeZip();

    private static byte[] MakeZip()
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true)) using (var s = z.CreateEntry("info.json").Open()) s.Write("{}"u8);
        return ms.ToArray();
    }

    private sealed class Folder : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "bmm-long-" + Guid.NewGuid().ToString("N")[..8]);
        public readonly string Mods;
        public readonly JsonLibraryStore Store;
        public readonly ModInstaller Inst;

        public Folder(int n)
        {
            Mods = Path.Combine(Root, "mods");
            Directory.CreateDirectory(Mods);
            for (var i = 0; i < n; i++) File.WriteAllBytes(Path.Combine(Mods, $"mod{i:D4}.zip"), TinyZip);
            var app = Path.Combine(Root, "app");
            Store = new JsonLibraryStore(Path.Combine(app, "library.json"));
            Inst = new ModInstaller(ModLayout.InAppFolder(Mods, app), () => DateTimeOffset.UtcNow, 3);
        }

        public GarageViewModel Garage() =>
            new(new LibraryGarageBackend(Store, Inst, new PreviewCache(Path.Combine(Root, "app", "previews")), () => false), () => DateTimeOffset.UtcNow);

        public void Dispose() { try { Directory.Delete(Root, true); } catch (IOException) { } catch (UnauthorizedAccessException) { } }
    }

    private static long Heap()
    {
        GC.Collect();
        GC.WaitForPendingFinalizers();
        GC.Collect();
        return GC.GetTotalMemory(true);
    }

    /// <summary>Создаёт объект в отдельном методе, чтобы на него не осталось ссылки в кадре теста.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    private static WeakReference Weak(Func<object> make) => new(make());

    private static bool Collected(params WeakReference[] refs)
    {
        for (var i = 0; i < 5 && refs.Any(r => r.IsAlive); i++) Heap();
        return refs.All(r => !r.IsAlive);
    }

    /// <summary>«Обновления» без сайтов: каждая проверка мгновенная, отчёт про один сайт.</summary>
    private sealed class QuietUpdates : IUpdatesBackend
    {
        public int Checks;
        public bool IsRunning => false;
        public UpdateRunReport? LastReport { get; private set; }
        public Task<UpdateRunReport> CheckAsync(bool force, IProgress<UpdateProgress>? progress = null)
        {
            Checks++;
            progress?.Report(new UpdateProgress("modland", "карточки", 1, 2));
            LastReport = new UpdateRunReport(Array.Empty<UpdateFinding>(),
                new[] { new SourceRunReport("modland", SourceStatus.Ok, 1, 1, 0, Array.Empty<string>(), null) }, 2, 1);
            return Task.FromResult(LastReport);
        }
        public void Cancel() { }
        public IReadOnlyList<SiteInfo> Sites() => new[] { new SiteInfo("modland", "Modland", true, SourceStatus.Ok, 3, new Uri("https://www.modland.net/")) };
        public DateTimeOffset? LastCheckedUtc() => null;
        public (int Checked, int Total) Coverage(DateTimeOffset since) => (1, 1);
        public bool Dismiss(string modId) => false;
        public void Restore(string modId) { }
        public DeclineUndo? Decline(string modId) => null;
        public bool UndoDecline(DeclineUndo undo) => false;
        public void ResolveHumanCheck(string source) { }
        public Task<IReadOnlyList<BindingSuggestion>> SuggestAsync(string modId, bool replacing, CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<BindingSuggestion>>(new[] { Suggestion(modId) });

        private static BindingSuggestion Suggestion(string modId)
        {
            var l = new Listing("modland", new Uri("https://www.modland.net/beamng.drive-mods/cars/" + modId + ".html"), modId, modId + " 1.1") { ModVersion = "1.1" };
            return new BindingSuggestion(l, Matching.DuplicateMatcher.Compare(l, l));
        }
        public Task<PageCheck> CheckPageAsync(string text, CancellationToken ct = default) => Task.FromResult(new PageCheck(null, "нет"));
        public bool Bind(string modId, Listing listing, string? replaceSource, bool verified) => true;
    }

    public static void Register(TestRunner t)
    {
        t.Group("Долгая работа: ничего не копится");

        t.Run("1000 кругов «перечитать, переключить, фильтр, поиск»: память не растёт, карточек столько же, сколько модов", () =>
        {
            using var f = new Folder(200);
            var vm = f.Garage();
            void Cycle(int i)
            {
                vm.Refresh();
                if (i % 3 == 0) vm.Toggle(vm.AllCards[i % 200]);
                vm.Filter = (GarageFilter)(i % 4);
                vm.SearchText = i % 2 == 0 ? "mod0" + i % 10 : "";
                vm.Selected = vm.Cards.Count > 0 ? vm.Cards[i % vm.Cards.Count] : null;
            }
            for (var i = 0; i < 100; i++) Cycle(i); // прогрев: кэши, JIT
            var before = Heap();
            for (var i = 0; i < 1000; i++) Cycle(i);
            vm.Filter = GarageFilter.All;
            vm.SearchText = "";
            var after = Heap();
            Assert.Eq(200, vm.AllCards.Count);
            Assert.Eq(200, vm.Cards.Count);
            Assert.True(after - before < 8 * 1024 * 1024, $"куча выросла на {(after - before) / 1024} КБ");
        });

        t.Run("карточки удалённых модов не остаются в памяти", () =>
        {
            using var f = new Folder(50);
            var vm = f.Garage();
            vm.Refresh();
            var refs = CardsOf(vm, "mod004");
            Assert.Eq(10, refs.Length);
            foreach (var file in Directory.GetFiles(f.Mods, "mod004*.zip")) File.Delete(file);
            vm.Refresh();
            Assert.Eq(40, vm.Cards.Count);
            Assert.True(Collected(refs), $"держатся {refs.Count(r => r.IsAlive)} из 10");

            // ссылки берутся в отдельном методе: временный список карточек не должен остаться в кадре теста
            [MethodImpl(MethodImplOptions.NoInlining)]
            static WeakReference[] CardsOf(GarageViewModel v, string prefix) =>
                v.AllCards.Where(c => c.FileName.StartsWith(prefix)).Select(c => new WeakReference(c)).ToArray();
        });

        t.Run("тост за тостом: держится только последний, прежние и их «Отменить» отпускаются", () =>
        {
            using var f = new Folder(5);
            var vm = f.Garage();
            vm.Refresh();
            var olds = Toasts(vm, 200);
            Assert.True(vm.Toast is not null, "последний на месте");
            Assert.True(Collected(olds), $"держатся {olds.Count(r => r.IsAlive)} из {olds.Length}");

            [MethodImpl(MethodImplOptions.NoInlining)]
            static WeakReference[] Toasts(GarageViewModel v, int n)
            {
                var list = new List<WeakReference>();
                for (var i = 0; i < n; i++)
                {
                    v.Toggle(v.AllCards[i % 5]);
                    if (i < n - 1) list.Add(new WeakReference(v.Toast!));
                }
                return list.ToArray();
            }
        });

        t.Run("окно «Указать страницу» 300 раз открыли и закрыли — ни одно не осталось в памяти", async () =>
        {
            using var f = new Folder(3);
            var g = f.Garage();
            g.Refresh();
            var backend = new QuietUpdates();
            var upd = new UpdatesViewModel(g, backend, new SettingsViewModel(new AppSettings()), null, () => DateTimeOffset.UtcNow);
            var refs = new List<WeakReference>();
            for (var i = 0; i < 300; i++)
            {
                var (w, task) = Open(backend, g.AllCards[i % 3]);
                await task;
                refs.Add(w);
            }
            Assert.True(Collected(refs.ToArray()), "окна привязки отпущены");
            Assert.Eq(3, g.AllCards.Count);

            [MethodImpl(MethodImplOptions.NoInlining)]
            static (WeakReference, Task) Open(IUpdatesBackend b, ModCard card)
            {
                var p = new PageBindViewModel(b, card, null);
                var load = p.LoadAsync();
                p.LinkText = "modland.net/x.html";
                if (p.Suggestions.Count > 0) p.Select(p.Suggestions[0]);
                return (new WeakReference(p), load);
            }
        });

        t.Run("500 проверок по таймеру: строк сайтов одна, ход проверки не копится, гараж перечитывается", async () =>
        {
            using var f = new Folder(10);
            var g = f.Garage();
            g.Refresh();
            var refreshed = 0;
            g.Refreshed += () => refreshed++;
            var backend = new QuietUpdates();
            var now = new DateTimeOffset(2026, 10, 16, 8, 0, 0, TimeSpan.Zero);
            var s = new SettingsViewModel(new AppSettings { AutoCheckEveryHours = 1 });
            var upd = new UpdatesViewModel(g, backend, s, null, () => now);
            var before = Heap();
            for (var i = 0; i < 500; i++)
            {
                now = now.AddMinutes(61);
                await upd.TickAsync();
            }
            var after = Heap();
            Assert.Eq(500, backend.Checks);
            Assert.Eq(1, upd.Sites.Count);
            Assert.True(refreshed >= 500, $"перечитываний {refreshed}");
            Assert.True(after - before < 8 * 1024 * 1024, $"куча выросла на {(after - before) / 1024} КБ");
        });

        t.Run("часы прыгнули назад (перевод, ручная настройка): проверки не идут каждую минуту; вперёд — одна", async () =>
        {
            using var f = new Folder(3);
            var g = f.Garage();
            g.Refresh();
            var backend = new QuietUpdates();
            var now = new DateTimeOffset(2026, 10, 16, 8, 0, 0, TimeSpan.Zero);
            var upd = new UpdatesViewModel(g, backend, new SettingsViewModel(new AppSettings { AutoCheckEveryHours = 6 }), null, () => now);
            await upd.TickAsync(); // первая
            Assert.Eq(1, backend.Checks);
            now = now.AddHours(-5); // часы отвели назад
            for (var i = 0; i < 60; i++) { now = now.AddMinutes(1); await upd.TickAsync(); }
            Assert.True(backend.Checks <= 2, $"проверок {backend.Checks}");
            var c = backend.Checks;
            now = now.AddDays(30); // компьютер спал месяц
            for (var i = 0; i < 10; i++) { now = now.AddMinutes(1); await upd.TickAsync(); }
            Assert.Eq(c + 1, backend.Checks);
        });

        t.Run("мод обновляли годами: прежних имён файла помнится не больше 20, и после сохранения тоже", () =>
        {
            using var f = new Folder(0);
            var path = Path.Combine(f.Root, "lib.json");
            var store = new JsonLibraryStore(path);
            store.UpsertMod(new InstalledMod("m", "Мод", "mod_v0.zip", "0", 1, DateTimeOffset.UtcNow));
            for (var i = 1; i <= 300; i++)
            {
                var m = store.GetMod("m")!;
                store.UpsertMod(m with { FileName = $"mod_v{i}.zip" });
                store.AddFileHistory("m", m.FileName);
            }
            var h = store.FileHistory("m");
            Assert.Eq(InMemoryLibraryStore.MaxFileHistory, h.Count);
            Assert.Eq("mod_v299.zip", h[^1]);
            store.Flush();
            Assert.Eq(InMemoryLibraryStore.MaxFileHistory, new JsonLibraryStore(path).FileHistory("m").Count);
        });

        t.Run("тысячи сохранений библиотеки: временные файлы не копятся, файл читается", () =>
        {
            using var f = new Folder(0);
            var path = Path.Combine(f.Root, "lib", "library.json");
            var store = new JsonLibraryStore(path);
            for (var i = 0; i < 2000; i++)
            {
                store.UpsertMod(new InstalledMod("m" + i % 50, "Мод " + i, $"m{i % 50}.zip", "1." + i, i, DateTimeOffset.UtcNow));
                if (i % 10 == 0) store.Flush();
            }
            store.Flush();
            var files = Directory.GetFiles(Path.GetDirectoryName(path)!);
            Assert.True(files.All(x => !x.EndsWith(".tmp")), "без .tmp: " + string.Join(", ", files.Select(Path.GetFileName)));
            Assert.True(files.Length <= 3, "файлов: " + string.Join(", ", files.Select(Path.GetFileName)));
            Assert.Eq(50, new JsonLibraryStore(path).Mods().Count);
        });

        t.Run("кэш картинок: удалённые и изменённые архивы чистятся при запуске, кэш не растёт без конца", () =>
        {
            using var f = new Folder(30);
            var cache = new PreviewCache(Path.Combine(f.Root, "app", "previews"));
            // прошлые запуски оставили картинки 30 модов и 500 давно удалённых
            foreach (var z in Directory.GetFiles(f.Mods, "*.zip")) cache.PutImage(ArchiveStamp.Of(z), new byte[] { 1 });
            for (var i = 0; i < 500; i++) cache.PutNoImage(new ArchiveStamp($"gone{i}.zip", i, DateTime.UtcNow));
            new LibraryGarageBackend(f.Store, f.Inst, cache, () => false).Load();
            Assert.Eq(30, Directory.GetFiles(cache.Folder).Length);
        });
    }
}
