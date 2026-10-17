using System.Collections.ObjectModel;
using BeamModManager.Core.Library;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Versioning;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Mvvm;
using BeamModManager.Ui.Settings;
using BeamModManager.Ui.Text;

namespace BeamModManager.Ui.Updates;

/// <summary>Фильтры раздела (доска «Обновления»): найденные обновления, скрытые версии, пропавшие страницы.</summary>
public enum UpdatesFilter { Available, Hidden, Missing }

/// <summary>Пункт фильтра: те же имена свойств, что у фильтров гаража, — верхняя панель у разделов общая.</summary>
public sealed class UpdatesFilterItem : Observable
{
    public UpdatesFilter Filter { get; }
    public string Title { get; }
    internal UpdatesFilterItem(UpdatesFilter f, string title) { Filter = f; Title = title; }

    private int _count;
    public int Count { get => _count; internal set { if (Set(ref _count, value)) { Raise(nameof(Label)); Raise(nameof(ShowCount)); } } }
    public bool ShowCount => Count > 0;
    public string Label => ShowCount ? $"{Title} {Count}" : Title;

    private bool _isActive;
    public bool IsActive { get => _isActive; internal set => Set(ref _isActive, value); }
}

public enum UpdateKind { Update, Possible, Missing, Hidden }

/// <summary>Что делает кнопка на карточке обновления.</summary>
public enum ItemAction { None, Update, OpenPage, Dismiss, Restore, FindReplacement, Decline }

/// <summary>
/// Карточка в «Обновлениях» (доска: фото слева, справа название, «1.0 → 1.5», две строки пояснения и до трёх кнопок).
/// Живёт, пока мод в списке, и обновляется на месте.
/// </summary>
public sealed class UpdateItem : Observable
{
    internal UpdateItem(ModCard card) => Card = card;

    /// <summary>Карточка гаража этого мода: фото и название берутся из неё.</summary>
    public ModCard Card { get; }
    public string ModId => Card.Id;

    private UpdateKind _kind;
    public UpdateKind Kind { get => _kind; internal set { if (Set(ref _kind, value)) Raise(nameof(Gray)); } }

    private string? _oldVersion, _newVersion, _chip, _line1, _line2;
    /// <summary>Установленная версия (зачёркнута) и новая: «1.0 → 1.5». Пусто — строки версий нет.</summary>
    public string? OldVersion { get => _oldVersion; internal set { if (Set(ref _oldVersion, value)) Raise(nameof(HasVersions)); } }
    public string? NewVersion { get => _newVersion; internal set { if (Set(ref _newVersion, value)) Raise(nameof(HasVersions)); } }
    public bool HasVersions => NewVersion is not null;
    /// <summary>Плашка над названием: «Возможно, обновлён», «Страница пропала», «Скрыто».</summary>
    public string? Chip { get => _chip; internal set => Set(ref _chip, value); }
    public string? Line1 { get => _line1; internal set => Set(ref _line1, value); }
    public string? Line2 { get => _line2; internal set => Set(ref _line2, value); }

    private ItemAction _primary, _second, _third;
    public ItemAction Primary { get => _primary; internal set => Set(ref _primary, value); }
    public ItemAction Second { get => _second; internal set => Set(ref _second, value); }
    public ItemAction Third { get => _third; internal set => Set(ref _third, value); }

    private string? _primaryLabel, _secondLabel, _thirdLabel;
    public string? PrimaryLabel { get => _primaryLabel; internal set => Set(ref _primaryLabel, value); }
    public string? SecondLabel { get => _secondLabel; internal set => Set(ref _secondLabel, value); }
    public string? ThirdLabel { get => _thirdLabel; internal set => Set(ref _thirdLabel, value); }

    private Uri? _pageUrl;
    public Uri? PageUrl { get => _pageUrl; internal set => Set(ref _pageUrl, value); }

    /// <summary>Фото серое: страница пропала (как у выключенного мода на доске).</summary>
    public bool Gray => Kind == UpdateKind.Missing;

