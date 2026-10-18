using BeamModManager.Core.Game;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Packs;
using BeamModManager.Core.Previews;
using BeamModManager.Core.Updates;
using BeamModManager.Core.Text;
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
    /// <summary>Для какой версии игры мод, по сайту привязанной страницы (null — сайт не пишет или страницы нет).</summary>
    public string? SiteGameVersion { get; init; }

    /// <summary>Название из репозитория игры (у модов, скачанных в самой игре).</summary>
    public string? RepoTitle { get; init; }

    /// <summary>Название из собственного info.json мода (<see cref="ArchiveTitleKnown"/> — архив уже читали).</summary>
    public string? ArchiveTitle { get; init; }

    /// <summary>Архив уже читали ради названия (оно в кэше, может быть «нет»): фону открывать его не нужно.</summary>
    public bool ArchiveTitleKnown { get; init; }

    /// <summary>Заголовок страницы, где следим за модом, как он был на сайте.</summary>
    public string? SiteTitle { get; init; }

    /// <summary>Название для карточки и панели (DESIGN_FLOWS 3.1а): репозиторий → info.json → сайт → имя файла.</summary>
    public string Title => ModTitles.Choose(RepoTitle, ArchiveTitle, SiteTitle, Mod.Name);
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

/// <summary>
/// Что поставили из пришедшего архива — чтобы «Отменить» вернул как было: новый файл уходит в копии, а прежняя версия
/// (если это было обновление) встаёт обратно из своей копии, в ту же папку и с той же записью в библиотеке.
/// </summary>
public sealed record IncomingTicket(string FileName, string? Replaced, string? ReplacedBackupId, bool ReplacedWasDisabled,
    string? PrevVersion, long? PrevSize, bool SameName);

