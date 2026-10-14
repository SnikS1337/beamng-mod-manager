using System.Text.Json;
using BeamModManager.Core.Models;

namespace BeamModManager.Core.Matching;

/// <summary>Ручное решение пользователя о паре записей: «Это один мод» или «Это разные».</summary>
public enum ManualMatch { Same, Different }

public sealed record MatchDecisionEntry(string KeyA, string KeyB, ManualMatch Decision, DateTimeOffset DecidedAtUtc);

/// <summary>Хранилище ручных решений по дублям. Решение относится к паре записей на сайтах и действует всегда, пока его не отменят.</summary>
public interface IMatchDecisions
{
    /// <summary>Решение пользователя по паре или null. Порядок записей в паре не важен.</summary>
    ManualMatch? Get(Listing a, Listing b);
    /// <summary>Запоминает решение. Записи с одного сайта не объединяются никогда (ArgumentException), «Это разные» для них смысла не имеет.</summary>
    void Set(Listing a, Listing b, ManualMatch decision);
    /// <summary>Отменяет решение («Вернуть как было»). Если решения не было, ничего не происходит.</summary>
    void Clear(Listing a, Listing b);
    IReadOnlyList<MatchDecisionEntry> All();
}

/// <summary>
/// Ручные решения по дублям; хранятся навсегда (PLAN.md, раздел 6). Без пути живёт в памяти (тесты), с путём пишется
/// в JSON-файл атомарно (временный файл и переименование) и читается терпимо: повреждённый файл откладывается в *.bad,
/// решения начинаются заново, о чём сообщает <see cref="LoadWarnings"/>.
/// </summary>
public sealed class MatchDecisionStore : IMatchDecisions
{
    private const long MaxBytes = 16 * 1024 * 1024;
    private const int CurrentSchema = 1;
    private readonly object _g = new();
    private readonly Dictionary<(string, string), MatchDecisionEntry> _items = new();
    private readonly string? _path;
    private readonly Func<DateTimeOffset> _now;

    public IReadOnlyList<string> LoadWarnings { get; }

    public MatchDecisionStore(string? path = null, Func<DateTimeOffset>? now = null)
    {
        _path = path;
        _now = now ?? (() => DateTimeOffset.UtcNow);
        var warnings = new List<string>();
        if (_path is not null) Load(warnings);
        LoadWarnings = warnings;
    }

    /// <summary>Ключ записи: «сайт:номер на сайте». Сайт без различия регистра, номер как есть.</summary>
    public static string KeyOf(Listing l) => $"{l.Source.Trim().ToLowerInvariant()}:{l.SourceId.Trim()}";

    /// <summary>Ключ из файла (его могли поправить руками): сайт к нижнему регистру, пробелы по краям убраны. Без «сайт:» — пусто.</summary>
    private static string NormKey(string k)
    {
        var i = k.IndexOf(':');
        if (i <= 0 || i == k.Length - 1) return "";
        var src = k[..i].Trim().ToLowerInvariant();
        var id = k[(i + 1)..].Trim();
        return src.Length == 0 || id.Length == 0 ? "" : $"{src}:{id}";
    }

    private static (string, string) Pair(string a, string b) => string.CompareOrdinal(a, b) <= 0 ? (a, b) : (b, a);

    public ManualMatch? Get(Listing a, Listing b)
    {
        var (ka, kb) = (KeyOf(a), KeyOf(b));
        if (ka == kb) return null;
        lock (_g) return _items.TryGetValue(Pair(ka, kb), out var e) ? e.Decision : null;
    }

    public void Set(Listing a, Listing b, ManualMatch decision)
    {
        var (ka, kb) = (KeyOf(a), KeyOf(b));
        if (ka == kb) throw new ArgumentException("Пара состоит из одной и той же записи.");
        if (string.Equals(a.Source.Trim(), b.Source.Trim(), StringComparison.OrdinalIgnoreCase))
            throw new ArgumentException("Записи с одного сайта не объединяются.");
        var (x, y) = Pair(ka, kb);
        lock (_g)
        {
            // Не удалось записать на диск: в памяти остаётся то, что было, чтобы окно не показывало решение, которого после перезапуска не будет.
            _items.TryGetValue((x, y), out var previous);
            _items[(x, y)] = new MatchDecisionEntry(x, y, decision, _now());
            try { Save(); }
            catch
            {
                if (previous is null) _items.Remove((x, y)); else _items[(x, y)] = previous;
                throw;
            }
        }
    }

