using BeamModManager.Core.Sources;

namespace BeamModManager.Core.Library;

/// <summary>Установленный мод в библиотеке.</summary>
public sealed record InstalledMod(
    string Id,
    string Name,
    string FileName,
    string? Version,
    long? SizeBytes,
    DateTimeOffset InstalledAtUtc,
    bool Enabled = true);

/// <summary>Связь установленного мода с записью на сайте. Неподтверждённые связи в проверке не участвуют.</summary>
public sealed record ModBinding(
    string ModId,
    string Source,
    string SourceId,
    Uri Url,
    bool Confirmed);

/// <summary>Что сайт показывал при последней проверке. Статус «есть обновление» не хранится, а считается при чтении.</summary>
public sealed record CheckState(
    string ModId,
    string Source,
    DateTimeOffset LastCheckedUtc,
    DateTimeOffset? LastChangedUtc,
    string? LastVersion,
    long? LastSize);

/// <summary>
/// Хранилище библиотеки. Сейчас есть версия в памяти (для тестов и разработки без Windows);
/// версия на SQLite добавится, когда будет доступен NuGet.
/// </summary>
public interface ILibraryStore
{
    IReadOnlyList<InstalledMod> Mods();
    InstalledMod? GetMod(string id);
    void UpsertMod(InstalledMod mod);
    /// <summary>Удаляет мод вместе с его привязками и состоянием проверок.</summary>
    void RemoveMod(string id);

    IReadOnlyList<ModBinding> Bindings();
    void UpsertBinding(ModBinding binding);
    void RemoveBinding(string modId, string source);

    CheckState? GetCheck(string modId, string source);
    void SaveCheck(CheckState state);
    IReadOnlyList<CheckState> Checks();

    SourceStatus GetSourceStatus(string source);
    void SetSourceStatus(string source, SourceStatus status);
    IReadOnlyDictionary<string, SourceStatus> SourceStatuses();
}

public sealed class InMemoryLibraryStore : ILibraryStore
{
    private readonly Dictionary<string, InstalledMod> _mods = new();
    private readonly Dictionary<(string, string), ModBinding> _bindings = new();
    private readonly Dictionary<(string, string), CheckState> _checks = new();
    private readonly Dictionary<string, SourceStatus> _statuses = new();

    public IReadOnlyList<InstalledMod> Mods() => _mods.Values.OrderBy(m => m.Name, StringComparer.OrdinalIgnoreCase).ToList();
    public InstalledMod? GetMod(string id) => _mods.GetValueOrDefault(id);
    public void UpsertMod(InstalledMod mod) => _mods[mod.Id] = mod;

    public void RemoveMod(string id)
    {
        _mods.Remove(id);
        foreach (var k in _bindings.Keys.Where(k => k.Item1 == id).ToList()) _bindings.Remove(k);
        foreach (var k in _checks.Keys.Where(k => k.Item1 == id).ToList()) _checks.Remove(k);
    }

    public IReadOnlyList<ModBinding> Bindings() => _bindings.Values.ToList();

    public void UpsertBinding(ModBinding b)
    {
        if (!_mods.ContainsKey(b.ModId)) throw new InvalidOperationException($"Нет мода {b.ModId}");
        _bindings[(b.ModId, b.Source)] = b;
    }

    public void RemoveBinding(string modId, string source)
    {
        _bindings.Remove((modId, source));
        _checks.Remove((modId, source));
    }

    public CheckState? GetCheck(string modId, string source) => _checks.GetValueOrDefault((modId, source));
    public void SaveCheck(CheckState s) => _checks[(s.ModId, s.Source)] = s;
    public IReadOnlyList<CheckState> Checks() => _checks.Values.ToList();

    public SourceStatus GetSourceStatus(string source) => _statuses.GetValueOrDefault(source, SourceStatus.Ok);
    public void SetSourceStatus(string source, SourceStatus status) => _statuses[source] = status;
    public IReadOnlyDictionary<string, SourceStatus> SourceStatuses() => new Dictionary<string, SourceStatus>(_statuses);
}
