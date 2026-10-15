using System.IO.Compression;

namespace BeamModManager.Core.Previews;

/// <summary>Откуда взята картинка. Порядок совпадает с порядком поиска (DESIGN.md раздел 9).</summary>
public enum PreviewSource { None, VehicleDefault, VehicleConfig, ModInfo, LevelPreview, AppIcon, OtherImage }

/// <summary>Что лежит в архиве по существу: нужно для заглушки, если картинок нет, а в db.json тип не указан.</summary>
public enum ArchiveContent { Unknown, Vehicle, Level, Sound, Script }

public sealed record PreviewPick(PreviewSource Source, string? Entry, int Width, int Height, ArchiveContent Content, string? Warning = null)
{
    public bool Found => Entry is not null;
}

/// <summary>
/// Выбирает одну картинку мода, читая только оглавление ZIP и заголовки нескольких картинок.
/// Архив целиком не распаковывается: игра тоже подключает ZIP как есть.
/// </summary>
public static class PreviewPicker
{
    public const int MinSide = 200;
    public const long MaxImageBytes = 30L * 1024 * 1024;
    private const int MaxHeaderProbes = 40;

    private static readonly string[] ImageExt = { ".jpg", ".jpeg", ".png" };

    public static PreviewPick Pick(string zipPath)
    {
        try
        {
            using var fs = new FileStream(zipPath, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            return Pick(fs);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            return new PreviewPick(PreviewSource.None, null, 0, 0, ArchiveContent.Unknown, "Архив не открылся: " + e.Message);
        }
    }

    public static PreviewPick Pick(Stream zip)
    {
        try
        {
            using var a = new ZipArchive(zip, ZipArchiveMode.Read, leaveOpen: true);
            return PickCore(a);
        }
        catch (Exception e) when (e is InvalidDataException or IOException or NotSupportedException or ArgumentException or OverflowException)
        {
            return new PreviewPick(PreviewSource.None, null, 0, 0, ArchiveContent.Unknown, "Архив повреждён, картинку не достать.");
        }
    }

    /// <summary>Байты выбранной картинки (уменьшает и кладёт в кэш приложение). null, если запись пропала или слишком большая.</summary>
    public static byte[]? Extract(string zipPath, string entry)
    {
        try
        {
            using var fs = new FileStream(zipPath, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            using var a = new ZipArchive(fs, ZipArchiveMode.Read);
            var e = a.GetEntry(entry);
            if (e is null || e.Length > MaxImageBytes) return null;
            using var s = e.Open();
            var ms = new MemoryStream((int)Math.Max(e.Length, 0));
            var buf = new byte[81920];
            int n;
            while ((n = s.Read(buf, 0, buf.Length)) > 0)
            {
                if (ms.Length + n > MaxImageBytes) return null;
                ms.Write(buf, 0, n);
            }
            return ms.ToArray();
        }
        catch (Exception e) when (e is InvalidDataException or IOException or NotSupportedException or ArgumentException or OverflowException or UnauthorizedAccessException)
        {
            return null;
        }
    }

    private sealed record Item(ZipArchiveEntry Entry, string Path, string[] Parts)
    {
        public string Lower => Path.ToLowerInvariant();
        public string Ext => System.IO.Path.GetExtension(Path).ToLowerInvariant();
        public string Stem => System.IO.Path.GetFileNameWithoutExtension(Path);
        public string Dir => Parts.Length > 1 ? string.Join('/', Parts[..^1]) : "";
    }

    private static PreviewPick PickCore(ZipArchive a)
    {
        var items = new List<Item>();
        foreach (var e in a.Entries)
        {
            if (e.FullName.EndsWith('/') || e.FullName.EndsWith('\\')) continue;
            var path = e.FullName.Replace('\\', '/').TrimStart('/');
            var parts = path.Split('/', StringSplitOptions.RemoveEmptyEntries);
            if (parts.Length == 0 || parts.Contains("..")) continue;
            items.Add(new Item(e, path, parts));
        }
        items.Sort((x, y) => StringComparer.OrdinalIgnoreCase.Compare(x.Path, y.Path));
        var content = ContentOf(items);
        var images = items.Where(i => ImageExt.Contains(i.Ext) && i.Entry.Length > 0 && i.Entry.Length <= MaxImageBytes).ToList();
        var probes = 0;

        PreviewPick? Try(PreviewSource src, IEnumerable<Item> cands, int minSide)
        {
            foreach (var c in cands)
            {
                if (probes++ >= MaxHeaderProbes) return null;
                var size = ImageHeader.Read(c.Entry);
                if (size is { } s && s.W >= minSide && s.H >= minSide)
                    return new PreviewPick(src, c.Entry.FullName, s.W, s.H, content);
            }
            return null;
        }

        static bool Under(Item i, string root, int depth) =>
            i.Parts.Length == depth && i.Parts[0].Equals(root, StringComparison.OrdinalIgnoreCase);

        // 1. Главная картинка машины, как в самой игре.
        var r = Try(PreviewSource.VehicleDefault,
            images.Where(i => Under(i, "vehicles", 3) && i.Stem.Equals("default", StringComparison.OrdinalIgnoreCase))
                  .OrderBy(i => i.Ext == ".jpg" ? 0 : 1), 64);
        if (r is not null) return r;

        // 2. Картинка конфигурации рядом с .pc того же имени.
        var configs = new HashSet<string>(items.Where(i => Under(i, "vehicles", 3) && i.Ext == ".pc")
            .Select(i => i.Dir + "/" + i.Stem), StringComparer.OrdinalIgnoreCase);
        r = Try(PreviewSource.VehicleConfig, images.Where(i => Under(i, "vehicles", 3) && configs.Contains(i.Dir + "/" + i.Stem)), 64);
        if (r is not null) return r;

        // 3. Картинки мода с сайта: mod_info/<ID>/images/*, затем значок mod_info/<ID>/icon.*.
        r = Try(PreviewSource.ModInfo, images.Where(i => i.Parts.Length == 4 && i.Parts[0].Equals("mod_info", StringComparison.OrdinalIgnoreCase)
                                                        && i.Parts[2].Equals("images", StringComparison.OrdinalIgnoreCase))
                                       .Concat(images.Where(i => Under(i, "mod_info", 3))), 64);
        if (r is not null) return r;

        // 4. Превью уровня: картинки прямо в папке карты (preview.jpg, <карта>_preview*.jpg), без текстур из art/.
        r = Try(PreviewSource.LevelPreview,
            images.Where(i => Under(i, "levels", 3) && !LooksLikeTexture(i))
                  .OrderBy(i => i.Lower.Contains("preview") ? 0 : 1), 64);
        if (r is not null) return r;

        // 5. Значок приложения у скриптовых модов.
        r = Try(PreviewSource.AppIcon,
            images.Where(i => i.Parts.Length == 5 && i.Lower.StartsWith("ui/modules/apps/") && i.Stem.Equals("app", StringComparison.OrdinalIgnoreCase)), 48);
        if (r is not null) return r;

        // 6. Любая другая картинка не меньше ~200 px, кроме текстур. Фото обычно JPG и лежат неглубоко.
        r = Try(PreviewSource.OtherImage,
            images.Where(i => !LooksLikeTexture(i))
                  .OrderBy(i => i.Ext == ".png" ? 1 : 0).ThenBy(i => i.Parts.Length).ThenByDescending(i => i.Entry.Length), MinSide);
        if (r is not null) return r;

        return new PreviewPick(PreviewSource.None, null, 0, 0, content);
    }

    private static readonly string[] TextureDirs = { "art", "textures", "txt", "skins", "paint", "bakes", "decals", "materials" };
    private static readonly string[] TextureMarks =
        { "_bake_", "_nm", ".normal", "_normal", "_diff", "_spec", "_rough", "_metal", "_ao.", "_ao_", ".color.", "_d.", "_s.", "_n.", "_ref.", "_opac", "_mask", "lensflare", "corona" };

    public static bool LooksLikeTexture(string path)
    {
        var p = path.Replace('\\', '/').ToLowerInvariant();
        var parts = p.Split('/');
        if (parts[..^1].Any(d => TextureDirs.Contains(d))) return true;
        var name = parts[^1];
        return TextureMarks.Any(name.Contains);
    }

    private static bool LooksLikeTexture(Item i) => LooksLikeTexture(i.Path);

    private static ArchiveContent ContentOf(List<Item> items)
    {
        bool Has(string root) => items.Any(i => i.Parts.Length > 1 && i.Parts[0].Equals(root, StringComparison.OrdinalIgnoreCase));
        if (Has("vehicles")) return ArchiveContent.Vehicle;
        if (Has("levels")) return ArchiveContent.Level;
        if (items.Any(i => i.Ext is ".ogg" or ".wav" or ".mp3" or ".bank")
            && !items.Any(i => i.Ext is ".lua" or ".js")) return ArchiveContent.Sound;
        if (items.Any(i => i.Ext is ".lua" or ".js" or ".cs")) return ArchiveContent.Script;
        return ArchiveContent.Unknown;
    }
}

/// <summary>Размер картинки по заголовку, без декодирования. Читает не больше 512 КБ начала записи.</summary>
public static class ImageHeader
{
    private const int MaxRead = 512 * 1024;

    public static (int W, int H)? Read(ZipArchiveEntry e)
    {
        try
        {
            using var s = e.Open();
            var buf = new byte[(int)Math.Min(MaxRead, Math.Max(e.Length, 0))];
            var got = 0;
            int n;
            while (got < buf.Length && (n = s.Read(buf, got, buf.Length - got)) > 0) got += n;
            return Parse(buf.AsSpan(0, got));
        }
        catch (Exception x) when (x is InvalidDataException or IOException or NotSupportedException)
        {
            return null;
        }
    }

    public static (int W, int H)? Parse(ReadOnlySpan<byte> b)
    {
        // PNG: подпись, затем блок IHDR с шириной и высотой.
        if (b.Length >= 24 && b[0] == 0x89 && b[1] == (byte)'P' && b[2] == (byte)'N' && b[3] == (byte)'G'
            && b[12] == (byte)'I' && b[13] == (byte)'H' && b[14] == (byte)'D' && b[15] == (byte)'R')
        {
            var w = (b[16] << 24) | (b[17] << 16) | (b[18] << 8) | b[19];
            var h = (b[20] << 24) | (b[21] << 16) | (b[22] << 8) | b[23];
            return w > 0 && h > 0 ? (w, h) : null;
        }
        // JPEG: идём по маркерам до кадра SOFn.
        if (b.Length >= 4 && b[0] == 0xFF && b[1] == 0xD8)
        {
            var i = 2;
            while (i + 4 <= b.Length)
            {
                if (b[i] != 0xFF) return null;
                var m = b[i + 1];
                if (m == 0xFF) { i++; continue; }
                if (m is 0xD8 or 0x01 || (m >= 0xD0 && m <= 0xD7)) { i += 2; continue; }
                if (m == 0xD9 || m == 0xDA) return null;
                var len = (b[i + 2] << 8) | b[i + 3];
                if (len < 2) return null;
                if (m >= 0xC0 && m <= 0xCF && m != 0xC4 && m != 0xC8 && m != 0xCC)
                {
                    if (i + 9 > b.Length) return null;
                    var h = (b[i + 5] << 8) | b[i + 6];
                    var w = (b[i + 7] << 8) | b[i + 8];
                    return w > 0 && h > 0 ? (w, h) : null;
                }
                i += 2 + len;
            }
        }
        return null;
    }
}
