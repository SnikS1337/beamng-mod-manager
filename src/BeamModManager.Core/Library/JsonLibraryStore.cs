using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Serialization;
using BeamModManager.Core.Sources;

namespace BeamModManager.Core.Library;

/// <summary>
/// Библиотека в файле JSON: то же поведение, что <see cref="InMemoryLibraryStore"/>, плюс сохранение между запусками.
/// Заменяет SQLite на первое время (решено 2026-10-15: SQLite требует пакетов, а библиотеке из сотен модов хватает файла).
/// Каждое изменение пишется сразу, атомарно (временный файл и переименование); серию изменений можно объединить в одну
/// запись через <see cref="Batch"/>. Повреждённый файл откладывается в *.bad-&lt;время&gt;, библиотека начинается заново и
/// пересобирается сверкой с папкой модов; причина в <see cref="LoadWarnings"/>.
/// </summary>
public sealed class JsonLibraryStore : ILibraryStore
{
    private const int CurrentSchema = 1;
    private const long MaxBytes = 64L * 1024 * 1024;
    private readonly InMemoryLibraryStore _m = new();
    private readonly string _path;
    private readonly Func<DateTimeOffset> _now;
    private readonly object _g = new();
    private int _batch;
    private bool _dirty;

    public IReadOnlyList<string> LoadWarnings { get; }

    /// <summary>
    /// Почему последняя запись не удалась (диск полон, файл держит антивирус или облачная папка); null — всё на диске.
    /// Изменения при этом не теряются: остаются в памяти и пишутся при следующем изменении или <see cref="Flush"/>.
    /// </summary>
    public string? SaveError { get { lock (_g) return _saveError; } }

    private string? _saveError;

    /// <summary>
    /// Файл библиотеки есть, но не прочитался (занят другой программой). Перезаписывать его нельзя: в нём привязки и история,
    /// которые сверкой с папкой не восстановить. До перезапуска менеджер работает только в памяти.
    /// </summary>
    private bool _saveBlocked;

    public JsonLibraryStore(string path, Func<DateTimeOffset>? now = null)
    {
        _path = path;
        _now = now ?? (() => DateTimeOffset.UtcNow);
        var w = new List<string>();
        Load(w);
        LoadWarnings = w;
    }

    public string Path => _path;

    /// <summary>Объединяет серию изменений в одну запись на диск (например, одна проверка обновлений). Вложенные можно.</summary>
    public IDisposable Batch()
    {
        lock (_g) _batch++;
        return new BatchScope(this);
    }

    private sealed class BatchScope : IDisposable
    {
        private JsonLibraryStore? _s;
        public BatchScope(JsonLibraryStore s) { _s = s; }
        public void Dispose()
        {
            var s = Interlocked.Exchange(ref _s, null);
            if (s is null) return;
            lock (s._g) { s._batch--; if (s._batch == 0 && s._dirty) s.SaveLocked(); }
        }
    }

    /// <summary>Записать несохранённое (например, при закрытии окна). true — на диске всё актуально.</summary>
    public bool Flush()
    {
        lock (_g)
        {
            if (_dirty) SaveLocked();
            return !_dirty;
        }
    }

    private void Changed()
    {
        lock (_g) { _dirty = true; if (_batch == 0) SaveLocked(); }
    }

    // ---- чтение: из памяти
    public IReadOnlyList<InstalledMod> Mods() => _m.Mods();
    public InstalledMod? GetMod(string id) => _m.GetMod(id);
    public IReadOnlyList<ModBinding> Bindings() => _m.Bindings();
    public CheckState? GetCheck(string modId, string source) => _m.GetCheck(modId, source);
    public IReadOnlyList<CheckState> Checks() => _m.Checks();
    public SourceStatus GetSourceStatus(string source) => _m.GetSourceStatus(source);
    public IReadOnlyDictionary<string, SourceStatus> SourceStatuses() => _m.SourceStatuses();
    public string? GetDismissedUpdate(string modId) => _m.GetDismissedUpdate(modId);
    public IReadOnlyList<string> FileHistory(string modId) => _m.FileHistory(modId);
    public bool IsCheckingDeclined(string modId) => _m.IsCheckingDeclined(modId);
    public PageSearch? GetPageSearch(string modId) => _m.GetPageSearch(modId);

