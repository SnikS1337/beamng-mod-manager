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
    public static ModLayout Beside(string modsDir)
    {
        var root = Path.Combine(Path.GetDirectoryName(Path.GetFullPath(modsDir).TrimEnd(Path.DirectorySeparatorChar))!, "BeamModManager");
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
}

public enum InstallError { None, BadName, Blocked, HasWarnings, AlreadyExists, Io }

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

    public ModInstaller(ModLayout layout, Func<DateTimeOffset>? now = null, int keepBackups = 3)
    {
        _layout = layout;
        _now = now ?? (() => DateTimeOffset.UtcNow);
        _keepBackups = keepBackups;
    }

    public static string? SafeFileName(string? name)
    {
        if (string.IsNullOrWhiteSpace(name)) return null;
        var n = name.Replace('\\', '/');
        n = n[(n.LastIndexOf('/') + 1)..].Trim();
        if (n.Length == 0 || n.StartsWith('.') || n.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0) return null;
        if (!n.EndsWith(".zip", StringComparison.OrdinalIgnoreCase)) return null;
        return n;
    }

    public InstallResult Install(Stream archive, string fileName, InstallOptions? options = null)
    {
        options ??= new InstallOptions();
        var name = SafeFileName(fileName);
        if (name is null) return InstallResult.Fail(InstallError.BadName, "Недопустимое имя файла (нужен .zip без путей).");

        Directory.CreateDirectory(_layout.ModsDir);
        var target = Path.Combine(_layout.ModsDir, name);
        if (File.Exists(target) && !options.ReplaceExisting)
            return InstallResult.Fail(InstallError.AlreadyExists, "Такой файл уже установлен.");

        var temp = Path.Combine(_layout.ModsDir, $".{Guid.NewGuid():N}.part");
        try
        {
            using (var fs = new FileStream(temp, FileMode.CreateNew, FileAccess.Write))
                archive.CopyTo(fs);

            InspectionResult inspection;
            using (var fs = File.OpenRead(temp))
                inspection = ArchiveInspector.Inspect(fs, options.Limits);

            if (inspection.IsBlocked)
                return InstallResult.Fail(InstallError.Blocked, "Архив не прошёл проверку безопасности.", inspection);
            if (inspection.HasWarnings && !options.AllowWarnings)
                return InstallResult.Fail(InstallError.HasWarnings, "В архиве есть предупреждения, нужно решение пользователя.", inspection);

            string hash;
            using (var fs = File.OpenRead(temp))
                hash = Convert.ToHexString(SHA256.HashData(fs)).ToLowerInvariant();

            if (File.Exists(target)) BackupFile(target, name);
            File.Move(temp, target, overwrite: true);
            return new InstallResult(true, InstallError.None, null, target, hash, inspection);
        }
        catch (IOException ex)
        {
            return InstallResult.Fail(InstallError.Io, ex.Message);
        }
        catch (UnauthorizedAccessException ex)
        {
            return InstallResult.Fail(InstallError.Io, ex.Message);
        }
        finally
        {
            if (File.Exists(temp)) File.Delete(temp);
        }
    }

    public OpResult Disable(string fileName) => Move(fileName, _layout.ModsDir, _layout.DisabledDir, "отключён");
    public OpResult Enable(string fileName) => Move(fileName, _layout.DisabledDir, _layout.ModsDir, "включён");

    /// <summary>Удаление не окончательное: файл уходит в резервные копии.</summary>
    public OpResult Remove(string fileName)
    {
        var name = SafeFileName(fileName);
        if (name is null) return new OpResult(false, "Недопустимое имя файла.");
        foreach (var dir in new[] { _layout.ModsDir, _layout.DisabledDir })
        {
            var p = Path.Combine(dir, name);
            if (!File.Exists(p)) continue;
            var backup = BackupFile(p, name, move: true);
            return new OpResult(true, "Удалён, копия в резервных.", backup);
        }
        return new OpResult(false, "Файл не найден.");
    }

    /// <summary>Установленные архивы: имя и включён ли.</summary>
    public IReadOnlyList<(string FileName, bool Enabled)> Scan()
    {
        var list = new List<(string, bool)>();
        foreach (var (dir, on) in new[] { (_layout.ModsDir, true), (_layout.DisabledDir, false) })
            if (Directory.Exists(dir))
                list.AddRange(Directory.EnumerateFiles(dir, "*.zip").Select(f => (Path.GetFileName(f), on)));
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
        var last = Backups(name).LastOrDefault();
        if (last is null) return new OpResult(false, "Резервных копий нет.");
        var target = Path.Combine(_layout.ModsDir, name);
        if (File.Exists(target)) BackupFile(target, name);
        Directory.CreateDirectory(_layout.ModsDir);
        File.Move(last, target, overwrite: true);
        return new OpResult(true, "Восстановлено.", target);
    }

    private OpResult Move(string fileName, string from, string to, string verb)
    {
        var name = SafeFileName(fileName);
        if (name is null) return new OpResult(false, "Недопустимое имя файла.");
        var src = Path.Combine(from, name);
        var dst = Path.Combine(to, name);
        if (!File.Exists(src)) return new OpResult(false, "Файл не найден.");
        if (File.Exists(dst)) return new OpResult(false, "В целевой папке уже есть файл с таким именем.");
        Directory.CreateDirectory(to);
        File.Move(src, dst);
        return new OpResult(true, $"Мод {verb}.", dst);
    }

    private string BackupFolderFor(string name) => Path.Combine(_layout.BackupDir, name);

    private string BackupFile(string path, string name, bool move = false)
    {
        var dir = BackupFolderFor(name);
        Directory.CreateDirectory(dir);
        var stamp = _now().UtcDateTime.ToString("yyyyMMdd-HHmmss-fff");
        var dst = Path.Combine(dir, $"{stamp}_{name}");
        var i = 1;
        while (File.Exists(dst)) dst = Path.Combine(dir, $"{stamp}-{i++}_{name}");
        if (move) File.Move(path, dst); else File.Copy(path, dst);

        var all = Directory.EnumerateFiles(dir).OrderBy(f => f, StringComparer.Ordinal).ToList();
        foreach (var old in all.Take(Math.Max(0, all.Count - _keepBackups))) File.Delete(old);
        return dst;
    }
}