    internal static string LabelOf(ItemAction a, UpdateKind k) => a switch
    {
        ItemAction.Update => "Обновить",
        ItemAction.OpenPage => "Открыть страницу",
        ItemAction.Dismiss => k == UpdateKind.Possible ? "Версия не изменилась" : "Пропустить эту версию",
        ItemAction.Restore => "Вернуть в список",
        ItemAction.FindReplacement => "Найти замену",
        ItemAction.Decline => "Не проверять этот мод",
        _ => "",
    };
}

public enum SiteDot { Ok, Warn, Off }

/// <summary>Строка сайта в панели «Проверка»: точка, название, состояние словами.</summary>
public sealed class SiteRow : Observable
{
    internal SiteRow(string id) => Id = id;
    public string Id { get; }

    private string _name = "", _state = "";
    private string? _note;
    private SiteDot _dot;
    private bool _needsHuman, _awaitingConfirm;
    private Uri? _home;
    public string Name { get => _name; internal set => Set(ref _name, value); }
    public string State { get => _state; internal set => Set(ref _state, value); }
    /// <summary>Подробность для подсказки («код 503», «сайт недавно не отвечал, повторим после 14:10»).</summary>
    public string? Note { get => _note; internal set => Set(ref _note, value); }
    public SiteDot Dot { get => _dot; internal set => Set(ref _dot, value); }
    public Uri? Home { get => _home; internal set => Set(ref _home, value); }
    /// <summary>Сайт просит «вы человек?»: в строке кнопка «Открыть сайт».</summary>
    public bool NeedsHuman { get => _needsHuman; internal set { if (Set(ref _needsHuman, value)) Raise(nameof(ShowOpen)); } }
    /// <summary>Сайт открыли: теперь «Готово» снимает паузу.</summary>
    public bool AwaitingConfirm { get => _awaitingConfirm; internal set { if (Set(ref _awaitingConfirm, value)) Raise(nameof(ShowOpen)); } }
    public bool ShowOpen => NeedsHuman && !AwaitingConfirm;
}

/// <summary>
/// Раздел «Обновления» без окна (доска «Обновления»): список найденного из карточек гаража, фильтры, поиск, действия
/// («Пропустить эту версию», «Вернуть», «Не проверять этот мод») и панель «Проверка»: кнопка, ход, итог, сайты, автопроверка.
/// Проверка идёт через один координатор: кнопка, запуск и повтор по таймеру не пересекаются.
/// </summary>
public sealed class UpdatesViewModel : Observable
{
    private readonly GarageViewModel _garage;
    private readonly IUpdatesBackend _backend;
    private readonly SettingsViewModel _settings;
    private readonly Action<Action> _post;
    private readonly Func<DateTimeOffset> _now;
    private readonly Dictionary<string, UpdateItem> _items = new();
    private readonly Dictionary<string, (int Done, int Total)> _progress = new();
    private readonly Dictionary<string, SiteRow> _siteRows = new();
    private DateTimeOffset? _autoAt;
    /// <summary>Сайты, где человек прошёл проверку «вы человек?» после последней проверки: прошлый отчёт о них устарел.</summary>
    private readonly HashSet<string> _resolved = new();
    private string? _result;
    private bool _offerForce;

    /// <param name="post">Выполнить в потоке окна (ход проверки приходит из фона).</param>
    public UpdatesViewModel(GarageViewModel garage, IUpdatesBackend backend, SettingsViewModel settings,
        Action<Action>? post = null, Func<DateTimeOffset>? now = null)
    {
        _garage = garage;
        _backend = backend;
        _settings = settings;
        _post = post ?? (a => a());
        _now = now ?? (() => DateTimeOffset.UtcNow);
        Filters = new[]
        {
            new UpdatesFilterItem(UpdatesFilter.Available, "Доступные") { IsActive = true },
            new UpdatesFilterItem(UpdatesFilter.Hidden, "Скрытые"),
            new UpdatesFilterItem(UpdatesFilter.Missing, "Страница пропала"),
        };
        garage.Refreshed += Rebuild;
        settings.Changed += _ => { Raise(nameof(AutoCheckOnStart)); RebuildPanel(); };
        Rebuild();
    }

    public ObservableCollection<UpdateItem> Items { get; } = new();
    public IReadOnlyList<UpdatesFilterItem> Filters { get; }
    public ObservableCollection<SiteRow> Sites { get; } = new();

