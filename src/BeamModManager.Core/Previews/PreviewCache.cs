using System.Security.Cryptography;
using System.Text;

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
/// Само уменьшение делает приложение: в ядре нет декодера изображений.
/// </summary>
public sealed class PreviewCache
{
    public string Folder { get; }

    public PreviewCache(string folder) => Folder = folder;

    public string ImagePath(ArchiveStamp s) => Path.Combine(Folder, s.Key + ".jpg");
    private string NonePath(ArchiveStamp s) => Path.Combine(Folder, s.Key + ".none");

    public CacheState State(ArchiveStamp s) =>
        File.Exists(ImagePath(s)) ? CacheState.Image : File.Exists(NonePath(s)) ? CacheState.NoImage : CacheState.Missing;

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

    /// <summary>Убирает картинки архивов, которых больше нет или которые изменились. Возвращает число удалённых файлов.</summary>
    public int Prune(IEnumerable<ArchiveStamp> alive)
    {
        if (!Directory.Exists(Folder)) return 0;
        var keep = new HashSet<string>(alive.Select(a => a.Key), StringComparer.OrdinalIgnoreCase);
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
