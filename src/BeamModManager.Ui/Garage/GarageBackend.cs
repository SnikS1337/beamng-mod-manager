using BeamModManager.Core.Game;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Packs;
using BeamModManager.Core.Previews;
using BeamModManager.Core.Updates;
using BeamModManager.Core.Versioning;

namespace BeamModManager.Ui.Garage;

/// <summary>Всё, что гараж знает об одном моде на момент чтения.</summary>
public sealed record GarageMod(
    InstalledMod Mod,
    UpdateFinding? Update,
    ModBinding? Binding,
    DateTimeOffset? LastCheckedUtc,
    bool PageMissing,
    bool CheckingDeclined,
    string? ArchivePath,
    string? PreviewPath,
    string? ModType,
    string? Author)
{
    public bool Tracked => Binding is { Confirmed: true };
    public bool NeedsUpdate => Update is { IsDismissed: false };

    /// <summary>Размер файла на сайте при последней проверке (для «Размер 142 → 151 МБ» в «Обновлениях»).</summary>
    public long? SiteSizeBytes { get; init; }

    /// <summary>Когда проверка в последний раз заметила изменение на странице.</summary>
    public DateTimeOffset? SiteChangedUtc { get; init; }
}

/// <summary>
/// Удалённый мод (фильтр «Удалённые», решено 2026-10-17): файла в папке нет, копия лежит в резервных. Название и версия —
/// из имени файла (запись в библиотеке уже забыта); картинка — из кэша, если архив был показан до удаления.
/// </summary>
public sealed record DeletedGarageMod(DeletedMod Copy, string Name, string? Version, string? PreviewPath)
{
    /// <summary>Ключ карточки: не пересекается с ключами модов в библиотеке.</summary>
    public string Id => "deleted:" + Copy.FileName.ToLowerInvariant();
}

public sealed record GarageSnapshot(IReadOnlyList<GarageMod> Mods, bool FolderAvailable, DateTimeOffset? LastCheckedUtc)
{
    /// <summary>Удалённые моды, которые можно вернуть (фильтр «Удалённые»).</summary>
    public IReadOnlyList<DeletedGarageMod> Deleted { get; init; } = Array.Empty<DeletedGarageMod>();

    /// <summary>Библиотека не пишется на диск: что случилось (из <see cref="JsonLibraryStore.SaveError"/>).</summary>
    public string? StorageWarning { get; init; }

    /// <summary>Сообщение один раз при запуске: библиотека или настройки были повреждены и начаты заново.</summary>
    public string? Notice { get; init; }

    public static readonly GarageSnapshot NoFolder = new(Array.Empty<GarageMod>(), false, null);
}

/// <summary>
/// Удалённый мод, пока виден тост «Отменить»: файл уже в копиях, а запись в библиотеке (привязка страницы, история имён)
/// ещё цела — «Отменить» возвращает всё как было, а не «новый» мод без страницы.
/// </summary>
public sealed record DeletedTicket(string ModId, string FileName, string BackupId, bool WasInDisabledFolder);

public sealed record DeleteResult(bool Success, string? Message, DeletedTicket? Ticket);

/// <summary>Чем гараж пользуется снаружи. В тестах подменяется, в приложении — <see cref="LibraryGarageBackend"/>.</summary>
public interface IGarageBackend
{
    GarageSnapshot Load();
    bool IsGameRunning();
    ToggleResult SetEnabled(string fileName, bool enabled);

    /// <summary>Помощник внутри запущенной игры отвечает: моды можно переключать сразу, не дожидаясь закрытия игры.</summary>
    bool LiveAvailable => false;

    /// <summary>Передать помощнику нужное состояние модов (имя файла → включён). false — не получилось, ждём закрытия игры.</summary>
    bool TrySendLive(IReadOnlyDictionary<string, bool> wanted) => false;

