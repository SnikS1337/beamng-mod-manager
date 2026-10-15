using System.IO.Compression;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace BeamModManager.Core.Game;

/// <summary>Ответ помощника на последнюю команду.</summary>
public sealed record BridgeAnswer(string? AckId, IReadOnlyDictionary<string, string> Results, string? Error);

/// <summary>
/// Помощник внутри игры (мод bmm_bridge.zip): включает и выключает моды, пока игра запущена, тем же способом, что и меню
/// модов игры (core_modmanager.activateMod / deactivateMod; игра сама обновляет db.json). Связь через файлы в папке
/// пользователя игры: команда — bmm_bridge/commands.json, ответ и признак жизни — bmm_bridge/state.json (раз в 2 с).
/// Если помощник не отвечает, менеджер работает как раньше: изменение ждёт закрытия игры.
/// </summary>
public sealed class GameBridge
{
    public const string ModFileName = "bmm_bridge.zip";
    public const string ModKey = "bmm_bridge";
    /// <summary>Помощник пишет признак жизни раз в 2 с; молчит дольше — считаем, что его нет (игра закрыта или мод выключен).</summary>
    public static readonly TimeSpan AliveWindow = TimeSpan.FromSeconds(6);

    private readonly string _dir;
    private readonly Func<DateTimeOffset> _now;
    private int _seq;

    /// <param name="modsDir">Папка mods; папка пользователя игры — её родитель.</param>
    public GameBridge(string modsDir, Func<DateTimeOffset>? now = null)
    {
        var userRoot = Path.GetDirectoryName(Path.GetFullPath(modsDir).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar)) ?? modsDir;
        _dir = Path.Combine(userRoot, "bmm_bridge");
        _now = now ?? (() => DateTimeOffset.UtcNow);
    }

    public string CommandPath => Path.Combine(_dir, "commands.json");
    public string StatePath => Path.Combine(_dir, "state.json");

    /// <summary>Помощник отвечал за последние <see cref="AliveWindow"/>.</summary>
    public bool IsAlive()
    {
        var s = ReadState();
        if (s?["alive"] is not JsonValue v || !v.TryGetValue<long>(out var alive)) return false;
        var age = _now() - DateTimeOffset.FromUnixTimeSeconds(alive);
        return age < AliveWindow && age > -AliveWindow;
    }

    /// <summary>Отправить нужное состояние модов (имя файла → включён). Возвращает номер команды или null, если не записалось.</summary>
    public string? Send(IReadOnlyDictionary<string, bool> wanted)
    {
        var id = $"{_now().ToUnixTimeMilliseconds()}-{Interlocked.Increment(ref _seq)}";
        var set = new JsonObject();
        foreach (var (file, on) in wanted)
        {
            var key = GameModDb.KeyOf(file);
            if (key.Length == 0 || key == ModKey) continue;
            set[key] = on;
        }
        var cmd = new JsonObject { ["id"] = id, ["time"] = _now().ToUnixTimeSeconds(), ["set"] = set };
        try
        {
            Directory.CreateDirectory(_dir);
            var tmp = CommandPath + $".{Environment.ProcessId}.tmp";
            File.WriteAllText(tmp, cmd.ToJsonString(), new System.Text.UTF8Encoding(false));
            File.Move(tmp, CommandPath, overwrite: true);
            return id;
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return null; }
    }

    public BridgeAnswer? ReadAnswer()
    {
        var s = ReadState();
        if (s is null) return null;
        var results = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        if (s["results"] is JsonObject r)
            foreach (var (k, v) in r)
                if (v is JsonValue jv && jv.TryGetValue<string>(out var str)) results[k] = str;
        return new BridgeAnswer(Str(s, "ackId"), results, Str(s, "error"));
    }

    private JsonObject? ReadState()
    {
        try
        {
            if (!File.Exists(StatePath)) return null;
            using var fs = new FileStream(StatePath, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            return JsonNode.Parse(fs) as JsonObject;
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or JsonException) { return null; }
    }

    private static string? Str(JsonObject o, string k) => o[k] is JsonValue v && v.TryGetValue<string>(out var s) ? s : null;

    // ───────────── установка ─────────────

    /// <summary>Архив помощника: всегда одинаковые байты, чтобы не переписывать файл без нужды.</summary>
    public static byte[] BuildMod()
    {
        using var ms = new MemoryStream();
        using (var zip = new ZipArchive(ms, ZipArchiveMode.Create, leaveOpen: true))
        {
            void Add(string entry, string resource)
            {
                var e = zip.CreateEntry(entry, CompressionLevel.Optimal);
                e.LastWriteTime = new DateTimeOffset(2026, 10, 15, 0, 0, 0, TimeSpan.Zero);
                using var src = typeof(GameBridge).Assembly.GetManifestResourceStream(resource)
                                ?? throw new InvalidOperationException("Нет ресурса " + resource);
                using var dst = e.Open();
                src.CopyTo(dst);
            }
            Add("lua/ge/extensions/bmmBridge.lua", "BeamModManager.Core.Game.Bridge.bmmBridge.lua");
            Add("scripts/bmm_bridge/modScript.lua", "BeamModManager.Core.Game.Bridge.modScript.lua");
        }
        return ms.ToArray();
    }

    /// <summary>
    /// Положить помощник в mods (или обновить). Запущенная игра подхватывает новый архив сама. Возвращает true, если файл
    /// на месте и актуален.
    /// </summary>
    public static bool EnsureInstalled(string modsDir)
    {
        try
        {
            if (!Directory.Exists(modsDir)) return false;
            var path = Path.Combine(modsDir, ModFileName);
            var bytes = BuildMod();
            if (File.Exists(path) && File.ReadAllBytes(path).AsSpan().SequenceEqual(bytes)) return true;
            var tmp = path + $".{Environment.ProcessId}.tmp";
            File.WriteAllBytes(tmp, bytes);
            File.Move(tmp, path, overwrite: true);
            return true;
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return false; }
    }
}
