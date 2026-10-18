using System.Collections.ObjectModel;
using BeamModManager.Core.Game;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Packs;
using BeamModManager.Core.Versioning;
using BeamModManager.Ui.Mvvm;
using BeamModManager.Ui.Text;

namespace BeamModManager.Ui.Garage;

/// <summary>Фильтры верхней панели: активен один, они заменяют профили (DESIGN.md раздел 4).</summary>
public enum GarageFilter { All, Enabled, NeedsUpdate, NotTracked, Deleted }

public sealed class FilterItem : Observable
{
    public GarageFilter Filter { get; }
    public string Title { get; }
    internal FilterItem(GarageFilter f, string title) { Filter = f; Title = title; }

    private int _count;
    /// <summary>Число рядом с названием («Нужно обновить 3»), чтобы объём работы был виден до нажатия.</summary>
    public int Count { get => _count; internal set { if (Set(ref _count, value)) { Raise(nameof(Label)); Raise(nameof(ShowCount)); } } }

    /// <summary>У «Все» и «Включённые» число не пишется: его показывает заголовок.</summary>
    public bool ShowCount => Filter is GarageFilter.NeedsUpdate or GarageFilter.NotTracked or GarageFilter.Deleted && Count > 0;

    public string Label => ShowCount ? $"{Title} {Count}" : Title;

    private bool _isActive;
    public bool IsActive { get => _isActive; internal set => Set(ref _isActive, value); }
}

public enum EmptyAction { None, OpenSearch, PickFolder, CheckNow, SearchSites, ShowAll, OpenSiteSettings, Retry }

/// <summary>Пустое состояние: приглашение к действию (DESIGN_FLOWS раздел 4).</summary>
public sealed record EmptyState(string Title, string Text, string? ActionLabel, EmptyAction Action);

/// <summary>Тост внизу: что сделано и «Отменить» на несколько секунд вместо вопроса «Вы уверены?».</summary>
public sealed record Toast(string Text, bool CanUndo, long Serial);

/// <summary>Главная кнопка правой панели меняется по состоянию мода (DESIGN.md раздел 4).</summary>
public enum MainAction { None, Update, OpenPage, Enable, FindReplacement, SetPage, Restore }

/// <summary>Строка блока данных в правой панели: «Размер — 312 МБ».</summary>
public sealed record Fact(string Label, string Value)
{
    /// <summary>Значение требует внимания (янтарным): например, мод для другой версии игры.</summary>
    public bool Attention { get; init; }
}

/// <summary>Строка «Прошлых версий»: «Версия 1.0» и «12 октября 2026 · 88 МБ», справа «Вернуть».</summary>
public sealed record VersionRow(string Title, string Detail, PastVersion Version);

/// <summary>«Внутри архива»: что мод кладёт в игру и что насторожило проверку (словами, янтарным).</summary>
public sealed record InsideInfo(IReadOnlyList<Fact> Facts, IReadOnlyList<string> Warnings);

/// <summary>Пункт меню карточки (DESIGN_COMPONENTS 3.9): тот же набор, что в правой панели.</summary>
public enum MenuAction { Open, Toggle, OpenPage, Update, Delete, Restore }

public sealed record MenuEntry(string Label, MenuAction Action, bool Danger = false);

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
    string? PendingText)
{
    /// <summary>Внизу «Удалить» с «Можно вернуть позже»; у удалённого — нет.</summary>
    public bool CanDelete { get; init; } = true;
}

