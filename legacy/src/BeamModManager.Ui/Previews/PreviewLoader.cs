using BeamModManager.Core.Packs;
using BeamModManager.Core.Previews;

namespace BeamModManager.Ui.Previews;

/// <summary>
/// Достаёт картинки для карточек в фоне: одна картинка из архива → уменьшение (делает окно, WPF) → кэш.
/// Архив, где картинок нет, помечается, чтобы не перечитывать его при каждом запуске. Тем же проходом — название мода
/// из его info.json (<see cref="ArchiveOverviewReader.TitleOf"/>), тоже в кэш.
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

    /// <summary>
    /// Название из архива: Known — известно (из кэша или прочитано сейчас, Read), Title — само название или null (в архиве
    /// его нет). Не Known — архива нет или он не открылся, спросим в другой раз.
    /// </summary>
    public (bool Known, string? Title, bool Read) EnsureTitle(string zipPath)
    {
        ArchiveStamp stamp;
        try
        {
            if (!File.Exists(zipPath)) return (false, null, false);
            stamp = ArchiveStamp.Of(zipPath);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return (false, null, false); }
        if (_cache.TryGetTitle(stamp, out var cached)) return (true, cached, false);
        var t = ArchiveOverviewReader.TitleOf(zipPath);
        if (!t.Known) return (false, null, false);
        _cache.PutTitle(stamp, t.Title);
        return (true, t.Title, true);
    }

    /// <summary>
    /// Пройти по списку архивов в фоне; onReady — для каждой найденной картинки, onTitle — для каждого известного названия,
    /// в том числе уже лежавшего в кэше: карточка могла собраться раньше, чем его туда положили (оба из фонового потока).
    /// Названия пишутся на диск пачками и в конце.
    /// </summary>
    public Task FillAsync(IEnumerable<(string Key, string ZipPath)> archives, Action<string, string> onReady, CancellationToken ct = default,
        Action<string, string?>? onTitle = null) =>
        Task.Run(() =>
        {
            var n = 0;
            try
            {
                foreach (var (key, path) in archives)
                {
                    if (ct.IsCancellationRequested) return;
                    if (onTitle is not null && EnsureTitle(path) is (true, var title, _)) onTitle(key, title);
                    if (Ensure(path) is { } p) onReady(key, p);
                    if (++n % 50 == 0) _cache.SaveTitles();
                }
            }
            finally { _cache.SaveTitles(); }
        }, ct);
}