public sealed record IncomingResult(bool Success, string? Message, IncomingTicket? Ticket);

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

    /// <summary>Версия установленной игры («0.39.4.0»); null — не узнать (игра не запускалась, папка не та).</summary>
    string? GameVersion => null;

    /// <summary>Разобрать скачанный или перетащенный архив: какие в нём моды и чем станет каждый (долго — звать не из окна).</summary>
    IncomingArchive InspectIncoming(string path, CancellationToken ct) => IncomingArchive.Fail(path, "Установка здесь недоступна");

    /// <summary>Поставить мод из разобранного архива: новым или обновлением, с копией прежней версии.</summary>
    IncomingResult InstallIncoming(IncomingMod mod) => new(false, "Установка здесь недоступна", null);

    /// <summary>«Отменить» после установки из архива.</summary>
    OpResult UndoIncoming(IncomingTicket ticket) => new(false, "Отменить не получилось");

    /// <summary>Архив больше не нужен (поставили или закрыли проверку): убрать то, что собрали во временной папке.</summary>
    void DiscardIncoming(IncomingArchive archive) { }
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
    private readonly IArchiveExtractor? _extractor;
    private readonly string _incomingRoot;

    /// <param name="extractor">Распаковщик RAR и 7z (в окне — tar из Windows); null — только zip.</param>
    /// <param name="incomingRoot">Где разбирать пришедшие архивы; по умолчанию во временной папке Windows.</param>
    public LibraryGarageBackend(ILibraryStore store, ModInstaller installer, PreviewCache previews, Func<bool>? gameRunning = null, string? startupNotice = null,
        IArchiveExtractor? extractor = null, string? incomingRoot = null)
    {
        _extractor = extractor;
        _incomingRoot = incomingRoot ?? Path.Combine(Path.GetTempPath(), "BeamModManager", "incoming");
        CleanupIncoming(TimeSpan.FromDays(1));
        var notes = new List<string>();
        if (!string.IsNullOrEmpty(startupNotice)) notes.Add(startupNotice);
        if (store is JsonLibraryStore { LoadWarnings.Count: > 0 } js) notes.Add("Данные программы: " + string.Join("; ", js.LoadWarnings));
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
            string? preview = null, title = null;
            if (_installer.ListBackups(d.FileName).FirstOrDefault() is { } last)
            {
                try
                {
                    var fi = new FileInfo(Path.Combine(_installer.Layout.BackupDir, d.FileName, last.Id));
                    // копия — тот же файл, перенесённый целиком: время и размер те же, имя — прежнее имя мода
                    var stamp = new ArchiveStamp(d.FileName, fi.Length, fi.LastWriteTimeUtc);
                    stamps.Add(stamp); // картинка удалённого не убирается из кэша, пока копия жива
                    if (cached(stamp) == CacheState.Image) preview = _previews.ImagePath(stamp);
                    _previews.TryGetTitle(stamp, out title); // копия — тот же архив: название прочитано, пока мод стоял
                }
                catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
            }
            var g = BindingSuggester.FromFileName(d.FileName);
            var name = title ?? (string.IsNullOrWhiteSpace(g.Name) ? Path.GetFileNameWithoutExtension(d.FileName) : g.Name);
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

    // ───────────── версия игры (совместимость, 2026-10-18) ─────────────

    private string? _gameVersion;
    private long _gameVersionAt = long.MinValue / 2;

    /// <summary>Из BeamNG.drive.ini или beamng.log папки пользователя (над папкой mods); перечитывается не чаще раза в минуту.</summary>
    public string? GameVersion
    {
        get
        {
            if (Environment.TickCount64 - _gameVersionAt < 60_000) return _gameVersion;
            _gameVersionAt = Environment.TickCount64;
            var user = Directory.GetParent(Path.GetFullPath(_installer.Layout.ModsDir).TrimEnd('\\', '/'))?.FullName;
            _gameVersion = user is null ? null : GameInfo.DetectVersion(user);
            return _gameVersion;
        }
    }

    // ───────────── установка из скачанного или перетащенного архива (2026-10-18) ─────────────

    /// <summary>Состав установленных архивов по их отметке: при каждом новом архиве заново читаем только изменившиеся.</summary>
    private readonly Dictionary<ArchiveStamp, Core.Packs.ArchiveContent> _contents = new();

    public IncomingArchive InspectIncoming(string path, CancellationToken ct)
    {
        if (!_installer.ModsFolderAvailable) return IncomingArchive.Fail(path, "Папка модов недоступна");
        var installed = new List<InstalledArchive>();
        var seen = new HashSet<ArchiveStamp>();
        foreach (var d in _installer.ScanDetailed())
        {
            ct.ThrowIfCancellationRequested();
            // моды из подпапок (скачанные в игре и разложенные вручную) тоже сравниваем: обновление такого программа не
            // поставит (Locked), но и вторую копию рядом не положит
            var p = _installer.PathOf(d);
            ArchiveStamp stamp;
            try { stamp = ArchiveStamp.Of(p); }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { continue; }
            seen.Add(stamp);
            Core.Packs.ArchiveContent? content;
            lock (_contents) _contents.TryGetValue(stamp, out content);
            if (content is null)
            {
                try { using var f = new FileStream(p, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete); content = PackContentReader.Read(f); }
                catch (Exception e) when (e is IOException or UnauthorizedAccessException) { content = Core.Packs.ArchiveContent.Empty; }
                lock (_contents) _contents[stamp] = content;
            }
            installed.Add(new InstalledArchive(d.FileName, d.Enabled, d.SizeBytes, p, content, d.SubFolder));
        }
        lock (_contents) foreach (var k in _contents.Keys.Where(k => !seen.Contains(k)).ToList()) _contents.Remove(k); // ушедшие не копим
        var work = Path.Combine(_incomingRoot, Guid.NewGuid().ToString("N"));
        try { return IncomingAnalyzer.Analyze(path, installed, work, _extractor, ct); }
        catch
        {
            TryDelete(work);
            throw;
        }
    }

    public IncomingResult InstallIncoming(IncomingMod mod)
    {
        if (mod.Blocked) return new(false, "Архив не прошёл проверку, ставить его нельзя", null);
        if (mod.Kind == IncomingKind.AlreadyInstalled) return new(false, "Этот мод уже установлен", null);
        if (mod.Locked) return new(false, $"Установленная версия лежит в подпапке «{mod.ReplacesSubFolder}» — такие моды программа не трогает", null);
        if (!_installer.ModsFolderAvailable) return new(false, "Папка модов недоступна", null);
        var disk = _installer.ScanDetailed();
        var old = mod.Kind == IncomingKind.Update && mod.Replaces is { } rep
            ? disk.FirstOrDefault(d => string.Equals(d.FileName, rep, StringComparison.OrdinalIgnoreCase) && !d.FromGameRepo)
            : null;
        var now = DateTimeOffset.UtcNow;
        var options = new InstallOptions { AllowWarnings = true }; // человек видел предупреждения и нажал «Установить»
        try
        {
            using var src = new FileStream(mod.ZipPath, FileMode.Open, FileAccess.Read, FileShare.Read);
            if (old is null)
            {
                var res = _installer.Install(src, mod.FileName, options);
                if (!res.Success) return new(false, Plain(res.Message), null);
                LibrarySync.RecordInstalled(_store, mod.FileName, mod.Size, null, enabled: true, now);
                return new(true, null, new IncomingTicket(mod.FileName, null, null, false, null, null, false));
            }
            var record = _store.Mods().FirstOrDefault(m => string.Equals(m.FileName, old.FileName, StringComparison.OrdinalIgnoreCase));
            var up = _installer.Update(old.FileName, src, mod.FileName, options);
            if (!up.Install.Success) return new(false, Plain(up.Install.Message), null);
            var sameName = string.Equals(old.FileName, mod.FileName, StringComparison.OrdinalIgnoreCase);
            var backup = _installer.ListBackups(old.FileName).FirstOrDefault()?.Id;
            LibrarySync.RecordInstalled(_store, mod.FileName, mod.Size, SiteVersion(record, mod.Size), up.WasEnabled, now, replacesFileName: old.FileName);
            return new(true, up.Install.Message, new IncomingTicket(mod.FileName, old.FileName, backup, !up.WasEnabled, record?.Version, record?.SizeBytes ?? old.SizeBytes, sameName));
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            return new(false, "Архив не читается: " + e.Message, null);
        }
    }

    /// <summary>Версия новой установки: та, что показывает сайт привязанного мода, если размер совпал; иначе пусть угадает имя файла.</summary>
    private string? SiteVersion(InstalledMod? record, long size)
    {
        if (record is null) return null;
        foreach (var b in _store.Bindings().Where(x => x.ModId == record.Id && x.Confirmed))
            if (_store.GetCheck(record.Id, b.Source) is { LastVersion: { } v, LastSize: { } s } && !SizeComparer.Differs(size, s))
                return v;
        return null;
    }

    public OpResult UndoIncoming(IncomingTicket t)
    {
        if (!_installer.ModsFolderAvailable) return new OpResult(false, "Папка модов недоступна");
        var now = DateTimeOffset.UtcNow;
        if (t.Replaced is null)
        {
            var gone = _installer.Remove(t.FileName);
            if (!gone.Success) return gone;
            if (_store.Mods().FirstOrDefault(m => string.Equals(m.FileName, t.FileName, StringComparison.OrdinalIgnoreCase)) is { } rec) _store.RemoveMod(rec.Id);
            return new OpResult(true);
        }
        if (t.ReplacedBackupId is null) return new OpResult(false, "Копия прошлой версии пропала");
        OpResult back;
        if (t.SameName) back = _installer.RestoreBackup(t.Replaced, t.ReplacedBackupId); // новая сама уходит в копии
        else
        {
            var gone = _installer.Remove(t.FileName);
            if (!gone.Success) return gone;
            back = _installer.Unremove(t.Replaced, t.ReplacedBackupId, t.ReplacedWasDisabled);
        }
        if (!back.Success) return back;
        LibrarySync.RecordInstalled(_store, t.Replaced, t.PrevSize ?? 0, t.PrevVersion, !t.ReplacedWasDisabled, now, replacesFileName: t.FileName);
        return new OpResult(true);
    }

    public void DiscardIncoming(IncomingArchive archive)
    {
        // только своя временная папка: путь пришёл из разбора, но лишний раз не доверяем
        if (archive.WorkDir is { } dir && IsUnder(dir, _incomingRoot)) TryDelete(dir);
    }

    /// <summary>Брошенные разборы (программу закрыли посреди проверки) старше суток убираются при запуске.</summary>
    private void CleanupIncoming(TimeSpan olderThan)
    {
        try
        {
            if (!Directory.Exists(_incomingRoot)) return;
            foreach (var d in Directory.GetDirectories(_incomingRoot))
                if (DateTime.UtcNow - Directory.GetCreationTimeUtc(d) > olderThan) TryDelete(d);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
    }

    private static bool IsUnder(string path, string root) =>
        Path.GetFullPath(path).StartsWith(Path.GetFullPath(root).TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase);

    private static void TryDelete(string dir)
    {
        try { if (Directory.Exists(dir)) Directory.Delete(dir, recursive: true); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
    }

    /// <summary>Сообщения установщика — для человека: без точки в конце (тост сам по себе фраза).</summary>
    private static string Plain(string? m) => string.IsNullOrEmpty(m) ? "Установить не получилось" : m.TrimEnd('.');

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
            string? preview = null, archiveTitle = null;
            var titleKnown = false;
            if (path is not null)
            {
                try
                {
                    // размер и дата уже известны из списка папки — без отдельного обращения к каждому архиву
                    var d = disk[m.FileName];
                    var stamp = d.ModifiedUtc is { } mt ? new ArchiveStamp(d.FileName, d.SizeBytes, mt) : ArchiveStamp.Of(path);
                    stamps.Add(stamp);
                    if (cached(stamp) == CacheState.Image) preview = _previews.ImagePath(stamp);
                    titleKnown = _previews.TryGetTitle(stamp, out archiveTitle);
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
                SiteGameVersion = uc?.LastGameVersion ?? c?.LastGameVersion,
                RepoTitle = entry?.Repo?.Title,
                ArchiveTitle = archiveTitle,
                ArchiveTitleKnown = titleKnown,
                SiteTitle = c?.LastTitle,
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
            StorageWarning = warn is null ? null : "Изменения не сохраняются на диск: " + warn,
            Notice = notice,
        };
    }

    private bool _pruned;
    private bool _bridgeChecked;

    private string? ArchivePath(InstalledMod m, IReadOnlyDictionary<string, DiskMod> disk) =>
        disk.TryGetValue(m.FileName, out var d) ? _installer.PathOf(d) : null;
}