/// <summary>
/// Экран «Гараж» без окна: карточки, фильтры с числами, поиск по своим модам, выбор, правая панель,
/// включение и выключение (как в игре, флажком в db.json; при запущенной игре — после её закрытия).
/// </summary>
public sealed class GarageViewModel : Observable
{
    /// <summary>Порог строки «Включено 34 мода. Игра может грузиться дольше» (DESIGN_FLOWS 6.9, AppSettings.ActiveModsWarnAbove).</summary>
    /// <remarks>Меняется из настроек на ходу: строка пересчитывается сразу. 0 — не предупреждать.</remarks>
    public int ManyEnabledThreshold
    {
        get => _manyThreshold;
        set { if (_manyThreshold == value) return; _manyThreshold = value; Rebuild(); }
    }
    private int _manyThreshold = 30;

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
        Filters = new ObservableCollection<FilterItem>
        {
            new FilterItem(GarageFilter.All, "Все") { IsActive = true },
            new FilterItem(GarageFilter.Enabled, "Включённые"),
            new FilterItem(GarageFilter.NeedsUpdate, "Нужно обновить"),
            new FilterItem(GarageFilter.NotTracked, "Не проверяются"),
        };
        SelectFilterCommand = new RelayCommand(p => { if (p is GarageFilter f) Filter = f; else if (p is FilterItem i) Filter = i.Filter; });
        ToggleSelectedCommand = new RelayCommand(ToggleSelected, () => Selected is not null);
        UndoCommand = new RelayCommand(Undo, () => _undo is not null);
        DeleteSelectedCommand = new RelayCommand(DeleteSelected, () => Selected is not null);
        RestoreVersionCommand = new RelayCommand(p => { if (p is VersionRow r) RestoreVersion(r); });
    }

    public ObservableCollection<ModCard> Cards { get; } = new();

    /// <summary>Все карточки без фильтра и поиска (для фоновой загрузки картинок).</summary>
    public IReadOnlyList<ModCard> AllCards => _snap.Mods.Select(m => _cards[m.Mod.Id]).ToList();
    /// <summary>Фильтры островка; «Удалённые» появляется пятым, только когда есть что вернуть (решено 2026-10-17).</summary>
    public ObservableCollection<FilterItem> Filters { get; }
    private readonly FilterItem _deletedFilter = new(GarageFilter.Deleted, "Удалённые");

    /// <summary>Пункт «Удалённые» (он то в списке, то нет): окно следит за его числом, чтобы перемерить островок.</summary>
    public FilterItem DeletedFilter => _deletedFilter;
    private readonly Dictionary<string, ModCard> _deletedCards = new();
    public RelayCommand SelectFilterCommand { get; }
    public RelayCommand ToggleSelectedCommand { get; }
    public RelayCommand UndoCommand { get; }
    public RelayCommand DeleteSelectedCommand { get; }
    public RelayCommand RestoreVersionCommand { get; }

    private GarageFilter _filter;
    public GarageFilter Filter
    {
        get => _filter;
        set { if (Set(ref _filter, value)) { foreach (var f in Filters) f.IsActive = f.Filter == value; Rebuild(); } }
    }

    private string _search = "";
    /// <summary>«Найти мод»: только среди своих модов, по названию и имени файла.</summary>
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
            DeleteSelectedCommand.Refresh();
            ReloadExtras();
        }
    }

    public ModDetails? Details => Selected is null ? null : BuildDetails(Selected);

    private string _countText = "";
    /// <summary>Рядом с названием «Гараж»: «24 мода».</summary>
    public string CountText { get => _countText; private set => Set(ref _countText, value); }

    private string _headerNote = "";
    /// <summary>Под крупным заголовком сверху (DESIGN.md 4, «Островки»): «15 модов · 3 ждут обновления».</summary>
    public string HeaderNote { get => _headerNote; private set => Set(ref _headerNote, value); }

    private string _headerCount = "";
    /// <summary>Число в маленькой капсуле заголовка, когда список прокручен: «15»; модов нет — пусто.</summary>
    public string HeaderCount { get => _headerCount; private set => Set(ref _headerCount, value); }

    private string _searchNote = "";
    /// <summary>В раскрытом поиске справа от поля: «найдено 2» или «ничего не нашли»; без запроса — пусто.</summary>
    public string SearchNote { get => _searchNote; private set => Set(ref _searchNote, value); }

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
            c.Apply(m, _pending.TryGetValue(m.Mod.FileName, out var p) ? p : null, LiveWaiting);
        }
        foreach (var gone in _cards.Keys.Where(k => !alive.Contains(k)).ToList()) _cards.Remove(gone);
        var aliveDeleted = new HashSet<string>();
        foreach (var d in _snap.Deleted)
        {
            aliveDeleted.Add(d.Id);
            if (!_deletedCards.TryGetValue(d.Id, out var c)) _deletedCards[d.Id] = c = new ModCard(d.Id);
            c.ApplyDeleted(d, _now());
        }
        foreach (var gone in _deletedCards.Keys.Where(k => !aliveDeleted.Contains(k)).ToList()) _deletedCards.Remove(gone);
        var files = new HashSet<string>(_snap.Mods.Select(m => m.Mod.FileName), StringComparer.OrdinalIgnoreCase);
        foreach (var f in _pending.Keys.Where(f => !files.Contains(f)).ToList()) _pending.Remove(f);
        Raise(nameof(HasPending));
        Rebuild();
        if (_snap.Notice is { } n) ShowToast(n, null);
        Refreshed?.Invoke();
    }

    /// <summary>Гараж перечитан: «Обновления» строят свой список из тех же карточек.</summary>
    public event Action? Refreshed;

    private void Rebuild()
    {
        var all = _snap.Mods.Select(m => _cards[m.Mod.Id]).ToList();
        Filters[0].Count = all.Count;
        Filters[1].Count = all.Count(c => c.Enabled);
        Filters[2].Count = all.Count(c => c.Data.NeedsUpdate);
        Filters[3].Count = all.Count(c => !c.Data.Tracked);

        var deleted = _snap.Deleted.Select(d => _deletedCards[d.Id]).ToList();
        _deletedFilter.Count = deleted.Count;
        // «Удалённые» видно, только пока есть что вернуть; вернули последний — назад во «Все»
        if (deleted.Count == 0 && _filter == GarageFilter.Deleted) { _filter = GarageFilter.All; foreach (var f in Filters) f.IsActive = f.Filter == _filter; Raise(nameof(Filter)); }
        if (deleted.Count > 0 && !Filters.Contains(_deletedFilter)) Filters.Add(_deletedFilter);
        else if (deleted.Count == 0 && Filters.Contains(_deletedFilter)) Filters.Remove(_deletedFilter);

        var tokens = Ru.Words(_search);
        var visible = (_filter == GarageFilter.Deleted ? deleted : all).Where(c => Pass(c, _filter) && Matches(c, tokens)).ToList();
        Cards.SyncTo(visible);

        CountText = Ru.Mods(all.Count);
        var upd = Filters[2].Count;
        Summary = all.Count == 0 ? "Модов пока нет"
            : upd == 0 ? Ru.Mods(all.Count)
            : $"{Ru.Mods(all.Count)}, {upd} {Ru.Plural(upd, "ждёт", "ждут", "ждут")} обновления";
        HeaderNote = all.Count == 0 ? "модов пока нет"
            : upd == 0 ? Ru.Mods(all.Count)
            : $"{Ru.Mods(all.Count)} · {upd} {Ru.Plural(upd, "ждёт", "ждут", "ждут")} обновления";
        HeaderCount = all.Count == 0 ? "" : all.Count.ToString(System.Globalization.CultureInfo.InvariantCulture);
        SearchNote = SearchNoteFor(tokens.Count > 0, visible.Count);
        Empty = _filter == GarageFilter.Deleted && visible.Count > 0 ? null : EmptyFor(all.Count, visible.Count, tokens.Count > 0);

        var enabled = Filters[1].Count;
        ManyEnabledBanner = !_bannerDismissed && ManyEnabledThreshold > 0 && enabled > ManyEnabledThreshold
            ? $"Включено {Ru.Mods(enabled)}. Игра может грузиться дольше" : null;

        if (Selected is not null && !visible.Contains(Selected)) Selected = null;
        else
        {
            RaiseDetails();
            if (Selected is not null && ExtrasKey(Selected) != _extrasKey) ReloadExtras(); // файл сменился: вернули версию, обновили
        }
    }

    /// <summary>Подпись в раскрытом поиске (общая для гаража и «Обновлений»).</summary>
    public static string SearchNoteFor(bool searching, int found) =>
        !searching ? "" : found == 0 ? "ничего не нашли" : $"найдено {found}";

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
        if (total == 0) return new("Модов пока нет", "Найдите мод в поиске или перетащите архив на окно", "Перейти в поиск", EmptyAction.OpenSearch);
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

    public void Toggle(ModCard card)
    {
        if (card.IsDeleted) return; // удалённый не включают: его возвращают
        SetEnabled(card, !card.Enabled, withUndo: true);
    }

    private void SetEnabled(ModCard card, bool want, bool withUndo)
    {
        var file = card.FileName;
        var before = card.Enabled;
        if (before == want) return;

        var running = _backend.IsGameRunning();
        if (running)
        {
            if (_backend.LiveAvailable)
            {
                // помощник в игре: переключаем сразу, как меню модов самой игры
                _pending[file] = want;
                _liveTries = 0; // новое действие человека — новая попытка
                SendLive();
                card.Apply(card.Data, want, LiveWaiting);
                Raise(nameof(HasPending));
                Rebuild();
            }
            else Queue(file, want, card);
        }
        else
        {
            // игра закрыта: пишем сразу; старое ожидание для этого мода (с прошлого раза) больше не нужно
            if (_pending.Remove(file)) Raise(nameof(HasPending));
            var r = _backend.SetEnabled(file, want);
            if (!r.Success && _backend.IsGameRunning()) Queue(file, want, card); // игру запустили между проверкой и записью
            else if (!r.Success) { ShowToast(r.Message, null); return; }
            else Refresh();
        }

        var label = want ? "Включён" : "Выключен";
        if (_pending.ContainsKey(file) && !LiveWaiting) label += ". Применится, когда закроете игру";
        ShowToast(label, withUndo ? () => { if (_cards.TryGetValue(card.Id, out var c)) SetEnabled(c, before, withUndo: false); } : null);
    }

    private void Queue(string file, bool want, ModCard card)
    {
        // вернули как было на диске — ждать нечего
        if (card.Data.Mod.Enabled == want) _pending.Remove(file); else _pending[file] = want;
        card.Apply(card.Data, _pending.TryGetValue(file, out var p) ? p : null, LiveWaiting);
        Raise(nameof(HasPending));
        Rebuild();
    }

    /// <summary>Сколько ждём, пока игра применит переданное помощнику (она пишет db.json сразу после переключения).</summary>
    public static readonly TimeSpan LiveTimeout = TimeSpan.FromSeconds(6);
    private DateTimeOffset? _liveSentAt;
    private int _liveTries; // передач без подтверждения подряд: больше двух — помощник не справляется, ждём закрытия игры

    /// <summary>Изменения переданы помощнику в игре и ждут подтверждения (карточка: «Выключается…»).</summary>
    public bool LiveWaiting => _pending.Count > 0 && _liveSentAt is { } t && _now() - t < LiveTimeout;

    private bool SendLive()
    {
        if (_pending.Count == 0 || !_backend.TrySendLive(PendingChanges)) return false;
        _liveSentAt = _now();
        _liveTries++;
        return true;
    }

    /// <summary>
    /// Окно зовёт раз в секунду-три, пока игра запущена и что-то ждёт: смотрим, применила ли игра (db.json), и, если помощник
    /// появился или промолчал, передаём ещё раз. Не ответил за <see cref="LiveTimeout"/> — ждём закрытия игры, как без помощника.
    /// </summary>
    public void TickLive()
    {
        if (_pending.Count == 0) { _liveSentAt = null; _liveTries = 0; return; }
        // помощник ответил, что применить не может: повторять бесполезно, ждём закрытия игры и говорим почему
        if (_liveSentAt is not null && _backend.LiveAnswer() is { } answer && Declined(answer) is { } why)
        {
            _liveSentAt = null;
            _liveTries = 2;
            Refresh(); // что получилось, игра уже записала в db.json — эти ожидания снимутся
            if (_pending.Count > 0) ShowToast(why, null);
            return;
        }
        var expired = _liveSentAt is { } t && _now() - t >= LiveTimeout;
        if ((_liveSentAt is null || expired) && _liveTries < 2 && _backend.IsGameRunning() && _backend.LiveAvailable && SendLive())
            expired = false; // помощник отвечает (или появился): передали заново
        if (expired) _liveSentAt = null;
        Refresh();
        if (expired && _pending.Count > 0)
            ShowToast("Игра не ответила. Изменения применятся, когда закроете её", null);
    }

    /// <summary>
    /// Почему помощник не применил переданное (текст для тоста) или null, если ждать стоит: «stale» — команда пришла поздно
    /// (игра грузилась), её просто передаём заново.
    /// </summary>
    private string? Declined(BridgeAnswer a)
    {
        if (a.Error is { } e)
            return e == "stale" ? null : "Игра не смогла переключить мод. Изменения применятся, когда закроете её";
        var unknown = _pending.Keys.Count(f => a.Results.TryGetValue(GameModDb.KeyOf(f), out var r) && r == "missing");
        if (unknown == 0) return null;
        return unknown == 1
            ? "Игра пока не видит этот мод. Применится, когда закроете её"
            : $"Игра пока не видит {unknown} {Ru.Plural(unknown, "мод", "мода", "модов")}. Применится, когда закроете её";
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

    // ───────────── удалить ─────────────

    public void DeleteSelected()
    {
        if (Selected is { } c) Delete(c);
    }

    /// <summary>
    /// «Удалить» без вопроса «Вы уверены?»: файл уходит в копии, на 8 с тост «Отменить» (DESIGN_COMPONENTS 3.7). Пока тост
    /// открыт, запись о моде держится (страница, история имён); погас — мод забыт, копия остаётся в «Копиях».
    /// </summary>
    public void Delete(ModCard card)
    {
        if (card.IsDeleted) return;
        var r = _backend.Delete(card.Data);
        if (!r.Success || r.Ticket is not { } t)
        {
            ShowToast(r.Message ?? "Удалить не получилось", null);
            return;
        }
        // переключение, ждущее закрытия игры, удалённому файлу не нужно
        if (_pending.Remove(card.FileName)) Raise(nameof(HasPending));
        // выбор не переходит к соседу: второй Delete подряд удалил бы другой мод
        if (ReferenceEquals(Selected, card)) Selected = null;
        Refresh();
        ShowToast($"Удалён {Ru.Quoted(card.Name, 34)}", () =>
        {
            var u = _backend.Undelete(t);
            Refresh();
            if (!u.Success) { ShowToast(u.Message ?? "Вернуть не получилось", null); return; }
            if (_cards.TryGetValue(t.ModId, out var back) && Cards.Contains(back)) Selected = back;
        }, () => _backend.ForgetDeleted(t));
    }

    /// <summary>«Вернуть» удалённый: копия встаёт в папку модов, мод снова в «Все». Тост «Возвращён «…»».</summary>
    public void RestoreDeleted(ModCard card)
    {
        if (card.Deleted is not { } d) return;
        var r = _backend.RestoreDeleted(d);
        if (!r.Success) { ShowToast(r.Message ?? "Вернуть не получилось", null); return; }
        if (ReferenceEquals(Selected, card)) Selected = null;
        Refresh();
        ShowToast($"Возвращён {Ru.Quoted(d.Name, 34)}", null);
    }

    // ───────────── прошлые версии и «Внутри архива» ─────────────

    private IReadOnlyList<VersionRow> _versions = Array.Empty<VersionRow>();
    /// <summary>«Прошлые версии» выбранного мода, от новых к старым; пусто — блока нет.</summary>
    public IReadOnlyList<VersionRow> Versions { get => _versions; private set { if (Set(ref _versions, value)) Raise(nameof(HasVersions)); } }

    public bool HasVersions => _versions.Count > 0;

    private InsideInfo? _inside;
    /// <summary>«Внутри архива» выбранного мода; null — ещё читаем или архива нет на месте.</summary>
    public InsideInfo? Inside { get => _inside; private set => Set(ref _inside, value); }

    /// <summary>Последнее чтение версий и архива (тесты ждут его; окно — нет).</summary>
    public Task ExtrasTask { get; private set; } = Task.CompletedTask;

    private string? _extrasKey;
    private long _extrasSerial;

    private static string ExtrasKey(ModCard c) => $"{c.Id}|{c.Data.Mod.FileName}|{c.Data.Mod.SizeBytes}|{c.Data.ArchivePath}";

    /// <summary>Архив и папка копий читаются в фоне: выбор карточки не ждёт диска. Устаревший ответ (выбрали другую) не показывается.</summary>
    private void ReloadExtras()
    {
        var serial = ++_extrasSerial;
        Versions = Array.Empty<VersionRow>();
        Inside = null;
        if (Selected is not { } c || c.IsDeleted) { _extrasKey = Selected is null ? null : ExtrasKey(Selected); ExtrasTask = Task.CompletedTask; return; }
        _extrasKey = ExtrasKey(c);
        var data = c.Data;
        ExtrasTask = LoadExtrasAsync(data, serial);
    }

    private async Task LoadExtrasAsync(GarageMod data, long serial)
    {
        IReadOnlyList<PastVersion> past;
        ArchiveOverview? inside;
        try
        {
            (past, inside) = await Task.Run(() => (_backend.PastVersions(data), _backend.Inside(data))).ConfigureAwait(true);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or InvalidOperationException) { return; }
        if (serial != _extrasSerial) return;
        Versions = past.Select(p => new VersionRow(
            p.Version.Length > 0 ? "Версия " + p.Version : "Прошлая копия",
            (p.Backup.SavedAtUtc is { } at ? Ru.When(at, _now()) + " · " : "") + Ru.Size(p.Backup.SizeBytes),
            p)).ToList();
        Inside = inside is null ? null : Describe(inside);
    }

    /// <summary>Словами, без технических слов: «Машина — ETK 800», «Скрипты — есть, 12 файлов Lua».</summary>
    public static InsideInfo Describe(ArchiveOverview o)
    {
        if (o.Broken) return new InsideInfo(Array.Empty<Fact>(), new[] { "Архив не читается: возможно, он повреждён" });
        var facts = new List<Fact>();
        if (o.Vehicles.Count > 0) facts.Add(new Fact(o.Vehicles.Count == 1 ? "Машина" : "Машины", Names(o.Vehicles)));
        if (o.Levels.Count > 0) facts.Add(new Fact(o.Levels.Count == 1 ? "Карта" : "Карты", Names(o.Levels)));
        facts.Add(new Fact("Скрипты", o.LuaFiles == 0 ? "нет" : $"есть, {o.LuaFiles} {Ru.Plural(o.LuaFiles, "файл", "файла", "файлов")} Lua"));
        var files = Ru.Number(o.Files);
        facts.Add(new Fact("Файлов", o.UnpackedBytes > 0 ? $"{files}, {Ru.Size(o.UnpackedBytes)} после распаковки" : files));
        return new InsideInfo(facts, o.Findings.Select(f => f.Message).Distinct().Take(3).ToList());
    }

    /// <summary>До трёх имён, остальное числом: «Audi A4, Audi A6, BMW M3 и ещё 9».</summary>
    private static string Names(IReadOnlyList<string> names) =>
        names.Count <= 3 ? string.Join(", ", names) : string.Join(", ", names.Take(3)) + $" и ещё {names.Count - 3}";

    /// <summary>«Вернуть»: тост «Возвращена версия 1.0» с «Отменить» (текущая версия ушла в копии, её можно вернуть обратно).</summary>
    public void RestoreVersion(VersionRow row)
    {
        if (Selected is not { } c) return;
        var before = c.Data;
        var r = _backend.RestoreVersion(before, row.Version);
        if (!r.Success) { ShowToast(r.Message ?? "Вернуть не получилось", null); return; }
        Refresh();
        var what = row.Version.Version.Length > 0 ? "версия " + row.Version.Version : "прошлая копия";
        ShowToast($"Возвращена {what}", () => UndoRestore(c.Id, before.Mod.FileName));
    }

    private void UndoRestore(string id, string fileName)
    {
        if (!_cards.TryGetValue(id, out var card)) { Refresh(); return; }
        // текущая версия перед «Вернуть» — самая свежая копия под прежним именем файла
        var back = _backend.PastVersions(card.Data).FirstOrDefault(p => string.Equals(p.Backup.FileName, fileName, StringComparison.OrdinalIgnoreCase));
        var r = back is null ? new OpResult(false, "Копия той версии пропала") : _backend.RestoreVersion(card.Data, back);
        Refresh();
        if (!r.Success) ShowToast(r.Message ?? "Вернуть не получилось", null);
    }

    // ───────────── меню карточки ─────────────

    /// <summary>Правая кнопка или клавиша «Меню»: «Открыть», «Включить»/«Выключить», «Открыть страницу», «Обновить до …», «Удалить».</summary>
    public IReadOnlyList<MenuEntry> MenuFor(ModCard c)
    {
        if (c.IsDeleted) return new[] { new MenuEntry("Открыть", MenuAction.Open), new MenuEntry("Вернуть", MenuAction.Restore) };
        var list = new List<MenuEntry> { new("Открыть", MenuAction.Open), new(c.Enabled ? "Выключить" : "Включить", MenuAction.Toggle) };
        if (PageOf(c) is not null) list.Add(new("Открыть страницу", MenuAction.OpenPage));
        if (DownloadsAllowed && c.Data.NeedsUpdate && c.Data.Update is { } u)
            list.Add(new(u.LatestVersion is null ? "Обновить" : $"Обновить до {u.LatestVersion}", MenuAction.Update));
        list.Add(new("Удалить", MenuAction.Delete, Danger: true));
        return list;
    }

    /// <summary>Куда ведёт «Открыть страницу»: страница с новой версией, иначе та, где следим.</summary>
    public static Uri? PageOf(ModCard c) => c.Data.NeedsUpdate && c.Data.Update is { } u ? u.Url : c.Data.Binding?.Url;

    // ───────────── тост ─────────────

    /// <summary>Сообщение без «Отменить» (например, «Программа обновлена до 0.1.30»).</summary>
    public void Notify(string text) => ShowToast(text, null);

    /// <summary>Тост от другого раздела («Версия 1.5 скрыта» в «Обновлениях»), с «Отменить», если передано действие.</summary>
    public void Notify(string text, Action? undo) => ShowToast(text, undo);

    /// <param name="commit">Что сделать, когда «Отменить» больше не нажать (тост погас или его сменил другой): удаление
    /// становится окончательным для библиотеки.</param>
    private void ShowToast(string text, Action? undo, Action? commit = null)
    {
        CommitToast();
        _undo = undo;
        _commit = commit;
        Toast = new Toast(Ru.Line(text), undo is not null, ++_toastSerial);
        UndoCommand.Refresh();
    }

    private Action? _commit;

    private void CommitToast()
    {
        var c = _commit;
        _commit = null;
        c?.Invoke();
    }

    /// <summary>Окно закрывается: отменить уже не успеют, отложенное по тосту применяем.</summary>
    public void CommitPending() => CommitToast();

    public void Undo()
    {
        var u = _undo;
        _undo = null;
        _commit = null; // отменили: доводить нечего
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
        CommitToast();
        UndoCommand.Refresh();
    }

    // ───────────── правая панель ─────────────

    private void RaiseDetails() => Raise(nameof(Details));

    private static string Host(Uri u) => u.Host.StartsWith("www.", StringComparison.OrdinalIgnoreCase) ? u.Host[4..] : u.Host;

    private ModDetails BuildDetails(ModCard c)
    {
        if (c.Deleted is { } del)
        {
            var dfacts = new List<Fact> { new("Размер", Ru.Size(del.Copy.LastSizeBytes)) };
            if (del.Copy.LastSavedAtUtc is { } at) dfacts.Add(new Fact("Удалён", Ru.When(at, _now())));
            if (del.Version is { } dv) dfacts.Add(new Fact("Версия", dv));
            if (del.Copy.BackupCount > 1) dfacts.Add(new Fact("Копий", del.Copy.BackupCount.ToString(System.Globalization.CultureInfo.InvariantCulture)));
            return new ModDetails(del.Name, null, MainAction.Restore, "Вернуть", "Мод снова появится в папке, игра включит его сама",
                dfacts, null, false, null, null) { CanDelete = false };
        }
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
                hint = "Прошлая версия сохранится, её можно вернуть";
            }
            else
            {
                act = MainAction.OpenPage;
                label = "Открыть страницу";
                hint = $"Новая версия{(u.LatestVersion is null ? "" : " " + u.LatestVersion)} на {Host(u.Url)}. Скачайте её там и перетащите архив на окно";
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
        else if (!d.Tracked)
        {
            // без страницы на сайте новую версию не узнать: предлагаем её указать (DESIGN_FLOWS 6.4)
            act = MainAction.SetPage;
            label = "Указать страницу";
            hint = d.CheckingDeclined ? "Сейчас не проверяем по вашему выбору. Укажете страницу — снова будем следить"
                : "Тогда программа будет сама замечать новые версии";
        }
        else act = MainAction.None;

        var facts = new List<Fact>();
        if (m.SizeBytes is { } size) facts.Add(new Fact("Размер", Ru.Size(size)));
        facts.Add(new Fact("Установлен", Ru.Date(m.InstalledAtUtc)));
        if (site is not null) facts.Add(new Fact("Следим на", site));
        else facts.Add(new Fact("Следим на", d.CheckingDeclined ? "Не проверяем по вашему выбору" : "Страница не указана"));
        if (d.LastCheckedUtc is { } t) facts.Add(new Fact("Проверен", Ru.Ago(t, _now())));
        if (m.Version is { } v) facts.Add(new Fact("Версия", v));
        // совместимость (DESIGN_FLOWS 6.8): только когда известны обе версии
        if (Core.Game.GameCompat.Describe(d.SiteGameVersion, _backend.GameVersion) is { } compat)
            facts.Add(new Fact("Для игры", compat) { Attention = Core.Game.GameCompat.Compare(d.SiteGameVersion, _backend.GameVersion) != Core.Game.CompatLevel.Same });

        return new ModDetails(m.Name, d.Author, act, label, hint, facts, d.Binding?.Url, act != MainAction.OpenPage && d.Binding is not null,
            act == MainAction.Enable ? null : c.Enabled ? "Выключить" : "Включить", c.PendingText);
    }
}