    // ---- изменения: в память и на диск
    // сверка с папкой при каждом возврате в окно переписывает те же записи: если ничего не изменилось, файл не трогаем
    public void UpsertMod(InstalledMod mod) { lock (_g) { if (_m.GetMod(mod.Id) == mod) return; _m.UpsertMod(mod); Changed(); } }
    public void RemoveMod(string id) { lock (_g) { _m.RemoveMod(id); Changed(); } }
    public void UpsertBinding(ModBinding binding)
    {
        lock (_g)
        {
            if (_m.Bindings().Contains(binding) && !(binding.Confirmed && _m.IsCheckingDeclined(binding.ModId))) return;
            _m.UpsertBinding(binding);
            Changed();
        }
    }
    public void RemoveBinding(string modId, string source) { lock (_g) { _m.RemoveBinding(modId, source); Changed(); } }
    public void SaveCheck(CheckState state) { lock (_g) { if (_m.GetCheck(state.ModId, state.Source) == state) return; _m.SaveCheck(state); Changed(); } }
    public void SetSourceStatus(string source, SourceStatus status) { lock (_g) { if (_m.SourceStatuses().TryGetValue(source, out var s) && s == status) return; _m.SetSourceStatus(source, status); Changed(); } }
    public void SetDismissedUpdate(string modId, string? key) { lock (_g) { if (_m.GetDismissedUpdate(modId) == (string.IsNullOrEmpty(key) ? null : key)) return; _m.SetDismissedUpdate(modId, key); Changed(); } }
    public void AddFileHistory(string modId, string fileName) { lock (_g) { _m.AddFileHistory(modId, fileName); Changed(); } }
    public void SetCheckingDeclined(string modId, bool declined) { lock (_g) { _m.SetCheckingDeclined(modId, declined); Changed(); } }
    public void SetPageSearch(string modId, PageSearch? state) { lock (_g) { if (_m.GetPageSearch(modId) == state) return; _m.SetPageSearch(modId, state); Changed(); } }

    // ---- файл
    private sealed record FileModel(
        int Schema,
        List<InstalledMod> Mods,
        List<ModBinding> Bindings,
        List<CheckState> Checks,
        Dictionary<string, SourceStatus> Sources,
        Dictionary<string, string> Dismissed,
        Dictionary<string, List<string>> History,
        List<string> Declined,
        // с 0.3.9: подбор страниц; в прежних файлах его нет
        Dictionary<string, PageSearch>? PageSearches = null);

    private static readonly JsonSerializerOptions Json = new()
    {
        WriteIndented = true,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        Converters = { new JsonStringEnumConverter() },
        AllowTrailingCommas = true,
        ReadCommentHandling = JsonCommentHandling.Skip,
    };

