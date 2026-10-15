using System.Diagnostics;
using System.Globalization;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace BeamModManager.Core.Game;

/// <summary>Сведения о моде из репозитория BeamNG, которые игра переносит в db.json из mod_info/&lt;ID&gt;/info.json архива.</summary>
public sealed record RepoInfo(string UniqueId, string? Version, long? ResourceId, string? Title, string? Author, DateTimeOffset? LastUpdate)
{
    /// <summary>Страница мода на beamng.com (по номеру ресурса сайт сам перенаправляет на полный адрес).</summary>
    public Uri? PageUrl => ResourceId is { } id ? new Uri($"https://beamng.com/resources/{id}/") : null;
}

/// <summary>Запись о моде в db.json игры.</summary>
public sealed record GameModEntry(
    string Key,
    string FileName,
    string? FullPath,
    bool Active,
    string ModType,
    DateTimeOffset? DateAdded,
    long? FileSize,
    RepoInfo? Repo)
{
    /// <summary>Мод лежит прямо в папке mods (а не в mods/repo, mods/unpacked и т. п.).</summary>
    public bool InModsRoot => FullPath is null || string.Equals(FullPath, "/mods/" + FileName, StringComparison.OrdinalIgnoreCase);
}

public sealed record GameDbReadResult(bool Exists, double? FormatVersion, IReadOnlyList<GameModEntry> Mods, IReadOnlyList<string> Warnings)
{
    public static readonly GameDbReadResult Missing = new(false, null, Array.Empty<GameModEntry>(), Array.Empty<string>());

    /// <summary>Запись по имени файла архива (регистр не важен). Null — игра этот файл ещё не видела.</summary>
    public GameModEntry? Find(string fileName)
    {
        var key = GameModDb.KeyOf(fileName);
        return Mods.FirstOrDefault(m => m.Key == key) ??
               Mods.FirstOrDefault(m => string.Equals(m.FileName, fileName, StringComparison.OrdinalIgnoreCase));
    }
}

public enum SetActiveOutcome
{
    /// <summary>Записано.</summary>
    Done,
    /// <summary>Всё и так было в нужном состоянии, файл не трогали.</summary>
    Unchanged,
    /// <summary>Игра запущена: она перезапишет db.json при выходе, поэтому сейчас не пишем.</summary>
    GameRunning,
    /// <summary>db.json нет (игру ни разу не запускали с этой папкой).</summary>
    NoDb,
    /// <summary>Игра этот файл ещё не видела: записи нет, флаг ставить некуда.</summary>
    NotInDb,
    /// <summary>db.json не разобрался или имеет неожиданный вид: не трогаем, чтобы не испортить.</summary>
    Unreadable,
    /// <summary>Файл изменился, пока мы его читали (игра или другой менеджер): не перезаписываем.</summary>
    ChangedMeanwhile,
    IoError,
}

public sealed record SetActiveResult(SetActiveOutcome Outcome, IReadOnlyList<string> NotInDb, string? Message = null)
{
    public bool Success => Outcome is SetActiveOutcome.Done or SetActiveOutcome.Unchanged;
}

/// <summary>
/// Файл mods/db.json игры (формат проверен на 0.39.4, PHASE0.md «Данные с Windows»). Ключ записи — имя архива без .zip
/// в нижнем регистре; включён ли мод — поле active; моды из репозитория несут modID и modData (версия на сайте и пр.).
/// Читается терпимо. Пишется только поле active, все остальные поля и неизвестные ключи сохраняются как есть; запись
/// только при закрытой игре, атомарно, с копией прежнего файла рядом (db.json.bmm-bak).
/// </summary>
public static class GameModDb
{
    public const string FileName = "db.json";
    public const string BackupSuffix = ".bmm-bak";
    private const long MaxBytes = 128L * 1024 * 1024;
    private static readonly object Gate = new();

    public static string KeyOf(string fileName)
    {
        var name = Path.GetFileName(fileName.Trim());
        if (name.EndsWith(".zip", StringComparison.OrdinalIgnoreCase)) name = name[..^4];
        return name.ToLowerInvariant();
    }

    public static string PathIn(string modsDir) => System.IO.Path.Combine(modsDir, FileName);