    public void Clear(Listing a, Listing b)
    {
        var (ka, kb) = (KeyOf(a), KeyOf(b));
        if (ka == kb) return;
        lock (_g)
        {
            var key = Pair(ka, kb);
            if (!_items.Remove(key, out var previous)) return;
            try { Save(); }
            catch { _items[key] = previous; throw; }
        }
    }

    public IReadOnlyList<MatchDecisionEntry> All()
    {
        lock (_g) return _items.Values.OrderBy(e => e.KeyA, StringComparer.Ordinal).ThenBy(e => e.KeyB, StringComparer.Ordinal).ToList();
    }

    private void Save()
    {
        if (_path is null) return;
        var dir = Path.GetDirectoryName(Path.GetFullPath(_path))!;
        Directory.CreateDirectory(dir);
        var payload = new
        {
            schema = CurrentSchema,
            items = _items.Values.OrderBy(e => e.KeyA, StringComparer.Ordinal).ThenBy(e => e.KeyB, StringComparer.Ordinal)
                .Select(e => new { a = e.KeyA, b = e.KeyB, decision = e.Decision == ManualMatch.Same ? "same" : "different", at = e.DecidedAtUtc }).ToList(),
        };
        var tmp = _path + ".tmp";
        try
        {
            File.WriteAllText(tmp, JsonSerializer.Serialize(payload, new JsonSerializerOptions { WriteIndented = true }));
            File.Move(tmp, _path, overwrite: true);
        }
        catch
        {
            try { File.Delete(tmp); } catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
            throw;
        }
    }

    private void Load(List<string> warnings)
    {
        if (!File.Exists(_path!)) return;
        string text;
        try
        {
            if (new FileInfo(_path!).Length > MaxBytes) { Quarantine("файл решений слишком большой", warnings); return; }
            text = File.ReadAllText(_path!);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            warnings.Add($"не удалось прочитать решения по дублям: {e.Message}");
            return;
        }

        JsonDocument doc;
        try { doc = JsonDocument.Parse(text, new JsonDocumentOptions { AllowTrailingCommas = true, CommentHandling = JsonCommentHandling.Skip }); }
        catch (JsonException) { Quarantine("файл решений повреждён", warnings); return; }

        using (doc)
        {
            if (doc.RootElement.ValueKind != JsonValueKind.Object || !doc.RootElement.TryGetProperty("items", out var arr) || arr.ValueKind != JsonValueKind.Array)
            {
                Quarantine("файл решений имеет неверный формат", warnings);
                return;
            }
            var skipped = 0;
            foreach (var e in arr.EnumerateArray())
            {
                if (e.ValueKind != JsonValueKind.Object ||
                    !e.TryGetProperty("a", out var a) || a.ValueKind != JsonValueKind.String ||
                    !e.TryGetProperty("b", out var b) || b.ValueKind != JsonValueKind.String ||
                    !e.TryGetProperty("decision", out var d) || d.ValueKind != JsonValueKind.String)
                { skipped++; continue; }
                var ka = NormKey(a.GetString()!); var kb = NormKey(b.GetString()!);
                var dec = d.GetString() switch { "same" => (ManualMatch?)ManualMatch.Same, "different" => ManualMatch.Different, _ => null };
                if (dec is null || ka.Length == 0 || kb.Length == 0 || ka == kb) { skipped++; continue; }
                var at = e.TryGetProperty("at", out var t) && t.ValueKind == JsonValueKind.String && t.TryGetDateTimeOffset(out var parsed) ? parsed : _now();
                var (x, y) = Pair(ka, kb);
                _items[(x, y)] = new MatchDecisionEntry(x, y, dec.Value, at);
            }
            if (skipped > 0) warnings.Add($"решений по дублям пропущено из-за неверной записи: {skipped}");
        }
    }

    private void Quarantine(string why, List<string> warnings)
    {
        string? moved = null;
        try
        {
            moved = $"{_path}.bad-{_now():yyyyMMdd-HHmmss}";
            File.Move(_path!, moved, overwrite: true);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { moved = null; }
        warnings.Add(moved is null ? $"{why}, решения начаты заново" : $"{why}: сохранён как {Path.GetFileName(moved)}, решения начаты заново");
    }
}
