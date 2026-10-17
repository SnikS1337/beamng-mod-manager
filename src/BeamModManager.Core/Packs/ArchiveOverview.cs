using System.IO.Compression;
using System.Text.Json;
using BeamModManager.Core.Safety;

namespace BeamModManager.Core.Packs;

/// <summary>
/// «Внутри архива» в правой панели (DESIGN.md 4): какие машины и карты кладёт мод, есть ли скрипты, сколько файлов
/// и что насторожило проверку. Названия машин и карт — из их info.json, если он читается, иначе имя папки.
/// </summary>
public sealed record ArchiveOverview(
    IReadOnlyList<string> Vehicles,
    IReadOnlyList<string> Levels,
    int LuaFiles,
    int Files,
    long UnpackedBytes,
    /// <summary>Предупреждения и запреты проверки (<see cref="ArchiveInspector"/>), без «справочных».</summary>
    IReadOnlyList<Finding> Findings)
{
    /// <summary>Архив не читается: показываем это словами, а не пустой блок.</summary>
    public bool Broken { get; init; }

    public static ArchiveOverview Unreadable { get; } = new(Array.Empty<string>(), Array.Empty<string>(), 0, 0, 0, Array.Empty<Finding>()) { Broken = true };
}

public static class ArchiveOverviewReader
{
    /// <summary>info.json больше этого не читаем: у настоящих машин и карт он в несколько килобайт.</summary>
    private const int MaxInfoBytes = 256 * 1024;

    /// <summary>Сколько info.json открываем за раз: в паке из сотни машин читаем первые, остальные — по имени папки.</summary>
    private const int MaxInfoReads = 24;

    public static ArchiveOverview Read(string zipPath)
    {
        try
        {
            using var f = new FileStream(zipPath, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete, 64 * 1024);
            return Read(f);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return ArchiveOverview.Unreadable; }
    }

    public static ArchiveOverview Read(Stream zip)
    {
        var inspection = ArchiveInspector.Inspect(zip);
        if (inspection.Findings.Any(f => f.Code == "not-zip")) return ArchiveOverview.Unreadable;
        var findings = inspection.Findings.Where(f => f.Severity != Severity.Info).ToList();
        try
        {
            zip.Position = 0;
            using var archive = new ZipArchive(zip, ZipArchiveMode.Read, leaveOpen: true);
            // vehicles/common — общие детали для всех машин, а не машина
            var content = ContentOf(archive, out var lua, out var files);
            var reads = 0;
            var vehicles = content.Vehicles.Where(v => v != "common").Select(v => NameOf(archive, "vehicles", v, "Name", "Brand", ref reads)).ToList();
            var levels = content.Levels.Select(l => NameOf(archive, "levels", l, "title", null, ref reads)).ToList();
            vehicles.Sort(StringComparer.CurrentCultureIgnoreCase);
            levels.Sort(StringComparer.CurrentCultureIgnoreCase);
            return new ArchiveOverview(vehicles, levels, lua, files, inspection.TotalUncompressedBytes, findings);
        }
        catch (Exception e) when (e is InvalidDataException or IOException or NotSupportedException or ArgumentException or OverflowException)
        {
            return ArchiveOverview.Unreadable;
        }
    }

    private static ArchiveContent ContentOf(ZipArchive archive, out int lua, out int files)
    {
        lua = 0;
        files = 0;
        foreach (var e in archive.Entries)
        {
            if (e.FullName.EndsWith('/') || e.FullName.EndsWith('\\')) continue;
            files++;
            if (e.FullName.EndsWith(".lua", StringComparison.OrdinalIgnoreCase)) lua++;
        }
        return PackContentReader.ReadFrom(archive);
    }

    /// <summary>
    /// Имя из info.json машины («Brand» + «Name») или карты («title»); ключи перевода игры («levels.x.info.title») не берём.
    /// info.json у BeamNG — «вольный» JSON: с запятыми в конце и комментариями.
    /// </summary>
    private static string NameOf(ZipArchive archive, string root, string folder, string key, string? brandKey, ref int reads)
    {
        if (reads >= MaxInfoReads) return folder;
        var entry = archive.Entries.FirstOrDefault(e => IsInfo(e.FullName, root, folder));
        if (entry is null || entry.Length > MaxInfoBytes) return folder;
        reads++;
        try
        {
            using var s = entry.Open();
            using var doc = JsonDocument.Parse(s, new JsonDocumentOptions { AllowTrailingCommas = true, CommentHandling = JsonCommentHandling.Skip, MaxDepth = 32 });
            if (doc.RootElement.ValueKind != JsonValueKind.Object) return folder;
            var name = Text(doc.RootElement, key);
            if (name is null) return folder;
            var brand = brandKey is null ? null : Text(doc.RootElement, brandKey);
            return brand is not null && !name.StartsWith(brand, StringComparison.OrdinalIgnoreCase) ? brand + " " + name : name;
        }
        catch (Exception e) when (e is JsonException or InvalidDataException or IOException or NotSupportedException) { return folder; }
    }

    private static string? Text(JsonElement o, string key)
    {
        foreach (var p in o.EnumerateObject())
        {
            if (!p.Name.Equals(key, StringComparison.OrdinalIgnoreCase) || p.Value.ValueKind != JsonValueKind.String) continue;
            var v = p.Value.GetString()?.Trim();
            if (string.IsNullOrEmpty(v) || v.Length > 80) return null;
            if (!v.Contains(' ') && v.Count(c => c == '.') >= 2) return null; // ключ перевода
            return v;
        }
        return null;
    }

    /// <summary>vehicles/&lt;папка&gt;/info.json, в том числе внутри одной обёрточной папки.</summary>
    private static bool IsInfo(string fullName, string root, string folder)
    {
        var p = fullName.Replace('\\', '/').Trim('/').Split('/');
        for (var i = 0; i + 2 < p.Length && i <= 1; i++)
            if (p.Length == i + 3 && p[i].Equals(root, StringComparison.OrdinalIgnoreCase) && p[i + 1].Equals(folder, StringComparison.OrdinalIgnoreCase)
                && p[i + 2].Equals("info.json", StringComparison.OrdinalIgnoreCase))
                return true;
        return false;
    }
}