    private UpdatesFilter _filter;
    public UpdatesFilter Filter
    {
        get => _filter;
        set { if (Set(ref _filter, value)) { foreach (var f in Filters) f.IsActive = f.Filter == value; Rebuild(); } }
    }

    private string _search = "";
    public string SearchText { get => _search; set { if (Set(ref _search, value ?? "")) Rebuild(); } }

    private string _countText = "";
    /// <summary>Рядом с заголовком: «3 доступно».</summary>
    public string CountText { get => _countText; private set => Set(ref _countText, value); }

    private string _headerCount = "";
    /// <summary>Число в маленькой капсуле заголовка, когда список прокручен: сколько доступно; нет новых — пусто.</summary>
    public string HeaderCount { get => _headerCount; private set => Set(ref _headerCount, value); }

    private string _searchNote = "";
    /// <summary>В раскрытом поиске: «найдено 2» или «ничего не нашли»; без запроса — пусто.</summary>
    public string SearchNote { get => _searchNote; private set => Set(ref _searchNote, value); }

    private EmptyState? _empty;
    public EmptyState? Empty { get => _empty; private set => Set(ref _empty, value); }

    // ───────────── панель «Проверка» ─────────────

    private bool _checking;
    public bool Checking { get => _checking; private set { if (Set(ref _checking, value)) RebuildPanel(); } }

    private string _checkText = "";
    /// <summary>Строка под заголовком «Проверка»: когда проверяли, что идёт сейчас, чем закончилось.</summary>
    public string CheckText { get => _checkText; private set => Set(ref _checkText, value); }

    private string _checkButton = "Проверить сейчас";
    /// <summary>«Проверить сейчас», «Остановить» или «Всё равно проверить».</summary>
    public string CheckButton { get => _checkButton; private set => Set(ref _checkButton, value); }

    private string? _progressText;
    /// <summary>«12 из 38», пока идёт проверка.</summary>
    public string? ProgressText { get => _progressText; private set => Set(ref _progressText, value); }

    private double _progressValue;
    public double ProgressValue { get => _progressValue; private set => Set(ref _progressValue, value); }

    private string _autoNote = "";
    /// <summary>Рядом с «Проверить сейчас»: как проверка запускается сама.</summary>
    public string AutoNote { get => _autoNote; private set => Set(ref _autoNote, value); }

    public bool AutoCheckOnStart { get => _settings.AutoCheckOnStart; set => _settings.AutoCheckOnStart = value; }

    // ───────────── список ─────────────

    private static bool IsAvailable(GarageMod d) => d.NeedsUpdate;
    private static bool IsHidden(GarageMod d) => d.Update is { IsDismissed: true };

    /// <summary>Список и числа строятся из карточек гаража: одно чтение библиотеки на оба раздела.</summary>
    public void Rebuild()
    {
        var all = _garage.AllCards;
        var available = all.Where(c => IsAvailable(c.Data)).ToList();
        var hidden = all.Where(c => IsHidden(c.Data)).ToList();
        var missing = all.Where(c => c.Data.PageMissing && c.Data.Tracked).ToList();
        Filters[0].Count = available.Count;
        Filters[1].Count = hidden.Count;
        Filters[2].Count = missing.Count;
        CountText = available.Count == 0 ? "нет новых" : $"{available.Count} доступно";
        HeaderCount = available.Count == 0 ? "" : available.Count.ToString(System.Globalization.CultureInfo.InvariantCulture);

        var byName = StringComparer.CurrentCultureIgnoreCase;
        var source = _filter switch
        {
            UpdatesFilter.Hidden => hidden.OrderBy(c => c.Name, byName).ToList(),
            UpdatesFilter.Missing => missing.OrderBy(c => c.Name, byName).ToList(),
            // сначала то, что точно обновилось, потом сомнительное; внутри — по названию (как в гараже)
            _ => available.OrderBy(c => c.Data.Update!.Status == UpdateStatus.Likely ? 0 : 1).ThenBy(c => c.Name, byName).ToList(),
        };
        var tokens = Ru.Words(_search);
        var visible = new List<UpdateItem>();
        var alive = new HashSet<string>();
        foreach (var c in source)
        {
            if (tokens.Count > 0)
            {
                var words = Ru.Words(c.Name + " " + c.FileName);
                if (!tokens.All(t => words.Any(w => w.StartsWith(t, StringComparison.Ordinal)))) continue;
            }
            if (!_items.TryGetValue(c.Id, out var it)) _items[c.Id] = it = new UpdateItem(c);
            Fill(it, c.Data, _filter);
            visible.Add(it);
            alive.Add(c.Id);
        }
        foreach (var k in _items.Keys.Where(k => !alive.Contains(k)).ToList()) _items.Remove(k);
        Items.SyncTo(visible);
        SearchNote = GarageViewModel.SearchNoteFor(tokens.Count > 0, visible.Count);
        Empty = EmptyFor(source.Count, visible.Count, tokens.Count > 0);
        RebuildPanel();
    }

