using System.Collections.ObjectModel;
using BeamModManager.Core.Library;
using BeamModManager.Core.Versioning;
using BeamModManager.Ui.Mvvm;
using BeamModManager.Ui.Text;

namespace BeamModManager.Ui.Garage;

/// <summary>Фильтры верхней панели: активен один, они заменяют профили (DESIGN.md раздел 4).</summary>
public enum GarageFilter { All, Enabled, NeedsUpdate, NotTracked }

public sealed class FilterItem : Observable
{
    public GarageFilter Filter { get; }
    public string Title { get; }
    internal FilterItem(GarageFilter f, string title) { Filter = f; Title = title; }

    private int _count;
    /// <summary>Число рядом с названием («Нужно обновить 3»), чтобы объём работы был виден до нажатия.</summary>
    public int Count { get => _count; internal set { if (Set(ref _count, value)) { Raise(nameof(Label)); Raise(nameof(ShowCount)); } } }

    /// <summary>У «Все» и «Включённые» число не пишется: его показывает заголовок.</summary>
    public bool ShowCount => Filter is GarageFilter.NeedsUpdate or GarageFilter.NotTracked && Count > 0;

    public string Label => ShowCount ? $"{Title} {Count}" : Title;

    private bool _isActive;
    public bool IsActive { get => _isActive; internal set => Set(ref _isActive, value); }
}

public enum EmptyAction { None, OpenSearch, PickFolder, CheckNow, SearchSites, ShowAll }

/// <summary>Пустое состояние: приглашение к действию (DESIGN_FLOWS раздел 4).</summary>
public sealed record EmptyState(string Title, string Text, string? ActionLabel, EmptyAction Action);

/// <summary>Тост внизу: что сделано и «Отменить» на несколько секунд вместо вопроса «Вы уверены?».</summary>
public sealed record Toast(string Text, bool CanUndo, long Serial);

/// <summary>Главная кнопка правой панели меняется по состоянию мода (DESIGN.md раздел 4).</summary>
public enum MainAction { None, Update, OpenPage, Enable, FindReplacement }

/// <summary>Строка блока данных в правой панели: «Размер — 312 МБ».</summary>
public sealed record Fact(string Label, string Value);

/// <summary>Правая панель выбранного мода.</summary>
public sealed record ModDetails(
    string Name,
    string? Author,
    MainAction Action,
    string? ActionLabel,
    string? ActionHint,
    IReadOnlyList<Fact> Facts,
    Uri? PageUrl,
    /// <summary>Нижняя кнопка «Открыть страницу»: не показывается, когда это уже главная кнопка.</summary>
    bool OpenPageLink,
    /// <summary>«Выключить» / «Включить» внизу панели; null, когда «Включить» уже главная кнопка (не дублируем).</summary>
    string? ToggleLabel,
    string? PendingText);

/// <summary>
/// Экран «Гараж» без окна: карточки, фильтры с числами, поиск по своим модам, выбор, правая панель,
/// включение и выключение (как в игре, флажком в db.json; при запущенной игре — после её закрытия).
/// </summary>
public sealed class GarageViewModel : Observable
{
    /// <summary>Порог строки «Включено 34 мода. Игра может грузиться дольше» (DESIGN_FLOWS 6.9, AppSettings.ActiveModsWarnAbove).</summary>
    public int ManyEnabledThreshold { get; set; } = 30;

    /// <summary>
    /// Загрузка разрешена в настройках. Выключена (по умолчанию) — кнопки «Обновить» нет, вместо неё «Открыть страницу»:
    /// человек обновляет сам, программа замечает новую версию при следующей проверке (DESIGN_FLOWS 3.3).
    /// </summary>
    public bool DownloadsAllowed { get; set; }