    /// <summary>Ответ помощника на последнюю переданную команду; null — ещё не ответил (или помощника нет).</summary>
    BridgeAnswer? LiveAnswer() => null;

    /// <summary>«Удалить»: файл уходит в копии. Пока тост открыт, мод не показывается, но запись в библиотеке держится.</summary>
    DeleteResult Delete(GarageMod mod) => new(false, "Удалять моды здесь нельзя", null);

    /// <summary>«Отменить» удаление: файл возвращается на место, в ту же папку.</summary>
    OpResult Undelete(DeletedTicket ticket) => new(false, "Вернуть не получилось");

    /// <summary>Тост погас: удаление окончательное для библиотеки (копия файла остаётся в «Копиях»).</summary>
    void ForgetDeleted(DeletedTicket ticket) { }

    /// <summary>«Прошлые версии» мода, от новых к старым.</summary>
    IReadOnlyList<PastVersion> PastVersions(GarageMod mod) => Array.Empty<PastVersion>();

    /// <summary>«Вернуть» прошлую версию: текущий файл уходит в копии, выбранная встаёт на его место.</summary>
    OpResult RestoreVersion(GarageMod mod, PastVersion version) => new(false, "Вернуть не получилось");

    /// <summary>«Внутри архива»; null — архива нет на месте. Читает файл: звать не из потока окна.</summary>
    ArchiveOverview? Inside(GarageMod mod) => null;

    /// <summary>«Вернуть» удалённый мод: последняя копия встаёт в папку модов (игра включит его сама).</summary>
    OpResult RestoreDeleted(DeletedGarageMod mod) => new(false, "Вернуть не получилось");
}

/// <summary>Гараж поверх настоящих библиотеки, папки модов и db.json игры.</summary>
public sealed class LibraryGarageBackend : IGarageBackend
{
    private readonly ILibraryStore _store;
    private readonly ModInstaller _installer;
    private readonly PreviewCache _previews;
    private readonly Func<bool> _gameRunning;

    private string? _notice;

    /// <param name="startupNotice">Что сказать один раз при запуске (например, настройки сброшены); предупреждения библиотеки добавятся сами.</param>
    public LibraryGarageBackend(ILibraryStore store, ModInstaller installer, PreviewCache previews, Func<bool>? gameRunning = null, string? startupNotice = null)
    {
        var notes = new List<string>();
        if (!string.IsNullOrEmpty(startupNotice)) notes.Add(startupNotice);
        if (store is JsonLibraryStore { LoadWarnings.Count: > 0 } js) notes.Add("Библиотека: " + string.Join("; ", js.LoadWarnings));
        _notice = notes.Count == 0 ? null : string.Join(". ", notes);
        _store = store;
        _installer = installer;
        _previews = previews;
        _gameRunning = gameRunning ?? GameProcess.IsRunning;
    }

    /// <summary>
    /// Поиск процесса игры занимает десятки миллисекунд, а при переключении мода его спрашивают несколько раз подряд:
    /// ответ держится секунду.
    /// </summary>
    public bool IsGameRunning()
    {
        lock (_runningGate)
        {
            if (Environment.TickCount64 - _runningAt < 1000) return _running;
        }
        var r = _gameRunning();
        lock (_runningGate) { _running = r; _runningAt = Environment.TickCount64; }
        return r;
    }

    private readonly object _runningGate = new();
    private bool _running;
    private long _runningAt = long.MinValue / 2;

    public ToggleResult SetEnabled(string fileName, bool enabled) => GameDbSync.SetEnabled(_installer, fileName, enabled, _gameRunning);

    // ───────────── удаление с «Отменить» ─────────────

    /// <summary>Удалённые, пока открыт тост: имя файла → квитанция и как файл выглядел в папке (для сверки).</summary>
    private readonly Dictionary<string, (DeletedTicket Ticket, DiskMod Disk)> _deleted = new(StringComparer.OrdinalIgnoreCase);
    private readonly object _deletedGate = new();

