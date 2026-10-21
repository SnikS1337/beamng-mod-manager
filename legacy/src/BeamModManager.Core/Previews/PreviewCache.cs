using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace BeamModManager.Core.Previews;

/// <summary>Отпечаток архива: картинка достаётся заново, только если изменились имя, размер или дата.</summary>
public readonly record struct ArchiveStamp(string FileName, long Size, DateTime LastWriteUtc)
{
    public static ArchiveStamp Of(string path)
    {
        var fi = new FileInfo(path);
        return new ArchiveStamp(fi.Name, fi.Length, fi.LastWriteTimeUtc);
    }

    public string Key
    {
        get
        {
            var raw = $"{FileName.ToLowerInvariant()}|{Size}|{LastWriteUtc.Ticks}";
            return Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(raw)))[..24].ToLowerInvariant();
        }
    }
}

public enum CacheState { Missing, Image, NoImage }

/// <summary>
/// Кэш уменьшенных картинок рядом с библиотекой: &lt;ключ&gt;.jpg или &lt;ключ&gt;.none (в архиве картинок нет).
/// Само уменьшение делает приложение: в ядре нет декодера изображений. Там же названия модов из их info.json
/// (titles.json: ключ архива → название, пустая строка — названия в архиве нет), чтобы не открывать архивы при каждом запуске.
/// </summary>
public sealed class PreviewCache
{
    public string Folder { get; }

    public PreviewCache(string folder) => Folder = folder;

    public string ImagePath(ArchiveStamp s) => Path.Combine(Folder, s.Key + ".jpg");
    private string NonePath(ArchiveStamp s) => Path.Combine(Folder, s.Key + ".none");

    public CacheState State(ArchiveStamp s) =>
        File.Exists(ImagePath(s)) ? CacheState.Image : File.Exists(NonePath(s)) ? CacheState.NoImage : CacheState.Missing;

    /// <summary>
    /// Состояние многих архивов разом: один список папки кэша вместо двух обращений к диску на каждый мод
    /// (гараж спрашивает про все моды при каждом перечитывании). Нет папки или она не читается — всё «нет в кэше».
    /// </summary>
    public Func<ArchiveStamp, CacheState> Snapshot()
    {
        HashSet<string> names;
        try
        {
            names = Directory.Exists(Folder)
                ? new HashSet<string>(Directory.EnumerateFiles(Folder).Select(f => Path.GetFileName(f)), StringComparer.OrdinalIgnoreCase)
                : new HashSet<string>();
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { names = new HashSet<string>(); }
        return s => names.Contains(s.Key + ".jpg") ? CacheState.Image : names.Contains(s.Key + ".none") ? CacheState.NoImage : CacheState.Missing;
    }

    public string PutImage(ArchiveStamp s, ReadOnlySpan<byte> jpeg)
    {
        Directory.CreateDirectory(Folder);
        var dst = ImagePath(s);
        var tmp = dst + "." + Guid.NewGuid().ToString("N") + ".tmp";
        File.WriteAllBytes(tmp, jpeg.ToArray());
        File.Move(tmp, dst, overwrite: true);
        TryDelete(NonePath(s));
        return dst;
    }

    public void PutNoImage(ArchiveStamp s)
    {
        Directory.CreateDirectory(Folder);
        File.WriteAllText(NonePath(s), s.FileName);
        TryDelete(ImagePath(s));
    }

    // ───────────── названия ─────────────

    /// <summary>titles.json больше этого не читаем: тысяча названий — около 60 КБ.</summary>
    private const long MaxTitlesBytes = 4L * 1024 * 1024;

    private readonly object _titlesGate = new();
    private Dictionary<string, string>? _titles;
    private bool _titlesDirty;

    private string TitlesPath => Path.Combine(Folder, "titles.json");

    private Dictionary<string, string> Titles()
    {
        if (_titles is not null) return _titles;
        var loaded = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        try
        {
            var fi = new FileInfo(TitlesPath);
            if (fi.Exists && fi.Length <= MaxTitlesBytes)
                using (var st = fi.OpenRead())
                    if (JsonSerializer.Deserialize<Dictionary<string, string>>(st) is { } d)
                        foreach (var (k, v) in d)
                            if (k.Length > 0 && v is not null && v.Length <= 200) loaded[k] = v;
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or JsonException) { } // нет или повреждён: прочитаем архивы заново
        return _titles = loaded;
    }

    /// <summary>Название архива: известно ли оно (архив уже читали) и само название (null — в архиве его нет).</summary>
    public bool TryGetTitle(ArchiveStamp s, out string? title)
    {
        lock (_titlesGate)
        {
            if (Titles().TryGetValue(s.Key, out var t)) { title = t.Length == 0 ? null : t; return true; }
        }
        title = null;
        return false;
    }

    /// <summary>Запомнить название (null — названия в архиве нет). На диск попадает при <see cref="SaveTitles"/>.</summary>
    public void PutTitle(ArchiveStamp s, string? title)
    {
        lock (_titlesGate)
        {
            var t = Titles();
            var v = title ?? "";
            if (t.TryGetValue(s.Key, out var old) && old == v) return;
            t[s.Key] = v;
            _titlesDirty = true;
        }
    }

    /// <summary>Записать изменившиеся названия (атомарно, через временный файл). Не вышло — останутся в памяти до следующего раза.</summary>
    public void SaveTitles()
    {
        byte[] bytes;
        lock (_titlesGate)
        {
            if (!_titlesDirty || _titles is null) return;
            bytes = JsonSerializer.SerializeToUtf8Bytes(_titles);
            _titlesDirty = false;
        }
        try
        {
            Directory.CreateDirectory(Folder);
            var tmp = TitlesPath + "." + Guid.NewGuid().ToString("N") + ".tmp";
            File.WriteAllBytes(tmp, bytes);
            File.Move(tmp, TitlesPath, overwrite: true);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { lock (_titlesGate) _titlesDirty = true; }
    }

    /// <summary>Убирает картинки и названия архивов, которых больше нет или которые изменились. Возвращает число удалённых файлов.</summary>
    public int Prune(IEnumerable<ArchiveStamp> alive)
    {
        var keep = new HashSet<string>(alive.Select(a => a.Key), StringComparer.OrdinalIgnoreCase);
        lock (_titlesGate)
        {
            var t = Titles();
            foreach (var k in t.Keys.Where(k => !keep.Contains(k)).ToList()) { t.Remove(k); _titlesDirty = true; }
        }
        SaveTitles();
        if (!Directory.Exists(Folder)) return 0;
        var removed = 0;
        foreach (var f in Directory.EnumerateFiles(Folder))
        {
            var name = Path.GetFileName(f);
            var isTmp = name.EndsWith(".tmp", StringComparison.OrdinalIgnoreCase);
            var ext = Path.GetExtension(name);
            if (!isTmp && ext is not (".jpg" or ".none")) continue;
            if (isTmp && DateTime.UtcNow - File.GetLastWriteTimeUtc(f) < TimeSpan.FromMinutes(5)) continue; // ещё пишется
            if (!isTmp && keep.Contains(Path.GetFileNameWithoutExtension(name))) continue;
            if (TryDelete(f)) removed++;
        }
        return removed;
    }

    private static bool TryDelete(string p)
    {
        try { if (!File.Exists(p)) return false; File.Delete(p); return true; }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return false; }
    }
}