    public static GameDbReadResult Read(string dbPath)
    {
        string text;
        try
        {
            if (!File.Exists(dbPath)) return GameDbReadResult.Missing;
            if (new FileInfo(dbPath).Length > MaxBytes) return new(true, null, Array.Empty<GameModEntry>(), new[] { "db.json слишком большой, не читаем" });
            text = ReadShared(dbPath);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            return new(true, null, Array.Empty<GameModEntry>(), new[] { $"не удалось прочитать db.json: {e.Message}" });
        }
        return Parse(text);
    }

    public static GameDbReadResult Parse(string json)
    {
        var warnings = new List<string>();
        JsonNode? root;
        try { root = JsonNode.Parse(json, documentOptions: new JsonDocumentOptions { AllowTrailingCommas = true, CommentHandling = JsonCommentHandling.Skip }); }
        catch (JsonException) { return new(true, null, Array.Empty<GameModEntry>(), new[] { "db.json повреждён" }); }
        if (root is not JsonObject obj) return new(true, null, Array.Empty<GameModEntry>(), new[] { "db.json имеет неожиданный вид" });

        double? version = obj["header"] is JsonObject h && h["version"] is JsonValue v && v.TryGetValue<double>(out var d) ? d : null;
        if (version is { } ver && ver >= 2) warnings.Add($"db.json новой версии формата ({ver.ToString(CultureInfo.InvariantCulture)}): читаем как 1.x");

        var mods = new List<GameModEntry>();
        if (obj["mods"] is JsonObject list)
        {
            foreach (var (key, node) in list)
            {
                if (node is not JsonObject m) { warnings.Add($"запись {key}: не объект"); continue; }
                var fileName = Str(m, "filename");
                if (string.IsNullOrWhiteSpace(fileName)) { warnings.Add($"запись {key}: нет имени файла"); continue; }
                var active = m["active"] is JsonValue a && a.TryGetValue<bool>(out var b) ? b : true; // игра по умолчанию включает
                long? size = m["stat"] is JsonObject st ? Long(st, "filesize") : null;
                mods.Add(new GameModEntry(key.ToLowerInvariant(), fileName, Str(m, "fullpath"), active, Str(m, "modType") ?? "unknown",
                    Unix(Long(m, "dateAdded")), size, Repo(m)));
            }
        }
        else if (obj["mods"] is not null) warnings.Add("поле mods не объект");
        return new(true, version, mods, warnings);
    }

    private static RepoInfo? Repo(JsonObject m)
    {
        var id = Str(m, "modID");
        var data = m["modData"] as JsonObject;
        if (string.IsNullOrWhiteSpace(id) && data is null) return null;
        id ??= data is not null ? Str(data, "tagid") : null;
        if (string.IsNullOrWhiteSpace(id)) return null;
        return new RepoInfo(id.Trim(),
            data is null ? null : Clean(Str(data, "version_string")),
            data is null ? null : Long(data, "resource_id"),
            data is null ? null : Clean(Str(data, "title")),
            data is null ? null : Clean(Str(data, "username")),
            data is null ? null : Unix(Long(data, "last_update")));
    }

