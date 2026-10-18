using System.IO.Compression;
using System.Security.Cryptography;
using System.Text.RegularExpressions;
using BeamModManager.Core.Library;
using BeamModManager.Core.Packs;
using BeamModManager.Core.Safety;

namespace BeamModManager.Core.Install;

/// <summary>
/// Распаковщик RAR и 7z (в окне — tar из Windows). Распаковывает в пустую папку; null — получилось, иначе понятная
/// причина. Обязан не выходить из папки и прерваться, если распакованное больше <paramref name="maxBytes"/>.
/// </summary>
public interface IArchiveExtractor
{
    string? Extract(string archivePath, string targetDir, long maxBytes, CancellationToken ct);
}

/// <summary>Что делать с модом из пришедшего архива.</summary>
public enum IncomingKind
{
    /// <summary>Такого мода нет: встанет новым.</summary>
    New,
    /// <summary>Новая версия установленного (<see cref="IncomingMod.Replaces"/>): старый файл уйдёт в копии.</summary>
    Update,
    /// <summary>Ровно этот файл уже стоит: ставить нечего.</summary>
    AlreadyInstalled,
}

/// <summary>Мод, найденный в пришедшем архиве: готовый zip (исходный или собранный во временной папке) и что с ним будет.</summary>
public sealed record IncomingMod(
    string ZipPath,
    string FileName,
    long Size,
    ArchiveOverview Overview,
    ArchiveContent Content,
    IncomingKind Kind,
    string? Replaces,
    bool ReplacesEnabled,
    /// <summary>Архив пришлось перепаковать: файлы лежали в лишней папке, и игра бы их не увидела.</summary>
    bool Repacked)
{
    /// <summary>Проверка запретила ставить (повреждён, опасный путь, исполняемые файлы, бомба).</summary>
    public bool Blocked => Overview.Broken || Overview.Findings.Any(f => f.Severity == Severity.Block);

    /// <summary>Как назвать мод человеку: название машины или карты из info.json, иначе имя файла без .zip.</summary>
    public string Title => Overview.Vehicles.Count + Overview.Levels.Count == 1
        ? Overview.Vehicles.Concat(Overview.Levels).First()
        : Path.GetFileNameWithoutExtension(FileName);
}

/// <summary>Пришедший архив целиком: моды в нём или причина, почему ставить нечего.</summary>
public sealed record IncomingArchive(string SourcePath, IReadOnlyList<IncomingMod> Mods, string? Problem)
{
    /// <summary>Временная папка разбора (распакованное, перепакованное); убрать, когда архив больше не нужен.</summary>
    public string? WorkDir { get; init; }

    public static IncomingArchive Fail(string source, string why) => new(source, Array.Empty<IncomingMod>(), why);
}

/// <summary>Установленный мод для сопоставления: по имени, по составу (машины и карты) и, при равном размере, по содержимому.</summary>
public sealed record InstalledArchive(string FileName, bool Enabled, long Size, string Path, ArchiveContent Content);

/// <summary>
/// Разбор архива, который человек скачал или перетащил на окно (2026-10-18): какие в нём моды BeamNG и чем каждый станет —
/// новым модом, обновлением установленного или «уже стоит».
/// <list type="bullet">
/// <item>zip, который сам мод, — как есть;</item>
/// <item>zip с модами-zip внутри (так часто выкладывают паки) — каждый внутренний отдельно;</item>
/// <item>zip, где всё лежит в лишней папке, — перепаковывается без неё (игра такой мод не видит);</item>
/// <item>RAR и 7z распаковываются (<see cref="IArchiveExtractor"/>), внутри ищутся zip или папка мода, которую собираем в zip.</item>
/// </list>
/// Исходный файл не меняется; всё, что собрано, лежит в <c>workDir</c> (его убирает вызывающий).
/// </summary>
public static class IncomingAnalyzer
{
    public static readonly IReadOnlyList<string> Extensions = new[] { ".zip", ".rar", ".7z" };

    /// <summary>Больше этого RAR и 7z не распаковываем (моды бывают в пару гигабайт, не в десятки).</summary>
    public const long MaxExtractBytes = 6L * 1024 * 1024 * 1024;

    /// <summary>Сколько вложенных zip берём из одного архива (пак из сотни машин — это уже не «скачал мод»).</summary>
    public const int MaxMods = 64;

    /// <summary>Похоже на архив мода по имени: zip, rar или 7z, не недокачанный браузером.</summary>
    public static bool IsCandidate(string path)
    {
        var ext = System.IO.Path.GetExtension(path);
        return Extensions.Contains(ext, StringComparer.OrdinalIgnoreCase) && !System.IO.Path.GetFileName(path).StartsWith('.');
    }

