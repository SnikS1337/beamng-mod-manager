using System.IO.Compression;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Matching;
using BeamModManager.Core.Models;
using BeamModManager.Core.Search;
using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources;

namespace BeamModManager.Core.Tests;

/// <summary>Четыре «мелких фикса» ядра (STATUS.md 3.1): ручные решения по дублям, возврат версий, настройки внешнего вида, «Не проверять этот мод».</summary>
public static class CoreFixesTests
{
    private static readonly DateTimeOffset T0 = new(2026, 10, 14, 12, 0, 0, TimeSpan.Zero);

    private static string Tmp()
    {
        var d = Path.Combine(Path.GetTempPath(), "bmm-fix-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(d);
        return d;
    }

    private static MemoryStream Zip(int size)
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true))
        {
            using (var w = z.CreateEntry("vehicles/car/info.json").Open()) w.Write(new byte[20]);
            using var w2 = z.CreateEntry("vehicles/car/car.jbeam").Open();
            w2.Write(Enumerable.Range(0, size).Select(i => (byte)(i % 251)).ToArray());
        }
        ms.Position = 0;
        return ms;
    }

    private sealed class Sandbox : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "bmm-fixsb-" + Guid.NewGuid().ToString("N"));
        public readonly ModLayout Layout;
        public DateTimeOffset Now = T0;
        public readonly ModInstaller Inst;
        public readonly InMemoryLibraryStore Store = new();
        public Sandbox(int keep = 5)
        {
            Layout = new ModLayout(Path.Combine(Root, "mods"), Path.Combine(Root, "bmm", "disabled"), Path.Combine(Root, "bmm", "backups"));
            Inst = new ModInstaller(Layout, () => Now = Now.AddSeconds(1), keep);
        }
        public void Dispose() { try { Directory.Delete(Root, true); } catch { } }

        public InstalledMod InstallNew(string name, int size)
        {
            var r = Inst.Install(Zip(size), name);
            Assert.True(r.Success, r.Message ?? "установка");
            return LibrarySync.RecordInstalled(Store, name, new FileInfo(Path.Combine(Layout.ModsDir, name)).Length, null, true, Now);
        }

        public InstalledMod UpdateTo(InstalledMod mod, string newName, int size)
        {
            var r = Inst.Update(mod.FileName, Zip(size), newName);
            Assert.True(r.Success, r.Install.Message ?? "обновление");
            return LibrarySync.RecordInstalled(Store, newName, new FileInfo(Path.Combine(Layout.ModsDir, newName)).Length, null, true, Now, replacesFileName: mod.FileName);
        }

        public string[] EnabledFiles() => Directory.Exists(Layout.ModsDir) ? Directory.GetFiles(Layout.ModsDir).Select(Path.GetFileName).OrderBy(x => x).Select(x => x!).ToArray() : Array.Empty<string>();
        public string[] DisabledFiles() => Directory.Exists(Layout.DisabledDir) ? Directory.GetFiles(Layout.DisabledDir).Select(Path.GetFileName).OrderBy(x => x).Select(x => x!).ToArray() : Array.Empty<string>();
    }

    private static Listing L(string source, string id, string title) => new(source, new Uri($"https://{source}.test/{id}"), id, title);

    private sealed class Fake : IModSource
    {
        private readonly Listing[] _items;
        public int Calls;
        public Fake(string id, params Listing[] items) { Id = id; DisplayName = id.ToUpperInvariant(); _items = items; }
        public string Id { get; }
        public string DisplayName { get; }
        public bool SupportsSearch => true;
        public Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<IReadOnlyList<Listing>> SearchAsync(string query, CancellationToken ct = default) { Interlocked.Increment(ref Calls); return Task.FromResult<IReadOnlyList<Listing>>(_items); }
        public Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default) => Task.FromResult<Listing?>(null);
        public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? since, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<IReadOnlyList<Offer>> GetOffersAsync(Listing listing, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Offer>>(Array.Empty<Offer>());
    }

    public static void Register(TestRunner t)
    {
        MatchDecisions(t);
        Versions(t);
        Appearance(t);
        Declined(t);
        Review2(t);
    }

    // ---------------------------------------------------------------- 1. Ручные решения по дублям
    private static void MatchDecisions(TestRunner t)
    {
        t.Group("Ручные решения по дублям");
        var a = L("modland", "a", "Toyota Sienna 2011-2017 2.2");
        var b = L("modsgaming", "b", "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE");
        var c = L("stmods", "c", "Совсем другая машина");

        t.Run("решение не зависит от порядка пары; отмена возвращает «нет решения»", () =>
        {
            var s = new MatchDecisionStore();
            Assert.True(s.Get(a, b) is null, "пока нет решения");
            s.Set(a, b, ManualMatch.Same);
            Assert.True(s.Get(b, a) == ManualMatch.Same, "обратный порядок");
            s.Set(b, a, ManualMatch.Different); // передумал: заменяется, а не добавляется
            Assert.Eq(1, s.All().Count);
            Assert.True(s.Get(a, b) == ManualMatch.Different, "новое решение");
            s.Clear(a, b);
            Assert.True(s.Get(a, b) is null, "отменено");
            s.Clear(a, b); // повторная отмена безвредна
        });

        t.Run("одна и та же запись или два листинга одного сайта не объединяются", () =>
        {
            var s = new MatchDecisionStore();
            var thrown = 0;
            try { s.Set(a, a, ManualMatch.Same); } catch (ArgumentException) { thrown++; }
            try { s.Set(a, L("MODLAND", "z", "x"), ManualMatch.Same); } catch (ArgumentException) { thrown++; }
            Assert.Eq(2, thrown);
            Assert.Eq(0, s.All().Count);
            Assert.True(s.Get(a, a) is null, "сама с собой");
        });

        t.Run("ключ записи: регистр сайта не важен, номер как есть", () =>
        {
            Assert.Eq("modland:a", MatchDecisionStore.KeyOf(L("ModLand", "a", "x")));
            var s = new MatchDecisionStore();
            s.Set(L("MODLAND", "a", "x"), b, ManualMatch.Same);
            Assert.True(s.Get(a, b) == ManualMatch.Same, "тот же ключ");
            Assert.True(s.Get(L("modland", "A", "x"), b) is null, "другой номер — другая запись");
        });

        t.Run("решения переживают перезапуск; временного файла не остаётся", () =>
        {
            var p = Path.Combine(Tmp(), "sub", "decisions.json");
            var s1 = new MatchDecisionStore(p, () => T0);
            s1.Set(a, b, ManualMatch.Same);
            s1.Set(a, c, ManualMatch.Different);
            var s2 = new MatchDecisionStore(p);
            Assert.Eq(0, s2.LoadWarnings.Count);
            Assert.True(s2.Get(a, b) == ManualMatch.Same && s2.Get(c, a) == ManualMatch.Different, "оба решения");
            Assert.Eq(T0, s2.All().First().DecidedAtUtc);
            Assert.False(File.Exists(p + ".tmp"), "tmp");
            s2.Clear(a, b);
            Assert.True(new MatchDecisionStore(p).Get(a, b) is null, "отмена тоже сохранена");
        });

        t.Run("не удалось записать на диск: ошибка выходит наружу, в памяти остаётся прежнее состояние", () =>
        {
            var d = Tmp();
            var blocker = Path.Combine(d, "file");
            File.WriteAllText(blocker, "x"); // «папка» на самом деле файл: записать решения некуда
            var s = new MatchDecisionStore(Path.Combine(blocker, "decisions.json"));
            var threw = false;
            try { s.Set(a, b, ManualMatch.Same); } catch (IOException) { threw = true; }
            Assert.True(threw, "ошибка выходит наружу");
            Assert.Eq(0, s.All().Count);
            Assert.True(s.Get(a, b) is null, "решения нет");

            var p = Path.Combine(d, "sub", "decisions.json");
            var s2 = new MatchDecisionStore(p);
            s2.Set(a, b, ManualMatch.Same);
            Directory.Delete(Path.Combine(d, "sub"), true);
            File.WriteAllText(Path.Combine(d, "sub"), "x");
            threw = false;
            try { s2.Set(a, b, ManualMatch.Different); } catch (IOException) { threw = true; }
            try { s2.Clear(a, b); } catch (IOException) { threw = true; }
            Assert.True(threw, "ошибка при записи");
            Assert.True(s2.Get(a, b) == ManualMatch.Same, "прежнее решение осталось");
        });

        t.Run("повреждённый файл решений откладывается, начинаем заново, без падения", () =>
        {
            var d = Tmp(); var p = Path.Combine(d, "decisions.json");
            File.WriteAllText(p, "{ это не json");
            var s = new MatchDecisionStore(p, () => T0);
            Assert.Eq(1, s.LoadWarnings.Count);
            Assert.Eq(0, s.All().Count);
            Assert.True(Directory.GetFiles(d, "decisions.json.bad-*").Length == 1, "карантин");
            s.Set(a, b, ManualMatch.Same); // и после этого работает
            Assert.True(new MatchDecisionStore(p).Get(a, b) == ManualMatch.Same, "сохранилось");
        });

        t.Run("неверные записи пропускаются с предупреждением, хорошие остаются", () =>
        {
            var p = Path.Combine(Tmp(), "decisions.json");
            File.WriteAllText(p, """
            { "schema": 1, "items": [
              { "a": "modland:a", "b": "stmods:c", "decision": "different", "at": "2026-10-01T00:00:00Z" },
              { "a": 5, "b": "x", "decision": "same" },
              { "a": "x:1", "b": "x:1", "decision": "same" },
              { "a": "x:1", "b": "y:2", "decision": "maybe" },
              "мусор" ] }
            """);
            var s = new MatchDecisionStore(p);
            Assert.Eq(1, s.All().Count);
            Assert.Eq(1, s.LoadWarnings.Count);
            Assert.True(s.Get(a, c) == ManualMatch.Different, "хорошая запись");
        });

        t.Run("файл неверного формата (не объект / нет items) откладывается", () =>
        {
            foreach (var body in new[] { "[1,2]", "{ \"items\": 5 }", "null" })
            {
                var d = Tmp(); var p = Path.Combine(d, "decisions.json");
                File.WriteAllText(p, body);
                var s = new MatchDecisionStore(p);
                Assert.Eq(1, s.LoadWarnings.Count);
                Assert.Eq(0, s.All().Count);
                Assert.False(File.Exists(p), "перенесён");
            }
        });

        t.Run("сравнение: «один мод» даёт Same с оценкой 1, «разные» даёт Different с оценкой 0; без решения как раньше", () =>
        {
            var s = new MatchDecisionStore();
            var plain = DuplicateMatcher.Compare(a, c);
            var none = DuplicateMatcher.Compare(a, c, s);
            Assert.True(plain.Verdict == none.Verdict && plain.Score == none.Score, "без решения то же самое");
            Assert.True(DuplicateMatcher.Compare(a, c, null).Verdict == plain.Verdict, "без хранилища то же самое");
            s.Set(a, c, ManualMatch.Same);
            var same = DuplicateMatcher.Compare(c, a, s);
            Assert.True(same.Verdict == MatchVerdict.Same && same.Score == 1.0, "Same");
            s.Set(a, c, ManualMatch.Different);
            var diff = DuplicateMatcher.Compare(a, c, s);
            Assert.True(diff.Verdict == MatchVerdict.Different && diff.Score == 0.0, "Different");
        });

        t.Run("поиск: «Это один мод» склеивает карточки, «Вернуть как было» расклеивает", async () =>
        {
            var s = new MatchDecisionStore();
            var m1 = new Fake("modland", a); var m2 = new Fake("modsgaming", b);
            var svc = new SearchService(new IModSource[] { m1, m2 }, decisions: s);
            var r = await svc.SearchAsync("sienna");
            Assert.Eq(2, r.Cards.Count);
            s.Set(a, b, ManualMatch.Same);
            var merged = svc.Regroup(r, "sienna");
            Assert.Eq(1, merged.Cards.Count);
            Assert.Eq(2, merged.Cards[0].Listings.Count);
            Assert.Eq(r.Sources.Count, merged.Sources.Count);
            s.Clear(a, b);
            Assert.Eq(2, svc.Regroup(merged, "sienna").Cards.Count);
            Assert.Eq(1, m1.Calls); // пересборка не ходит на сайты
            Assert.Eq(1, m2.Calls);
        });

        t.Run("поиск: ручное «Это один мод» работает, даже когда признаки сильно расходятся", async () =>
        {
            var s = new MatchDecisionStore();
            s.Set(a, c, ManualMatch.Same);
            var svc = new SearchService(new IModSource[] { new Fake("modland", a), new Fake("stmods", c) }, decisions: s);
            Assert.Eq(1, (await svc.SearchAsync("sienna")).Cards.Count);
        });

        t.Run("поиск: «Это разные» разводит записи, которые сами склеились бы", async () =>
        {
            var offers = new[] { new Offer(OfferKind.ExternalHost, "modsfire", new Uri("https://modsfire.com/9nDwN00BJ04Ef55"), "9nDwN00BJ04Ef55") };
            var x = L("modland", "a", "Toyota Sienna 2011-2017 2.2") with { Offers = offers };
            var y = L("modsgaming", "b", "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE") with { Offers = offers };
            var srcs = () => new IModSource[] { new Fake("modland", x), new Fake("modsgaming", y) };
            Assert.Eq(1, (await new SearchService(srcs()).SearchAsync("sienna")).Cards.Count);
            var s = new MatchDecisionStore();
            s.Set(x, y, ManualMatch.Different);
            var r = await new SearchService(srcs(), decisions: s).SearchAsync("sienna");
            Assert.Eq(2, r.Cards.Count);
            Assert.True(r.Cards.All(cd => cd.PossibleSameAs.Count == 0), "и «возможно то же» не предлагается");
        });

        t.Run("поиск: «Это разные» для одной пары из трёх не склеивает их через третью запись", async () =>
        {
            var offers = new[] { new Offer(OfferKind.ExternalHost, "modsfire", new Uri("https://modsfire.com/9nDwN00BJ04Ef55"), "9nDwN00BJ04Ef55") };
            var x = L("modland", "a", "Toyota Sienna 2011-2017 2.2") with { Offers = offers };
            var y = L("modsgaming", "b", "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE") with { Offers = offers };
            var z = L("stmods", "c", "Toyota Sienna (2011-2017) версия 2.2 для BeamNG.drive (v0.39.x)") with { Offers = offers };
            var s = new MatchDecisionStore();
            s.Set(x, y, ManualMatch.Different);
            var svc = new SearchService(new IModSource[] { new Fake("modland", x), new Fake("modsgaming", y), new Fake("stmods", z) }, decisions: s);
            var r = await svc.SearchAsync("sienna");
            Assert.Eq(2, r.Cards.Count);
            Assert.False(r.Cards.Any(cd => cd.Listings.Any(l => l.Source == "modland") && cd.Listings.Any(l => l.Source == "modsgaming")), "x и y врозь");
            Assert.Eq(3, r.Cards.Sum(cd => cd.Listings.Count));
        });
    }

    // ---------------------------------------------------------------- 2. Прошлые версии, копии, удалённые моды
    private static void Versions(TestRunner t)
    {
        t.Group("Прошлые версии и удалённые моды");

        t.Run("список копий: от новых к старым, с размером, временем и номером", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(Zip(10), "car.zip");
            sb.Inst.Install(Zip(20), "car.zip", new InstallOptions { ReplaceExisting = true });
            sb.Inst.Install(Zip(30), "car.zip", new InstallOptions { ReplaceExisting = true });
            var l = sb.Inst.ListBackups("car.zip");
            Assert.Eq(2, l.Count);
            Assert.True(l[0].Sequence > l[1].Sequence, "новая первой");
            Assert.True(l[0].SavedAtUtc is not null && l[0].SavedAtUtc >= l[1].SavedAtUtc, "время разобрано");
            Assert.True(l[0].SizeBytes > l[1].SizeBytes, "размеры");
            Assert.True(l.All(x => x.FileName == "car.zip"), "имя мода");
            Assert.Eq(0, sb.Inst.ListBackups("нет.zip").Count);
            Assert.Eq(0, sb.Inst.ListBackups("../evil.zip").Count);
            Assert.Eq(0, sb.Inst.ListBackups("").Count);
        });

        t.Run("возврат конкретной копии: она встаёт на место, текущий файл уходит в копии", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(Zip(10), "car.zip");
            sb.Inst.Install(Zip(20), "car.zip", new InstallOptions { ReplaceExisting = true });
            sb.Inst.Install(Zip(30), "car.zip", new InstallOptions { ReplaceExisting = true });
            var oldest = sb.Inst.ListBackups("car.zip").Last();
            var curSize = new FileInfo(Path.Combine(sb.Layout.ModsDir, "car.zip")).Length;
            var r = sb.Inst.RestoreBackup("car.zip", oldest.Id);
            Assert.True(r.Success, r.Message ?? "");
            Assert.Eq(oldest.SizeBytes, new FileInfo(Path.Combine(sb.Layout.ModsDir, "car.zip")).Length);
            Assert.True(sb.Inst.ListBackups("car.zip").Any(x => x.SizeBytes == curSize), "прежняя текущая сохранена");
            Assert.False(sb.Inst.ListBackups("car.zip").Any(x => x.Id == oldest.Id), "выбранная копия взята из списка");
        });

        t.Run("идентификатор копии не пускает наружу папки копий", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(Zip(10), "car.zip");
            sb.Inst.Install(Zip(20), "car.zip", new InstallOptions { ReplaceExisting = true });
            var before = new FileInfo(Path.Combine(sb.Layout.ModsDir, "car.zip")).Length;
            foreach (var bad in new[] { "", "../../mods/car.zip", "..\\..\\x", Path.Combine(sb.Layout.ModsDir, "car.zip"), "несуществует", "/etc/passwd" })
                Assert.False(sb.Inst.RestoreBackup("car.zip", bad).Success, $"отклонено: {bad}");
            Assert.Eq(before, new FileInfo(Path.Combine(sb.Layout.ModsDir, "car.zip")).Length);
            Assert.False(sb.Inst.RestoreBackup("../car.zip", "x").Success, "имя файла с путём");
        });

        t.Run("прошлые версии после обновления с другим именем файла: видны копии под всеми прежними именами", () =>
        {
            using var sb = new Sandbox();
            var m = sb.InstallNew("car_v1.0.zip", 10);
            m = sb.UpdateTo(m, "car_v2.0.zip", 20);
            m = sb.UpdateTo(m, "car_v3.0.zip", 30);
            Assert.Eq(2, sb.Store.FileHistory(m.Id).Count);
            var past = VersionHistory.For(sb.Inst, sb.Store, m);
            Assert.Eq(2, past.Count);
            Assert.Eq("car_v2.0.zip", past[0].Backup.FileName); // новее первая
            Assert.Eq("car_v1.0.zip", past[1].Backup.FileName);
            Assert.True(past[0].Version.Length > 0, "версия угадана по имени файла");
            Assert.Eq(1, sb.EnabledFiles().Length);
        });

        t.Run("вернуть версию под другим именем: в папке одна включённая, запись в библиотеке следует, текущая в копиях", () =>
        {
            using var sb = new Sandbox();
            var m = sb.InstallNew("car_v1.0.zip", 10);
            m = sb.UpdateTo(m, "car_v2.0.zip", 20);
            var v1 = VersionHistory.For(sb.Inst, sb.Store, m).Single();
            var r = VersionHistory.Restore(sb.Inst, sb.Store, m, v1, sb.Now);
            Assert.True(r.Success, r.Message ?? "");
            Assert.True(sb.EnabledFiles().SequenceEqual(new[] { "car_v1.0.zip" }), "одна версия в mods");
            var rec = sb.Store.Mods().Single();
            Assert.Eq(m.Id, rec.Id);
            Assert.Eq("car_v1.0.zip", rec.FileName);
            Assert.Eq(v1.Backup.SizeBytes, rec.SizeBytes);
            // снова можно вернуть v2: она теперь среди копий
            var again = VersionHistory.For(sb.Inst, sb.Store, rec);
            Assert.True(again.Any(p => p.Backup.FileName == "car_v2.0.zip"), "v2 в копиях");
            Assert.True(VersionHistory.Restore(sb.Inst, sb.Store, rec, again.First(p => p.Backup.FileName == "car_v2.0.zip"), sb.Now).Success, "и обратно");
            Assert.True(sb.EnabledFiles().SequenceEqual(new[] { "car_v2.0.zip" }), "одна версия в mods после обратного");
        });

        t.Run("отключённый мод остаётся отключённым при возврате версии", () =>
        {
            using var sb = new Sandbox();
            var m = sb.InstallNew("car_v1.0.zip", 10);
            m = sb.UpdateTo(m, "car_v2.0.zip", 20);
            Assert.True(sb.Inst.Disable("car_v2.0.zip").Success, "отключён");
            m = m with { Enabled = false };
            sb.Store.UpsertMod(m);
            var r = VersionHistory.Restore(sb.Inst, sb.Store, m, VersionHistory.For(sb.Inst, sb.Store, m).Single(), sb.Now);
            Assert.True(r.Success, r.Message ?? "");
            Assert.Eq(0, sb.EnabledFiles().Length);
            Assert.True(sb.DisabledFiles().SequenceEqual(new[] { "car_v1.0.zip" }), "в отключённых");
            Assert.False(sb.Store.Mods().Single().Enabled, "в записи тоже отключён");
        });

        t.Run("файл с именем возвращаемой версии уже лежит в папке: отказ, ничего не потеряно", () =>
        {
            using var sb = new Sandbox();
            var m = sb.InstallNew("car_v1.0.zip", 10);
            m = sb.UpdateTo(m, "car_v2.0.zip", 20);
            File.WriteAllBytes(Path.Combine(sb.Layout.ModsDir, "car_v1.0.zip"), new byte[7]); // кто-то положил вручную
            var v1 = VersionHistory.For(sb.Inst, sb.Store, m).Single();
            var r = VersionHistory.Restore(sb.Inst, sb.Store, m, v1, sb.Now);
            Assert.False(r.Success, "отказ");
            Assert.True(File.Exists(Path.Combine(sb.Layout.ModsDir, "car_v2.0.zip")), "текущая на месте");
            Assert.Eq(7L, new FileInfo(Path.Combine(sb.Layout.ModsDir, "car_v1.0.zip")).Length);
            Assert.Eq("car_v2.0.zip", sb.Store.Mods().Single().FileName);
            Assert.Eq(1, sb.Inst.ListBackups("car_v1.0.zip").Count);
        });

        t.Run("сбой посреди возврата версии: текущий файл возвращается на место, копия не теряется", () =>
        {
            using var sb = new Sandbox();
            var m = sb.InstallNew("car_v1.0.zip", 10);
            m = sb.UpdateTo(m, "car_v2.0.zip", 20);
            var curSize = new FileInfo(Path.Combine(sb.Layout.ModsDir, "car_v2.0.zip")).Length;
            Directory.CreateDirectory(Path.Combine(sb.Layout.ModsDir, "car_v1.0.zip")); // на месте будущего файла лежит папка: перенос сорвётся
            var v1 = VersionHistory.For(sb.Inst, sb.Store, m).Single();
            var r = VersionHistory.Restore(sb.Inst, sb.Store, m, v1, sb.Now);
            Assert.False(r.Success, "сбой сообщён");
            Assert.True(File.Exists(Path.Combine(sb.Layout.ModsDir, "car_v2.0.zip")), "текущая на месте");
            Assert.Eq(curSize, new FileInfo(Path.Combine(sb.Layout.ModsDir, "car_v2.0.zip")).Length);
            Assert.Eq(1, sb.Inst.ListBackups("car_v1.0.zip").Count);
            Assert.Eq(0, sb.Inst.ListBackups("car_v2.0.zip").Count);
            Assert.Eq("car_v2.0.zip", sb.Store.Mods().Single().FileName);
        });

        t.Run("текущего файла нет (удалён руками): копия встаёт без ошибки", () =>
        {
            using var sb = new Sandbox();
            var m = sb.InstallNew("car_v1.0.zip", 10);
            m = sb.UpdateTo(m, "car_v2.0.zip", 20);
            File.Delete(Path.Combine(sb.Layout.ModsDir, "car_v2.0.zip"));
            var r = VersionHistory.Restore(sb.Inst, sb.Store, m, VersionHistory.For(sb.Inst, sb.Store, m).Single(), sb.Now);
            Assert.True(r.Success, r.Message ?? "");
            Assert.True(sb.EnabledFiles().SequenceEqual(new[] { "car_v1.0.zip" }), "на месте");
        });

        t.Run("возврат версии с тем же именем файла работает как обычный возврат копии", () =>
        {
            using var sb = new Sandbox();
            var m = sb.InstallNew("car.zip", 10);
            sb.Inst.Install(Zip(20), "car.zip", new InstallOptions { ReplaceExisting = true });
            m = LibrarySync.RecordInstalled(sb.Store, "car.zip", new FileInfo(Path.Combine(sb.Layout.ModsDir, "car.zip")).Length, null, true, sb.Now);
            var p = VersionHistory.For(sb.Inst, sb.Store, m).Single();
            Assert.True(VersionHistory.Restore(sb.Inst, sb.Store, m, p, sb.Now).Success, "возврат");
            Assert.Eq(1, sb.Store.Mods().Count);
            Assert.Eq(p.Backup.SizeBytes, sb.Store.Mods().Single().SizeBytes);
            Assert.Eq(1, sb.EnabledFiles().Length);
        });

        t.Run("история имён файла: без дублей, без текущего имени, чистится вместе с модом", () =>
        {
            var s = new InMemoryLibraryStore();
            s.UpsertMod(new InstalledMod("m", "M", "m_v2.zip", "2", 1, T0));
            s.AddFileHistory("m", "m_v1.zip");
            s.AddFileHistory("m", "M_V1.ZIP");
            s.AddFileHistory("m", "m_v2.zip"); // текущее имя
            s.AddFileHistory("m", "  ");
            Assert.Eq(1, s.FileHistory("m").Count);
            Assert.Eq("m_v1.zip", s.FileHistory("m")[0].ToLowerInvariant());
            s.RemoveMod("m");
            Assert.Eq(0, s.FileHistory("m").Count);
        });

        t.Run("ручная замена файла (переименование на диске) тоже попадает в историю имён", () =>
        {
            using var sb = new Sandbox();
            var m = sb.InstallNew("car_v1.0.zip", 10);
            File.Move(Path.Combine(sb.Layout.ModsDir, "car_v1.0.zip"), Path.Combine(sb.Layout.ModsDir, "car_v1.1.zip"));
            LibrarySync.Reconcile(sb.Store, sb.Inst.ScanDetailed());
            var rec = sb.Store.Mods().Single();
            Assert.Eq("car_v1.1.zip", rec.FileName);
            Assert.Eq(m.Id, rec.Id);
            Assert.True(sb.Store.FileHistory(m.Id).SequenceEqual(new[] { "car_v1.0.zip" }), "прежнее имя запомнено");
        });

        t.Run("удалённые моды: виден удалённый, не виден тот, что на месте или отключён", () =>
        {
            using var sb = new Sandbox();
            sb.InstallNew("a.zip", 10); sb.InstallNew("b.zip", 11); sb.InstallNew("c.zip", 12);
            Assert.Eq(0, sb.Inst.DeletedMods().Count);
            Assert.True(sb.Inst.Remove("a.zip").Success, "удалён a");
            Assert.True(sb.Inst.Disable("b.zip").Success, "отключён b");
            var del = sb.Inst.DeletedMods();
            Assert.Eq(1, del.Count);
            Assert.Eq("a.zip", del[0].FileName);
            Assert.Eq(1, del[0].BackupCount);
            Assert.True(del[0].LastSizeBytes > 0, "размер последней копии");
            Assert.True(sb.Inst.Remove("c.zip").Success, "удалён c");
            Assert.Eq("c.zip", sb.Inst.DeletedMods()[0].FileName); // позднее удалённый первым
        });

        t.Run("удалённые моды: пустой или посторонний каталог копий не мешает", () =>
        {
            using var sb = new Sandbox();
            Assert.Eq(0, sb.Inst.DeletedMods().Count); // папки копий ещё нет
            Directory.CreateDirectory(Path.Combine(sb.Layout.BackupDir, "пусто.zip"));
            File.WriteAllText(Path.Combine(sb.Layout.BackupDir, "stray.txt"), "x");
            Assert.Eq(0, sb.Inst.DeletedMods().Count);
        });

        t.Run("вернуть удалённый мод: встаёт включённым, запись в библиотеке создаётся, из списка удалённых уходит", () =>
        {
            using var sb = new Sandbox();
            sb.InstallNew("car_v1.0.zip", 10);
            sb.Inst.Remove("car_v1.0.zip");
            sb.Store.RemoveMod(sb.Store.Mods().Single().Id);
            var del = sb.Inst.DeletedMods().Single();
            var r = VersionHistory.RestoreDeleted(sb.Inst, sb.Store, del, sb.Now);
            Assert.True(r.Success, r.Message ?? "");
            Assert.True(sb.EnabledFiles().SequenceEqual(new[] { "car_v1.0.zip" }), "файл на месте");
            Assert.Eq("car_v1.0.zip", sb.Store.Mods().Single().FileName);
            Assert.Eq(0, sb.Inst.DeletedMods().Count);
        });

        t.Run("вернуть удалённый мод: конкретная более ранняя копия; без копий — отказ", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(Zip(10), "car.zip");
            sb.Inst.Install(Zip(20), "car.zip", new InstallOptions { ReplaceExisting = true });
            sb.Inst.Remove("car.zip"); // в копиях: 10, 20
            var del = sb.Inst.DeletedMods().Single();
            Assert.Eq(2, del.BackupCount);
            var oldest = sb.Inst.ListBackups("car.zip").Last();
            Assert.True(VersionHistory.RestoreDeleted(sb.Inst, sb.Store, del, sb.Now, oldest.Id).Success, "возврат");
            Assert.Eq(oldest.SizeBytes, new FileInfo(Path.Combine(sb.Layout.ModsDir, "car.zip")).Length);
            Assert.False(VersionHistory.RestoreDeleted(sb.Inst, sb.Store, new DeletedMod("нет.zip", 0, null, 0), sb.Now).Success, "нет копий");
        });

        t.Run("несколько возвратов подряд не ломают порядок и не теряют копии (с лимитом копий)", () =>
        {
            using var sb = new Sandbox(keep: 3);
            var m = sb.InstallNew("car_v1.0.zip", 10);
            m = sb.UpdateTo(m, "car_v2.0.zip", 20);
            for (var i = 0; i < 6; i++)
            {
                var p = VersionHistory.For(sb.Inst, sb.Store, m).First();
                var r = VersionHistory.Restore(sb.Inst, sb.Store, m, p, sb.Now);
                Assert.True(r.Success, $"возврат {i}: {r.Message}");
                m = sb.Store.Mods().Single();
                Assert.Eq(1, sb.EnabledFiles().Length);
            }
            Assert.Eq(1, sb.Store.Mods().Count);
        });
    }

    // ---------------------------------------------------------------- 3. Настройки внешнего вида и прочие
    private static void Appearance(TestRunner t)
    {
        t.Group("Настройки внешнего вида");

        t.Run("значения по умолчанию совпадают с дизайном", () =>
        {
            var d = new AppSettings();
            Assert.True(d.LiquidGlass && d.UiAnimations && d.AnimatedBackground && d.Fireflies, "всё включено");
            Assert.True(AppearancePresets.Detect(d) == AppearancePreset.Beautiful, "по умолчанию — «Красиво»");
            Assert.True(d.Theme == BackgroundTheme.Night, "фон «Ночь»");
            Assert.Eq(30, d.ActiveModsWarnAbove);
            Assert.False(d.FirstRunDone, "первый запуск не пройден");
            Assert.Eq(0, d.WindowWidth);
            Assert.False(d.WindowMaximized, "окно");
        });

        t.Run("значения вне диапазона приводятся к допустимым", () =>
        {
            var s = new AppSettings { ActiveModsWarnAbove = -4, WindowWidth = 5, WindowHeight = 99999, Theme = (BackgroundTheme)42 }.Normalize();
            Assert.Eq(0, s.ActiveModsWarnAbove);
            Assert.Eq(0, s.WindowWidth);
            Assert.Eq(0, s.WindowHeight);
            Assert.True(s.Theme == BackgroundTheme.Night, "неизвестная тема фона");
            Assert.Eq(0, new AppSettings { ActiveModsWarnAbove = 0 }.Normalize().ActiveModsWarnAbove);
            var low = new AppSettings { ActiveModsWarnAbove = 100000, WindowWidth = 1280, WindowHeight = 720 }.Normalize();
            Assert.Eq(1000, low.ActiveModsWarnAbove);
            Assert.True(low.WindowWidth == 1280 && low.WindowHeight == 720, "нормальный размер окна сохраняется");
            var small = new AppSettings { WindowWidth = 999, WindowHeight = 679 }.Normalize();
            Assert.True(small.WindowWidth == 0 && small.WindowHeight == 0, "меньше минимума окна 1000 × 680 — по умолчанию");
            Assert.False(new AppSettings().BackgroundTilt, "наклон фона по умолчанию выключен");
        });

        t.Run("сохранение и чтение возвращают все новые поля", () =>
        {
            var p = Path.Combine(Tmp(), "settings.json");
            var want = new AppSettings
            {
                LiquidGlass = false, Theme = BackgroundTheme.Dusk,
                UiAnimations = false, AnimatedBackground = false, Fireflies = false, BackgroundTilt = true, ActiveModsWarnAbove = 55, FirstRunDone = true,
                WindowWidth = 1600, WindowHeight = 900, WindowMaximized = true,
            };
            var st = new SettingsStore(p);
            st.Save(want);
            var r = st.Load();
            Assert.False(r.HadProblems, "без предупреждений: " + string.Join("; ", r.Warnings));
            Assert.Eq(want with { DisabledSources = r.Settings.DisabledSources }, r.Settings);
        });

        t.Run("каждая настройка переживает перезапуск: любое поле, изменённое по отдельности, читается обратно", () =>
        {
            // защита на будущее: новое поле в AppSettings, которое забыли прочитать в SettingsStore, уронит этот тест
            var p = Path.Combine(Tmp(), "settings.json");
            var st = new SettingsStore(p);
            var baseline = new AppSettings();
            foreach (var prop in typeof(AppSettings).GetProperties().Where(x => x.CanWrite && x.Name != nameof(AppSettings.SchemaVersion)))
            {
                object changed = prop.PropertyType switch
                {
                    var tp when tp == typeof(bool) => !(bool)prop.GetValue(baseline)!,
                    var tp when tp == typeof(int) => prop.Name switch
                    {
                        nameof(AppSettings.WindowWidth) => 1234, nameof(AppSettings.WindowHeight) => 777,
                        nameof(AppSettings.AutoCheckEveryHours) => 12,
                        _ => (int)prop.GetValue(baseline)! + 3,
                    },
                    var tp when tp == typeof(string) => @"D:\Игры\mods",
                    var tp when tp.IsEnum => Enum.GetValues(tp).Cast<object>().Last(),
                    var tp when tp == typeof(IReadOnlyList<string>) => new List<string> { "stmods" },
                    _ => throw new InvalidOperationException($"тест не умеет менять поле {prop.Name} ({prop.PropertyType})"),
                };
                var want = baseline with { };
                prop.SetValue(want, changed);
                st.Save(want);
                var got = st.Load();
                Assert.False(got.HadProblems, $"{prop.Name}: без предупреждений");
                var back = prop.GetValue(got.Settings);
                var ok = back is IEnumerable<string> list ? list.SequenceEqual((IEnumerable<string>)changed) : Equals(back, prop.GetValue(want.Normalize()));
                Assert.True(ok, $"{prop.Name}: записано {changed}, прочитано {back}");
            }
        });

        t.Run("старый файл без новых полей читается с значениями по умолчанию", () =>
        {
            var p = Path.Combine(Tmp(), "settings.json");
            File.WriteAllText(p, "{ \"AllowDownloads\": true, \"KeepBackups\": 5 }");
            var r = new SettingsStore(p).Load();
            Assert.False(r.HadProblems, "без предупреждений");
            Assert.True(r.Settings.AllowDownloads && r.Settings.KeepBackups == 5, "старое сохранено");
            Assert.True(r.Settings.LiquidGlass && r.Settings.Fireflies, "новое по умолчанию");
            Assert.Eq(30, r.Settings.ActiveModsWarnAbove);
        });

        t.Run("неверные типы новых полей: предупреждение и значение по умолчанию, остальное читается", () =>
        {
            var p = Path.Combine(Tmp(), "settings.json");
            File.WriteAllText(p, """
            { "LiquidGlass": "нет", "UiAnimations": 0, "Theme": "Night",
              "ActiveModsWarnAbove": null, "WindowWidth": true, "FirstRunDone": true, "Fireflies": false }
            """);
            var r = new SettingsStore(p).Load();
            Assert.True(r.HadProblems, "предупреждения есть");
            Assert.True(r.Settings.LiquidGlass && r.Settings.UiAnimations, "по умолчанию");
            Assert.True(r.Settings.Theme == BackgroundTheme.Night, "фон по умолчанию");
            Assert.Eq(30, r.Settings.ActiveModsWarnAbove);
            Assert.Eq(0, r.Settings.WindowWidth);
            Assert.True(r.Settings.FirstRunDone && !r.Settings.Fireflies, "верные поля прочитаны");
        });

        t.Run("значения вне диапазона в файле приводятся к допустимым, тема фона из мусора — «Ночь»", () =>
        {
            var p = Path.Combine(Tmp(), "settings.json");
            File.WriteAllText(p, "{ \"KeepBackups\": 77, \"Theme\": 99, \"ActiveModsWarnAbove\": 5000000000 }");
            var s = new SettingsStore(p).Load().Settings;
            Assert.Eq(20, s.KeepBackups);
            Assert.True(s.Theme == BackgroundTheme.Night, "фон");
            Assert.True(s.ActiveModsWarnAbove is >= 0 and <= 1000, "порог в пределах");
        });

        t.Run("наборы внешнего вида: применяются, распознаются, свои настройки распознаются как «свои»", () =>
        {
            foreach (var p in AppearancePresets.All)
            {
                var applied = AppearancePresets.Apply(new AppSettings { AllowDownloads = true, ModsFolder = "C:\\m", FirstRunDone = true }, p);
                Assert.True(applied.AllowDownloads && applied.ModsFolder == "C:\\m" && applied.FirstRunDone, $"{p}: чужие настройки не тронуты");
                Assert.True(AppearancePresets.Detect(applied) == p, $"{p}: распознан");
            }
            Assert.Eq(2, AppearancePresets.All.Count);
            var nice = AppearancePresets.Apply(new AppSettings { LiquidGlass = false, Fireflies = false, AnimatedBackground = false, UiAnimations = false }, AppearancePreset.Beautiful);
            Assert.True(nice.LiquidGlass && nice.Fireflies && nice.AnimatedBackground && nice.UiAnimations, "«Красиво»: всё включено");
            var fast = AppearancePresets.Apply(new AppSettings { BackgroundTilt = true }, AppearancePreset.Fast);
            Assert.True(!fast.LiquidGlass && !fast.AnimatedBackground && !fast.Fireflies && !fast.BackgroundTilt, "«Быстро»: ничего тяжёлого");
            Assert.False(fast.UiAnimations, "«Быстро»: анимации интерфейса тоже выключены");
            Assert.True(AppearancePresets.Detect(new AppSettings { Fireflies = false }) is null, "свои");
            Assert.True(AppearancePresets.Detect(new AppSettings { Theme = BackgroundTheme.Dusk }) == AppearancePreset.Beautiful, "тема фона на набор не влияет");
            Assert.True(AppearancePresets.Detect(new AppSettings { Mode = UiMode.Light }) == AppearancePreset.Beautiful, "оформление на набор не влияет");
            Assert.True(AppearancePresets.Detect(new AppSettings()) == AppearancePreset.Beautiful, "по умолчанию — «Красиво»");
            Assert.True(AppearancePresets.Detect(fast with { UiAnimations = true }) is null, "«Быстро» с анимациями — свой");
        });

        t.Run("наборы: каждое сочетание четырёх переключателей распознаётся не больше чем одним набором", () =>
        {
            var hits = 0;
            for (var m = 0; m < 16; m++)
            {
                var s = new AppSettings { LiquidGlass = (m & 1) != 0, UiAnimations = (m & 2) != 0, AnimatedBackground = (m & 4) != 0, Fireflies = (m & 8) != 0 };
                var p = AppearancePresets.Detect(s);
                if (p is null) continue;
                hits++;
                var back = AppearancePresets.Apply(s, p.Value);
                Assert.True(back.LiquidGlass == s.LiquidGlass && back.UiAnimations == s.UiAnimations && back.AnimatedBackground == s.AnimatedBackground
                    && back.Fireflies == s.Fireflies, $"{m}: набор ничего не меняет в своих же настройках");
            }
            Assert.Eq(2, hits, "ровно два сочетания — два набора");
        });

        t.Run("старый файл с инеем и искрами льда читается без предупреждений; огоньки остаются как были", () =>
        {
            var p = Path.Combine(Tmp(), "settings.json");
            File.WriteAllText(p, "{ \"Frost\": true, \"FrostStrength\": 2, \"FrostSparks\": false, \"Fireflies\": true, \"LiquidGlass\": false }");
            var st = new SettingsStore(p);
            var r = st.Load();
            Assert.False(r.HadProblems, string.Join("; ", r.Warnings));
            Assert.True(r.Settings.Fireflies && !r.Settings.LiquidGlass, "прочитано");
            st.Save(r.Settings);
            var text = File.ReadAllText(p);
            Assert.False(text.Contains("Frost"), "в новом файле инея больше нет");
            File.WriteAllText(p, "{ \"Frost\": \"мусор\", \"FrostStrength\": [1], \"FrostSparks\": {} }");
            Assert.False(st.Load().HadProblems, "мусор в старых полях не мешает");
        });
    }

    // ---------------------------------------------------------------- 4. «Не проверять этот мод»
    private static void Declined(TestRunner t)
    {
        t.Group("«Не проверять этот мод»");

        InMemoryLibraryStore Bound()
        {
            var s = new InMemoryLibraryStore();
            s.UpsertMod(new InstalledMod("e34", "BMW E34", "bmw_e34.zip", "1.0", 100, T0));
            s.UpsertBinding(new ModBinding("e34", "stmods", "bmw_e34", new Uri("https://stmods.org/bmw_e34/"), true));
            s.UpsertBinding(new ModBinding("e34", "modland", "bmw-e34", new Uri("https://modland.net/bmw-e34/"), true));
            return s;
        }
        var cand = new[] { new Listing("stmods", new Uri("https://stmods.org/bmw_e34/"), "bmw_e34", "BMW E34") { ModVersion = "1.0" } };

        t.Run("снимает все привязки, состояние проверок и пропущенную версию; запоминает отказ", () =>
        {
            var s = Bound();
            s.SaveCheck(new CheckState("e34", "stmods", T0, null, "1.1", 120));
            s.SetDismissedUpdate("e34", "stmods:1.1");
            Assert.Eq(2, BindingActions.DeclineChecking(s, "e34"));
            Assert.Eq(0, s.Bindings().Count);
            Assert.Eq(0, s.Checks().Count);
            Assert.True(s.GetDismissedUpdate("e34") is null, "пропуск снят");
            Assert.True(s.IsCheckingDeclined("e34"), "отказ запомнен");
            Assert.True(s.GetMod("e34") is not null, "сам мод остаётся");
        });

        t.Run("подсказки привязки для такого мода пусты, для других работают", () =>
        {
            var s = Bound();
            Assert.True(BindingSuggester.Suggest(s, s.GetMod("e34")!, cand).Count > 0, "до отказа предлагает");
            BindingActions.DeclineChecking(s, "e34");
            Assert.Eq(0, BindingSuggester.Suggest(s, s.GetMod("e34")!, cand).Count);
            s.UpsertMod(new InstalledMod("other", "BMW E34", "bmw_e34_copy.zip", "1.0", 100, T0));
            Assert.True(BindingSuggester.Suggest(s, s.GetMod("other")!, cand).Count > 0, "другой мод");
        });

        t.Run("поиск привязки для такого мода не ходит в сеть", async () =>
        {
            var s = Bound();
            BindingActions.DeclineChecking(s, "e34");
            var src = new Fake("stmods", cand);
            var svc = new SearchService(new IModSource[] { src });
            var found = await BindingSuggester.FindAsync(s, s.GetMod("e34")!, svc);
            Assert.Eq(0, found.Count);
            Assert.Eq(0, src.Calls);
        });

        t.Run("отказ переживает смену имени файла (тот же Id), а подтверждённая привязка снимает его", () =>
        {
            using var sb = new Sandbox();
            var m = sb.InstallNew("car_v1.0.zip", 10);
            BindingActions.DeclineChecking(sb.Store, m.Id);
            File.Move(Path.Combine(sb.Layout.ModsDir, "car_v1.0.zip"), Path.Combine(sb.Layout.ModsDir, "car_v1.1.zip"));
            LibrarySync.Reconcile(sb.Store, sb.Inst.ScanDetailed());
            Assert.True(sb.Store.IsCheckingDeclined(m.Id), "отказ сохранён");
            sb.Store.UpsertBinding(new ModBinding(m.Id, "stmods", "car", new Uri("https://stmods.org/car/"), true));
            Assert.False(sb.Store.IsCheckingDeclined(m.Id), "пользователь привязал сам — отказа нет");
        });

        t.Run("неподтверждённая привязка отказ не снимает; удаление мода чистит отказ", () =>
        {
            var s = Bound();
            BindingActions.DeclineChecking(s, "e34");
            s.UpsertBinding(new ModBinding("e34", "stmods", "x", new Uri("https://a/"), false));
            Assert.True(s.IsCheckingDeclined("e34"), "подсказка не отменяет отказ");
            s.RemoveMod("e34");
            Assert.False(s.IsCheckingDeclined("e34"), "после удаления мода");
        });

        t.Run("возобновление проверки: подсказки снова предлагаются; привязка сама не появляется", () =>
        {
            var s = Bound();
            BindingActions.DeclineChecking(s, "e34");
            BindingActions.ResumeChecking(s, "e34");
            Assert.False(s.IsCheckingDeclined("e34"), "флаг снят");
            Assert.Eq(0, s.Bindings().Count);
            Assert.True(BindingSuggester.Suggest(s, s.GetMod("e34")!, cand).Count > 0, "предлагает");
        });

        t.Run("«Отменить» после отказа возвращает привязки, проверки и пропущенную версию", () =>
        {
            var s = Bound();
            var check = new CheckState("e34", "stmods", T0, null, "1.1", 120);
            s.SaveCheck(check);
            s.SetDismissedUpdate("e34", "stmods:1.1");
            var undo = BindingActions.Decline(s, "e34")!;
            Assert.Eq(2, undo.Bindings.Count);
            Assert.True(BindingActions.Undo(s, undo), "отменено");
            Assert.Eq(2, s.Bindings().Count);
            Assert.True(s.GetCheck("e34", "stmods") == check, "состояние проверки вернулось");
            Assert.Eq("stmods:1.1", s.GetDismissedUpdate("e34"));
            Assert.False(s.IsCheckingDeclined("e34"), "отказа нет");
        });

        t.Run("«Отменить» не затирает привязку, сделанную после отказа, и безвредна для удалённого мода", () =>
        {
            var s = Bound();
            var undo = BindingActions.Decline(s, "e34")!;
            var fresh = new ModBinding("e34", "stmods", "new_page", new Uri("https://stmods.org/new_page/"), true);
            s.UpsertBinding(fresh);
            Assert.True(BindingActions.Undo(s, undo), "отменено");
            Assert.Eq("new_page", s.Bindings().Single(b => b.Source == "stmods").SourceId);
            Assert.Eq(2, s.Bindings().Count); // modland вернулась
            var s2 = Bound();
            var undo2 = BindingActions.Decline(s2, "e34")!;
            s2.RemoveMod("e34");
            Assert.False(BindingActions.Undo(s2, undo2), "мода нет");
            Assert.Eq(0, s2.Bindings().Count);
            Assert.True(BindingActions.Decline(s2, "e34") is null, "нет мода — нечего снимать");
        });

        t.Run("несуществующий мод и повторный отказ безвредны", () =>
        {
            var s = Bound();
            Assert.Eq(0, BindingActions.DeclineChecking(s, "нет"));
            Assert.False(s.IsCheckingDeclined("нет"), "флаг для несуществующего не ставится");
            Assert.Eq(2, BindingActions.DeclineChecking(s, "e34"));
            Assert.Eq(0, BindingActions.DeclineChecking(s, "e34"));
            Assert.True(s.IsCheckingDeclined("e34"), "остаётся");
        });

        t.Run("мод с отказом не попадает в проверку обновлений", () =>
        {
            var s = Bound();
            BindingActions.DeclineChecking(s, "e34");
            Assert.Eq(0, UpdateCheckServiceTracked(s));
            Assert.Eq(0, BindingSuggesterCount(s));
        });
    }

    // ---------------------------------------------------------------- Повторная ревизия (найдено при проверке)
    private static void Review2(TestRunner t)
    {
        t.Group("Повторная ревизия новых функций");

        t.Run("после обновления с новым именем старое имя не попадает в «Удалённые моды»", () =>
        {
            using var sb = new Sandbox();
            var m = sb.InstallNew("car_v1.0.zip", 10);
            sb.UpdateTo(m, "car_v2.0.zip", 20);
            Assert.Eq(0, VersionHistory.Deleted(sb.Inst, sb.Store).Count);
        });

        t.Run("старое имя не считается удалённым, даже если история имён потеряна (тот же мод по названию)", () =>
        {
            using var sb = new Sandbox();
            var m = sb.InstallNew("car_v1.0.zip", 10);
            sb.UpdateTo(m, "car_v2.0.zip", 20);
            var fresh = new InMemoryLibraryStore(); // библиотека без истории (например, после перезапуска)
            LibrarySync.Reconcile(fresh, sb.Inst.ScanDetailed());
            Assert.Eq(0, VersionHistory.Deleted(sb.Inst, fresh).Count);
            Assert.Eq(1, VersionHistory.For(sb.Inst, fresh, fresh.Mods().Single()).Count); // и в «Прошлых версиях» копия видна
        });

        t.Run("по-настоящему удалённый мод в списке остаётся", () =>
        {
            using var sb = new Sandbox();
            sb.InstallNew("car_v1.0.zip", 10);
            sb.InstallNew("truck.zip", 12);
            sb.Inst.Remove("truck.zip");
            LibrarySync.Reconcile(sb.Store, sb.Inst.ScanDetailed());
            var d = VersionHistory.Deleted(sb.Inst, sb.Store);
            Assert.Eq(1, d.Count);
            Assert.Eq("truck.zip", d[0].FileName);
        });

        t.Run("посторонние файлы в папке копий (desktop.ini, Thumbs.db) не считаются копиями и не возвращаются", () =>
        {
            using var sb = new Sandbox();
            sb.Inst.Install(Zip(10), "car.zip");
            sb.Inst.Install(Zip(20), "car.zip", new InstallOptions { ReplaceExisting = true });
            var dir = Path.Combine(sb.Layout.BackupDir, "car.zip");
            File.WriteAllText(Path.Combine(dir, "desktop.ini"), "[.ShellClassInfo]");
            File.WriteAllText(Path.Combine(dir, "Thumbs.db"), "x");
            Assert.Eq(1, sb.Inst.ListBackups("car.zip").Count);
            Assert.Eq(1, sb.Inst.Backups("car.zip").Count);
            var expected = sb.Inst.ListBackups("car.zip")[0].SizeBytes;
            Assert.True(sb.Inst.RestoreLatestBackup("car.zip").Success, "возврат");
            Assert.Eq(expected, new FileInfo(Path.Combine(sb.Layout.ModsDir, "car.zip")).Length);
            Assert.False(sb.Inst.RestoreBackup("car.zip", "desktop.ini").Success, "посторонний файл не возвращается");
            Assert.True(File.Exists(Path.Combine(dir, "desktop.ini")), "и не удаляется при чистке копий");
        });

        t.Run("ручное «Это один мод» сильнее автоматической склейки с другой карточкой", async () =>
        {
            var offers = new[] { new Offer(OfferKind.ExternalHost, "modsfire", new Uri("https://modsfire.com/9nDwN00BJ04Ef55"), "9nDwN00BJ04Ef55") };
            var x = L("modland", "a", "Toyota Sienna 2011-2017 2.2") with { Offers = offers };
            var y = L("modsgaming", "b", "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE") with { Offers = offers };
            var niva = new[] { new Offer(OfferKind.ExternalHost, "modsfire", new Uri("https://modsfire.com/NivaNivaNiva123"), "NivaNivaNiva123") };
            var other = L("stmods", "z", "Lada Niva 4x4") with { Offers = niva };
            var w = L("site4", "w", "Lada Niva 4x4 (1977)") with { Offers = niva }; // сама склеится с other
            var s = new MatchDecisionStore();
            s.Set(y, other, ManualMatch.Same); // пользователь сказал: y — это Нива (x и y по признакам один мод)
            var svc = new SearchService(new IModSource[] { new Fake("stmods", other), new Fake("site4", w), new Fake("modland", x), new Fake("modsgaming", y) }, decisions: s);
            var cards = (await svc.SearchAsync("sienna")).Cards;
            Assert.True(cards.Any(c => c.Listings.Contains(y) && c.Listings.Contains(other)), "y вместе с выбранной пользователем записью");
        });

        t.Run("пересборка после «Вернуть как было» даёт те же карточки, что исходный поиск", async () =>
        {
            var a = L("modland", "a", "Toyota Sienna 2011-2017 2.2");
            var b = L("modsgaming", "b", "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE");
            var c = L("stmods", "c", "Toyota Sienna (2011-2017) версия 2.2 для BeamNG.drive (v0.39.x)");
            var s = new MatchDecisionStore();
            var svc = new SearchService(new IModSource[] { new Fake("modland", a), new Fake("modsgaming", b), new Fake("stmods", c) }, decisions: s);
            var r0 = await svc.SearchAsync("sienna");
            s.Set(a, c, ManualMatch.Same);
            var r1 = svc.Regroup(r0, "sienna");
            s.Clear(a, c);
            var r2 = svc.Regroup(r1, "sienna");
            Assert.Eq(string.Join("|", r0.Cards.Select(x => x.Id)), string.Join("|", r2.Cards.Select(x => x.Id)));
            Assert.True(r2.Listings.SequenceEqual(r0.Listings), "порядок записей как у сайтов, а не как в карточках");
            Assert.Eq(3, r0.Listings.Count);
        });

        t.Run("ручная правка файла решений: регистр сайта в ключе не мешает", () =>
        {
            var p = Path.Combine(Tmp(), "decisions.json");
            File.WriteAllText(p, "{ \"items\": [ { \"a\": \"ModLand:a\", \"b\": \" STMods :c\", \"decision\": \"same\" } ] }");
            var s = new MatchDecisionStore(p);
            Assert.True(s.Get(L("modland", "a", "x"), L("stmods", "c", "y")) == ManualMatch.Same, "найдено");
        });

        t.Run("набор не меняет тему фона: это выбор цвета, а не нагрузки", () =>
        {
            var light = AppearancePresets.Apply(new AppSettings { Theme = BackgroundTheme.Deep }, AppearancePreset.Fast);
            Assert.True(light.Theme == BackgroundTheme.Deep, "тема осталась");
        });

        t.Run("старый файл с фото («Background», «BackgroundDim», «BackgroundImage») читается без предупреждений, фон «Ночь»", () =>
        {
            var p = Path.Combine(Tmp(), "settings.json");
            File.WriteAllText(p, "{ \"Background\": 2, \"BackgroundDim\": 70, \"BackgroundImage\": \"C:\\\\x.jpg\", \"Fireflies\": false }");
            var r = new SettingsStore(p).Load();
            Assert.False(r.HadProblems, string.Join("; ", r.Warnings));
            Assert.True(r.Settings.Theme == BackgroundTheme.Night && !r.Settings.Fireflies, "прочитано");
        });
    }

    private static int UpdateCheckServiceTracked(ILibraryStore s) => UpdateCheckService.Tracked(s).Count;
    private static int BindingSuggesterCount(ILibraryStore s) => s.Bindings().Count;
}