    private void Fill(UpdateItem it, GarageMod d, UpdatesFilter f)
    {
        var u = d.Update;
        var site = u is not null ? SiteNames.Of(u.Source) : d.Binding is { } b ? SiteNames.Of(b.Source) : "";
        it.PageUrl = u?.Url ?? d.Binding?.Url;
        string? sizes = d.Mod.SizeBytes is { } a && d.SiteSizeBytes is { } s && SizeComparer.Differs(a, s) ? $"{Ru.Size(a)} → {Ru.Size(s)}" : null;
        var seen = d.SiteChangedUtc is { } ch ? $"новую версию заметили {Ru.Ago(ch, _now())}" : null;

        if (f == UpdatesFilter.Missing)
        {
            it.Kind = UpdateKind.Missing;
            it.Chip = "Страница пропала";
            it.OldVersion = it.NewVersion = null;
            it.Line1 = "Страницу не нашли две проверки подряд.";
            it.Line2 = "Мод в игре работает как раньше, не обновляется только проверка.";
            Actions(it, ItemAction.FindReplacement, ItemAction.Decline, ItemAction.None);
        }
        else if (f == UpdatesFilter.Hidden && u is not null)
        {
            it.Kind = UpdateKind.Hidden;
            it.Chip = "Скрыто";
            it.OldVersion = u.Status == UpdateStatus.Likely ? d.Mod.Version ?? "?" : null;
            it.NewVersion = u.Status == UpdateStatus.Likely ? u.LatestVersion ?? "?" : null;
            it.Line1 = u.Status == UpdateStatus.Likely && u.LatestVersion is { } lv
                ? $"Версию {lv} вы пропустили. Другая версия снова покажет обновление."
                : "Изменение файла вы отметили как «версия не изменилась». Новое изменение снова покажет его.";
            it.Line2 = site;
            Actions(it, ItemAction.Restore, ItemAction.OpenPage, ItemAction.None);
        }
        else if (u is { Status: UpdateStatus.Possible })
        {
            it.Kind = UpdateKind.Possible;
            it.Chip = "Возможно, обновлён";
            it.OldVersion = it.NewVersion = null;
            it.Line1 = sizes is null ? "Файл на сайте изменился, а номер версии тот же."
                : u.LatestVersion is null ? $"Номер версии на сайте не указан, а размер изменился: {sizes}."
                : $"Версия та же, но размер на сайте изменился: {sizes}.";
            it.Line2 = $"{site}. Возможно, автор перезалил файл. Проверьте описание на странице.";
            Actions(it, ItemAction.OpenPage, ItemAction.Dismiss, ItemAction.None);
        }
        else if (u is not null)
        {
            it.Kind = UpdateKind.Update;
            it.Chip = null;
            it.OldVersion = d.Mod.Version ?? "?";
            it.NewVersion = u.LatestVersion ?? "?";
            it.Line1 = "Версия на сайте выше установленной.";
            var parts = new List<string> { site };
            if (seen is not null) parts[0] += ", " + seen;
            if (sizes is not null) parts.Add("Размер " + sizes);
            it.Line2 = string.Join(". ", parts) + ".";
            // загрузки в программе пока нет: главная — «Открыть страницу», скачивает человек сам (DESIGN_FLOWS 3.3)
            if (_garage.DownloadsAllowed) Actions(it, ItemAction.Update, ItemAction.OpenPage, ItemAction.Dismiss);
            else Actions(it, ItemAction.OpenPage, ItemAction.Dismiss, ItemAction.None);
        }
    }

