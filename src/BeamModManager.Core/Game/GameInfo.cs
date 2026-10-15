using System.Text.RegularExpressions;

namespace BeamModManager.Core.Game;

/// <summary>Сведения об установке игры, проверенные на 0.39.4 (PHASE0.md «Данные с Windows»).</summary>
public static class GameInfo
{
    private static readonly Regex LogVersion = new(@"Log started\s*-\s*v\s*(?<v>\d+(?:\.\d+){1,3})", RegexOptions.Compiled);
    private static readonly Regex AnyVersion = new(@"(?<!\d)(?<v>0\.\d+(?:\.\d+){0,2})(?!\d)", RegexOptions.Compiled);
    private static readonly Regex UserPathLine = new(@"^\s*UserPath\s*=\s*(?<p>.*?)\s*$", RegexOptions.Compiled | RegexOptions.Multiline);
    private static readonly Regex Section = new(@"^\s*\[(?<s>[^\]]+)\]\s*$", RegexOptions.Compiled | RegexOptions.Multiline);

    /// <summary>Версия из начала beamng.log: «Log started - v 0.39.4.0 - x64 - build 20972».</summary>
    public static string? VersionFromLog(string logText)
    {
        var head = logText.Length > 4096 ? logText[..4096] : logText;
        var m = LogVersion.Match(head);
        return m.Success ? m.Groups["v"].Value : null;
    }

    /// <summary>Версия из %LOCALAPPDATA%\BeamNG\BeamNG.drive.ini (игра пишет её туда при запуске; точный вид строки не важен).</summary>
    public static string? VersionFromIni(string iniText)
    {
        var m = AnyVersion.Match(iniText);
        return m.Success ? m.Groups["v"].Value : null;
    }

    /// <summary>
    /// Версия установленной игры: сначала BeamNG.drive.ini рядом с папкой пользователя, затем beamng.log в ней.
    /// <paramref name="userFolder"/> — папка пользователя (…\BeamNG\BeamNG.drive\current). Null — не нашли.
    /// </summary>
    public static string? DetectVersion(string userFolder)
    {
        try
        {
            var root = Directory.GetParent(Directory.GetParent(Path.GetFullPath(userFolder).TrimEnd('\\', '/'))?.FullName ?? "")?.FullName;
            if (root is not null)
            {
                var ini = Path.Combine(root, "BeamNG.drive.ini");
                if (File.Exists(ini) && new FileInfo(ini).Length < 64 * 1024 && VersionFromIni(File.ReadAllText(ini)) is { } v1) return v1;
            }
            var log = Path.Combine(userFolder, "beamng.log");
            if (File.Exists(log))
            {
                using var s = new FileStream(log, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
                var buf = new byte[4096];
                var n = s.Read(buf, 0, buf.Length);
                return VersionFromLog(System.Text.Encoding.UTF8.GetString(buf, 0, n));
            }
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or ArgumentException) { }
        return null;
    }

    /// <summary>
    /// Папка пользователя, заданная в startup.ini папки игры (раздел [filesystem], ключ UserPath). Пусто или файла нет — null
    /// (игра берёт место по умолчанию, GameFolders.DefaultUserRoots). Относительный путь считается от папки игры.
    /// Закомментированные строки (;UserPath = …) не учитываются.
    /// </summary>
    public static string? UserRootFromStartupIni(string gameDir)
    {
        try
        {
            var ini = Path.Combine(gameDir, "startup.ini");
            if (!File.Exists(ini) || new FileInfo(ini).Length > 64 * 1024) return null;
            return UserRootFromStartupIniText(File.ReadAllText(ini), gameDir);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or ArgumentException) { return null; }
    }

    public static string? UserRootFromStartupIniText(string text, string gameDir)
    {
        string? section = null;
        foreach (var raw in text.Split('\n'))
        {
            var line = raw.TrimEnd('\r');
            var s = Section.Match(line);
            if (s.Success) { section = s.Groups["s"].Value.Trim().ToLowerInvariant(); continue; }
            if (section != "filesystem" || line.TrimStart().StartsWith(';') || line.TrimStart().StartsWith('#')) continue;
            var m = UserPathLine.Match(line);
            if (!m.Success) continue;
            var p = m.Groups["p"].Value.Trim().Trim('"');
            if (p.Length == 0) return null;
            return Path.GetFullPath(Path.IsPathRooted(p) ? p : Path.Combine(gameDir, p));
        }
        return null;
    }
}
