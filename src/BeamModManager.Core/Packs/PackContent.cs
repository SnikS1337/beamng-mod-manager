using System.IO.Compression;

namespace BeamModManager.Core.Packs;

/// <summary>Что мод кладёт в игру: папки машин (vehicles/&lt;имя&gt;) и карт (levels/&lt;имя&gt;), имена в нижнем регистре.</summary>
public sealed record ArchiveContent(IReadOnlySet<string> Vehicles, IReadOnlySet<string> Levels)
{
    public static readonly ArchiveContent Empty = new(new HashSet<string>(), new HashSet<string>());

    public int Count => Vehicles.Count + Levels.Count;
    public IEnumerable<string> Keys => Vehicles.Select(v => "vehicles/" + v).Concat(Levels.Select(l => "levels/" + l));

    /// <summary>Паком считаем архив, в котором несколько машин или карт (по содержимому, а не по названию).</summary>
    public bool IsPack => Count >= 2;
}

/// <summary>Читает список машин и карт из ZIP, не распаковывая его. Понимает обёртку в одну папку верхнего уровня.</summary>
public static class PackContentReader
{
    public static ArchiveContent Read(Stream zip)
    {
        try { return ReadCore(zip); }
        catch (Exception e) when (e is InvalidDataException or IOException or NotSupportedException or ArgumentException or OverflowException)
        {
            return ArchiveContent.Empty; // повреждённый архив: состав неизвестен
        }
    }

    private static ArchiveContent ReadCore(Stream zip)
    {
        ZipArchive archive;
        try { archive = new ZipArchive(zip, ZipArchiveMode.Read, leaveOpen: true); }
        catch (InvalidDataException) { return ArchiveContent.Empty; }

        using (archive)
        {
            var paths = archive.Entries
                .Select(e => e.FullName.Replace('\\', '/').Trim('/'))
                .Where(p => p.Length > 0)
                .Select(p => p.Split('/'))
                .ToList();

            var vehicles = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            var levels = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

            // Корень: либо сам архив, либо единственная папка верхнего уровня, внутри которой лежат vehicles/levels.
            var offset = 0;
            if (!paths.Any(p => IsRoot(p[0])) && paths.Select(p => p[0]).Distinct(StringComparer.OrdinalIgnoreCase).Count() == 1
                && paths.Any(p => p.Length > 1 && IsRoot(p[1])))
                offset = 1;

            foreach (var p in paths)
            {
                // Нужен хотя бы один файл или вложенная папка внутри <имя>/, пустая запись папки имя не создаёт.
                if (p.Length <= offset + 1) continue;
                var root = p[offset];
                var name = p[offset + 1];
                if (p.Length == offset + 2 && !IsDirEntry(archive, p)) continue; // файл прямо в vehicles/
                if (root.Equals("vehicles", StringComparison.OrdinalIgnoreCase)) vehicles.Add(name.ToLowerInvariant());
                else if (root.Equals("levels", StringComparison.OrdinalIgnoreCase)) levels.Add(name.ToLowerInvariant());
            }
            return new ArchiveContent(vehicles, levels);
        }
    }

    private static bool IsRoot(string s) =>
        s.Equals("vehicles", StringComparison.OrdinalIgnoreCase) || s.Equals("levels", StringComparison.OrdinalIgnoreCase);

    private static bool IsDirEntry(ZipArchive a, string[] parts)
    {
        var joined = string.Join('/', parts);
        return a.Entries.Any(e => e.FullName.Replace('\\', '/').TrimEnd('/').Equals(joined, StringComparison.OrdinalIgnoreCase)
                                  && (e.FullName.EndsWith('/') || e.FullName.EndsWith('\\')));
    }
}
