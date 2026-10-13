namespace BeamModManager.Core.Versioning;

public enum UpdateStatus
{
    /// <summary>Изменений не видно.</summary>
    None,

    /// <summary>Версия на сайте больше установленной.</summary>
    Likely,

    /// <summary>Версия не разобрана или совпадает, но размер или хеш изменились.</summary>
    Possible,
}

/// <summary>Что мы знаем об одной версии мода: установленной или найденной на странице.</summary>
public sealed record VersionSnapshot(string? Version, long? SizeBytes = null, string? ContentHash = null);

/// <summary>
/// Решает, показывать ли «есть обновление». Ложные срабатывания допустимы, пропуски нежелательны.
/// Порядок: сравнение версий, затем размер, затем хеш.
/// </summary>
public static class UpdateDecider
{
    public static UpdateStatus Decide(VersionSnapshot installed, VersionSnapshot current)
    {
        var order = VersionComparer.Compare(installed.Version, current.Version);

        if (order == VersionOrder.Newer) return UpdateStatus.Likely;
        if (order == VersionOrder.Older) return UpdateStatus.None; // на сайте версия старее: не откатываем

        var changed =
            SizeComparer.Differs(installed.SizeBytes, current.SizeBytes) ||
            (installed.ContentHash is not null && current.ContentHash is not null && installed.ContentHash != current.ContentHash);

        return changed ? UpdateStatus.Possible : UpdateStatus.None;
    }
}
