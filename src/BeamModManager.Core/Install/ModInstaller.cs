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

public sealed record OpResult(bool Success, string? Message = null, string? Path = null);

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

    public IReadOnlyList<string> Backups(string fileName)
    {
        var dir = BackupFolderFor(SafeFileName(fileName) ?? "");
        return Directory.Exists(dir)
            ? Directory.EnumerateFiles(dir).OrderBy(f => f, StringComparer.Ordinal).ToList()
            : Array.Empty<string>();
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
            var all = Directory.EnumerateFiles(BackupFolderFor(name)).OrderBy(f => f, StringComparer.Ordinal).ToList();
            foreach (var old in all.Take(Math.Max(0, all.Count - _keepBackups))) File.Delete(old);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
    }
}
