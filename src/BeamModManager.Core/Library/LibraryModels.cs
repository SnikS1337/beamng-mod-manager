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
    long? LastSize,
    /// <summary>Сколько проверок подряд страница мода не нашлась (0 — страница есть). От <see cref="Updates.UpdateCheckPlanner.DeadAfterMisses"/> привязка считается потерянной.</summary>
    int MissCount = 0);

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

    /// <summary>«Пропустить эту версию»: ключ обновления, которое пользователь отклонил для мода (null — не отклонял).</summary>
    string? GetDismissedUpdate(string modId);
    void SetDismissedUpdate(string modId, string? key);
}

public sealed class InMemoryLibraryStore : ILibraryStore
{
    // Интерфейс могут читать из окна, пока идёт проверка в фоне: все обращения под одной блокировкой.
    private readonly object _g = new();
    private readonly Dictionary<string, InstalledMod> _mods = new();
    private readonly Dictionary<(string, string), ModBinding> _bindings = new();
    private readonly Dictionary<(string, string), CheckState> _checks = new();
    private readonly Dictionary<string, SourceStatus> _statuses = new();
    private readonly Dictionary<string, string> _dismissed = new();

    public IReadOnlyList<InstalledMod> Mods() { lock (_g) return _mods.Values.OrderBy(m => m.Name, StringComparer.OrdinalIgnoreCase).ToList(); }
    public InstalledMod? GetMod(string id) { lock (_g) return _mods.GetValueOrDefault(id); }
    public void UpsertMod(InstalledMod mod) { lock (_g) _mods[mod.Id] = mod; }

    public void RemoveMod(string id)
    {
        lock (_g)
        {
            _mods.Remove(id);
            foreach (var k in _bindings.Keys.Where(k => k.Item1 == id).ToList()) _bindings.Remove(k);
            foreach (var k in _checks.Keys.Where(k => k.Item1 == id).ToList()) _checks.Remove(k);
            _dismissed.Remove(id);
        }
    }

    public string? GetDismissedUpdate(string modId) { lock (_g) return _dismissed.GetValueOrDefault(modId); }

    public void SetDismissedUpdate(string modId, string? key)
    {
        lock (_g)
        {
            if (!_mods.ContainsKey(modId)) return; // мода уже нет: ничего не запоминаем
            if (string.IsNullOrEmpty(key)) _dismissed.Remove(modId); else _dismissed[modId] = key;
        }
    }

    public IReadOnlyList<ModBinding> Bindings() { lock (_g) return _bindings.Values.ToList(); }

    public void UpsertBinding(ModBinding b)
    {
        lock (_g)
        {
            if (!_mods.ContainsKey(b.ModId)) throw new InvalidOperationException($"Нет мода {b.ModId}");
            _bindings[(b.ModId, b.Source)] = b;
        }
    }

    public void RemoveBinding(string modId, string source)
    {
        lock (_g)
        {
            _bindings.Remove((modId, source));
            _checks.Remove((modId, source));
        }
    }

    public CheckState? GetCheck(string modId, string source) { lock (_g) return _checks.GetValueOrDefault((modId, source)); }

    /// <summary>Состояние проверки для привязки, которой уже нет (мод удалили во время проверки), не сохраняется.</summary>
    public void SaveCheck(CheckState s) { lock (_g) { if (_bindings.ContainsKey((s.ModId, s.Source))) _checks[(s.ModId, s.Source)] = s; } }
    public IReadOnlyList<CheckState> Checks() { lock (_g) return _checks.Values.ToList(); }

    public SourceStatus GetSourceStatus(string source) { lock (_g) return _statuses.GetValueOrDefault(source, SourceStatus.Ok); }
    public void SetSourceStatus(string source, SourceStatus status) { lock (_g) _statuses[source] = status; }
    public IReadOnlyDictionary<string, SourceStatus> SourceStatuses() { lock (_g) return new Dictionary<string, SourceStatus>(_statuses); }
}