    public DeleteResult Delete(GarageMod mod)
    {
        if (!_installer.ModsFolderAvailable) return new(false, "Папка модов недоступна", null);
        var disk = _installer.ScanDetailed().FirstOrDefault(d => string.Equals(d.FileName, mod.Mod.FileName, StringComparison.OrdinalIgnoreCase));
        if (disk is null) return new(false, "Файла этого мода уже нет в папке", null);
        // моды из репозитория игры скачивает и убирает сама игра: удалённый здесь она скачала бы снова
        if (disk.FromGameRepo) return new(false, "Этот мод скачан в самой игре. Удалите его там, в «Репозитории»", null);
        var inDisabled = File.Exists(Path.Combine(_installer.Layout.DisabledDir, disk.FileName));
        var r = _installer.Remove(disk.FileName);
        if (!r.Success || r.Path is null) return new(false, r.Message ?? "Удалить не получилось", null);
        var ticket = new DeletedTicket(mod.Mod.Id, disk.FileName, Path.GetFileName(r.Path), inDisabled);
        lock (_deletedGate) _deleted[disk.FileName] = (ticket, disk);
        return new(true, null, ticket);
    }

    public OpResult Undelete(DeletedTicket ticket)
    {
        lock (_deletedGate) _deleted.Remove(ticket.FileName);
        return _installer.Unremove(ticket.FileName, ticket.BackupId, ticket.WasInDisabledFolder);
    }

    public void ForgetDeleted(DeletedTicket ticket)
    {
        lock (_deletedGate)
        {
            if (!_deleted.Remove(ticket.FileName)) return;
        }
        // файл с тем же именем могли положить заново, пока висел тост: тогда запись нужна новому файлу
        if (_installer.ModsFolderAvailable && _installer.ScanDetailed().Any(d => string.Equals(d.FileName, ticket.FileName, StringComparison.OrdinalIgnoreCase))) return;
        if (_store.GetMod(ticket.ModId) is { } m && string.Equals(m.FileName, ticket.FileName, StringComparison.OrdinalIgnoreCase))
            _store.RemoveMod(ticket.ModId);
    }

    public OpResult RestoreDeleted(DeletedGarageMod mod)
    {
        if (!_installer.ModsFolderAvailable) return new OpResult(false, "Папка модов недоступна");
        return VersionHistory.RestoreDeleted(_installer, _store, mod.Copy, DateTimeOffset.UtcNow);
    }

    /// <summary>Удалённые для фильтра: копия последней версии, её картинка из кэша (по имени, размеру и времени файла).</summary>
    private IReadOnlyList<DeletedGarageMod> DeletedList(Func<ArchiveStamp, CacheState> cached, List<ArchiveStamp> stamps)
    {
        var result = new List<DeletedGarageMod>();
        foreach (var d in VersionHistory.Deleted(_installer, _store))
        {
            string? preview = null;
            if (_installer.ListBackups(d.FileName).FirstOrDefault() is { } last)
            {
                try
                {
                    var fi = new FileInfo(Path.Combine(_installer.Layout.BackupDir, d.FileName, last.Id));
                    // копия — тот же файл, перенесённый целиком: время и размер те же, имя — прежнее имя мода
                    var stamp = new ArchiveStamp(d.FileName, fi.Length, fi.LastWriteTimeUtc);
                    stamps.Add(stamp); // картинка удалённого не убирается из кэша, пока копия жива
                    if (cached(stamp) == CacheState.Image) preview = _previews.ImagePath(stamp);
                }
                catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
            }
            var g = BindingSuggester.FromFileName(d.FileName);
            var name = string.IsNullOrWhiteSpace(g.Name) ? Path.GetFileNameWithoutExtension(d.FileName) : g.Name;
            result.Add(new DeletedGarageMod(d, name, g.Version, preview));
        }
        return result;
    }

    // ───────────── прошлые версии и «Внутри архива» ─────────────

