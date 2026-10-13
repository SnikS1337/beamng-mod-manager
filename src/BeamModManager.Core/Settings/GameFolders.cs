using System.Text.RegularExpressions;

namespace BeamModManager.Core.Settings;

public enum FolderProblem { None, Empty, NotFound, NotADirectory, NoModsFolder, NotWritable }

public sealed record FolderCheck(FolderProblem Problem, string? ModsDir, string Message)
{
    public bool Ok => Problem == FolderProblem.None;
}

/// <summary>
/// Подбор и проверка папки mods. Расположение папки пользователя BeamNG (обычно %LOCALAPPDATA%\BeamNG.drive\&lt;версия&gt;\mods)
/// пока не проверено на реальной установке: пользователь всегда может указать папку явно.
/// Мы ничего не создаём и не меняем, кроме пробного временного файла при проверке записи.
/// </summary>
public static class GameFolders
{
    private static readonly Regex VersionDir = new(@"^\d+(\.\d+)*$", RegexOptions.Compiled);

    public static string DefaultUserRoot() =>
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "BeamNG.drive");

    /// <summary>Папки mods внутри папки пользователя: версии по убыванию (0.39 раньше 0.38, 0.100 позже 0.39 по числам).</summary>
    public static IReadOnlyList<string> FindModsFolders(string userRoot)
    {
        if (!Directory.Exists(userRoot)) return Array.Empty<string>();
        var result = new List<string>();
        var direct = Path.Combine(userRoot, "mods");
        if (Directory.Exists(direct)) result.Add(direct);
        var versions = Directory.EnumerateDirectories(userRoot)
            .Select(d => (Dir: d, Name: Path.GetFileName(d)))
            .Where(x => VersionDir.IsMatch(x.Name) && Directory.Exists(Path.Combine(x.Dir, "mods")))
            .OrderByDescending(x => Version(x.Name))
            .Select(x => Path.Combine(x.Dir, "mods"));
        result.AddRange(versions);
        return result;
    }

    /// <summary>
    /// Что пользователь выбрал, то и берём: сама папка mods, либо папка пользователя / версии, внутри которой есть mods.
    /// Если подходящих папок несколько, берётся самая новая версия.
    /// </summary>
    public static FolderCheck Resolve(string? selected, bool checkWritable = true)
    {
        if (string.IsNullOrWhiteSpace(selected)) return new(FolderProblem.Empty, null, "Папка не выбрана.");
        var path = selected.Trim();
        if (File.Exists(path)) return new(FolderProblem.NotADirectory, null, "Выбран файл, а нужна папка.");
        if (!Directory.Exists(path)) return new(FolderProblem.NotFound, null, "Папка не найдена.");

        string mods;
        if (string.Equals(Path.GetFileName(path.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar)), "mods", StringComparison.OrdinalIgnoreCase))
            mods = path;
        else if (FindModsFolders(path) is { Count: > 0 } found)
            mods = found[0];
        else
            return new(FolderProblem.NoModsFolder, null, "Внутри нет папки mods. Выберите папку mods или папку пользователя BeamNG.");

        if (checkWritable && !CanWrite(mods))
            return new(FolderProblem.NotWritable, mods, "В папку mods нельзя записывать (нет прав или она только для чтения).");
        return new(FolderProblem.None, mods, "Папка подходит.");
    }

    /// <summary>Итоговая папка mods по настройкам: явная папка важнее, потом папка пользователя.</summary>
    public static FolderCheck FromSettings(AppSettings s, bool checkWritable = true)
    {
        if (s.ModsFolder is not null) return Resolve(s.ModsFolder, checkWritable);
        if (s.UserFolder is not null) return Resolve(s.UserFolder, checkWritable);
        return new(FolderProblem.Empty, null, "Папка игры не выбрана.");
    }

    private static bool CanWrite(string dir)
    {
        try
        {
            var probe = Path.Combine(dir, $".bmm-probe-{Guid.NewGuid():N}.tmp");
            File.WriteAllText(probe, "");
            File.Delete(probe);
            return true;
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return false; }
    }

    private static Version Version(string name)
    {
        // «0.39» → 0.39; слишком длинные/большие номера не должны ронять сортировку.
        var parts = name.Split('.').Select(p => int.TryParse(p, out var n) ? Math.Min(n, 100000) : 0).Take(4).ToList();
        while (parts.Count < 2) parts.Add(0);
        return new Version(parts[0], parts[1], parts.Count > 2 ? parts[2] : 0, parts.Count > 3 ? parts[3] : 0);
    }
}
