using System.Security.Cryptography;
using BeamModManager.Core.Safety;

namespace BeamModManager.Core.Install;

/// <summary>
/// Папки, которыми управляет менеджер. ModsDir задаёт пользователь (папка игры в настройках);
/// остальные лежат рядом и принадлежат менеджеру, игра их не читает.
/// Отключённый мод переезжает в DisabledDir, а db.json игры мы пока не трогаем.
/// </summary>
public sealed record ModLayout(string ModsDir, string DisabledDir, string BackupDir)
{
    /// <summary>
    /// Служебные папки в папке менеджера (например, %LOCALAPPDATA%\BeamModManager), отдельно для каждой папки mods.
    /// Рекомендуемый вариант: игра при крупных обновлениях чистит свою папку пользователя, и всё чужое там может пропасть.
    /// </summary>
    public static ModLayout InAppFolder(string modsDir, string appFolder)
    {
        var key = Convert.ToHexString(SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(
            Path.GetFullPath(modsDir).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar).ToLowerInvariant())))[..10].ToLowerInvariant();
        var root = Path.Combine(appFolder, key);
        return new ModLayout(modsDir, Path.Combine(root, "disabled"), Path.Combine(root, "backups"));
    }

    /// <summary>Рядом с папкой mods (в папке пользователя игры). Подходит для тестов; в приложении лучше <see cref="InAppFolder"/>.</summary>
    public static ModLayout Beside(string modsDir)
    {
        var full = Path.GetFullPath(modsDir).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        var parent = Path.GetDirectoryName(full) ?? full; // корень диска: родителя нет
        var root = Path.Combine(parent, "BeamModManager");
        return new ModLayout(modsDir, Path.Combine(root, "disabled"), Path.Combine(root, "backups"));
    }
}

public sealed class InstallOptions
{
    /// <summary>Ставить, даже если проверка нашла предупреждения (решение пользователя).</summary>
    public bool AllowWarnings { get; init; }
    /// <summary>Заменить уже установленный файл с таким именем (обновление). Старый уйдёт в резервную копию.</summary>
    public bool ReplaceExisting { get; init; }
    public ArchiveLimits Limits { get; init; } = new();
    /// <summary>Предел размера самого ZIP: слишком большой ответ прерываем, не заполняя диск.</summary>
    public long MaxArchiveBytes { get; init; } = 8L * 1024 * 1024 * 1024;
}

public enum InstallError { None, BadName, Blocked, HasWarnings, AlreadyExists, Io, InUse, TooLarge }

public sealed record InstallResult(
    bool Success, InstallError Error, string? Message, string? Path, string? Sha256, InspectionResult? Inspection)
{
    public static InstallResult Fail(InstallError e, string msg, InspectionResult? ins = null) => new(false, e, msg, null, null, ins);
}

/// <summary>Архив мода на диске.</summary>
/// <param name="SubFolder">Подпапка внутри mods: <see cref="ModInstaller.RepoFolder"/> для модов, скачанных в самой игре
/// из репозитория BeamNG; null — корень mods или папка выключенных.</param>
public sealed record DiskMod(string FileName, bool Enabled, long SizeBytes, string? SubFolder = null)
{
    /// <summary>Мод из репозитория игры: файлом управляет игра, менеджер его не переносит и не заменяет.</summary>
    public bool FromGameRepo => SubFolder is not null;
}

/// <summary>Итог обновления мода: установка новой версии и судьба старого файла.</summary>
public sealed record UpdateResult(InstallResult Install, bool OldRemoved, bool WasEnabled)
{
    public bool Success => Install.Success;
}

public sealed record OpResult(bool Success, string? Message = null, string? Path = null);

/// <summary>
/// Резервная копия файла мода. <see cref="Id"/> — имя файла копии, по нему копию можно вернуть. Время берётся из имени
/// (оно записано при создании копии) и нужно для показа; порядок копий задаёт <see cref="Sequence"/>.
/// </summary>
public sealed record BackupEntry(string Id, string FileName, long Sequence, DateTimeOffset? SavedAtUtc, long SizeBytes);

