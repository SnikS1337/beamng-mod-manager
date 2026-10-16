namespace BeamModManager.Core.Game;

/// <summary>
/// Где Steam держит игры: файл libraryfolders.vdf (формат KeyValues: строки в кавычках и фигурные скобки). Нужен, чтобы
/// найти папку игры на другом диске, а в ней startup.ini с перенесённой папкой пользователя (UserPath).
/// Читаем только пути; испорченный файл — пустой список, без исключений.
/// </summary>
public static class SteamLibraries
{
    /// <summary>BeamNG.drive в Steam.</summary>
    public const string BeamNgAppId = "284160";

    /// <summary>Папки библиотек из libraryfolders.vdf; сначала те, где Steam записал BeamNG.drive.</summary>
    public static IReadOnlyList<string> Parse(string vdf)
    {
        var root = ParseKeyValues(vdf);
        if (root is null) return Array.Empty<string>();
        var folders = root.TryGetValue("libraryfolders", out var lf) && lf is Dictionary<string, object> d ? d : root;
        var withGame = new List<string>();
        var other = new List<string>();
        foreach (var (key, v) in folders)
        {
            // старый вид файла: "1" "D:\\SteamLibrary" — путь сразу строкой
            if (v is string old && int.TryParse(key, out _) && old.Trim().Length > 0) { other.Add(old.Trim()); continue; }
            if (v is not Dictionary<string, object> lib) continue;
            if (!lib.TryGetValue("path", out var p) || p is not string path || path.Trim().Length == 0) continue;
            var hasGame = lib.TryGetValue("apps", out var apps) && apps is Dictionary<string, object> a && a.ContainsKey(BeamNgAppId);
            (hasGame ? withGame : other).Add(path.Trim());
        }
        return withGame.Concat(other).Distinct(StringComparer.OrdinalIgnoreCase).ToList();
    }

    /// <summary>Папки игры, которые есть на диске: &lt;библиотека&gt;\steamapps\common\BeamNG.drive.</summary>
    public static IReadOnlyList<string> GameDirs(string steamRoot)
    {
        var libs = new List<string> { steamRoot };
        foreach (var vdf in new[] { Path.Combine(steamRoot, "steamapps", "libraryfolders.vdf"), Path.Combine(steamRoot, "config", "libraryfolders.vdf") })
        {
            try
            {
                if (File.Exists(vdf) && new FileInfo(vdf).Length < 1024 * 1024) libs.InsertRange(0, Parse(File.ReadAllText(vdf)));
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
        }
        var result = new List<string>();
        foreach (var lib in libs.Distinct(StringComparer.OrdinalIgnoreCase))
        {
            try
            {
                var dir = Path.GetFullPath(Path.Combine(lib, "steamapps", "common", "BeamNG.drive"));
                if (Directory.Exists(dir) && !result.Contains(dir, StringComparer.OrdinalIgnoreCase)) result.Add(dir);
            }
            catch (Exception e) when (e is ArgumentException or NotSupportedException or IOException or UnauthorizedAccessException) { }
        }
        return result;
    }

    /// <summary>KeyValues → вложенные словари (значение — строка или словарь). null — файл не разобрался.</summary>
    private static Dictionary<string, object>? ParseKeyValues(string text)
    {
        var tokens = Tokens(text);
        if (tokens is null) return null;
        var i = 0;
        var root = Block(tokens, ref i, depth: 0, top: true);
        return root;
    }

    private static Dictionary<string, object>? Block(List<(bool Str, string V)> t, ref int i, int depth, bool top)
    {
        if (depth > 16) return null;
        var d = new Dictionary<string, object>(StringComparer.OrdinalIgnoreCase);
        while (i < t.Count)
        {
            if (!t[i].Str)
            {
                if (t[i].V == "}" && !top) { i++; return d; }
                return null; // «{» без ключа или лишняя «}»
            }
            var key = t[i++].V;
            if (i >= t.Count) return null;
            if (t[i].Str) { d[key] = t[i++].V; continue; }
            if (t[i].V != "{") return null;
            i++;
            var child = Block(t, ref i, depth + 1, top: false);
            if (child is null) return null;
            d[key] = child;
        }
        return top ? d : null; // вложенный блок не закрыт
    }

    private static List<(bool Str, string V)>? Tokens(string s)
    {
        var list = new List<(bool, string)>();
        for (var i = 0; i < s.Length; i++)
        {
            var c = s[i];
            if (char.IsWhiteSpace(c)) continue;
            if (c == '/' && i + 1 < s.Length && s[i + 1] == '/') { while (i < s.Length && s[i] != '\n') i++; continue; }
            if (c is '{' or '}') { list.Add((false, c.ToString())); continue; }
            if (c != '"') return null;
            var sb = new System.Text.StringBuilder();
            for (i++; ; i++)
            {
                if (i >= s.Length) return null; // строка не закрыта
                if (s[i] == '\\' && i + 1 < s.Length) { i++; sb.Append(s[i] switch { 'n' => '\n', 't' => '\t', var x => x }); continue; }
                if (s[i] == '"') break;
                sb.Append(s[i]);
            }
            list.Add((true, sb.ToString()));
        }
        return list;
    }
}
