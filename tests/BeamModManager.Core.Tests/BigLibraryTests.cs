using System.Diagnostics;
using System.IO.Compression;
using System.Text;
using BeamModManager.Core.Game;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Previews;
using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Settings;
using BeamModManager.Ui.Updates;

namespace BeamModManager.Core.Tests;

/// <summary>
/// Тысячи модов. Время должно расти вместе с числом модов, а не с его квадратом: тогда и на 5000 окно не задумывается.
/// Сравниваем 1000 и 5000 (линейно — ×5, квадратично — ×25), плюс правильность счёта на большой библиотеке.
/// </summary>
public static class BigLibraryTests
{
    private static readonly byte[] TinyZip = MakeZip();

    private static byte[] MakeZip()
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true)) using (var s = z.CreateEntry("info.json").Open()) s.Write("{}"u8);
        return ms.ToArray();
    }

    /// <summary>Папка с N модами и db.json, как у игры: каждый четвёртый выключен флажком, каждый седьмой — карта.</summary>
    private sealed class Big : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "bmm-big-" + Guid.NewGuid().ToString("N")[..8]);
        public readonly string Mods;
        public readonly ModInstaller Inst;
        public readonly JsonLibraryStore Store;
        public readonly PreviewCache Cache;
        public readonly int N;

        public static string Name(int i) => $"Мод_{i:D5} Car v1.{i % 10}.zip";

        public Big(int n)
        {
            N = n;
            Mods = Path.Combine(Root, "mods");
            Directory.CreateDirectory(Mods);
            var db = new StringBuilder("{\"header\":{\"version\":1.1},\"mods\":{");
            for (var i = 0; i < n; i++)
            {
                var name = Name(i);
                File.WriteAllBytes(Path.Combine(Mods, name), TinyZip);
                var key = GameModDb.KeyOf(name);
                if (i > 0) db.Append(',');
                db.Append($"\"{key}\":{{\"active\":{(i % 4 == 0 ? "false" : "true")},\"dirname\":\"/mods/\",\"filename\":\"{name}\",\"fullpath\":\"/mods/{name}\",\"modType\":\"{(i % 7 == 0 ? "terrain" : "vehicle")}\",\"modname\":\"{key}\",\"dateAdded\":1792060320}}");
            }
            File.WriteAllText(GameModDb.PathIn(Mods), db.Append("}}").ToString());
            var app = Path.Combine(Root, "app");
            Inst = new ModInstaller(ModLayout.InAppFolder(Mods, app), () => DateTimeOffset.UtcNow, 3);
            Store = new JsonLibraryStore(Path.Combine(app, "library.json"));
            Cache = new PreviewCache(Path.Combine(app, "previews"));
        }

        public LibraryGarageBackend Backend() => new(Store, Inst, Cache, () => false);

        public void Dispose() { try { Directory.Delete(Root, true); } catch (IOException) { } catch (UnauthorizedAccessException) { } }
    }

    /// <summary>Лучшее из трёх (меньше шума от сборщика мусора и соседних задач).</summary>
    private static double Best(Action a, int runs = 3)
    {
        var best = double.MaxValue;
        for (var i = 0; i < runs; i++)
        {
            var sw = Stopwatch.StartNew();
            a();
            best = Math.Min(best, sw.Elapsed.TotalMilliseconds);
        }
        return best;
    }

    /// <summary>
    /// Рост времени от 1000 к 5000 ближе к линейному (×5), чем к квадратичному (×25). Мелкие времена не сравниваем:
    /// при единицах миллисекунд отношение — шум.
    /// </summary>
    private static void Linear(string what, double t1000, double t5000)
    {
        Console.WriteLine($"        {what}: 1000 — {t1000:0} мс, 5000 — {t5000:0} мс");
        if (t5000 < 40) return;
        Assert.True(t5000 < 12 * Math.Max(t1000, 3), $"{what}: 1000 — {t1000:0} мс, 5000 — {t5000:0} мс (растёт быстрее, чем число модов)");
    }

    public static void Register(TestRunner t)
    {
        t.Group("Большая библиотека: тысячи модов");

        t.Run("перечитывание гаража (папка, db.json, сверка, картинки) растёт линейно", () =>
        {
            using var small = new Big(1000);
            using var big = new Big(5000);
            var bs = small.Backend();
            var bb = big.Backend();
            bs.Load();
            bb.Load(); // первый раз — заводятся записи
            Linear("перечитывание", Best(() => bs.Load()), Best(() => bb.Load()));
            Assert.True(Best(() => bb.Load()) < 5000, "5000 модов — меньше 5 секунд даже на медленной машине");
        });

        t.Run("первый запуск: 5000 новых модов заводятся линейно", () =>
        {
            var t1 = Best(() => { using var b = new Big(1000); var sw = Stopwatch.StartNew(); b.Backend().Load(); }, runs: 1);
            using var big = new Big(5000);
            var sw = Stopwatch.StartNew();
            var snap = big.Backend().Load();
            var t5 = sw.Elapsed.TotalMilliseconds;
            Assert.Eq(5000, snap.Mods.Count);
            Assert.True(t5 < 10000, $"первый запуск {t5:0} мс");
        });

        t.Run("5000 модов: счёт в фильтрах, выключенные из db.json, типы, даты — всё на месте", () =>
        {
            using var big = new Big(5000);
            var vm = new GarageViewModel(big.Backend(), () => DateTimeOffset.UtcNow);
            vm.Refresh();
            Assert.Eq(5000, vm.Filters[0].Count);
            Assert.Eq(3750, vm.Filters[1].Count); // каждый четвёртый выключен
            Assert.Eq(5000, vm.Cards.Count);
            Assert.False(vm.AllCards.Single(c => c.FileName == Big.Name(4000)).Enabled, "4000 — выключен");
            Assert.True(vm.AllCards.Single(c => c.FileName == Big.Name(4001)).Enabled, "4001 — включён");
            Assert.Eq(DateTimeOffset.FromUnixTimeSeconds(1792060320), big.Store.Mods().Single(m => m.FileName == Big.Name(1)).InstalledAtUtc);
        });

        t.Run("5000 модов: переключение пишет нужную строку db.json и не трогает остальные", () =>
        {
            using var big = new Big(5000);
            var vm = new GarageViewModel(big.Backend(), () => DateTimeOffset.UtcNow);
            vm.Refresh();
            var card = vm.AllCards.Single(c => c.FileName == Big.Name(4321));
            var sw = Stopwatch.StartNew();
            vm.Toggle(card);
            Assert.True(sw.ElapsedMilliseconds < 5000, $"переключение {sw.ElapsedMilliseconds} мс");
            var db = GameModDb.Read(GameModDb.PathIn(big.Mods));
            Assert.False(db.Find(Big.Name(4321))!.Active, "выключен");
            Assert.Eq(3749, db.Mods.Count(m => m.Active));
            Assert.Eq(3749, vm.Filters[1].Count);
        });

        t.Run("поиск и фильтры по 5000 карточкам — линейно, находится ровно нужное", () =>
        {
            using var small = new Big(1000);
            using var big = new Big(5000);
            var vs = new GarageViewModel(small.Backend(), () => DateTimeOffset.UtcNow);
            var vb = new GarageViewModel(big.Backend(), () => DateTimeOffset.UtcNow);
            vs.Refresh();
            vb.Refresh();
            void Cycle(GarageViewModel v)
            {
                v.SearchText = "мод_0042";
                v.SearchText = "";
                v.Filter = GarageFilter.Enabled;
                v.Filter = GarageFilter.All;
            }
            Linear("поиск и фильтры", Best(() => Cycle(vs)), Best(() => Cycle(vb)));
            vb.SearchText = "мод_0042";
            Assert.Eq(10, vb.Cards.Count); // 00420…00429
            vb.SearchText = "";
            vb.Filter = GarageFilter.Enabled;
            Assert.Eq(3750, vb.Cards.Count);
        });

        t.Run("раздел «Обновления» строится из 5000 карточек линейно", () =>
        {
            using var small = new Big(1000);
            using var big = new Big(5000);
            double Build(Big b)
            {
                var g = new GarageViewModel(b.Backend(), () => DateTimeOffset.UtcNow);
                g.Refresh();
                return Best(() => _ = new UpdatesViewModel(g, new NoUpdates(), new SettingsViewModel(new AppSettings()), null, () => DateTimeOffset.UtcNow));
            }
            Linear("«Обновления»", Build(small), Build(big));
        });

        t.Run("список обновлений по 5000 модам с привязками и проверками — линейно", () =>
        {
            InMemoryLibraryStore Fill(int n)
            {
                var s = new InMemoryLibraryStore();
                var now = DateTimeOffset.UtcNow;
                for (var i = 0; i < n; i++)
                {
                    s.UpsertMod(new InstalledMod($"m{i}", $"m{i}", $"m{i}.zip", "1.0", 100, now));
                    s.UpsertBinding(new ModBinding($"m{i}", "modland", $"m{i}", new Uri($"https://www.modland.net/beamng.drive-mods/cars/m{i}.html"), true));
                    s.SaveCheck(new CheckState($"m{i}", "modland", now, now, i % 5 == 0 ? "1.1" : "1.0", 100));
                }
                return s;
            }
            var s1 = Fill(1000);
            var s5 = Fill(5000);
            Linear("список обновлений", Best(() => UpdateCheckService.Pending(s1)), Best(() => UpdateCheckService.Pending(s5)));
            Assert.Eq(1000, UpdateCheckService.Pending(s5).Count);
        });

        t.Run("библиотека на 5000 модов с привязками: сохраняется и читается быстро, без потерь", () =>
        {
            using var big = new Big(0);
            var path = Path.Combine(big.Root, "lib.json");
            var store = new JsonLibraryStore(path);
            var now = DateTimeOffset.UtcNow;
            using (store.Batch())
                for (var i = 0; i < 5000; i++)
                {
                    store.UpsertMod(new InstalledMod($"m{i}", $"Мод {i}", $"m{i}.zip", "1.0", 100, now));
                    store.UpsertBinding(new ModBinding($"m{i}", "stmods", $"m{i}", new Uri($"https://stmods.org/beamng_drive/mods/m{i}/"), true));
                    store.SaveCheck(new CheckState($"m{i}", "stmods", now, now, "1.0", 100));
                }
            var sw = Stopwatch.StartNew();
            store.Flush();
            var save = sw.ElapsedMilliseconds;
            sw.Restart();
            var back = new JsonLibraryStore(path);
            Assert.Eq(5000, back.Mods().Count);
            Assert.Eq(5000, back.Bindings().Count);
            Assert.Eq("1.0", back.GetCheck("m4999", "stmods")!.LastVersion);
            var load = sw.ElapsedMilliseconds;
            Assert.True(save < 5000 && load < 5000, $"сохранение {save} мс, чтение {load} мс");
            Assert.True(new FileInfo(path).Length < 20 * 1024 * 1024, "файл разумного размера");
        });

        t.Run("db.json на 20000 записей: поиск каждой — по словарю (раньше был перебор)", () =>
        {
            var sb = new StringBuilder("{\"header\":{\"version\":1.1},\"mods\":{");
            for (var i = 0; i < 20000; i++)
                sb.Append(i > 0 ? "," : "").Append($"\"m{i}\":{{\"active\":true,\"filename\":\"M{i}.zip\",\"fullpath\":\"/mods/M{i}.zip\"}}");
            var db = GameModDb.Parse(sb.Append("}}").ToString());
            var sw = Stopwatch.StartNew();
            for (var i = 0; i < 20000; i++) Assert.True(db.Find($"M{i}.zip") is not null, "найден");
            Assert.True(sw.ElapsedMilliseconds < 2000, $"{sw.ElapsedMilliseconds} мс");
            Assert.True(db.Find("нет.zip") is null, "чужого нет");
            Assert.Eq("m7", db.Find("m7.ZIP")!.Key);
        });

        t.Run("db.json: при повторах побеждает первая запись, как раньше; поиск по имени файла, если ключ другой", () =>
        {
            var db = GameModDb.Parse("""
            {"header":{"version":1.1},"mods":{
              "a":{"active":false,"filename":"A.zip","fullpath":"/mods/A.zip"},
              "a2":{"active":true,"filename":"A.zip","fullpath":"/mods/repo/A.zip"},
              "other_key":{"active":false,"filename":"Странный Ключ.zip","fullpath":"/mods/Странный Ключ.zip"}
            }}
            """);
            Assert.False(db.Find("A.zip")!.Active, "первая");
            Assert.Eq("other_key", db.Find("странный ключ.zip")!.Key);
        });

        t.Group("Большая библиотека: картинки и даты файлов");

        t.Run("дата файла из списка папки совпадает с отдельным запросом: кэш картинок узнаёт архив", () =>
        {
            using var big = new Big(50);
            File.SetLastWriteTimeUtc(Path.Combine(big.Mods, Big.Name(3)), new DateTime(2025, 1, 2, 3, 4, 5, 678, DateTimeKind.Utc));
            foreach (var d in big.Inst.ScanDetailed())
            {
                var direct = ArchiveStamp.Of(big.Inst.PathOf(d));
                Assert.Eq(direct, new ArchiveStamp(d.FileName, d.SizeBytes, d.ModifiedUtc!.Value));
            }
        });

        t.Run("снимок кэша картинок отвечает так же, как проверка по одному; нет папки — всё «нет в кэше»", () =>
        {
            using var big = new Big(0);
            var cache = new PreviewCache(Path.Combine(big.Root, "previews"));
            var none = cache.Snapshot();
            var stamps = Enumerable.Range(0, 300).Select(i => new ArchiveStamp($"m{i}.zip", i, new DateTime(2026, 1, 1).AddMinutes(i))).ToList();
            Assert.True(stamps.All(s => none(s) == CacheState.Missing), "папки нет");
            for (var i = 0; i < 300; i += 3) cache.PutImage(stamps[i], new byte[] { 1 });
            for (var i = 1; i < 300; i += 3) cache.PutNoImage(stamps[i]);
            var snap = cache.Snapshot();
            foreach (var s in stamps) Assert.Eq(cache.State(s), snap(s));
        });

        t.Run("картинка из кэша попадает на карточку и при 5000 модах", () =>
        {
            using var big = new Big(5000);
            var zip = Path.Combine(big.Mods, Big.Name(2500));
            big.Cache.PutImage(ArchiveStamp.Of(zip), new byte[] { 1 });
            var vm = new GarageViewModel(big.Backend(), () => DateTimeOffset.UtcNow);
            vm.Refresh();
            Assert.Eq(big.Cache.ImagePath(ArchiveStamp.Of(zip)), vm.AllCards.Single(c => c.FileName == Big.Name(2500)).PreviewPath);
            Assert.Eq(1, vm.AllCards.Count(c => c.PreviewPath is not null));
        });

        t.Group("Большая библиотека: проверка обновлений");

        t.Run("5000 модов на сайте: планировщик не открывает тысячи карточек за раз, сайт не заваливаем", async () =>
        {
            var s = new InMemoryLibraryStore();
            var old = DateTimeOffset.UtcNow.AddDays(-3);
            for (var i = 0; i < 5000; i++)
            {
                s.UpsertMod(new InstalledMod($"m{i}", $"m{i}", $"m{i}.zip", "1.0", 100, old));
                s.UpsertBinding(new ModBinding($"m{i}", "x", $"m{i}", new Uri($"https://x.test/m{i}"), true));
                s.SaveCheck(new CheckState($"m{i}", "x", old, old, "1.0", 100));
            }
            var site = new CountingSite("x");
            var sw = Stopwatch.StartNew();
            var r = await UpdateRunner.RunAsync(s, new IModSource[] { site }, DateTimeOffset.UtcNow);
            Assert.True(site.Cards <= 200, $"открыто карточек: {site.Cards}");
            Assert.True(r.Sources.Single().Postponed > 0, "остальное отложено на следующие проверки");
            Assert.True(sw.ElapsedMilliseconds < 10000, $"{sw.ElapsedMilliseconds} мс");
        });
    }

    /// <summary>Сайт, который на всё отвечает «без изменений» и считает запросы.</summary>
    private sealed class CountingSite(string id) : IModSource
    {
        public int Cards;
        public string Id => id;
        public string DisplayName => id;
        public bool SupportsSearch => false;
        public Task<IReadOnlyList<Models.Listing>> FeedPageAsync(int page, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Models.Listing>>(Array.Empty<Models.Listing>());
        public Task<Models.Listing?> GetListingAsync(Uri url, CancellationToken ct = default)
        {
            Interlocked.Increment(ref Cards);
            var slug = url.AbsolutePath.Trim('/');
            return Task.FromResult<Models.Listing?>(new Models.Listing(id, url, slug, slug) { ModVersion = "1.0", SizeBytes = 100 });
        }
        public Task<IReadOnlyList<Models.Listing>> SearchAsync(string q, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Models.Listing>>(Array.Empty<Models.Listing>());
        public Task<IReadOnlyList<Models.Listing>> LatestAsync(DateTimeOffset? s, CancellationToken ct = default) => FeedPageAsync(1, ct);
        public Task<IReadOnlyList<Models.Offer>> GetOffersAsync(Models.Listing l, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Models.Offer>>(Array.Empty<Models.Offer>());
    }
}