    private readonly IGarageBackend _backend;
    private readonly Func<DateTimeOffset> _now;
    private readonly Dictionary<string, ModCard> _cards = new();
    private readonly Dictionary<string, bool> _pending = new(StringComparer.OrdinalIgnoreCase); // имя файла → нужное состояние
    private GarageSnapshot _snap = GarageSnapshot.NoFolder;
    private Action? _undo;
    private long _toastSerial;
    private bool _bannerDismissed;

    public GarageViewModel(IGarageBackend backend, Func<DateTimeOffset>? now = null)
    {
        _backend = backend;
        _now = now ?? (() => DateTimeOffset.UtcNow);
        Filters = new[]
        {
            new FilterItem(GarageFilter.All, "Все") { IsActive = true },
            new FilterItem(GarageFilter.Enabled, "Включённые"),
            new FilterItem(GarageFilter.NeedsUpdate, "Нужно обновить"),
            new FilterItem(GarageFilter.NotTracked, "Не проверяются"),
        };
        SelectFilterCommand = new RelayCommand(p => { if (p is GarageFilter f) Filter = f; else if (p is FilterItem i) Filter = i.Filter; });
        ToggleSelectedCommand = new RelayCommand(ToggleSelected, () => Selected is not null);
        UndoCommand = new RelayCommand(Undo, () => _undo is not null);
    }

    public ObservableCollection<ModCard> Cards { get; } = new();

    /// <summary>Все карточки без фильтра и поиска (для фоновой загрузки картинок).</summary>
    public IReadOnlyList<ModCard> AllCards => _snap.Mods.Select(m => _cards[m.Mod.Id]).ToList();
    public IReadOnlyList<FilterItem> Filters { get; }
    public RelayCommand SelectFilterCommand { get; }
    public RelayCommand ToggleSelectedCommand { get; }
    public RelayCommand UndoCommand { get; }

    private GarageFilter _filter;
    public GarageFilter Filter
    {
        get => _filter;
        set { if (Set(ref _filter, value)) { foreach (var f in Filters) f.IsActive = f.Filter == value; Rebuild(); } }
    }

    private string _search = "";
    /// <summary>«Найти в гараже»: только среди своих модов, по названию и имени файла.</summary>
    public string SearchText
    {
        get => _search;
        set { if (Set(ref _search, value ?? "")) Rebuild(); }
    }

    private ModCard? _selected;
    public ModCard? Selected
    {
        get => _selected;
        set
        {
            if (ReferenceEquals(_selected, value)) return;
            if (_selected is not null) _selected.IsSelected = false;
            _selected = value;
            if (value is not null) value.IsSelected = true;
            Raise();
            RaiseDetails();
            ToggleSelectedCommand.Refresh();
        }
    }

    public ModDetails? Details => Selected is null ? null : BuildDetails(Selected);

    private string _countText = "";
    /// <summary>Рядом с названием «Гараж»: «24 мода».</summary>
    public string CountText { get => _countText; private set => Set(ref _countText, value); }

    private string _summary = "";
    /// <summary>Правая панель, когда мод не выбран: «24 мода, 3 ждут обновления».</summary>
    public string Summary { get => _summary; private set => Set(ref _summary, value); }

    private EmptyState? _empty;
    public EmptyState? Empty { get => _empty; private set => Set(ref _empty, value); }

    private string? _banner;
    public string? ManyEnabledBanner { get => _banner; private set => Set(ref _banner, value); }

    private Toast? _toast;
    public Toast? Toast { get => _toast; private set => Set(ref _toast, value); }

    private bool _folderAvailable;
    public bool FolderAvailable { get => _folderAvailable; private set => Set(ref _folderAvailable, value); }

    /// <summary>Есть изменения, которые ждут закрытия игры.</summary>
    public bool HasPending => _pending.Count > 0;

    /// <summary>Отложенные переключения (имя файла → включить): сохраняются при закрытии менеджера, пока игра запущена.</summary>
    public IReadOnlyDictionary<string, bool> PendingChanges => new Dictionary<string, bool>(_pending, StringComparer.OrdinalIgnoreCase);