    private static void Actions(UpdateItem it, ItemAction p, ItemAction s, ItemAction t)
    {
        it.Primary = p; it.Second = s; it.Third = t;
        it.PrimaryLabel = p == ItemAction.None ? null : UpdateItem.LabelOf(p, it.Kind);
        it.SecondLabel = s == ItemAction.None ? null : UpdateItem.LabelOf(s, it.Kind);
        it.ThirdLabel = t == ItemAction.None ? null : UpdateItem.LabelOf(t, it.Kind);
    }

    private EmptyState? EmptyFor(int inFilter, int visible, bool searching)
    {
        if (visible > 0) return null;
        if (searching && inFilter > 0) return new("Нет такого мода", "Ищем только среди найденного в этом списке", null, EmptyAction.None);
        switch (_filter)
        {
            case UpdatesFilter.Hidden: return new("Скрытых нет", "Здесь будут версии, которые вы пропустили", null, EmptyAction.None);
            case UpdatesFilter.Missing: return new("Все страницы на месте", "Если страница мода пропадёт с сайта, он появится здесь", null, EmptyAction.None);
        }
        if (!_garage.FolderAvailable) return new("Не нашли папку с модами", "Укажите её, и мы проверим моды на сайтах", "Указать папку", EmptyAction.PickFolder);
        if (_garage.AllCards.Count == 0) return new("Модов пока нет", "Проверять пока нечего", null, EmptyAction.None);
        if (!_garage.AllCards.Any(c => c.Data.Tracked))
            return new("Пока ни за одним модом не следим", "Откройте мод на вкладке «Моды» и укажите его страницу на сайте", "Перейти к модам", EmptyAction.ShowAll);
        return _backend.LastCheckedUtc() is { } t
            ? new("Всё свежее", "Проверено " + Ru.Ago(t, _now()), Checking ? null : "Проверить сейчас", EmptyAction.CheckNow)
            : new("Ещё не проверяли", "Проверим ваши моды на сайтах", Checking ? null : "Проверить сейчас", EmptyAction.CheckNow);
    }

    // ───────────── проверка ─────────────

    private void RebuildPanel()
    {
        var hours = _settings.AutoCheckEveryHours;
        var every = $"раз в {Ru.Count(hours, "час", "часа", "часов")}";
        AutoNote = (hours > 0, _settings.AutoCheckOnStart) switch
        {
            (true, true) => $"Проверяем сами при запуске и {every}",
            (true, false) => $"Проверяем сами {every}",
            (false, true) => "Проверяем сами при запуске",
            _ => "Проверяем только по кнопке",
        };
        if (Checking)
        {
            CheckButton = "Остановить";
            CheckText = "Проверяем сайты. Окно можно закрыть: найденное сохранится.";
        }
        else
        {
            CheckButton = _offerForce ? "Всё равно проверить" : "Проверить сейчас";
            CheckText = _result ?? (_backend.LastCheckedUtc() is { } t
                ? $"Последняя {Ru.When(t, _now())}. Окно можно закрыть: найденное сохранится."
                : "Ещё не проверяли. Проверим моды, за которыми следим на сайтах.");
        }
        RebuildSites();
    }