    private void SaveLocked()
    {
        var mods = _m.Mods().ToList();
        var model = new FileModel(CurrentSchema, mods, _m.Bindings().ToList(), _m.Checks().ToList(),
            _m.SourceStatuses().ToDictionary(k => k.Key, v => v.Value),
            mods.Select(m => (m.Id, D: _m.GetDismissedUpdate(m.Id))).Where(x => x.D is not null).ToDictionary(x => x.Id, x => x.D!),
            mods.Select(m => (m.Id, H: _m.FileHistory(m.Id))).Where(x => x.H.Count > 0).ToDictionary(x => x.Id, x => x.H.ToList()),
            mods.Where(m => _m.IsCheckingDeclined(m.Id)).Select(m => m.Id).ToList(),
            mods.Select(m => (m.Id, P: _m.GetPageSearch(m.Id))).Where(x => x.P is not null).ToDictionary(x => x.Id, x => x.P!));
        if (_saveBlocked)
        {
            _saveError = "файл библиотеки был занят при запуске; чтобы не потерять его, изменения не пишутся до перезапуска";
            return;
        }
        // своё имя временного файла у каждого процесса: два менеджера не портят записи друг друга
        var tmp = $"{_path}.{Environment.ProcessId}.tmp";
        try
        {
            Directory.CreateDirectory(System.IO.Path.GetDirectoryName(System.IO.Path.GetFullPath(_path))!);
            Safety.DurableFile.WriteAllText(tmp, JsonSerializer.Serialize(model, Json)); // на диск до замены
            File.Move(tmp, _path, overwrite: true);
            _dirty = false;
            _saveError = null;
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            // правка уже в памяти: окно не роняем, запишем при следующем изменении
            _saveError = e.Message;
            try { File.Delete(tmp); } catch (Exception e2) when (e2 is IOException or UnauthorizedAccessException) { }
        }
    }

    private void Load(List<string> w)
    {
        if (!File.Exists(_path)) return;
        FileModel? f;
        string? text = null;
        for (var attempt = 0; text is null; attempt++)
        {
            try
            {
                if (new FileInfo(_path).Length > MaxBytes) { Quarantine("файл библиотеки слишком большой", w); return; }
                text = File.ReadAllText(_path);
            }
            catch (Exception e) when ((e is IOException or UnauthorizedAccessException) && attempt < 3)
            {
                Thread.Sleep(150 * (attempt + 1)); // антивирус и облачные папки обычно держат файл недолго
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException)
            {
                _saveBlocked = true;
                w.Add($"библиотека не прочиталась ({e.Message}); изменения не сохранятся до перезапуска программы");
                return;
            }
        }
        try { f = JsonSerializer.Deserialize<FileModel>(text, Json); }
        catch (JsonException) { Quarantine("файл библиотеки повреждён", w); return; }
        catch (NotSupportedException) { Quarantine("файл библиотеки имеет неверный формат", w); return; }
        if (f?.Mods is null) { Quarantine("файл библиотеки имеет неверный формат", w); return; }

        var skipped = 0;
        foreach (var m in f.Mods)
            if (m is not null && !string.IsNullOrWhiteSpace(m.Id) && !string.IsNullOrWhiteSpace(m.FileName)) _m.UpsertMod(m); else skipped++;
        foreach (var b in f.Bindings ?? new())
        {
            try { if (b?.Url is not null) _m.UpsertBinding(b); else skipped++; }
            catch (InvalidOperationException) { skipped++; } // привязка к моду, которого нет
        }
        foreach (var c in f.Checks ?? new()) if (c is not null) _m.SaveCheck(c);
        foreach (var (k, v) in f.Sources ?? new()) _m.SetSourceStatus(k, v);
        foreach (var (k, v) in f.Dismissed ?? new()) _m.SetDismissedUpdate(k, v);
        foreach (var (k, list) in f.History ?? new()) foreach (var n in list ?? new()) _m.AddFileHistory(k, n);
        foreach (var id in f.Declined ?? new()) _m.SetCheckingDeclined(id, true); // после привязок: подтверждённая привязка снимает отказ
        foreach (var (k, v) in f.PageSearches ?? new()) if (v is not null) _m.SetPageSearch(k, v);
        if (skipped > 0) w.Add($"записей библиотеки пропущено из-за неверных данных: {skipped}");
    }

    private void Quarantine(string why, List<string> w)
    {
        string? moved = null;
        try { moved = $"{_path}.bad-{_now():yyyyMMdd-HHmmss}"; File.Move(_path, moved, overwrite: true); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { moved = null; }
        w.Add(moved is null ? $"{why}, библиотека начата заново" : $"{why}: сохранён как {System.IO.Path.GetFileName(moved)}, библиотека начата заново");
    }
}