    /// <summary>Вернуть отложенное с прошлого запуска; уже совпавшее с диском отбрасывается при обновлении.</summary>
    public void RestorePending(IReadOnlyDictionary<string, bool> changes)
    {
        foreach (var (file, want) in changes) _pending[file] = want;
        Refresh();
    }

    private string? _storageWarning;
    /// <summary>Библиотека не пишется на диск (занята, нет места): что случилось, одной строкой.</summary>
    public string? StorageWarning { get => _storageWarning; private set => Set(ref _storageWarning, value); }

    // ───────────── загрузка ─────────────

    public void Refresh()
    {
        _snap = _backend.Load();
        FolderAvailable = _snap.FolderAvailable;
        StorageWarning = _snap.StorageWarning;
        var alive = new HashSet<string>();
        foreach (var m in _snap.Mods)
        {
            alive.Add(m.Mod.Id);
            if (!_cards.TryGetValue(m.Mod.Id, out var c)) _cards[m.Mod.Id] = c = new ModCard(m.Mod.Id);
            // ожидание, совпавшее с тем, что уже на диске, больше не нужно (например, переключили в самой игре)
            if (_pending.TryGetValue(m.Mod.FileName, out var want) && want == m.Mod.Enabled) _pending.Remove(m.Mod.FileName);
            c.Apply(m, _pending.TryGetValue(m.Mod.FileName, out var p) ? p : null);
        }
        foreach (var gone in _cards.Keys.Where(k => !alive.Contains(k)).ToList()) _cards.Remove(gone);
        var files = new HashSet<string>(_snap.Mods.Select(m => m.Mod.FileName), StringComparer.OrdinalIgnoreCase);
        foreach (var f in _pending.Keys.Where(f => !files.Contains(f)).ToList()) _pending.Remove(f);
        Raise(nameof(HasPending));
        Rebuild();
        if (_snap.Notice is { } n) ShowToast(n, null);
    }

    private void Rebuild()
    {
        var all = _snap.Mods.Select(m => _cards[m.Mod.Id]).ToList();
        Filters[0].Count = all.Count;
        Filters[1].Count = all.Count(c => c.Enabled);
        Filters[2].Count = all.Count(c => c.Data.NeedsUpdate);
        Filters[3].Count = all.Count(c => !c.Data.Tracked);

        var tokens = Ru.Words(_search);
        var visible = all.Where(c => Pass(c, _filter) && Matches(c, tokens)).ToList();
        Cards.SyncTo(visible);

        CountText = Ru.Mods(all.Count);
        var upd = Filters[2].Count;
        Summary = all.Count == 0 ? "Модов пока нет"
            : upd == 0 ? Ru.Mods(all.Count)
            : $"{Ru.Mods(all.Count)}, {upd} {Ru.Plural(upd, "ждёт", "ждут", "ждут")} обновления";
        Empty = EmptyFor(all.Count, visible.Count, tokens.Count > 0);

        var enabled = Filters[1].Count;
        ManyEnabledBanner = !_bannerDismissed && ManyEnabledThreshold > 0 && enabled > ManyEnabledThreshold
            ? $"Включено {Ru.Mods(enabled)}. Игра может грузиться дольше" : null;

        if (Selected is not null && !visible.Contains(Selected)) Selected = null;
        else RaiseDetails();
    }

    private static bool Pass(ModCard c, GarageFilter f) => f switch
    {
        GarageFilter.Enabled => c.Enabled,
        GarageFilter.NeedsUpdate => c.Data.NeedsUpdate,
        GarageFilter.NotTracked => !c.Data.Tracked,
        _ => true,
    };

    /// <summary>Каждое слово запроса — начало какого-нибудь слова в названии или имени файла: «b» найдёт BMW, но не Subaru.</summary>
    private static bool Matches(ModCard c, IReadOnlyList<string> tokens)
    {
        if (tokens.Count == 0) return true;
        var words = Ru.Words(c.Name + " " + c.FileName);
        return tokens.All(tok => words.Any(w => w.StartsWith(tok, StringComparison.Ordinal)));
    }