    private void RebuildSites()
    {
        var report = _backend.LastReport;
        var rows = new List<SiteRow>();
        foreach (var s in _backend.Sites())
        {
            if (!_siteRows.TryGetValue(s.Id, out var r)) _siteRows[s.Id] = r = new SiteRow(s.Id);
            r.Name = s.Name;
            r.Home = s.Home;
            var rep = report?.Sources.FirstOrDefault(x => x.Source == s.Id);
            // «вы человек?» помнит библиотека, пока человек не нажмёт «Готово»; прошлый отчёт об этом сайте после «Готово» не в счёт
            if (_resolved.Contains(s.Id)) rep = null;
            r.NeedsHuman = s.Enabled && s.Status == SourceStatus.NeedsHumanCheck;
            if (!r.NeedsHuman) r.AwaitingConfirm = false;
            r.Note = rep?.Note;
            (r.Dot, r.State) = !s.Enabled ? (SiteDot.Off, "выключен")
                : r.NeedsHuman ? (SiteDot.Warn, "нужна проверка")
                : _resolved.Contains(s.Id) ? (SiteDot.Off, "проверим в следующий раз")
                : rep is { Status: SourceStatus.Unavailable } ? (SiteDot.Warn, "не отвечает")
                : rep is { LayoutChanged: true } ? (SiteDot.Warn, "возможно, сайт изменился")
                : rep is { Note: not null } ? (SiteDot.Warn, "не всё успели")
                : s.Tracked == 0 ? (SiteDot.Off, "нет модов")
                : rep is not null ? (SiteDot.Ok, "готово")
                : (SiteDot.Off, Ru.Mods(s.Tracked));
            rows.Add(r);
        }
        Sites.SyncTo(rows);
    }

    /// <summary>Кнопка в панели «Проверка»: запустить, остановить или проверить принудительно (после «Проверено 6 минут назад»).</summary>
    public Task CheckNowAsync()
    {
        if (Checking) { _backend.Cancel(); return Task.CompletedTask; }
        var force = _offerForce;
        return RunAsync(force, manual: true);
    }

    /// <summary>При запуске окна, если включено в настройках.</summary>
    public Task StartupAsync() => _settings.AutoCheckOnStart ? RunAsync(false, manual: false) : Task.CompletedTask;

    /// <summary>Окно зовёт раз в минуту: повтор раз в N часов (настройка «Проверять ещё раз через»), пока программа открыта.</summary>
    public Task TickAsync()
    {
        var hours = _settings.AutoCheckEveryHours;
        if (hours <= 0 || Checking) return Task.CompletedTask;
        var last = _autoAt ?? _backend.LastCheckedUtc();
        if (last is { } l && _now() - l < TimeSpan.FromHours(hours) && _now() >= l) return Task.CompletedTask;
        return RunAsync(false, manual: false);
    }

    private async Task RunAsync(bool force, bool manual)
    {
        if (Checking) return;
        var started = _now();
        _autoAt = started;
        _progress.Clear();
        ProgressText = null;
        ProgressValue = 0;
        _result = null;
        _offerForce = false;
        _resolved.Clear();
        Checking = true;
        Rebuild(); // пустое состояние прячет «Проверить сейчас», пока идёт проверка
        var progress = new Relay(p => _post(() => OnProgress(p)));
        UpdateRunReport? report = null;
        try
        {
            report = await _backend.CheckAsync(force, progress);
        }
        catch (Exception e) when (e is not OutOfMemoryException)
        {
            // сбой проверки не роняет окно: найденное раньше остаётся, причина — строкой в панели
            _result = "Проверка не удалась: " + e.Message;
        }
        Checking = false;
        ProgressText = null;
        if (report is not null) _result = Summarize(report, started, manual);
        _garage.Refresh(); // новые версии видны и в гараже (плашки, фильтр «Нужно обновить», бейдж)
        Rebuild();
    }

    private void OnProgress(UpdateProgress p)
    {
        if (!Checking || p.Stage == "готово" || p.Source.Length == 0) return;
        _progress[p.Source + "|" + p.Stage] = (p.Done, Math.Max(p.Done, p.Total));
        // карточки — основная работа; ленты считаем, пока до карточек не дошли
        var cards = _progress.Where(kv => kv.Key.EndsWith("|карточки", StringComparison.Ordinal)).Select(kv => kv.Value).ToList();
        var parts = cards.Count > 0 ? cards : _progress.Values.ToList();
        var done = parts.Sum(x => x.Done);
        var total = parts.Sum(x => x.Total);
        if (total <= 0) return;
        ProgressText = cards.Count > 0 ? $"{done} из {total}" : "читаем ленты сайтов";
        ProgressValue = Math.Clamp(done / (double)total, 0, 1);
    }