    /// <summary>
    /// Ставит active для нескольких архивов за одну запись. Пишет, только если игра не запущена и файл не изменился с момента
    /// чтения. Архивы, которых в db.json нет, перечисляются в <see cref="SetActiveResult.NotInDb"/>; остальные записываются.
    /// </summary>
    public static SetActiveResult SetActive(string dbPath, IReadOnlyDictionary<string, bool> wanted, Func<bool>? gameRunning = null)
    {
        gameRunning ??= GameProcess.IsRunning;
        lock (Gate)
        {
            if (gameRunning()) return new(SetActiveOutcome.GameRunning, Array.Empty<string>(), "Игра запущена: закройте её, и изменения применятся.");
            if (!File.Exists(dbPath)) return new(SetActiveOutcome.NoDb, wanted.Keys.ToList(), "Игра ещё не создала список модов: запустите её один раз.");

            string text; DateTime stamp; long length;
            try
            {
                var fi = new FileInfo(dbPath);
                stamp = fi.LastWriteTimeUtc; length = fi.Length;
                if (length > MaxBytes) return new(SetActiveOutcome.Unreadable, Array.Empty<string>(), "db.json слишком большой.");
                text = ReadShared(dbPath);
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException)
            {
                return new(SetActiveOutcome.IoError, Array.Empty<string>(), e.Message);
            }

            JsonObject? root;
            try { root = JsonNode.Parse(text, documentOptions: new JsonDocumentOptions { AllowTrailingCommas = true, CommentHandling = JsonCommentHandling.Skip }) as JsonObject; }
            catch (JsonException) { root = null; }
            if (root?["mods"] is not JsonObject mods) return new(SetActiveOutcome.Unreadable, Array.Empty<string>(), "db.json не разобрался, не трогаем его.");

            var missing = new List<string>();
            var changed = false;
            foreach (var (file, active) in wanted)
            {
                var key = KeyOf(file);
                var entry = mods[key] as JsonObject
                            ?? mods.Select(kv => kv.Value as JsonObject).FirstOrDefault(o => o is not null && string.Equals(Str(o, "filename"), Path.GetFileName(file), StringComparison.OrdinalIgnoreCase));
                if (entry is null) { missing.Add(file); continue; }
                var now = entry["active"] is JsonValue a && a.TryGetValue<bool>(out var b) ? b : (bool?)null;
                if (now == active) continue;
                entry["active"] = active;
                changed = true;
            }
            if (!changed) return new(missing.Count == wanted.Count && missing.Count > 0 ? SetActiveOutcome.NotInDb : SetActiveOutcome.Unchanged, missing);

            try
            {
                if (gameRunning()) return new(SetActiveOutcome.GameRunning, missing, "Игра запущена: закройте её, и изменения применятся.");
                var fi = new FileInfo(dbPath);
                if (fi.LastWriteTimeUtc != stamp || fi.Length != length)
                    return new(SetActiveOutcome.ChangedMeanwhile, missing, "Список модов игры изменился, пока мы его читали. Повторите.");
                File.Copy(dbPath, dbPath + BackupSuffix, overwrite: true);
                var tmp = dbPath + ".bmm-tmp";
                var json = root.ToJsonString(new JsonSerializerOptions { WriteIndented = true, Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping });
                File.WriteAllText(tmp, json, new System.Text.UTF8Encoding(false)); // как у игры: UTF-8 без BOM
                File.Move(tmp, dbPath, overwrite: true);
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException)
            {
                try { File.Delete(dbPath + ".bmm-tmp"); } catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
                return new(SetActiveOutcome.IoError, missing, e.Message);
            }
            return new(SetActiveOutcome.Done, missing);
        }
    }

    public static SetActiveResult SetActive(string dbPath, string fileName, bool active, Func<bool>? gameRunning = null) =>
        SetActive(dbPath, new Dictionary<string, bool> { [fileName] = active }, gameRunning);

    /// <summary>Читаем, не мешая игре: она может держать файл открытым.</summary>
    private static string ReadShared(string path)
    {
        using var s = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
        using var r = new StreamReader(s, detectEncodingFromByteOrderMarks: true);
        return r.ReadToEnd();
    }

    private static string? Str(JsonObject o, string k) => o[k] is JsonValue v && v.TryGetValue<string>(out var s) ? s : null;
    private static string? Clean(string? s) => string.IsNullOrWhiteSpace(s) ? null : s.Trim();
    private static long? Long(JsonObject o, string k)
    {
        if (o[k] is not JsonValue v) return null;
        if (v.TryGetValue<long>(out var l)) return l;
        if (v.TryGetValue<double>(out var d) && d is >= long.MinValue and <= long.MaxValue) return (long)d;
        if (v.TryGetValue<string>(out var s) && long.TryParse(s, NumberStyles.Integer, CultureInfo.InvariantCulture, out var p)) return p;
        return null;
    }
    private static DateTimeOffset? Unix(long? s) => s is > 0 and < 32503680000 ? DateTimeOffset.FromUnixTimeSeconds(s.Value) : null;
}

/// <summary>Запущена ли игра. Имена процессов проверены на 0.39.4: BeamNG.drive.x64 (игра) и BeamNG.drive (лаунчер).</summary>
public static class GameProcess
{
    private static readonly string[] Names = { "BeamNG.drive.x64", "BeamNG.drive", "BeamNG.tech.x64" };

    public static bool IsRunning()
    {
        foreach (var n in Names)
        {
            Process[] ps;
            try { ps = Process.GetProcessesByName(n); } catch (Exception e) when (e is InvalidOperationException or PlatformNotSupportedException) { continue; }
            var any = ps.Length > 0;
            foreach (var p in ps) p.Dispose();
            if (any) return true;
        }
        return false;
    }
}