    private EmptyState? EmptyFor(int total, int visible, bool searching)
    {
        if (!_snap.FolderAvailable) return new("Не нашли папку с модами", "Укажите её, и моды появятся здесь", "Указать папку", EmptyAction.PickFolder);
        if (total == 0) return new("В гараже пока пусто", "Найдите первый мод в поиске", "Перейти в поиск", EmptyAction.OpenSearch);
        if (visible > 0) return null;
        if (searching) return new("Нет такого мода", "Проверьте написание. Ищем только среди ваших модов", "Искать на сайтах", EmptyAction.SearchSites);
        return _filter switch
        {
            GarageFilter.NeedsUpdate => new("Всё свежее",
                _snap.LastCheckedUtc is { } t ? "Проверено " + Ru.Ago(t, _now()) : "Ещё не проверяли", "Проверить сейчас", EmptyAction.CheckNow),
            GarageFilter.NotTracked => new("У всех модов есть страница", "Обновления видны для каждого", null, EmptyAction.None),
            GarageFilter.Enabled => new("Все моды выключены", "Включите нужные в общем списке", "Показать все", EmptyAction.ShowAll),
            _ => null,
        };
    }

    public void DismissBanner()
    {
        _bannerDismissed = true;
        ManyEnabledBanner = null;
    }

    // ───────────── выбор и клавиатура ─────────────

    /// <summary>Стрелки по сетке: dx — по строке, dy — между строками при заданном числе колонок.</summary>
    public void MoveSelection(int dx, int dy, int columns)
    {
        if (Cards.Count == 0) return;
        columns = Math.Max(1, columns);
        if (Selected is null) { Selected = Cards[0]; return; }
        var i = Cards.IndexOf(Selected);
        var j = i + dx + dy * columns;
        if (dy != 0 && (j < 0 || j >= Cards.Count)) return; // вверх с первой строки и вниз с последней — на месте
        Selected = Cards[Math.Clamp(j, 0, Cards.Count - 1)];
    }

    // ───────────── включить и выключить ─────────────

    public void ToggleSelected()
    {
        if (Selected is { } c) Toggle(c);
    }

    public void Toggle(ModCard card) => SetEnabled(card, !card.Enabled, withUndo: true);

    private void SetEnabled(ModCard card, bool want, bool withUndo)
    {
        var file = card.FileName;
        var before = card.Enabled;
        if (before == want) return;

        if (_backend.IsGameRunning() || _pending.ContainsKey(file))
        {
            Queue(file, want, card);
        }
        else
        {
            var r = _backend.SetEnabled(file, want);
            if (!r.Success && _backend.IsGameRunning()) Queue(file, want, card); // игру запустили между проверкой и записью
            else if (!r.Success) { ShowToast(r.Message, null); return; }
            else Refresh();
        }

        var label = want ? "Включён" : "Выключен";
        if (_pending.ContainsKey(file)) label += ". Применится, когда закроете игру";
        ShowToast(label, withUndo ? () => { if (_cards.TryGetValue(card.Id, out var c)) SetEnabled(c, before, withUndo: false); } : null);
    }

    private void Queue(string file, bool want, ModCard card)
    {
        // вернули как было на диске — ждать нечего
        if (card.Data.Mod.Enabled == want) _pending.Remove(file); else _pending[file] = want;
        card.Apply(card.Data, _pending.TryGetValue(file, out var p) ? p : null);
        Raise(nameof(HasPending));
        Rebuild();
    }