    private string Summarize(UpdateRunReport r, DateTimeOffset started, bool manual)
    {
        if (r.Reused) return "Проверили только что. Ещё раз можно через минуту.";
        if (r.Cancelled) return "Остановлено. Найденное сохранено.";
        var news = r.NewFindings.Count;
        var found = news > 0 ? $"Нашли {Ru.Count(news, "новую версию", "новые версии", "новых версий")}." : null;
        var failed = r.Sources.Where(s => s.Status != SourceStatus.Ok).Select(s => SiteNames.Of(s.Source)).ToList();
        if (r.Requests == 0 && failed.Count == 0 && r.FreshSkipped > 0)
        {
            // всё проверено недавно: сайты не трогали (срок — настройка «не чаще раза в N часов»)
            _offerForce = manual;
            var last = _backend.LastCheckedUtc();
            return $"Проверено {(last is { } l ? Ru.Ago(l, _now()) : "недавно")}. Новое смотрим не чаще раза в {Ru.Count(_settings.Current.CheckTtlHours, "час", "часа", "часов")}.";
        }
        if (!r.Complete)
        {
            var (done, total) = _backend.Coverage(started);
            var why = failed.Count > 0 ? $" Не ответили: {string.Join(", ", failed)}." : "";
            // сайт ответил, но страницы не понял: это не «обновлений нет», об этом надо сказать прямо
            var changed = r.Sources.Where(s => s.Status == SourceStatus.Ok && s.LayoutChanged).Select(s => SiteNames.Of(s.Source)).ToList();
            if (changed.Count > 0) why += $" Не смогли прочитать страницы: {string.Join(", ", changed)}. Возможно, сайт изменился.";
            var head = total > 0 && done < total ? $"Проверили {done} из {total}. Остальные при следующей проверке." : "Проверили не всё.";
            return string.Join(" ", new[] { found, head + why }.Where(x => x is not null));
        }
        return found ?? "Новых версий нет. Всё свежее.";
    }

    /// <summary>Ход проверки без захвата потока (его передаём в окно сами через post).</summary>
    private sealed class Relay(Action<UpdateProgress> on) : IProgress<UpdateProgress>
    {
        public void Report(UpdateProgress value) => on(value);
    }

    // ───────────── действия ─────────────

    /// <summary>
    /// Кнопки карточки, которые делает сам раздел: скрыть версию, вернуть, не проверять. «Открыть страницу», «Обновить» и
    /// «Найти замену» открывает окно (браузер, загрузка, диалог).
    /// </summary>
    public void Act(UpdateItem item, ItemAction action)
    {
        var id = item.ModId;
        var name = item.Card.Name;
        switch (action)
        {
            case ItemAction.Dismiss:
            {
                var kind = item.Kind;
                var ver = item.NewVersion;
                if (!_backend.Dismiss(id)) return;
                _garage.Refresh();
                var text = kind == UpdateKind.Possible ? $"Отметили: у «{name}» версия не изменилась"
                    : ver is not null && ver != "?" ? $"Версия {ver} скрыта. Другая версия снова покажет обновление"
                    : "Обновление скрыто. Другая версия снова его покажет";
                _garage.Notify(text, () => { _backend.Restore(id); _garage.Refresh(); });
                break;
            }
            case ItemAction.Restore:
                _backend.Restore(id);
                _garage.Refresh();
                _garage.Notify($"«{name}» снова в списке обновлений", null);
                break;
            case ItemAction.Decline:
            {
                var undo = _backend.Decline(id);
                if (undo is null) return;
                _garage.Refresh();
                _garage.Notify($"Не проверяем «{name}». Мод в фильтре «Не проверяются»", () => { _backend.UndoDecline(undo); _garage.Refresh(); });
                break;
            }
        }
    }

    /// <summary>Сайт просил «вы человек?»: человек открыл его в браузере — показываем «Готово».</summary>
    public void SiteOpened(SiteRow row) => row.AwaitingConfirm = true;

    /// <summary>«Готово» после проверки на сайте: снимаем паузу, следующая проверка его опросит.</summary>
    public void SiteConfirmed(SiteRow row)
    {
        _backend.ResolveHumanCheck(row.Id);
        _resolved.Add(row.Id);
        RebuildSites();
    }
}
