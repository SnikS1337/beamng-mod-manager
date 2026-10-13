using System.IO.Compression;
using BeamModManager.Core.Install;
using BeamModManager.Core.Models;
using BeamModManager.Core.Library;
using BeamModManager.Core.Net;
using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Text;
using BeamModManager.Core.Updates;
using BeamModManager.Core.Versioning;

namespace BeamModManager.Core.Tests;

/// <summary>Второй проход ревизии: параллельные операции, резервные копии, разбор имён.</summary>
public static class Revision2Tests
{
    private static MemoryStream Mod(int size = 50)
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true))
        {
            using (var w = z.CreateEntry("vehicles/car/info.json").Open()) w.Write(new byte[20]);
            using (var w = z.CreateEntry("vehicles/car/car.jbeam").Open()) w.Write(new byte[size]);
        }
        ms.Position = 0;
        return ms;
    }

    private sealed class Recording : IPageFetcher
    {
        public readonly List<Uri> Requested = new();
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct) { Requested.Add(url); return Task.FromResult(new RawResponse(200, "<html><h1>x</h1></html>")); }
    }

    private sealed class Rendezvous : IModSource
    {
        private readonly TaskCompletionSource _mine = new(), _other;
        public Rendezvous(string id, TaskCompletionSource mine, TaskCompletionSource other) { Id = id; DisplayName = id; _mine = mine; _other = other; }
        public string Id { get; }
        public string DisplayName { get; }
        public bool SupportsSearch => false;
        public async Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default)
        {
            _mine.TrySetResult();
            // Дожидаемся, что второй сайт тоже начал работу: при последовательной проверке этого не случится.
            var finished = await Task.WhenAny(_other.Task, Task.Delay(5000, ct));
            if (finished != _other.Task) throw new TimeoutException("второй сайт не начал работу — проверка последовательная");
            return page == 1 ? new[] { new Listing(Id, new Uri($"https://{Id}.test/m"), "m", "m") { ModVersion = "1.0" } } : Array.Empty<Listing>();
        }
        public Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default) => Task.FromResult<Listing?>(null);
        public Task<IReadOnlyList<Listing>> SearchAsync(string q, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? s, CancellationToken ct = default) => FeedPageAsync(1, ct);
        public Task<IReadOnlyList<Offer>> GetOffersAsync(Listing l, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Offer>>(Array.Empty<Offer>());
    }

    /// <summary>Сайт с лентой по датам (как репозиторий BeamNG): страницы по 100, новые сверху.</summary>
    private sealed class DatedSite : IModSource
    {
        public readonly List<Listing> Items = new();   // от новых к старым
        public int FeedCalls, CardCalls;
        public DatedSite(string id) { Id = id; DisplayName = id; }
        public string Id { get; }
        public string DisplayName { get; }
        public bool SupportsSearch => false;
        public Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default)
        {
            Interlocked.Increment(ref FeedCalls);
            return Task.FromResult<IReadOnlyList<Listing>>(Items.Skip((page - 1) * 100).Take(100).ToList());
        }
        public Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default)
        {
            Interlocked.Increment(ref CardCalls);
            return Task.FromResult(Items.FirstOrDefault(l => l.Url == url));
        }
        public Task<IReadOnlyList<Listing>> SearchAsync(string q, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? s, CancellationToken ct = default) => FeedPageAsync(1, ct);
        public Task<IReadOnlyList<Offer>> GetOffersAsync(Listing l, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Offer>>(Array.Empty<Offer>());
    }

    /// <summary>Сайт без дат в ленте: лента пуста, всё решают карточки.</summary>
    private sealed class Fake100 : IModSource
    {
        public readonly HashSet<string> NewVersionFor = new();
        public int CardCalls;
        public Fake100(string id) { Id = id; DisplayName = id; }
        public string Id { get; }
        public string DisplayName { get; }
        public bool SupportsSearch => false;
        public Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<Listing>>(page == 1 ? new[] { new Listing(Id, new Uri("https://stmods.test/other"), "other", "other") { ModVersion = "9.9" } } : Array.Empty<Listing>());
        public Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default)
        {
            Interlocked.Increment(ref CardCalls);
            var id = url.AbsolutePath.Trim('/');
            return Task.FromResult<Listing?>(new Listing(Id, url, id, id) { ModVersion = NewVersionFor.Contains(id) ? "2.0" : "1.0" });
        }
        public Task<IReadOnlyList<Listing>> SearchAsync(string q, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Listing>>(Array.Empty<Listing>());
        public Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? s, CancellationToken ct = default) => FeedPageAsync(1, ct);
        public Task<IReadOnlyList<Offer>> GetOffersAsync(Listing l, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<Offer>>(Array.Empty<Offer>());
    }

    private sealed class Box : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "bmm-r2-" + Guid.NewGuid().ToString("N"));
        public readonly ModLayout Layout;
        public readonly ModInstaller Inst;
        private long _tick;
        public Box(int keep = 3)
        {
            Layout = new ModLayout(Path.Combine(Root, "mods"), Path.Combine(Root, "bmm", "disabled"), Path.Combine(Root, "bmm", "backups"));
            Inst = new ModInstaller(Layout, () => new DateTimeOffset(2026, 10, 13, 12, 0, 0, TimeSpan.Zero).AddMilliseconds(Interlocked.Increment(ref _tick)), keep);
        }
        public void Dispose() { try { Directory.Delete(Root, true); } catch { } }
        public int Copies(string name) => Scan(Layout.ModsDir, name) + Scan(Layout.DisabledDir, name);
        private static int Scan(string dir, string name) => Directory.Exists(dir) && File.Exists(Path.Combine(dir, name)) ? 1 : 0;
    }

    public static void Register(TestRunner t)
    {
        t.Group("Ревизия 2: установка и параллельность");

        t.Run("хеш установленного файла совпадает с хешем содержимого архива", () =>
        {
            using var b = new Box();
            var data = Mod(5000).ToArray();
            var r = b.Inst.Install(new MemoryStream(data), "car.zip");
            Assert.True(r.Success, r.Message ?? "");
            Assert.Eq(Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(data)).ToLowerInvariant(), r.Sha256);
            Assert.Eq(Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(File.ReadAllBytes(r.Path!))).ToLowerInvariant(), r.Sha256);
        });

        t.Run("8 одновременных установок одного имени: успешна одна, файл один, без исключений", () =>
        {
            using var b = new Box();
            var results = new InstallResult[8];
            Parallel.For(0, 8, new ParallelOptions { MaxDegreeOfParallelism = 8 }, i => results[i] = b.Inst.Install(Mod(40 + i), "car.zip"));
            Assert.Eq(1, results.Count(r => r.Success));
            Assert.Eq(7, results.Count(r => r.Error == InstallError.AlreadyExists));
            Assert.Eq(1, b.Copies("car.zip"));
            Assert.Eq(0, Directory.GetFiles(b.Layout.ModsDir, ".*.part").Length);
        });

        t.Run("одновременные включение/отключение: мод всегда ровно в одном месте", () =>
        {
            using var b = new Box();
            b.Inst.Install(Mod(), "car.zip");
            Parallel.For(0, 200, new ParallelOptions { MaxDegreeOfParallelism = 8 }, i =>
            {
                if (i % 2 == 0) b.Inst.Disable("car.zip"); else b.Inst.Enable("car.zip");
            });
            Assert.Eq(1, b.Copies("car.zip"));
        });

        t.Run("одновременное обновление и удаление: файл и резервные копии целы", () =>
        {
            using var b = new Box();
            b.Inst.Install(Mod(), "car.zip");
            var tasks = Enumerable.Range(0, 20).Select(i => Task.Run(() =>
            {
                if (i % 3 == 0) b.Inst.Remove("car.zip");
                else b.Inst.Install(Mod(30 + i), "car.zip", new InstallOptions { ReplaceExisting = true });
            })).ToArray();
            Task.WaitAll(tasks);
            Assert.True(b.Copies("car.zip") <= 1, "не больше одной копии");
            Assert.True(b.Inst.Backups("car.zip").Count <= 3, "резервных не больше лимита");
            Assert.Eq(0, Directory.GetFiles(b.Layout.ModsDir, ".*.part").Length);
        });

        t.Run("резервные копии: лимит 0 или 1 не стирает единственную копию при удалении", () =>
        {
            foreach (var keep in new[] { 0, 1 })
            {
                using var b = new Box(keep);
                b.Inst.Install(Mod(), "car.zip");
                var r = b.Inst.Remove("car.zip");
                Assert.True(r.Success, "удалён");
                Assert.Eq(1, b.Inst.Backups("car.zip").Count);
            }
        });

        t.Run("восстановление при лимите 1 не теряет данные", () =>
        {
            using var b = new Box(1);
            b.Inst.Install(Mod(30), "car.zip");
            b.Inst.Install(Mod(60), "car.zip", new InstallOptions { ReplaceExisting = true });
            var r = b.Inst.RestoreLatestBackup("car.zip");
            Assert.True(r.Success, r.Message ?? "");
            Assert.Eq(1, b.Copies("car.zip"));
            Assert.Eq(1, b.Inst.Backups("car.zip").Count);
        });

        t.Run("восстановление отключённого мода кладёт файл в отключённые, а не создаёт дубликат", () =>
        {
            using var b = new Box();
            b.Inst.Install(Mod(30), "car.zip");
            b.Inst.Install(Mod(60), "car.zip", new InstallOptions { ReplaceExisting = true });
            b.Inst.Disable("car.zip");
            var r = b.Inst.RestoreLatestBackup("car.zip");
            Assert.True(r.Success, r.Message ?? "");
            Assert.Eq(1, b.Copies("car.zip"));
            Assert.True(File.Exists(Path.Combine(b.Layout.DisabledDir, "car.zip")), "осталось среди отключённых");
        });

        t.Run("недозаписанные временные файлы убираются, свежие не трогаются", () =>
        {
            using var b = new Box();
            Directory.CreateDirectory(b.Layout.ModsDir);
            var old = Path.Combine(b.Layout.ModsDir, ".aaa.part");
            var fresh = Path.Combine(b.Layout.ModsDir, ".bbb.part");
            File.WriteAllText(old, "x"); File.WriteAllText(fresh, "x");
            var clock = new DateTime(2026, 10, 13, 12, 0, 0, DateTimeKind.Utc);
            File.SetLastWriteTimeUtc(old, clock.AddHours(-5)); File.SetLastWriteTimeUtc(fresh, clock);
            Assert.Eq(1, b.Inst.CleanupStaleTemp(TimeSpan.FromHours(1)));
            Assert.False(File.Exists(old), "старый убран");
            Assert.True(File.Exists(fresh), "свежий на месте");
        });

        t.Run("список модов не включает .zipx и прочие «похожие» расширения", () =>
        {
            using var b = new Box();
            Directory.CreateDirectory(b.Layout.ModsDir);
            File.WriteAllText(Path.Combine(b.Layout.ModsDir, "a.zip"), "x");
            File.WriteAllText(Path.Combine(b.Layout.ModsDir, "b.zipx"), "x");
            File.WriteAllText(Path.Combine(b.Layout.ModsDir, "c.zip.bak"), "x");
            Assert.Eq(1, b.Inst.Scan().Count);
        });

        t.Run("имя файла: символы, недопустимые в Windows, отклоняются на любой платформе", () =>
        {
            foreach (var bad in new[] { "a:b.zip", "a*.zip", "a?.zip", "a\"b.zip", "a<b.zip", "a>b.zip", "a|b.zip", "car.zip:stream" })
                Assert.Null(ModInstaller.SafeFileName(bad));
            Assert.Eq("Машина №1 (v2).zip", ModInstaller.SafeFileName("Машина №1 (v2).zip"));
        });

        t.Run("папка рядом с корнем диска не падает", () =>
        {
            var l = ModLayout.Beside("/");
            Assert.True(l.DisabledDir.Contains("BeamModManager"), "путь построен");
        });

        t.Group("Ревизия 2: папка игры");

        string Tmp() { var d = Path.Combine(Path.GetTempPath(), "bmm-gf-" + Guid.NewGuid().ToString("N")); Directory.CreateDirectory(d); return d; }

        t.Run("новая раскладка игры (0.37+): BeamNG.drive\\current\\mods находится из любой из трёх папок", () =>
        {
            var root = Tmp();
            var mods = Path.Combine(root, "BeamNG", "BeamNG.drive", "current", "mods");
            Directory.CreateDirectory(mods);
            Assert.Eq(mods, GameFolders.Resolve(Path.Combine(root, "BeamNG", "BeamNG.drive")).ModsDir);
            Assert.Eq(mods, GameFolders.Resolve(Path.Combine(root, "BeamNG", "BeamNG.drive", "current")).ModsDir);
            Assert.Eq(mods, GameFolders.Resolve(Path.Combine(root, "BeamNG")).ModsDir);
        });

        t.Run("путь в кавычках (как копирует проводник), с переменной окружения; относительный путь отклоняется", () =>
        {
            var d = Tmp(); var mods = Path.Combine(d, "mods"); Directory.CreateDirectory(mods);
            Assert.True(GameFolders.Resolve("\"" + mods + "\"").Ok, "кавычки");
            Environment.SetEnvironmentVariable("BMM_TEST_DIR", d);
            Assert.True(GameFolders.Resolve(Path.Combine("%BMM_TEST_DIR%", "mods")).Ok, "переменная");
            Assert.False(GameFolders.Resolve("mods").Ok, "относительный путь");
            Assert.Eq(FolderProblem.Empty, GameFolders.Resolve("\"\"").Problem);
        });

        t.Run("служебные папки в папке менеджера: у разных mods разные, внутри mods ничего нет", () =>
        {
            var a = ModLayout.InAppFolder("/g/a/mods", "/app");
            var b = ModLayout.InAppFolder("/g/b/mods", "/app");
            Assert.True(a.DisabledDir != b.DisabledDir, "разные папки");
            Assert.True(a.DisabledDir.StartsWith("/app") && a.BackupDir.StartsWith("/app"), "в папке менеджера");
            Assert.Eq(a.DisabledDir, ModLayout.InAppFolder("/g/a/mods/", "/app").DisabledDir);
        });

        t.Group("Ревизия 2: адаптеры");

        t.Run("карточка по адресу чужого сайта или не http: сайт не опрашивается, ответ «страницы нет»", async () =>
        {
            var rec = new Recording();
            var src = new ModlandSource(new PoliteFetcher(rec, new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero }));
            foreach (var u in new[] { "https://evil.example/x", "https://modland.net.evil.example/x", "http://127.0.0.1/admin", "file:///etc/passwd", "ftp://www.modland.net/x" })
                Assert.Null(await src.GetListingAsync(new Uri(u)));
            Assert.Eq(0, rec.Requested.Count);
            await src.GetListingAsync(new Uri("https://www.modland.net/some-mod"));
            await src.GetListingAsync(new Uri("https://modland.net/other-mod"));
            Assert.Eq(2, rec.Requested.Count);
        });

        t.Run("поисковый запрос из одних точек не превращается в переход по адресу", async () =>
        {
            var rec = new Recording();
            var src = new ModlandSource(new PoliteFetcher(rec, new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero }));
            Assert.Eq(0, (await src.SearchAsync("..")).Count);
            Assert.Eq(0, rec.Requested.Count);
        });

        t.Group("Ревизия 2: архивы");

        t.Run("архив с десятками тысяч файлов прямо в vehicles/: чтение не квадратичное", () =>
        {
            var ms = new MemoryStream();
            using (var z = new ZipArchive(ms, ZipArchiveMode.Create, true))
                for (var i = 0; i < 40_000; i++) z.CreateEntry($"vehicles/f{i}", CompressionLevel.NoCompression);
            ms.Position = 0;
            var sw = System.Diagnostics.Stopwatch.StartNew();
            BeamModManager.Core.Packs.PackContentReader.Read(ms);
            Assert.True(sw.Elapsed < TimeSpan.FromSeconds(5), $"заняло {sw.Elapsed.TotalSeconds:0.0} с");
        });

        t.Run("пути «./vehicles/…» понимаются как обычные; «... » и «.. » в пути — небезопасно", () =>
        {
            Stream One(string name) { var m = new MemoryStream(); using (var z = new ZipArchive(m, ZipArchiveMode.Create, true)) { using var w = z.CreateEntry(name).Open(); w.WriteByte(1); } m.Position = 0; return m; }
            var ok = BeamModManager.Core.Safety.ArchiveInspector.Inspect(One("./vehicles/car/a.jbeam"));
            Assert.False(ok.HasWarnings, "«./» не считается лишней папкой");
            Assert.True(BeamModManager.Core.Safety.ArchiveInspector.Inspect(One("vehicles/.. /x")).IsBlocked, "«.. »");
            Assert.True(BeamModManager.Core.Safety.ArchiveInspector.Inspect(One("vehicles/.../x")).IsBlocked, "«...»");
            var c = BeamModManager.Core.Packs.PackContentReader.Read(One("./vehicles/car/a.jbeam"));
            Assert.True(c.Vehicles.Contains("car"), "машина найдена");
        });

        t.Group("Ревизия 2: параллельная проверка сайтов");

        t.Run("два сайта опрашиваются одновременно, результаты обоих собраны", async () =>
        {
            var T0 = new DateTimeOffset(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);
            var store = new InMemoryLibraryStore();
            foreach (var src in new[] { "a", "b" })
            {
                store.UpsertMod(new InstalledMod("m" + src, "m" + src, "m.zip", "1.0", null, T0.AddDays(-9)));
                store.UpsertBinding(new ModBinding("m" + src, src, "m", new Uri($"https://{src}.test/m"), true));
            }
            var ta = new TaskCompletionSource(); var tb = new TaskCompletionSource();
            var report = await UpdateRunner.RunAsync(store, new IModSource[] { new Rendezvous("a", ta, tb), new Rendezvous("b", tb, ta) }, T0);
            Assert.Eq(2, report.Sources.Count);
            Assert.True(report.Sources.All(s => s.Status == SourceStatus.Ok && s.Note is null), string.Join("; ", report.Sources.Select(s => s.Note)));
            Assert.Eq(4, report.Requests); // у каждого сайта страница 1 и пустая страница 2
            Assert.Eq("1.0", store.GetCheck("ma", "a")!.LastVersion);
            Assert.Eq("1.0", store.GetCheck("mb", "b")!.LastVersion);
        });

        t.Group("Ревизия 2: заходы раз в неделю или месяц");

        var Now = new DateTimeOffset(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);

        // 1500 записей, по ~70 обновлений в сутки: новые сверху. Мод m{i} стоит в позиции pos[i].
        DatedSite Repo(int total = 1500)
        {
            var site = new DatedSite("repo");
            for (var i = 0; i < total; i++)
                site.Items.Add(new Listing("repo", new Uri($"https://repo.test/r{i}"), $"r{i}", $"r{i}")
                { ModVersion = "1.0", UpdatedAt = Now.AddMinutes(-i * 20) });
            return site;
        }

        InMemoryLibraryStore Library(int[] positions, DateTimeOffset checkedAt)
        {
            var store = new InMemoryLibraryStore();
            foreach (var pos in positions)
            {
                store.UpsertMod(new InstalledMod($"m{pos}", $"m{pos}", $"m{pos}.zip", "1.0", null, checkedAt.AddDays(-100)));
                store.UpsertBinding(new ModBinding($"m{pos}", "repo", $"r{pos}", new Uri($"https://repo.test/r{pos}"), true));
                store.SaveCheck(new CheckState($"m{pos}", "repo", checkedAt, checkedAt.AddDays(-100), "1.0", null));
            }
            return store;
        }

        t.Run("неделя без заходов: лента читается до даты последней проверки, неизменные моды без карточек, обновление найдено", async () =>
        {
            var site = Repo();
            var positions = Enumerable.Range(0, 40).Select(i => 400 + i * 25).ToArray(); // разбросаны по глубине ленты
            var store = Library(positions, Now.AddDays(-7));
            site.Items[510] = site.Items[510] with { ModVersion = "1.1", UpdatedAt = Now.AddDays(-2) }; // позиция 510 — не из наших
            // у мода в позиции 450 вышла новая версия два дня назад: в ленте он поднимается наверх
            var updated = site.Items[450] with { ModVersion = "2.0", UpdatedAt = Now.AddDays(-2) };
            site.Items.RemoveAt(450); site.Items.Insert(3, updated);
            // порядок «новые сверху» должен сохраниться: 
            var sorted = site.Items.OrderByDescending(l => l.UpdatedAt).ToList(); site.Items.Clear(); site.Items.AddRange(sorted);

            var r = await UpdateRunner.RunAsync(store, new IModSource[] { site }, Now);
            Assert.Eq(1, r.Findings.Count);
            Assert.Eq("m450", r.Findings[0].ModId);
            Assert.Eq("2.0", r.Findings[0].LatestVersion);
            Assert.True(site.CardCalls <= 3, $"карточек открыто: {site.CardCalls}"); // вместо 40
            Assert.True(site.FeedCalls <= 10, $"страниц ленты: {site.FeedCalls}");
            Assert.Eq(0, r.Sources.Sum(s => s.Postponed));
        });

        t.Run("заход каждый день: лента останавливается рано (не 10 страниц)", async () =>
        {
            var site = Repo();
            var store = Library(new[] { 5, 90, 250 }, Now.AddHours(-30));
            var r = await UpdateRunner.RunAsync(store, new IModSource[] { site }, Now);
            Assert.True(site.FeedCalls <= 3, $"страниц ленты: {site.FeedCalls}");
            Assert.Eq(0, site.CardCalls);
            Assert.Eq(0, r.Findings.Count);
        });

        t.Run("полгода без заходов, лента не достаёт до даты проверки: моды из хвоста проверяются карточками", async () =>
        {
            var site = Repo();
            var store = Library(new[] { 100, 1400 }, Now.AddDays(-180));
            var r = await UpdateRunner.RunAsync(store, new IModSource[] { site }, Now);
            Assert.True(site.FeedCalls <= 10, "не больше предела страниц");
            Assert.True(site.CardCalls >= 1, "моды дальше ленты проверены карточками");
        });

        t.Run("лента не по порядку дат или без дат: доказательства нет, моды проверяются карточками", () =>
        {
            var shuffled = Enumerable.Range(0, 100).Select(i => new FeedEntry($"r{i}", "1.0", null, Now.AddDays(-(i * 7 % 50)))).ToList();
            Assert.True(FeedCoverage.Compute(shuffled) is null, "нет покрытия");
            var undated = Enumerable.Range(0, 100).Select(i => new FeedEntry($"r{i}", "1.0", null, null)).ToList();
            Assert.True(FeedCoverage.Compute(undated) is null, "нет покрытия");
            Assert.True(FeedCoverage.Compute(shuffled.Take(10).ToList()) is null, "нет покрытия");
        });

        t.Run("закреплённые записи вверху ленты порядок не ломают", () =>
        {
            var e = new List<FeedEntry>();
            for (var i = 0; i < 3; i++) e.Add(new FeedEntry($"p{i}", "1", null, Now.AddYears(-3)));
            for (var i = 0; i < 100; i++) e.Add(new FeedEntry($"r{i}", "1", null, Now.AddHours(-i)));
            var c = FeedCoverage.Compute(e);
            Assert.True(c is not null && c > Now.AddDays(-6), "граница по хвосту, а не по закреплённым");
        });

        t.Run("месяц без заходов, сайт без дат: все 100 модов проверяются за один заход", async () =>
        {
            var site = new Fake100("stmods");
            var store = new InMemoryLibraryStore();
            for (var i = 0; i < 100; i++)
            {
                var when = Now.AddDays(-30);
                store.UpsertMod(new InstalledMod($"m{i}", $"m{i}", $"m{i}.zip", "1.0", null, when.AddDays(-50)));
                store.UpsertBinding(new ModBinding($"m{i}", "stmods", $"s{i}", new Uri($"https://stmods.test/s{i}"), true));
                store.SaveCheck(new CheckState($"m{i}", "stmods", when, when.AddDays(-50), "1.0", null));
            }
            site.NewVersionFor.Add("s77");
            var r = await UpdateRunner.RunAsync(store, new IModSource[] { site }, Now);
            Assert.Eq(100, site.CardCalls);
            Assert.Eq(1, r.Findings.Count);
            Assert.Eq("m77", r.Findings[0].ModId);
            Assert.Eq(0, r.Sources[0].Postponed);
            Assert.True(r.Complete, "проверено всё");
        });

        t.Group("Ревизия 2: большая библиотека");

        t.Run("5000 модов: список обновлений и план проверки считаются быстро", () =>
        {
            var T0 = new DateTimeOffset(2026, 10, 13, 12, 0, 0, TimeSpan.Zero);
            var store = new InMemoryLibraryStore();
            for (var i = 0; i < 5000; i++)
            {
                store.UpsertMod(new InstalledMod($"m{i}", $"mod {i}", $"m{i}.zip", "1.0", 1000 + i, T0));
                store.UpsertBinding(new ModBinding($"m{i}", i % 2 == 0 ? "a" : "b", $"s{i}", new Uri($"https://x.test/{i}"), true));
                store.SaveCheck(new CheckState($"m{i}", i % 2 == 0 ? "a" : "b", T0.AddDays(-3), T0.AddDays(-3), i % 10 == 0 ? "1.1" : "1.0", 1000 + i));
            }
            var sw = System.Diagnostics.Stopwatch.StartNew();
            var pending = UpdateCheckService.Pending(store);
            var plan = UpdateCheckService.PlanFeeds(store, T0, new BeamModManager.Core.Updates.UpdateCheckOptions());
            Assert.Eq(500, pending.Count);
            Assert.Eq(5000, plan.Due.Count);
            Assert.True(sw.Elapsed < TimeSpan.FromSeconds(10), $"заняло {sw.Elapsed.TotalSeconds:0.0} с");
        });

        t.Group("Ревизия 2: версии");

        t.Run("буква вплотную («1.2b») — исправленная версия, а не бета: обновление не пропускается", () =>
        {
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.2", "1.2b"));
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.2a", "1.2b"));
            Assert.Eq(VersionOrder.Older, VersionComparer.Compare("1.2b", "1.2"));
            Assert.Eq(VersionOrder.Same, VersionComparer.Compare("1.2b", "v1.2B"));
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.2b", "1.3"));
            // с номером или словом — по-прежнему пред-релиз
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.0b2", "1.0"));
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.0-beta", "1.0"));
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.0b1", "1.0b2"));
        });

        t.Group("Ревизия 2: разбор имён");

        t.Run("имя файла: версия через точку, «V8» — двигатель, метка сайта", () =>
        {
            var a = BindingSuggester.FromFileName("BMW_M3_E30_v1.2.zip");
            Assert.Eq("BMW M3 E30", a.Name); Assert.Eq("1.2", a.Version);
            var b = BindingSuggester.FromFileName("Ford_V8.zip");
            Assert.Eq("Ford V8", b.Name); Assert.Null(b.Version);
            var c = BindingSuggester.FromFileName("stmods_car.zip");
            Assert.Eq("car", c.Name);
            var d = BindingSuggester.FromFileName("xstmods.zip");
            Assert.Eq("xstmods", d.Name);
        });

        t.Run("заголовок: «(Premium)» и «(VIP)» в скобках — комплектация, а не метка платного", () =>
        {
            Assert.False(TitleParser.HasPaidMarker("Car Pro (Premium) 3.1"));
            Assert.False(TitleParser.HasPaidMarker("Limo (VIP)"));
            Assert.True(TitleParser.HasPaidMarker("Car [Premium]"));
            Assert.True(TitleParser.HasPaidMarker("Car (PAID)"));
        });
    }
}