    /// <summary>Окно заметило, что игра закрылась: применяем отложенное. Возвращает число неудач.</summary>
    public int OnGameExited()
    {
        if (_pending.Count == 0 || _backend.IsGameRunning()) return 0;
        var failed = new List<string>();
        foreach (var (file, want) in _pending.ToList())
        {
            var r = _backend.SetEnabled(file, want);
            if (r.Success) _pending.Remove(file);
            else if (_backend.IsGameRunning()) break; // игру снова запустили: ждём следующего закрытия
            else { _pending.Remove(file); failed.Add(r.Message); }
        }
        Refresh();
        if (failed.Count > 0) ShowToast(failed.Count == 1 ? failed[0] : $"Не удалось применить {failed.Count} {Ru.Plural(failed.Count, "изменение", "изменения", "изменений")}", null);
        return failed.Count;
    }

    // ───────────── тост ─────────────

    private void ShowToast(string text, Action? undo)
    {
        _undo = undo;
        Toast = new Toast(text, undo is not null, ++_toastSerial);
        UndoCommand.Refresh();
    }

    public void Undo()
    {
        var u = _undo;
        _undo = null;
        Toast = null;
        UndoCommand.Refresh();
        u?.Invoke();
    }

    /// <summary>Тост погас по таймеру; serial защищает от гашения более нового тоста.</summary>
    public void DismissToast(long serial)
    {
        if (Toast?.Serial != serial) return;
        Toast = null;
        _undo = null;
        UndoCommand.Refresh();
    }

    // ───────────── правая панель ─────────────

    private void RaiseDetails() => Raise(nameof(Details));

    private static string Host(Uri u) => u.Host.StartsWith("www.", StringComparison.OrdinalIgnoreCase) ? u.Host[4..] : u.Host;

    private ModDetails BuildDetails(ModCard c)
    {
        var d = c.Data;
        var m = d.Mod;
        MainAction act;
        string? label = null, hint = null;
        var site = d.Binding is { } bs ? Host(bs.Url) : null;
        if (d.NeedsUpdate && d.Update is { Status: UpdateStatus.Likely } u)
        {
            if (DownloadsAllowed)
            {
                act = MainAction.Update;
                label = u.LatestVersion is null ? "Обновить" : $"Обновить до {u.LatestVersion}";
                hint = "Прежняя версия сохранится, её можно вернуть";
            }
            else
            {
                act = MainAction.OpenPage;
                label = "Открыть страницу";
                hint = $"Новая версия{(u.LatestVersion is null ? "" : " " + u.LatestVersion)} на {Host(u.Url)}. Скачайте её там, менеджер заметит обновление сам";
            }
        }
        else if (d.NeedsUpdate && d.Update is { Status: UpdateStatus.Possible } p)
        {
            act = DownloadsAllowed ? MainAction.Update : MainAction.OpenPage;
            label = DownloadsAllowed ? "Обновить" : "Открыть страницу";
            hint = $"Файл на {Host(p.Url)} изменился, а номер версии тот же";
        }
        else if (d.PageMissing)
        {
            act = MainAction.FindReplacement;
            label = "Найти замену";
            hint = "Страница мода пропала с сайта";
        }
        else if (!c.Enabled)
        {
            act = MainAction.Enable;
            label = "Включить";
        }
        else act = MainAction.None;

        var facts = new List<Fact>();
        if (m.SizeBytes is { } size) facts.Add(new Fact("Размер", Ru.Size(size)));
        facts.Add(new Fact("Установлен", Ru.Date(m.InstalledAtUtc)));
        if (site is not null) facts.Add(new Fact("Следим на", site));
        else facts.Add(new Fact("Следим на", d.CheckingDeclined ? "Не проверяем по вашему выбору" : "Страница не указана"));
        if (d.LastCheckedUtc is { } t) facts.Add(new Fact("Проверен", Ru.Ago(t, _now())));
        if (m.Version is { } v) facts.Add(new Fact("Версия", v));

        return new ModDetails(m.Name, d.Author, act, label, hint, facts, d.Binding?.Url, act != MainAction.OpenPage && d.Binding is not null,
            act == MainAction.Enable ? null : c.Enabled ? "Выключить" : "Включить", c.PendingText);
    }
}
