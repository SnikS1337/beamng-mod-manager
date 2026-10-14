using System.Text.Json;

namespace BeamModManager.Core.Settings;

public sealed record SettingsLoadResult(AppSettings Settings, IReadOnlyList<string> Warnings)
{
    public bool HadProblems => Warnings.Count > 0;
}

/// <summary>
/// Файл настроек (JSON). Читается терпимо: лишние поля игнорируются, поле с неверным типом заменяется значением по умолчанию
/// (остальные настройки сохраняются), повреждённый файл откладывается в *.bad и заменяется настройками по умолчанию.
/// Запись атомарная: временный файл рядом и переименование, поэтому обрыв посреди записи не портит настройки.
/// </summary>
public sealed class SettingsStore
{
    private const long MaxBytes = 1024 * 1024;
    private readonly string _path;
    private readonly Func<DateTimeOffset> _now;
    private readonly object _lock = new();

    public SettingsStore(string path, Func<DateTimeOffset>? now = null)
    {
        _path = path;
        _now = now ?? (() => DateTimeOffset.UtcNow);
    }

    public string Path => _path;

    public SettingsLoadResult Load()
    {
        lock (_lock)
        {
            if (!File.Exists(_path)) return new SettingsLoadResult(new AppSettings(), Array.Empty<string>());

            string text;
            try
            {
                if (new FileInfo(_path).Length > MaxBytes) return Quarantine("файл настроек слишком большой");
                text = File.ReadAllText(_path);
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException)
            {
                return new SettingsLoadResult(new AppSettings(), new[] { $"не удалось прочитать настройки: {e.Message}" });
            }

            JsonDocument doc;
            try { doc = JsonDocument.Parse(text, new JsonDocumentOptions { AllowTrailingCommas = true, CommentHandling = JsonCommentHandling.Skip }); }
            catch (JsonException) { return Quarantine("файл настроек повреждён"); }

            using (doc)
            {
                if (doc.RootElement.ValueKind != JsonValueKind.Object) return Quarantine("файл настроек имеет неверный формат");
                var warnings = new List<string>();
                var r = doc.RootElement;
                var d = new AppSettings();
                var s = new AppSettings
                {
                    AllowDownloads = Bool(r, nameof(AppSettings.AllowDownloads), d.AllowDownloads, warnings),
                    ModsFolder = Str(r, nameof(AppSettings.ModsFolder), warnings),
                    UserFolder = Str(r, nameof(AppSettings.UserFolder), warnings),
                    CheckTtlHours = Int(r, nameof(AppSettings.CheckTtlHours), d.CheckTtlHours, warnings),
                    AutoCheckOnStart = Bool(r, nameof(AppSettings.AutoCheckOnStart), d.AutoCheckOnStart, warnings),
                    AutoCheckEveryHours = Int(r, nameof(AppSettings.AutoCheckEveryHours), d.AutoCheckEveryHours, warnings),
                    DisabledSources = Strs(r, nameof(AppSettings.DisabledSources), warnings),
                    KeepBackups = Int(r, nameof(AppSettings.KeepBackups), d.KeepBackups, warnings),
                    MaxCardChecksPerSource = Int(r, nameof(AppSettings.MaxCardChecksPerSource), d.MaxCardChecksPerSource, warnings),
                    LiquidGlass = Bool(r, nameof(AppSettings.LiquidGlass), d.LiquidGlass, warnings),
                    Frost = Bool(r, nameof(AppSettings.Frost), d.Frost, warnings),
                    FrostStrength = Int(r, nameof(AppSettings.FrostStrength), d.FrostStrength, warnings),
                    FrostSparks = Bool(r, nameof(AppSettings.FrostSparks), d.FrostSparks, warnings),
                    Background = (BackgroundMode)Int(r, nameof(AppSettings.Background), (int)d.Background, warnings),
                    BackgroundDim = Int(r, nameof(AppSettings.BackgroundDim), d.BackgroundDim, warnings),
                    UiAnimations = Bool(r, nameof(AppSettings.UiAnimations), d.UiAnimations, warnings),
                    AnimatedBackground = Bool(r, nameof(AppSettings.AnimatedBackground), d.AnimatedBackground, warnings),
                    Fireflies = Bool(r, nameof(AppSettings.Fireflies), d.Fireflies, warnings),
                    BackgroundTilt = Bool(r, nameof(AppSettings.BackgroundTilt), d.BackgroundTilt, warnings),
                    ActiveModsWarnAbove = Int(r, nameof(AppSettings.ActiveModsWarnAbove), d.ActiveModsWarnAbove, warnings),
                    FirstRunDone = Bool(r, nameof(AppSettings.FirstRunDone), d.FirstRunDone, warnings),
                    WindowWidth = Int(r, nameof(AppSettings.WindowWidth), d.WindowWidth, warnings),
                    WindowHeight = Int(r, nameof(AppSettings.WindowHeight), d.WindowHeight, warnings),
                    WindowMaximized = Bool(r, nameof(AppSettings.WindowMaximized), d.WindowMaximized, warnings),
                };
                return new SettingsLoadResult(s.Normalize(), warnings);
            }
        }
    }