    public static IncomingArchive Analyze(string path, IReadOnlyList<InstalledArchive> installed, string workDir,
        IArchiveExtractor? extractor, CancellationToken ct = default) =>
        AnalyzeCore(path, installed, workDir, extractor, ct) with { WorkDir = workDir };

    private static IncomingArchive AnalyzeCore(string path, IReadOnlyList<InstalledArchive> installed, string workDir,
        IArchiveExtractor? extractor, CancellationToken ct)
    {
        var ext = System.IO.Path.GetExtension(path).ToLowerInvariant();
        if (!File.Exists(path)) return IncomingArchive.Fail(path, "Файл не найден.");
        try
        {
            Directory.CreateDirectory(workDir);
            List<(string Zip, string Name, bool Repacked)> zips;
            if (ext == ".zip") zips = FromZip(path, workDir, ct);
            else if (ext is ".rar" or ".7z")
            {
                if (extractor is null) return IncomingArchive.Fail(path, "RAR и 7z здесь не открываются.");
                var dir = System.IO.Path.Combine(workDir, "unpacked");
                Directory.CreateDirectory(dir);
                var err = extractor.Extract(path, dir, MaxExtractBytes, ct);
                if (err is not null) return IncomingArchive.Fail(path, err);
                zips = FromFolder(dir, StemOf(path), workDir, ct);
            }
            else return IncomingArchive.Fail(path, "Это не архив мода (нужен zip, rar или 7z).");

            if (zips.Count == 0)
                return IncomingArchive.Fail(path, "В архиве нет мода BeamNG: нет папок vehicles, levels и других, которые ждёт игра.");
            var mods = new List<IncomingMod>();
            var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            foreach (var (zip, name, repacked) in zips.Take(MaxMods))
            {
                ct.ThrowIfCancellationRequested();
                var file = UniqueName(name, names);
                var overview = ArchiveOverviewReader.Read(zip);
                ArchiveContent content;
                using (var f = File.OpenRead(zip)) content = PackContentReader.Read(f);
                var size = new FileInfo(zip).Length;
                var (kind, replaces, enabled) = Match(zip, file, size, content, installed);
                mods.Add(new IncomingMod(zip, file, size, overview, content, kind, replaces, enabled, repacked));
            }
            return new IncomingArchive(path, mods, null);
        }
        catch (OperationCanceledException) { throw; }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or InvalidDataException)
        {
            return IncomingArchive.Fail(path, "Архив не читается: " + e.Message);
        }
    }

    // ───────────── откуда берутся zip ─────────────

    private static List<(string, string, bool)> FromZip(string path, string workDir, CancellationToken ct)
    {
        var name = CleanName(System.IO.Path.GetFileName(path));
        using var fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
        ZipArchive zip;
        try { zip = new ZipArchive(fs, ZipArchiveMode.Read, leaveOpen: true); }
        catch (InvalidDataException) { return new() { (path, name, false) }; } // повреждённый: проверка скажет, что именно
        using (zip)
        {
            var shape = ShapeOf(zip.Entries.Select(e => e.FullName));
            if (shape.IsMod) return new() { (path, name, false) };
            // моды-zip внутри
            var inner = zip.Entries.Where(e => e.FullName.EndsWith(".zip", StringComparison.OrdinalIgnoreCase) && !ArchiveInspector.IsUnsafeEntryPath(e.FullName))
                .Take(MaxMods).ToList();
            if (inner.Count > 0)
            {
                var list = new List<(string, string, bool)>();
                var dir = System.IO.Path.Combine(workDir, "inner");
                Directory.CreateDirectory(dir);
                var i = 0;
                foreach (var e in inner)
                {
                    ct.ThrowIfCancellationRequested();
                    if (e.Length > MaxExtractBytes) continue;
                    var target = System.IO.Path.Combine(dir, $"{i++}.zip");
                    using (var src = e.Open())
                    using (var dst = File.Create(target)) CopyLimited(src, dst, MaxExtractBytes, ct);
                    list.AddRange(Normalize(target, CleanName(e.Name), workDir, ct));
                }
                return list;
            }
            if (shape.WrapFolder is { } wrap)
                return new() { (Repack(zip.Entries, wrap, System.IO.Path.Combine(workDir, "repacked-" + Guid.NewGuid().ToString("N")[..8] + ".zip"), ct), name, true) };
            return new();
        }
    }

    /// <summary>Внутренний zip: мод как есть, мод в лишней папке (перепаковать) или не мод (пропустить).</summary>
    private static IEnumerable<(string, string, bool)> Normalize(string zipPath, string name, string workDir, CancellationToken ct)
    {
        using var fs = File.OpenRead(zipPath);
        ZipArchive zip;
        try { zip = new ZipArchive(fs, ZipArchiveMode.Read, leaveOpen: true); }
        catch (InvalidDataException) { return new[] { (zipPath, name, false) }; }
        using (zip)
        {
            var shape = ShapeOf(zip.Entries.Select(e => e.FullName));
            if (shape.IsMod) return new[] { (zipPath, name, false) };
            if (shape.WrapFolder is { } wrap)
                return new[] { (Repack(zip.Entries, wrap, zipPath + ".repacked.zip", ct), name, true) };
            return Array.Empty<(string, string, bool)>();
        }
    }

    /// <summary>Распакованный RAR или 7z: zip внутри (как файлы) или папка мода, которую собираем в zip.</summary>
    private static List<(string, string, bool)> FromFolder(string dir, string stem, string workDir, CancellationToken ct)
    {
        var zips = Directory.EnumerateFiles(dir, "*.zip", new EnumerationOptions { RecurseSubdirectories = true, MaxRecursionDepth = 4, IgnoreInaccessible = true })
            .OrderBy(p => p, StringComparer.OrdinalIgnoreCase).Take(MaxMods).ToList();
        if (zips.Count > 0)
        {
            var list = new List<(string, string, bool)>();
            foreach (var z in zips)
            {
                ct.ThrowIfCancellationRequested();
                list.AddRange(Normalize(z, CleanName(System.IO.Path.GetFileName(z)), workDir, ct));
            }
            return list;
        }
        // папка, в которой лежат vehicles, levels…: сам распакованный корень или папка не глубже трёх уровней
        var root = FindModFolder(dir, 3);
        if (root is null) return new();
        var name = root == dir ? stem : new DirectoryInfo(root).Name;
        var target = System.IO.Path.Combine(workDir, "folder-" + Guid.NewGuid().ToString("N")[..8] + ".zip");
        ZipFolder(root, target, ct);
        return new() { (target, CleanName(name + ".zip"), root != dir) };
    }

    private static string? FindModFolder(string dir, int depth)
    {
        var queue = new Queue<(string Dir, int Depth)>();
        queue.Enqueue((dir, 0));
        while (queue.Count > 0)
        {
            var (d, level) = queue.Dequeue();
            var subs = Directory.GetDirectories(d);
            if (subs.Any(s => ArchiveInspector.IsKnownRoot(System.IO.Path.GetFileName(s)))) return d;
            if (level < depth) foreach (var s in subs.OrderBy(x => x, StringComparer.OrdinalIgnoreCase)) queue.Enqueue((s, level + 1));
        }
        return null;
    }

    private static void ZipFolder(string root, string target, CancellationToken ct)
    {
        using var zip = ZipFile.Open(target, ZipArchiveMode.Create);
        foreach (var file in Directory.EnumerateFiles(root, "*", new EnumerationOptions { RecurseSubdirectories = true, IgnoreInaccessible = true }))
        {
            ct.ThrowIfCancellationRequested();
            var rel = System.IO.Path.GetRelativePath(root, file).Replace('\\', '/');
            if (ArchiveInspector.IsUnsafeEntryPath(rel)) continue;
            zip.CreateEntryFromFile(file, rel, CompressionLevel.Optimal);
        }
    }

    /// <summary>Перепаковка без лишней верхней папки: «Мод/vehicles/…» → «vehicles/…».</summary>
    private static string Repack(IEnumerable<ZipArchiveEntry> entries, string wrap, string target, CancellationToken ct)
    {
        var prefix = wrap + "/";
        using var dst = ZipFile.Open(target, ZipArchiveMode.Create);
        foreach (var e in entries)
        {
            ct.ThrowIfCancellationRequested();
            var full = e.FullName.Replace('\\', '/');
            if (!full.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)) continue;
            var rel = full[prefix.Length..];
            if (rel.Length == 0 || ArchiveInspector.IsUnsafeEntryPath(rel)) continue;
            var ne = dst.CreateEntry(rel, CompressionLevel.Optimal);
            ne.LastWriteTime = e.LastWriteTime;
            if (rel.EndsWith('/')) continue;
            using var s = e.Open();
            using var d = ne.Open();
            CopyLimited(s, d, MaxExtractBytes, ct);
        }
        return target;
    }

    /// <summary>Какой это архив: мод (есть vehicles, levels… в корне), мод в одной лишней папке или ни то ни другое.</summary>
    public static (bool IsMod, string? WrapFolder) ShapeOf(IEnumerable<string> entryNames)
    {
        // только файлы: записи-папки («Мод/») ничего не решают
        var paths = entryNames.Select(n => n.Replace('\\', '/')).Where(n => !n.EndsWith('/'))
            .Select(n => n.Trim('/')).Where(n => n.Length > 0).Select(n => n.Split('/')).ToList();
        if (paths.Count == 0) return (false, null);
        if (paths.Any(p => p.Length > 1 && ArchiveInspector.IsKnownRoot(p[0]))) return (true, null);
        var tops = paths.Select(p => p[0]).Distinct(StringComparer.OrdinalIgnoreCase).ToList();
        if (tops.Count == 1 && paths.All(p => p.Length > 1) && paths.Any(p => p.Length > 2 && ArchiveInspector.IsKnownRoot(p[1])))
            return (false, tops[0]);
        return (false, null);
    }

    // ───────────── сопоставление с установленными ─────────────

    private static (IncomingKind, string?, bool) Match(string zip, string fileName, long size, ArchiveContent content, IReadOnlyList<InstalledArchive> installed)
    {
        // 1. ровно этот файл уже стоит (размер, потом содержимое)
        string? hash = null;
        foreach (var m in installed.Where(m => m.Size == size))
        {
            hash ??= Sha(zip);
            if (File.Exists(m.Path) && Sha(m.Path) == hash) return (IncomingKind.AlreadyInstalled, m.FileName, m.Enabled);
        }
        // 2. то же имя файла
        var same = installed.FirstOrDefault(m => string.Equals(m.FileName, fileName, StringComparison.OrdinalIgnoreCase));
        if (same is not null) return (IncomingKind.Update, same.FileName, same.Enabled);
        // 3. те же машины и карты: доля общего от большего из двух — не меньше половины; двух равных лучших не выбираем
        if (content.Count > 0)
        {
            var keys = content.Keys.ToHashSet(StringComparer.OrdinalIgnoreCase);
            var scored = installed.Where(m => m.Content.Count > 0)
                .Select(m => (Mod: m, Shared: m.Content.Keys.Count(keys.Contains), Score: m.Content.Keys.Count(keys.Contains) / (double)Math.Max(m.Content.Count, content.Count)))
                .Where(x => x.Shared > 0 && x.Score >= 0.5)
                .OrderByDescending(x => x.Score).ToList();
            if (scored.Count == 1 || (scored.Count > 1 && scored[0].Score > scored[1].Score))
                return (IncomingKind.Update, scored[0].Mod.FileName, scored[0].Mod.Enabled);
            if (scored.Count > 1) return (IncomingKind.New, null, true); // неоднозначно — не гадаем
        }
        // 4. то же название без версии («audi_v1.0» и «audi_v1.5»), если такой один
        var nk = LibrarySync.NameKey(fileName);
        if (nk.Length > 0)
        {
            var byName = installed.Where(m => LibrarySync.NameKey(m.FileName) == nk).ToList();
            if (byName.Count == 1) return (IncomingKind.Update, byName[0].FileName, byName[0].Enabled);
        }
        return (IncomingKind.New, null, true);
    }

    // ───────────── мелочи ─────────────

    /// <summary>Имя файла для папки модов: без «(1)», который дописывает браузер к повторной загрузке, и всегда .zip.</summary>
    public static string CleanName(string name)
    {
        var stem = System.IO.Path.GetFileNameWithoutExtension(name);
        stem = Regex.Replace(stem, @"\s*\(\d{1,3}\)$", "").Trim().TrimEnd('.');
        stem = new string(stem.Select(c => WindowsInvalid.Contains(c) || c < 32 ? '_' : c).ToArray());
        if (stem.Length == 0 || stem.StartsWith('.')) stem = "mod" + stem;
        if (stem.Length > 120) stem = stem[..120].TrimEnd(' ', '.');
        return stem + ".zip";
    }

    /// <summary>Знаки, недопустимые в имени файла Windows (на любой системе, где идут тесты).</summary>
    private const string WindowsInvalid = "\\/:*?\"<>|";

    private static string StemOf(string path) => System.IO.Path.GetFileNameWithoutExtension(CleanName(System.IO.Path.GetFileName(path)));

    private static string UniqueName(string name, HashSet<string> taken)
    {
        var n = ModInstaller.SafeFileName(name) ?? "mod.zip";
        var stem = n[..^4];
        for (var i = 2; !taken.Add(n); i++) n = $"{stem}_{i}.zip";
        return n;
    }

    private static void CopyLimited(Stream src, Stream dst, long max, CancellationToken ct)
    {
        var buf = new byte[81920];
        long total = 0;
        int n;
        while ((n = src.Read(buf, 0, buf.Length)) > 0)
        {
            ct.ThrowIfCancellationRequested();
            total += n;
            if (total > max) throw new InvalidDataException("распакованный файл слишком большой");
            dst.Write(buf, 0, n);
        }
    }

    private static string Sha(string path)
    {
        using var f = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete, 1 << 16);
        return Convert.ToHexString(SHA256.HashData(f));
    }
}
