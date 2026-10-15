using BeamModManager.Core.Previews;

namespace BeamModManager.Ui.Previews;

/// <summary>
/// Достаёт картинки для карточек в фоне: одна картинка из архива → уменьшение (делает окно, WPF) → кэш.
/// Архив, где картинок нет, помечается, чтобы не перечитывать его при каждом запуске.
/// </summary>
public sealed class PreviewLoader
{
    private readonly PreviewCache _cache;
    private readonly Func<byte[], byte[]?> _shrink;

    /// <param name="shrink">Уменьшает картинку до 616 × 356 и возвращает JPEG; null — не удалось декодировать.</param>
    public PreviewLoader(PreviewCache cache, Func<byte[], byte[]?> shrink)
    {
        _cache = cache;
        _shrink = shrink;
    }

    /// <summary>Путь к картинке в кэше или null (картинок нет, архив не читается). Повторный вызов ничего не читает.</summary>
    public string? Ensure(string zipPath)
    {
        try { return EnsureCore(zipPath); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return null; } // кэш занят или диск полон: без картинки
    }

    private string? EnsureCore(string zipPath)
    {
        ArchiveStamp stamp;
        try { stamp = ArchiveStamp.Of(zipPath); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return null; }
        if (!File.Exists(zipPath)) return null;

        switch (_cache.State(stamp))
        {
            case CacheState.Image: return _cache.ImagePath(stamp);
            case CacheState.NoImage: return null;
        }

        var pick = PreviewPicker.Pick(zipPath);
        if (!pick.Found)
        {
            // повреждённый или занятый архив не помечаем: может, в следующий раз прочитается
            if (pick.Warning is null) _cache.PutNoImage(stamp);
            return null;
        }
        var raw = PreviewPicker.Extract(zipPath, pick.Entry!);
        var small = raw is null ? null : _shrink(raw);
        if (small is null) return null;
        return _cache.PutImage(stamp, small);
    }

    /// <summary>Пройти по списку архивов в фоне; onReady вызывается для каждого найденного (из фонового потока).</summary>
    public Task FillAsync(IEnumerable<(string Key, string ZipPath)> archives, Action<string, string> onReady, CancellationToken ct = default) =>
        Task.Run(() =>
        {
            foreach (var (key, path) in archives)
            {
                if (ct.IsCancellationRequested) return;
                if (Ensure(path) is { } p) onReady(key, p);
            }
        }, ct);
}