    public void Save(AppSettings settings)
    {
        lock (_lock)
        {
            var dir = System.IO.Path.GetDirectoryName(System.IO.Path.GetFullPath(_path))!;
            Directory.CreateDirectory(dir);
            var json = JsonSerializer.Serialize(settings.Normalize(), new JsonSerializerOptions { WriteIndented = true });
            var tmp = _path + ".tmp";
            try
            {
                File.WriteAllText(tmp, json);
                File.Move(tmp, _path, overwrite: true);
            }
            catch
            {
                try { File.Delete(tmp); } catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
                throw;
            }
        }
    }

    private SettingsLoadResult Quarantine(string why)
    {
        string? moved = null;
        try
        {
            moved = $"{_path}.bad-{_now():yyyyMMdd-HHmmss}";
            File.Move(_path, moved, overwrite: true);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { moved = null; }
        var msg = moved is null ? $"{why}, использованы настройки по умолчанию" : $"{why}: сохранён как {System.IO.Path.GetFileName(moved)}, использованы настройки по умолчанию";
        return new SettingsLoadResult(new AppSettings(), new[] { msg });
    }

    private static bool Bool(JsonElement r, string name, bool def, List<string> w)
    {
        if (!r.TryGetProperty(name, out var v)) return def;
        if (v.ValueKind is JsonValueKind.True) return true;
        if (v.ValueKind is JsonValueKind.False) return false;
        w.Add($"{name}: ожидалось true или false, использовано значение по умолчанию");
        return def; // для AllowDownloads это false: «да» строкой или числом загрузку не включает
    }

    private static int Int(JsonElement r, string name, int def, List<string> w)
    {
        if (!r.TryGetProperty(name, out var v)) return def;
        if (v.ValueKind == JsonValueKind.Number && v.TryGetInt32(out var i)) return i;
        w.Add($"{name}: ожидалось целое число, использовано значение по умолчанию");
        return def;
    }

    private static string? Str(JsonElement r, string name, List<string> w)
    {
        if (!r.TryGetProperty(name, out var v) || v.ValueKind == JsonValueKind.Null) return null;
        if (v.ValueKind == JsonValueKind.String) return v.GetString();
        w.Add($"{name}: ожидалась строка, значение сброшено");
        return null;
    }

    private static IReadOnlyList<string> Strs(JsonElement r, string name, List<string> w)
    {
        if (!r.TryGetProperty(name, out var v) || v.ValueKind == JsonValueKind.Null) return Array.Empty<string>();
        if (v.ValueKind != JsonValueKind.Array) { w.Add($"{name}: ожидался список, значение сброшено"); return Array.Empty<string>(); }
        var list = new List<string>();
        foreach (var e in v.EnumerateArray())
            if (e.ValueKind == JsonValueKind.String) list.Add(e.GetString()!);
            else { w.Add($"{name}: элемент не строка пропущен"); }
        return list;
    }
}