    public IReadOnlyList<PastVersion> PastVersions(GarageMod mod)
    {
        // мод из репозитория игры лежит в mods/repo: «Вернуть» поставило бы старую версию рядом с ним, вторым файлом
        if (!_installer.ModsFolderAvailable || IsRepo(mod)) return Array.Empty<PastVersion>();
        try { return VersionHistory.For(_installer, _store, mod.Mod); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return Array.Empty<PastVersion>(); }
    }

    public OpResult RestoreVersion(GarageMod mod, PastVersion version)
    {
        if (IsRepo(mod)) return new OpResult(false, "Этот мод скачан в самой игре: его версиями управляет она");
        return VersionHistory.Restore(_installer, _store, mod.Mod, version, DateTimeOffset.UtcNow);
    }

    private bool IsRepo(GarageMod mod) =>
        mod.ArchivePath is { } p && Path.GetDirectoryName(p) is { } dir &&
        string.Equals(Path.GetFullPath(dir).TrimEnd('\\', '/'), Path.GetFullPath(Path.Combine(_installer.Layout.ModsDir, ModInstaller.RepoFolder)).TrimEnd('\\', '/'),
            StringComparison.OrdinalIgnoreCase);

    /// <summary>Разобранные архивы по их отметке (имя, размер, время): заново читаем только изменившийся файл.</summary>
    private readonly Dictionary<ArchiveStamp, ArchiveOverview> _inside = new();

    public ArchiveOverview? Inside(GarageMod mod)
    {
        if (mod.ArchivePath is not { } path) return null;
        ArchiveStamp stamp;
        try { stamp = ArchiveStamp.Of(path); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return null; } // архив только что убрали
        lock (_inside) { if (_inside.TryGetValue(stamp, out var known)) return known; }
        var o = ArchiveOverviewReader.Read(path);
        lock (_inside)
        {
            if (_inside.Count >= 64) _inside.Clear(); // десятки разобранных архивов — память не копим
            _inside[stamp] = o;
        }
        return o;
    }

    private GameBridge Bridge => _bridge ??= new GameBridge(_installer.Layout.ModsDir);
    private GameBridge? _bridge;

    public bool LiveAvailable => _installer.ModsFolderAvailable && Bridge.IsAlive();

    public bool TrySendLive(IReadOnlyDictionary<string, bool> wanted)
    {
        if (!LiveAvailable) return false;
        _liveId = Bridge.Send(wanted);
        return _liveId is not null;
    }

    private string? _liveId;

    public BridgeAnswer? LiveAnswer() =>
        _liveId is { } id && _installer.ModsFolderAvailable && Bridge.ReadAnswer() is { } a && a.AckId == id ? a : null;

    public GarageSnapshot Load()
    {
        if (!_installer.ModsFolderAvailable) return GarageSnapshot.NoFolder;
        // папка есть, но пропала посреди чтения (флешка, сетевой диск) или её не дают читать: «не нашли папку», а не ошибка.
        // Сверку при этом не применяем: неполный список файлов выглядел бы как «моды удалили».
        try { return LoadCore(); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return GarageSnapshot.NoFolder; }
    }