/// <summary>Мод, которого уже нет в папках, но копии остались («Удалённые моды»).</summary>
public sealed record DeletedMod(string FileName, int BackupCount, DateTimeOffset? LastSavedAtUtc, long LastSizeBytes);

public sealed class ModInstaller
{
    private readonly ModLayout _layout;
    private readonly Func<DateTimeOffset> _now;
    private readonly int _keepBackups;

    // Операции меняют файлы в общих папках, а вызывать их могут из разных потоков (окно, фоновое обновление, профиль).
    // Одна блокировка на процесс: проверка «файла нет» и перемещение не должны разъезжаться.
    private static readonly object Gate = new();

    private static readonly char[] InvalidNameChars = { '<', '>', ':', '"', '/', '\\', '|', '?', '*' };

    public ModInstaller(ModLayout layout, Func<DateTimeOffset>? now = null, int keepBackups = 3)
    {
        _layout = layout;
        _now = now ?? (() => DateTimeOffset.UtcNow);
        _keepBackups = Math.Max(1, keepBackups); // 0 стёр бы только что сделанную копию, а «удаление» стало бы окончательным
    }

    private static readonly HashSet<string> ReservedNames = new(StringComparer.Ordinal)
    {
        "CON", "PRN", "AUX", "NUL", "CLOCK$",
        "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    };

    public static string? SafeFileName(string? name)
    {
        if (string.IsNullOrWhiteSpace(name)) return null;
        var n = name.Replace('\\', '/');
        n = n[(n.LastIndexOf('/') + 1)..].Trim();
        if (n.Length == 0 || n.Length > 200 || n.StartsWith('.') || n.IndexOfAny(InvalidNameChars) >= 0 || n.IndexOf('\0') >= 0) return null;
        if (n.Any(c => c < 32 || c is '\u202E' or '\u200E' or '\u200F')) return null; // управляющие символы и перестановка направления текста
        if (!n.EndsWith(".zip", StringComparison.OrdinalIgnoreCase)) return null;
        var stem = n[..^4];
        if (stem.Length == 0 || stem.EndsWith(' ') || stem.EndsWith('.')) return null; // Windows тихо обрезает такие имена
        var device = stem.Split('.')[0].TrimEnd().ToUpperInvariant();
        if (ReservedNames.Contains(device)) return null; // CON, NUL, COM1 и т.п. — особые имена Windows
        return n;
    }

    public InstallResult Install(Stream archive, string fileName, InstallOptions? options = null)
    {
        options ??= new InstallOptions();
        var name = SafeFileName(fileName);
        if (name is null) return InstallResult.Fail(InstallError.BadName, "Недопустимое имя файла (нужен .zip без путей).");

        var temp = Path.Combine(_layout.ModsDir, $".{Guid.NewGuid():N}.part");
        try
        {
            Directory.CreateDirectory(_layout.ModsDir);
            // Ранняя проверка, чтобы не качать зря; окончательная — под блокировкой ниже.
            var early = CheckTarget(name, options);
            if (early is not null) return early;

            // Копирование и проверка — вне блокировки: поток может быть медленным (скачивание).
            // Хеш считается при копировании: отдельное чтение файла (до гигабайтов) не нужно.
            using var hasher = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
            using (var fs = new FileStream(temp, FileMode.CreateNew, FileAccess.Write))
            {
                var buffer = new byte[81920];
                long total = 0;
                int read;
                while ((read = archive.Read(buffer, 0, buffer.Length)) > 0)
                {
                    total += read;
                    if (total > options.MaxArchiveBytes)
                        return InstallResult.Fail(InstallError.TooLarge, "Файл больше допустимого размера, загрузка прервана.");
                    fs.Write(buffer, 0, read);
                    hasher.AppendData(buffer, 0, read);
                }
            }

            InspectionResult inspection;
            using (var fs = File.OpenRead(temp))
                inspection = ArchiveInspector.Inspect(fs, options.Limits);

            if (inspection.IsBlocked)
                return InstallResult.Fail(InstallError.Blocked, "Архив не прошёл проверку безопасности.", inspection);
            if (inspection.HasWarnings && !options.AllowWarnings)
                return InstallResult.Fail(InstallError.HasWarnings, "В архиве есть предупреждения, нужно решение пользователя.", inspection);

            var hash = Convert.ToHexString(hasher.GetHashAndReset()).ToLowerInvariant();

            lock (Gate)
            {
                var late = CheckTarget(name, options);
                if (late is not null) return late;
                var target = TargetPath(name);

                // Старый файл уходит в резервные перемещением, а не копированием: если он занят игрой, перемещение
                // не удастся и ничего лишнего не появится (копии при неудачных повторах вытесняли бы настоящие).
                string? moved = null;
                if (File.Exists(target)) moved = BackupFile(target, name, move: true);
                try
                {
                    Directory.CreateDirectory(Path.GetDirectoryName(target)!);
                    File.Move(temp, target, overwrite: false);
                }
                catch
                {
                    if (moved is not null && !File.Exists(target)) { try { File.Move(moved, target); moved = null; } catch (IOException) { } }
                    throw;
                }
                Rotate(name);
                return new InstallResult(true, InstallError.None, null, target, hash, inspection);
            }
        }
        catch (IOException ex)
        {
            return IsSharingViolation(ex)
                ? InstallResult.Fail(InstallError.InUse, "Файл занят другой программой (возможно, запущена игра). Закройте игру и повторите.")
                : InstallResult.Fail(InstallError.Io, ex.Message);
        }
        catch (UnauthorizedAccessException ex)
        {
            return InstallResult.Fail(InstallError.Io, ex.Message);
        }
        finally
        {
            try { if (File.Exists(temp)) File.Delete(temp); } catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
        }
    }

    /// <summary>
    /// Обновляет мод: ставит новую версию и убирает старый файл. Если у новой версии другое имя файла (так бывает почти всегда:
    /// «audi_v1.0.zip» → «audi_v1.5.zip»), без этого в папке остались бы обе версии включёнными, и игра получила бы два
    /// одинаковых автомобиля. Отключённый мод остаётся отключённым. Старый файл уходит в резервные копии.
    /// Если старый файл убрать не удалось (занят игрой), новая версия откатывается, чтобы не оставить две.
    /// </summary>
    public UpdateResult Update(string oldFileName, Stream archive, string newFileName, InstallOptions? options = null)
    {
        options = new InstallOptions
        {
            AllowWarnings = options?.AllowWarnings ?? false,
            Limits = options?.Limits ?? new ArchiveLimits(),
            MaxArchiveBytes = options?.MaxArchiveBytes ?? new InstallOptions().MaxArchiveBytes,
            ReplaceExisting = true, // обновление по определению заменяет
        };
        var oldName = SafeFileName(oldFileName);
        var newName = SafeFileName(newFileName);
        if (oldName is null || newName is null)
            return new UpdateResult(InstallResult.Fail(InstallError.BadName, "Недопустимое имя файла (нужен .zip без путей)."), false, true);

        bool wasEnabled;
        bool oldExists;
        lock (Gate)
        {
            var inMods = File.Exists(Path.Combine(_layout.ModsDir, oldName));
            var inDisabled = File.Exists(Path.Combine(_layout.DisabledDir, oldName));
            oldExists = inMods || inDisabled;
            wasEnabled = !inDisabled || inMods;
        }

        var sameName = string.Equals(oldName, newName, StringComparison.OrdinalIgnoreCase);
        // Файл с новым именем уже мог лежать в папке (не тот, что обновляем): его заменять нельзя молча.
        if (!sameName && (File.Exists(Path.Combine(_layout.ModsDir, newName)) || File.Exists(Path.Combine(_layout.DisabledDir, newName))))
            return new UpdateResult(InstallResult.Fail(InstallError.AlreadyExists, "Файл с именем новой версии уже есть в папке."), false, wasEnabled);

        var install = Install(archive, newName, options);
        if (!install.Success || sameName || !oldExists) return new UpdateResult(install, sameName, wasEnabled);

        lock (Gate)
        {
            var removed = Remove(oldName);
            if (!removed.Success)
            {
                // Старый не убрался: новую версию откатываем, иначе в игре окажутся обе. Файл только что создан нами, потерь нет.
                try { File.Delete(Path.Combine(_layout.ModsDir, newName)); } catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
                return new UpdateResult(InstallResult.Fail(InstallError.InUse, removed.Message ?? "Не удалось убрать старую версию."), false, wasEnabled);
            }
            if (!wasEnabled)
            {
                var off = Disable(newName);
                if (!off.Success)
                    return new UpdateResult(install with { Message = "Обновлено, но мод остался включённым: " + off.Message }, true, wasEnabled);
            }
        }
        return new UpdateResult(install, true, wasEnabled);
    }

    /// <summary>Если мод сейчас отключён, обновление заменяет отключённую копию и мод остаётся отключённым:
    /// иначе получились бы два файла с одним именем, и включить мод обратно было бы нельзя.</summary>
    private string TargetPath(string name) =>
        File.Exists(Path.Combine(_layout.DisabledDir, name)) ? Path.Combine(_layout.DisabledDir, name) : Path.Combine(_layout.ModsDir, name);

    private InstallResult? CheckTarget(string name, InstallOptions options)
    {
        var inDisabled = File.Exists(Path.Combine(_layout.DisabledDir, name));
        var inMods = File.Exists(Path.Combine(_layout.ModsDir, name));
        if (inDisabled && inMods)
            return InstallResult.Fail(InstallError.AlreadyExists, "Файл с таким именем есть и среди включённых, и среди отключённых; уберите лишнюю копию.");
        if ((inDisabled || inMods) && !options.ReplaceExisting)
            return InstallResult.Fail(InstallError.AlreadyExists, "Такой файл уже установлен.");
        return null;
    }

    /// <summary>Убирает недокачанные временные файлы, оставшиеся после сбоя или закрытия программы посреди установки.</summary>
    public int CleanupStaleTemp(TimeSpan olderThan)
    {
        var n = 0;
        if (!Directory.Exists(_layout.ModsDir)) return 0;
        foreach (var f in Directory.EnumerateFiles(_layout.ModsDir, ".*.part"))
        {
            try
            {
                if (_now().UtcDateTime - File.GetLastWriteTimeUtc(f) < olderThan) continue;
                File.Delete(f); n++;
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
        }
        return n;
    }

    /// <summary>Windows: ERROR_SHARING_VIOLATION (32) и ERROR_LOCK_VIOLATION (33) — файл открыт другой программой.</summary>
    private static bool IsSharingViolation(IOException e) => (e.HResult & 0xFFFF) is 32 or 33;

    public OpResult Disable(string fileName) => Move(fileName, _layout.ModsDir, _layout.DisabledDir, "отключён");
    public OpResult Enable(string fileName) => Move(fileName, _layout.DisabledDir, _layout.ModsDir, "включён");

    /// <summary>Удаление не окончательное: файл уходит в резервные копии.</summary>
    public OpResult Remove(string fileName)
    {
        var name = SafeFileName(fileName);
        if (name is null) return new OpResult(false, "Недопустимое имя файла.");
        lock (Gate)
        foreach (var dir in new[] { _layout.ModsDir, _layout.DisabledDir })
        {
            var p = Path.Combine(dir, name);
            if (!File.Exists(p)) continue;
            try
            {
                var backup = BackupFile(p, name, move: true);
                Rotate(name);
                return new OpResult(true, "Удалён, копия в резервных.", backup);
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException)
            {
                return new OpResult(false, IoMessage(e));
            }
        }
        return new OpResult(false, "Файл не найден.");
    }

    /// <summary>Установленные архивы: имя и включён ли.</summary>
    public IReadOnlyList<(string FileName, bool Enabled)> Scan()
    {
        var list = new List<(string, bool)>();
        foreach (var (dir, on) in new[] { (_layout.ModsDir, true), (_layout.DisabledDir, false) })
            if (Directory.Exists(dir))
                list.AddRange(Directory.EnumerateFiles(dir, "*.zip")
                    .Select(Path.GetFileName)
                    .Where(f => f!.EndsWith(".zip", StringComparison.OrdinalIgnoreCase)) // шаблон «*.zip» в Windows захватывает и «.zipx»
                    .Select(f => (f!, on)));
        return list.OrderBy(x => x.Item1, StringComparer.OrdinalIgnoreCase).ToList();
    }

    /// <summary>Подпапка mods, куда игра кладёт моды, скачанные из её репозитория.</summary>
    public const string RepoFolder = "repo";

    /// <summary>
    /// Установленные архивы с размером: корень mods, выключенные менеджером и mods/repo (скачанные в самой игре).
    /// Снимок делается под общей блокировкой: установка в этот момент не разорвёт картину.
    /// </summary>
    public IReadOnlyList<DiskMod> ScanDetailed()
    {
        lock (Gate)
        {
            var result = new List<DiskMod>();
            var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            foreach (var (name, on) in Scan())
            {
                try
                {
                    var fi = new FileInfo(Path.Combine(on ? _layout.ModsDir : _layout.DisabledDir, name));
                    if (fi.Exists && seen.Add(name)) result.Add(new DiskMod(name, on, fi.Length));
                }
                catch (Exception e) when (e is IOException or UnauthorizedAccessException) { /* файл исчез или занят: в этот раз пропускаем */ }
            }
            // игра ищет архивы во всех подпапках mods (core_modmanager: findFiles("/mods/", "*.zip", -1)), кроме распакованных;
            // различает моды по имени файла без папки, поэтому архив с тем же именем в корне главнее
            foreach (var (name, sub, path) in SubfolderZips())
            {
                if (!seen.Add(name)) continue;
                try { result.Add(new DiskMod(name, true, new FileInfo(path).Length, sub)); }
                catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
            }
            // помощник менеджера внутри игры — служебный, в гараже его нет
            result.RemoveAll(d => string.Equals(d.FileName, Game.GameBridge.ModFileName, StringComparison.OrdinalIgnoreCase) && d.SubFolder is null);
            return result.OrderBy(d => d.FileName, StringComparer.OrdinalIgnoreCase).ToList();
        }
    }

    private IEnumerable<(string Name, string Sub, string Path)> SubfolderZips()
    {
        var root = _layout.ModsDir;
        if (!Directory.Exists(root)) yield break;
        // ссылки на папки (junction, symlink) не обходим: петля на родителя увела бы обход в бесконечность; глубина ограничена
        static bool Plain(string d) { try { return (File.GetAttributes(d) & FileAttributes.ReparsePoint) == 0; } catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return false; } }
        const int MaxDepth = 8;
        var stack = new Stack<(string Dir, int Depth)>();
        try { foreach (var d in Directory.EnumerateDirectories(root).Where(Plain).OrderByDescending(x => x, StringComparer.OrdinalIgnoreCase)) stack.Push((d, 1)); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { yield break; }
        while (stack.Count > 0)
        {
            var (dir, depth) = stack.Pop();
            var rel = Path.GetRelativePath(root, dir).Replace('\\', '/');
            if (string.Equals(rel, "unpacked", StringComparison.OrdinalIgnoreCase) || rel.StartsWith("unpacked/", StringComparison.OrdinalIgnoreCase)) continue;
            List<string> files, dirs;
            try
            {
                files = Directory.EnumerateFiles(dir, "*.zip").Order(StringComparer.OrdinalIgnoreCase).ToList();
                dirs = depth >= MaxDepth ? new List<string>() : Directory.EnumerateDirectories(dir).Where(Plain).OrderByDescending(x => x, StringComparer.OrdinalIgnoreCase).ToList();
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { continue; }
            foreach (var f in files)
            {
                var name = Path.GetFileName(f);
                if (name.EndsWith(".zip", StringComparison.OrdinalIgnoreCase)) yield return (name, rel, f);
            }
            foreach (var d in dirs) stack.Push((d, depth + 1));
        }
    }

    /// <summary>Где лежит архив на диске.</summary>
    public string PathOf(DiskMod d) =>
        d.SubFolder is { } sub ? Path.Combine(_layout.ModsDir, sub, d.FileName) : Path.Combine(d.Enabled ? _layout.ModsDir : _layout.DisabledDir, d.FileName);

    /// <summary>Папки, с которыми работает установщик.</summary>
    public ModLayout Layout => _layout;

    /// <summary>Папка модов существует (иначе пустой снимок означает «диск недоступен», а не «модов нет»).</summary>
    public bool ModsFolderAvailable => Directory.Exists(_layout.ModsDir);

    public IReadOnlyList<string> Backups(string fileName)
    {
        var name = SafeFileName(fileName);
        if (name is null) return Array.Empty<string>();
        var dir = BackupFolderFor(name);
        return Directory.Exists(dir)
            ? Directory.EnumerateFiles(dir).Where(f => IsBackupOf(Path.GetFileName(f), name)).OrderBy(f => f, StringComparer.Ordinal).ToList()
            : Array.Empty<string>();
    }

    /// <summary>
    /// Копия, сделанная менеджером: «номер_время_имя». Посторонние файлы в папке копий (desktop.ini, Thumbs.db, что-то положенное
    /// руками) копиями не считаются: их нельзя «вернуть» вместо мода, и чистка лишних копий их не трогает.
    /// </summary>
    private static bool IsBackupOf(string fn, string name)
    {
        var digits = fn.TakeWhile(char.IsAsciiDigit).Count();
        return digits is > 0 and <= 15 && fn.Length > digits + 1 + name.Length && fn[digits] == '_' &&
               fn.EndsWith("_" + name, StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>Имена файлов модов, для которых в папке копий есть хотя бы одна копия.</summary>
    public IReadOnlyList<string> BackedUpFileNames()
    {
        lock (Gate)
        {
            if (!Directory.Exists(_layout.BackupDir)) return Array.Empty<string>();
            return Directory.EnumerateDirectories(_layout.BackupDir)
                .Select(d => SafeFileName(Path.GetFileName(d)))
                .Where(n => n is not null && Backups(n).Count > 0)
                .Select(n => n!)
                .OrderBy(n => n, StringComparer.OrdinalIgnoreCase).ToList();
        }
    }

    /// <summary>Копии мода, от новых к старым (так их показывает «Прошлые версии»).</summary>
    public IReadOnlyList<BackupEntry> ListBackups(string fileName)
    {
        var name = SafeFileName(fileName);
        if (name is null) return Array.Empty<BackupEntry>();
        lock (Gate)
        {
            var list = new List<BackupEntry>();
            foreach (var path in Backups(name))
            {
                try
                {
                    var fi = new FileInfo(path);
                    if (!fi.Exists) continue;
                    list.Add(ParseBackup(fi, name));
                }
                catch (Exception e) when (e is IOException or UnauthorizedAccessException) { /* копия исчезла или занята: пропускаем */ }
            }
            return list.OrderByDescending(x => x.Sequence).ToList();
        }
    }

    private static BackupEntry ParseBackup(FileInfo fi, string modName)
    {
        var fn = fi.Name;
        long seq = 0;
        DateTimeOffset? saved = null;
        var digits = fn.TakeWhile(char.IsAsciiDigit).Count();
        if (digits is > 0 and <= 15 && long.TryParse(fn.AsSpan(0, digits), out var n)) seq = n;
        const int StampLen = 19; // yyyyMMdd-HHmmss-fff
        if (digits > 0 && fn.Length >= digits + 1 + StampLen && fn[digits] == '_' &&
            DateTime.TryParseExact(fn.Substring(digits + 1, StampLen), "yyyyMMdd-HHmmss-fff", System.Globalization.CultureInfo.InvariantCulture,
                System.Globalization.DateTimeStyles.AssumeUniversal | System.Globalization.DateTimeStyles.AdjustToUniversal, out var dt))
            saved = new DateTimeOffset(dt, TimeSpan.Zero);
        return new BackupEntry(fn, modName, seq, saved, fi.Length);
    }

    /// <summary>Возвращает последнюю резервную копию на место; текущий файл сам уходит в резервные.</summary>
    public OpResult RestoreLatestBackup(string fileName)
    {
        var name = SafeFileName(fileName);
        if (name is null) return new OpResult(false, "Недопустимое имя файла.");
        lock (Gate)
        {
            var last = Backups(name).LastOrDefault();
            if (last is null) return new OpResult(false, "Резервных копий нет.");
            return RestoreFrom(name, last);
        }
    }

    /// <summary>Возвращает на место конкретную копию (по <see cref="BackupEntry.Id"/>); текущий файл с тем же именем уходит в резервные.</summary>
    public OpResult RestoreBackup(string fileName, string backupId)
    {
        var name = SafeFileName(fileName);
        if (name is null) return new OpResult(false, "Недопустимое имя файла.");
        lock (Gate)
        {
            var path = FindBackup(name, backupId);
            return path is null ? new OpResult(false, "Такой резервной копии нет.") : RestoreFrom(name, path);
        }
    }

    /// <summary>Идентификатор копии принимается только как имя файла из папки копий этого мода: путь составить из него нельзя.</summary>
    private string? FindBackup(string name, string backupId) =>
        string.IsNullOrEmpty(backupId) ? null : Backups(name).FirstOrDefault(p => string.Equals(Path.GetFileName(p), backupId, StringComparison.Ordinal));

    /// <summary>
    /// «Вернуть» прошлую версию, у которой другое имя файла (после обновления имя обычно меняется): текущий файл уходит в резервные,
    /// копия встаёт на его место с тем же состоянием «включён/отключён». Если вернуть не удалось, текущий файл остаётся как был.
    /// </summary>
    public OpResult RestoreVersion(string currentFileName, string backupFileName, string backupId)
    {
        var cur = SafeFileName(currentFileName);
        var old = SafeFileName(backupFileName);
        if (cur is null || old is null) return new OpResult(false, "Недопустимое имя файла.");
        if (string.Equals(cur, old, StringComparison.OrdinalIgnoreCase)) return RestoreBackup(old, backupId);

        lock (Gate)
        {
            var src = FindBackup(old, backupId);
            if (src is null) return new OpResult(false, "Такой резервной копии нет.");

            var curEnabled = File.Exists(Path.Combine(_layout.ModsDir, cur));
            var curDisabled = File.Exists(Path.Combine(_layout.DisabledDir, cur));
            if (curEnabled && curDisabled) return new OpResult(false, "Файл с таким именем есть и среди включённых, и среди отключённых; уберите лишнюю копию.");
            var dir = curDisabled ? _layout.DisabledDir : _layout.ModsDir;
            var target = Path.Combine(dir, old);
            // Файл с именем возвращаемой версии уже лежит в одной из папок (не копия): молча затирать нельзя.
            if (File.Exists(Path.Combine(_layout.ModsDir, old)) || File.Exists(Path.Combine(_layout.DisabledDir, old)))
                return new OpResult(false, "Файл с именем той версии уже есть в папке.");

            string? moved = null;
            try
            {
                if (curEnabled || curDisabled) moved = BackupFile(Path.Combine(dir, cur), cur, move: true);
                Directory.CreateDirectory(dir);
                try { File.Move(src, target, overwrite: false); }
                catch
                {
                    var currentPath = Path.Combine(dir, cur);
                    if (moved is not null && !File.Exists(currentPath)) { try { File.Move(moved, currentPath); } catch (IOException) { } }
                    throw;
                }
                Rotate(cur);
                Rotate(old);
                return new OpResult(true, "Восстановлено.", target);
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException)
            {
                return new OpResult(false, IoMessage(e));
            }
        }
    }

    /// <summary>
    /// Моды, которых нет ни среди включённых, ни среди отключённых, но резервные копии остались: удалённые вами (или пропавшие с диска).
    /// От новых к старым по времени последней копии. Это сырой список по папкам: в нём есть и прежние имена файлов модов,
    /// обновлённых под новым именем. Интерфейс показывает <see cref="Library.VersionHistory.Deleted"/>, где они отсеяны.
    /// </summary>
    public IReadOnlyList<DeletedMod> DeletedMods()
    {
        lock (Gate)
        {
            var result = new List<DeletedMod>();
            if (!Directory.Exists(_layout.BackupDir)) return result;
            foreach (var dir in Directory.EnumerateDirectories(_layout.BackupDir))
            {
                var name = SafeFileName(Path.GetFileName(dir));
                if (name is null) continue;
                if (File.Exists(Path.Combine(_layout.ModsDir, name)) || File.Exists(Path.Combine(_layout.DisabledDir, name))) continue;
                var list = ListBackups(name);
                if (list.Count == 0) continue;
                result.Add(new DeletedMod(name, list.Count, list[0].SavedAtUtc, list[0].SizeBytes));
            }
            return result.OrderByDescending(d => d.LastSavedAtUtc ?? DateTimeOffset.MinValue).ThenBy(d => d.FileName, StringComparer.OrdinalIgnoreCase).ToList();
        }
    }

    /// <summary>Общая часть возврата копии: текущий файл (если есть) уходит в резервные, копия встаёт на его место.</summary>
    private OpResult RestoreFrom(string name, string last)
    {
        // Мод отключён — восстанавливаем отключённую копию, иначе получился бы второй файл с тем же именем.
        var target = TargetPath(name);
        string? moved = null;
        try
        {
            if (File.Exists(target)) moved = BackupFile(target, name, move: true);
            Directory.CreateDirectory(Path.GetDirectoryName(target)!);
            try { File.Move(last, target, overwrite: false); }
            catch
            {
                if (moved is not null && !File.Exists(target)) { try { File.Move(moved, target); } catch (IOException) { } }
                throw;
            }
            Rotate(name);
            return new OpResult(true, "Восстановлено.", target);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            return new OpResult(false, IoMessage(e));
        }
    }

    private OpResult Move(string fileName, string from, string to, string verb)
    {
        var name = SafeFileName(fileName);
        if (name is null) return new OpResult(false, "Недопустимое имя файла.");
        var src = Path.Combine(from, name);
        var dst = Path.Combine(to, name);
        lock (Gate)
        try
        {
            if (!File.Exists(src)) return new OpResult(false, "Файл не найден.");
            if (File.Exists(dst)) return new OpResult(false, "В целевой папке уже есть файл с таким именем.");
            Directory.CreateDirectory(to);
            File.Move(src, dst);
            return new OpResult(true, $"Мод {verb}.", dst);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            return new OpResult(false, IoMessage(e));
        }
    }

    private static string IoMessage(Exception e) =>
        e is IOException io && IsSharingViolation(io)
            ? "Файл занят другой программой (возможно, запущена игра). Закройте игру и повторите."
            : $"Не удалось выполнить операцию с файлом: {e.Message}";

    private string BackupFolderFor(string name) => Path.Combine(_layout.BackupDir, name);

    private string BackupFile(string path, string name, bool move = false)
    {
        var dir = BackupFolderFor(name);
        Directory.CreateDirectory(dir);
        var stamp = _now().UtcDateTime.ToString("yyyyMMdd-HHmmss-fff");
        // Порядок копий задаёт порядковый номер, а не часы: после перевода часов назад новая копия не окажется «старше» прежних
        // (иначе восстановление взяло бы не ту, а лишние копии удалили бы самую свежую). Время в имени — для глаз.
        var dst = Path.Combine(dir, $"{NextSequence(dir):D10}_{stamp}_{name}");
        if (move) File.Move(path, dst); else File.Copy(path, dst);
        return dst;
    }

    private static long NextSequence(string dir)
    {
        long max = 0;
        foreach (var f in Directory.EnumerateFiles(dir))
        {
            var fn = Path.GetFileName(f);
            var digits = fn.TakeWhile(char.IsAsciiDigit).Count();
            if (digits > 0 && digits <= 15 && long.TryParse(fn.AsSpan(0, digits), out var n)) max = Math.Max(max, n);
        }
        return max + 1;
    }

    /// <summary>Лишние старые копии удаляются только после успешной операции: сбой не должен стоить настоящих копий.</summary>
    private void Rotate(string name)
    {
        try
        {
            var all = Backups(name).ToList();
            foreach (var old in all.Take(Math.Max(0, all.Count - _keepBackups))) File.Delete(old);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
    }
}
