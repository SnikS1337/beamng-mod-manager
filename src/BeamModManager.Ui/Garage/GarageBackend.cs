using BeamModManager.Core.Game;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
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
}

public sealed record GarageSnapshot(IReadOnlyList<GarageMod> Mods, bool FolderAvailable, DateTimeOffset? LastCheckedUtc)
{
    /// <summary>Библиотека не пишется на диск: что случилось (из <see cref="JsonLibraryStore.SaveError"/>).</summary>
    public string? StorageWarning { get; init; }

    /// <summary>Сообщение один раз при запуске: библиотека или настройки были повреждены и начаты заново.</summary>
    public string? Notice { get; init; }

    public static readonly GarageSnapshot NoFolder = new(Array.Empty<GarageMod>(), false, null);
}

/// <summary>Чем гараж пользуется снаружи. В тестах подменяется, в приложении — <see cref="LibraryGarageBackend"/>.</summary>
public interface IGarageBackend
{
    GarageSnapshot Load();
    bool IsGameRunning();
    ToggleResult SetEnabled(string fileName, bool enabled);
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

    public bool IsGameRunning() => _gameRunning();

    public ToggleResult SetEnabled(string fileName, bool enabled) => GameDbSync.SetEnabled(_installer, fileName, enabled, _gameRunning);

    public GarageSnapshot Load()
    {
        if (!_installer.ModsFolderAvailable) return GarageSnapshot.NoFolder;
        GameDbReadResult db;
        using ((_store as JsonLibraryStore)?.Batch())
        {
            GameDbSync.Reconcile(_store, _installer);
            db = GameModDb.Read(GameModDb.PathIn(_installer.Layout.ModsDir));
            GameDbSync.ApplyRepoInfo(_store, db);
        }

        var updates = UpdateCheckService.Pending(_store, includeDismissed: true).ToDictionary(f => f.ModId);
        var bindings = _store.Bindings().Where(b => b.Confirmed).ToLookup(b => b.ModId);
        var result = new List<GarageMod>();
        var stamps = new List<ArchiveStamp>();
        DateTimeOffset? lastAll = null;
        foreach (var m in _store.Mods())
        {
            var b = bindings[m.Id].FirstOrDefault();
            var c = b is null ? null : _store.GetCheck(m.Id, b.Source);
            if (c is not null && (lastAll is null || c.LastCheckedUtc > lastAll)) lastAll = c.LastCheckedUtc;
            var path = ArchivePath(m);
            string? preview = null;
            if (path is not null)
            {
                try
                {
                    var stamp = ArchiveStamp.Of(path);
                    stamps.Add(stamp);
                    if (_previews.State(stamp) == CacheState.Image) preview = _previews.ImagePath(stamp);
                }
                catch (Exception e) when (e is IOException or UnauthorizedAccessException) { } // архив только что удалили или он занят
            }
            var entry = db.Find(m.FileName);
            result.Add(new GarageMod(m, updates.GetValueOrDefault(m.Id), b, c?.LastCheckedUtc,
                c is { MissCount: >= UpdateCheckPlanner.DeadAfterMisses }, _store.IsCheckingDeclined(m.Id),
                path, preview, entry?.ModType, entry?.Repo?.Author));
        }
        // картинки удалённых и изменившихся архивов из кэша убираем раз за запуск
        if (!_pruned) { _pruned = true; _previews.Prune(stamps); }
        var warn = (_store as JsonLibraryStore)?.SaveError;
        var notice = _notice;
        _notice = null;
        return new GarageSnapshot(result, true, lastAll)
        {
            StorageWarning = warn is null ? null : "Библиотека не сохраняется на диск: " + warn,
            Notice = notice,
        };
    }

    private bool _pruned;

    private string? ArchivePath(InstalledMod m)
    {
        var l = _installer.Layout;
        foreach (var dir in m.Enabled ? new[] { l.ModsDir, l.DisabledDir } : new[] { l.DisabledDir, l.ModsDir })
        {
            var p = Path.Combine(dir, m.FileName);
            if (File.Exists(p)) return p;
        }
        return null;
    }
}
