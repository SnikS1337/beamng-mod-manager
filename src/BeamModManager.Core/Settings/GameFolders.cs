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

    /// <summary>
    /// Где игра хранит папку пользователя. С версии 0.37 это %LOCALAPPDATA%\BeamNG\BeamNG.drive\current
    /// (источник: официальный анонс «User Folder and Migration Changes»); раньше — %LOCALAPPDATA%\BeamNG.drive\&lt;версия&gt;.
    /// Возвращаются оба корня, новый первым.
    /// </summary>
    public static IReadOnlyList<string> DefaultUserRoots()
    {
        var local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        return new[] { Path.Combine(local, "BeamNG", "BeamNG.drive"), Path.Combine(local, "BeamNG.drive") };
    }

    /// <summary>Первый существующий корень из <see cref="DefaultUserRoots"/>; если ни одного нет — новый путь по умолчанию.</summary>
    public static string DefaultUserRoot()
    {
        var roots = DefaultUserRoots();
        return roots.FirstOrDefault(Directory.Exists) ?? roots[0];
    }

    /// <summary>
    /// Папки mods внутри папки пользователя, лучшие первыми: сама mods, затем current\mods, затем версии по убыванию
    /// (0.39 раньше 0.38, 0.100 позже 0.39 — по числам). Понимает и корень «BeamNG», внутри которого лежит BeamNG.drive.
    /// </summary>
    public static IReadOnlyList<string> FindModsFolders(string userRoot)
    {
        if (!Directory.Exists(userRoot)) return Array.Empty<string>();
        var result = new List<string>();
        void AddIfExists(string dir) { if (Directory.Exists(dir) && !result.Contains(dir)) result.Add(dir); }

        AddIfExists(Path.Combine(userRoot, "mods"));
        AddIfExists(Path.Combine(userRoot, "current", "mods"));
        var versions = Directory.EnumerateDirectories(userRoot)
            .Select(d => (Dir: d, Name: Path.GetFileName(d)))
            .Where(x => VersionDir.IsMatch(x.Name))
            .OrderByDescending(x => Version(x.Name))
            .Select(x => Path.Combine(x.Dir, "mods"));
        foreach (var v in versions) AddIfExists(v);

        // Выбрали папку «BeamNG» (родитель BeamNG.drive).
        var nested = Path.Combine(userRoot, "BeamNG.drive");
        if (result.Count == 0 && Directory.Exists(nested)) result.AddRange(FindModsFolders(nested));
        return result;
    }

    /// <summary>Путь из поля ввода: без кавычек вокруг («Копировать как путь» в проводнике их добавляет), с раскрытыми %переменными%.</summary>
    public static string CleanPath(string raw)
    {
        var p = raw.Trim();
        if (p.Length >= 2 && ((p[0] == '"' && p[^1] == '"') || (p[0] == '\'' && p[^1] == '\''))) p = p[1..^1].Trim();
        return Environment.ExpandEnvironmentVariables(p);
    }

    /// <summary>
    /// Что пользователь выбрал, то и берём: сама папка mods, либо папка пользователя / версии, внутри которой есть mods.
    /// Если подходящих папок несколько, берётся самая новая версия.
    /// </summary>
    public static FolderCheck Resolve(string? selected, bool checkWritable = true)
    {
        if (string.IsNullOrWhiteSpace(selected)) return new(FolderProblem.Empty, null, "Папка не выбрана.");
        var path = CleanPath(selected);
        if (path.Length == 0) return new(FolderProblem.Empty, null, "Папка не выбрана.");
        if (!Path.IsPathRooted(path)) return new(FolderProblem.NotFound, null, "Нужен полный путь к папке (например, C:\\Users\\...).");
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
