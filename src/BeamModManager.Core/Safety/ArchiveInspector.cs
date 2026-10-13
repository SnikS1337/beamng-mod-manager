using System.IO.Compression;

namespace BeamModManager.Core.Safety;

public enum Severity { Info, Warning, Block }

public sealed record Finding(Severity Severity, string Code, string Message, string? Entry = null);

public sealed record InspectionResult(IReadOnlyList<Finding> Findings, int EntryCount, long TotalUncompressedBytes)
{
    public bool IsBlocked => Findings.Any(f => f.Severity == Severity.Block);
    public bool HasWarnings => Findings.Any(f => f.Severity == Severity.Warning);
}

public sealed class ArchiveLimits
{
    public int MaxEntries { get; init; } = 50_000;
    public long MaxTotalUncompressedBytes { get; init; } = 20L * 1024 * 1024 * 1024;

    /// <summary>Подозрительное сжатие считаем только для крупных файлов, чтобы не ловить обычный текст.</summary>
    public long RatioCheckMinBytes { get; init; } = 100L * 1024 * 1024;

    public double MaxCompressionRatio { get; init; } = 200;
}

/// <summary>
/// Структурная проверка ZIP до установки: защита от zip-slip и архивных бомб, поиск исполняемых файлов,
/// подсказки о нетипичной структуре мода. Это эвристика, а не антивирус.
/// </summary>
public static class ArchiveInspector
{
    private static readonly HashSet<string> BlockedExtensions = new(StringComparer.OrdinalIgnoreCase)
    {
        ".exe", ".dll", ".bat", ".cmd", ".com", ".scr", ".msi", ".ps1", ".vbs", ".sh", ".jar",
    };

    // Папки верхнего уровня, которые ожидает игра.
    private static readonly HashSet<string> KnownRoots = new(StringComparer.OrdinalIgnoreCase)
    {
        "vehicles", "levels", "art", "lua", "scripts", "ui", "core", "settings", "camera", "trackeditor", "gameplay",
    };

    public static InspectionResult Inspect(Stream zip, ArchiveLimits? limits = null)
    {
        limits ??= new ArchiveLimits();
        var findings = new List<Finding>();

        ZipArchive archive;
        try
        {
            archive = new ZipArchive(zip, ZipArchiveMode.Read, leaveOpen: true);
        }
        catch (InvalidDataException)
        {
            return new InspectionResult(
                new[] { new Finding(Severity.Block, "not-zip", "Файл не является корректным ZIP-архивом.") }, 0, 0);
        }

        using (archive)
        {
            var entries = archive.Entries;
            if (entries.Count > limits.MaxEntries)
            {
                findings.Add(new Finding(Severity.Block, "too-many-entries",
                    $"В архиве {entries.Count} файлов, лимит {limits.MaxEntries}."));
                return new InspectionResult(findings, entries.Count, 0);
            }

            long total = 0;
            var topLevel = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            var rootHasFiles = false;

            foreach (var entry in entries)
            {
                var name = entry.FullName.Replace('\\', '/');
                total += entry.Length;

                if (IsUnsafePath(name))
                    findings.Add(new Finding(Severity.Block, "zip-slip", "Небезопасный путь внутри архива.", entry.FullName));

                var isDirectory = name.EndsWith('/');
                if (!isDirectory && BlockedExtensions.Contains(Path.GetExtension(name)))
                    findings.Add(new Finding(Severity.Block, "executable",
                        "Исполняемый файл или скрипт: моды BeamNG так не распространяются.", entry.FullName));

                if (!isDirectory && entry.Length >= limits.RatioCheckMinBytes && entry.CompressedLength > 0 &&
                    (double)entry.Length / entry.CompressedLength > limits.MaxCompressionRatio)
                    findings.Add(new Finding(Severity.Block, "compression-bomb",
                        "Подозрительно высокая степень сжатия.", entry.FullName));

                var first = name.TrimStart('/').Split('/', 2)[0];
                if (first.Length > 0) topLevel.Add(first);
                if (!isDirectory && !name.TrimStart('/').Contains('/')) rootHasFiles = true;
            }

            if (total > limits.MaxTotalUncompressedBytes)
                findings.Add(new Finding(Severity.Block, "too-large",
                    $"Распакованный размер {total / (1024 * 1024)} МБ превышает лимит."));

            AddStructureHints(findings, entries.Count, topLevel, rootHasFiles);
            return new InspectionResult(findings, entries.Count, total);
        }
    }

    private static bool IsUnsafePath(string name)
    {
        if (name.StartsWith('/')) return true;
        if (name.Length >= 2 && name[1] == ':') return true; // C:\...
        return name.Split('/').Any(seg => seg == "..");
    }

    private static void AddStructureHints(List<Finding> findings, int count, HashSet<string> topLevel, bool rootHasFiles)
    {
        if (count == 0)
        {
            findings.Add(new Finding(Severity.Warning, "empty", "Архив пуст."));
            return;
        }

        if (topLevel.Any(KnownRoots.Contains)) return;

        // Все файлы лежат внутри одной лишней папки: игра такой мод не увидит.
        if (topLevel.Count == 1 && !rootHasFiles)
        {
            findings.Add(new Finding(Severity.Warning, "nested-root",
                $"Все файлы лежат внутри папки «{topLevel.First()}»: возможно, игра не увидит мод без перепаковки."));
            return;
        }

        findings.Add(new Finding(Severity.Warning, "unusual-structure",
            "В корне архива нет привычных папок мода (vehicles, levels, art, lua, ui и т. п.)."));
    }
}