    private GarageSnapshot LoadCore()
    {
        // помощник для переключения на ходу: кладём (или обновляем) один раз за запуск; запущенная игра подхватит его сама
        if (!_bridgeChecked) { _bridgeChecked = true; GameBridge.EnsureInstalled(_installer.Layout.ModsDir); }
        GameDbReadResult db;
        // папку читаем один раз: и для сверки, и для карточек
        var scan = _installer.ScanDetailed();
        // удалённые, пока открыт тост «Отменить»: для сверки их файл как будто на месте, иначе запись (страница, история имён)
        // пропала бы и «Отменить» вернуло бы мод без них. Файл с тем же именем снова появился — квитанция больше не нужна.
        HashSet<string> hidden;
        var forSync = scan;
        lock (_deletedGate)
        {
            foreach (var d in scan) _deleted.Remove(d.FileName);
            hidden = new HashSet<string>(_deleted.Keys, StringComparer.OrdinalIgnoreCase);
            if (_deleted.Count > 0) forSync = scan.Concat(_deleted.Values.Select(v => v.Disk)).ToList();
        }
        using ((_store as JsonLibraryStore)?.Batch())
        {
            db = GameModDb.ReadStable(GameModDb.PathIn(_installer.Layout.ModsDir));
            GameDbSync.Reconcile(_store, _installer, db, forSync);
            GameDbSync.ApplyRepoInfo(_store, db);
        }

        var disk = scan.ToDictionary(d => d.FileName, StringComparer.OrdinalIgnoreCase);
        var cached = _previews.Snapshot();
        var updates = UpdateCheckService.Pending(_store, includeDismissed: true).ToDictionary(f => f.ModId);
        var bindings = _store.Bindings().Where(b => b.Confirmed).ToLookup(b => b.ModId);
        var result = new List<GarageMod>();
        var stamps = new List<ArchiveStamp>();
        DateTimeOffset? lastAll = null;
        foreach (var m in _store.Mods())
        {
            if (hidden.Contains(m.FileName)) continue;
            var b = bindings[m.Id].FirstOrDefault();
            var c = b is null ? null : _store.GetCheck(m.Id, b.Source);
            if (c is not null && (lastAll is null || c.LastCheckedUtc > lastAll)) lastAll = c.LastCheckedUtc;
            var path = ArchivePath(m, disk);
            string? preview = null;
            if (path is not null)
            {
                try
                {
                    // размер и дата уже известны из списка папки — без отдельного обращения к каждому архиву
                    var d = disk[m.FileName];
                    var stamp = d.ModifiedUtc is { } mt ? new ArchiveStamp(d.FileName, d.SizeBytes, mt) : ArchiveStamp.Of(path);
                    stamps.Add(stamp);
                    if (cached(stamp) == CacheState.Image) preview = _previews.ImagePath(stamp);
                }
                catch (Exception e) when (e is IOException or UnauthorizedAccessException) { } // архив только что удалили или он занят
            }
            var entry = db.Find(m.FileName);
            // обновление может быть найдено не на первой привязке: размер и дату берём с того сайта, где нашли
            var u = updates.GetValueOrDefault(m.Id);
            var uc = u is null ? c : _store.GetCheck(m.Id, u.Source) ?? c;
            result.Add(new GarageMod(m, u, b, c?.LastCheckedUtc,
                c is { MissCount: >= UpdateCheckPlanner.DeadAfterMisses }, _store.IsCheckingDeclined(m.Id),
                path, preview, entry?.ModType, entry?.Repo?.Author)
            {
                SiteSizeBytes = uc?.LastSize,
                SiteChangedUtc = uc?.LastChangedUtc,
            });
        }
        IReadOnlyList<DeletedGarageMod> deleted;
        try { deleted = DeletedList(cached, stamps); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { deleted = Array.Empty<DeletedGarageMod>(); } // папку копий не прочитать
        // картинки удалённых и изменившихся архивов из кэша убираем раз за запуск
        if (!_pruned) { _pruned = true; _previews.Prune(stamps); }
        var warn = (_store as JsonLibraryStore)?.SaveError;
        var notice = _notice;
        _notice = null;
        return new GarageSnapshot(result, true, lastAll)
        {
            Deleted = deleted,
            StorageWarning = warn is null ? null : "Библиотека не сохраняется на диск: " + warn,
            Notice = notice,
        };
    }

    private bool _pruned;
    private bool _bridgeChecked;

    private string? ArchivePath(InstalledMod m, IReadOnlyDictionary<string, DiskMod> disk) =>
        disk.TryGetValue(m.FileName, out var d) ? _installer.PathOf(d) : null;
}
