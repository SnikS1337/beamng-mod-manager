using System.Collections.ObjectModel;
using BeamModManager.Core.Matching;
using BeamModManager.Core.Models;
using BeamModManager.Core.Search;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Text;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Mvvm;
using BeamModManager.Ui.Text;
using BeamModManager.Ui.Updates;
using Found = BeamModManager.Core.Search.ModCard;

namespace BeamModManager.Ui.Search;

/// <summary>Фильтры раздела «Поиск» (DESIGN.md 5.0.1): «Все», «Бесплатные», «Установленные».</summary>
public enum SearchFilter { All, Free, Installed }

/// <summary>Пункт фильтра: те же имена свойств, что у фильтров гаража, — островок сверху у разделов общий.</summary>
public sealed class SearchFilterItem : Observable
{
    public SearchFilter Filter { get; }
    public string Title { get; }
    internal SearchFilterItem(SearchFilter f, string title) { Filter = f; Title = title; }

    private int _count;
    public int Count { get => _count; internal set { if (Set(ref _count, value)) { Raise(nameof(Label)); Raise(nameof(ShowCount)); } } }

    /// <summary>У «Все» число не пишется: его показывает заголовок.</summary>
    public bool ShowCount => Filter != SearchFilter.All && Count > 0;
    public string Label => ShowCount ? $"{Title} {Count}" : Title;

    private bool _isActive;
    public bool IsActive { get => _isActive; internal set => Set(ref _isActive, value); }
}

/// <summary>Плашка цены: «Бесплатно» (белая), «Платный» (янтарная точка), «Цена не указана» (контурная) — бесплатным не считаем.</summary>
public enum PriceKind { Free, Paid, Unknown }

/// <summary>Главная кнопка карточки: «Установить» (загрузка разрешена), «Страница», «Купить у автора» (платный).</summary>
public enum FoundAction { Install, Page, Buy }

/// <summary>Карточка поиска: один мод, найденный на одном или нескольких сайтах.</summary>
public sealed class FoundCard : Observable
{
    internal FoundCard(string id) => Id = id;
    public string Id { get; }

    private Found _data = null!;
    public Found Data { get => _data; private set => Set(ref _data, value); }

    private string _title = "", _subtitle = "", _sites = "", _priceText = "", _primaryLabel = "";
    public string Title { get => _title; private set => Set(ref _title, value); }
    /// <summary>«1.5 · garage47»: версия и автор, что известно.</summary>
    public string Subtitle { get => _subtitle; private set => Set(ref _subtitle, value); }
    /// <summary>«Modland · STMods»: где нашёлся.</summary>
    public string SitesText { get => _sites; private set => Set(ref _sites, value); }

    private PriceKind _price;
    public PriceKind Price { get => _price; private set => Set(ref _price, value); }
    public string PriceText { get => _priceText; private set => Set(ref _priceText, value); }

    private FoundAction _primary;
    public FoundAction Primary { get => _primary; private set { if (Set(ref _primary, value)) Raise(nameof(ShowPage)); } }
    public string PrimaryLabel { get => _primaryLabel; private set => Set(ref _primaryLabel, value); }
    /// <summary>Рядом с «Установить» — «Страница»; у «Страница» и «Купить у автора» вторая кнопка не нужна.</summary>
    public bool ShowPage => Primary == FoundAction.Install;

    private Uri _pageUrl = null!;
    /// <summary>Куда ведут «Страница» и «Купить у автора»: бесплатная запись, если она есть.</summary>
    public Uri PageUrl { get => _pageUrl; private set => Set(ref _pageUrl, value); }

    private Uri? _previewUrl;
    public Uri? PreviewUrl { get => _previewUrl; private set => Set(ref _previewUrl, value); }

    private string? _previewPath;
    /// <summary>Картинка, скачанная окном (кэш на диске); null — заглушка.</summary>
    public string? PreviewPath { get => _previewPath; set => Set(ref _previewPath, value); }

    private PlaceholderKind _placeholder;
    public PlaceholderKind Placeholder { get => _placeholder; private set => Set(ref _placeholder, value); }

    private bool _installed;
    /// <summary>Такой мод уже есть в «Моды» (по странице или по названию): плашка «Установлен».</summary>
    public bool Installed { get => _installed; private set => Set(ref _installed, value); }

    private string? _maybeText;
    /// <summary>
    /// «Возможно, тот же мод, что на ModsGaming» с действиями «Это один мод» / «Это разные» (DESIGN_FLOWS 6.6); null — плашки нет.
    /// Сайт, а не название: названия у таких карточек почти одинаковые, по ним не понять, о какой речь.
    /// </summary>
    public string? MaybeText { get => _maybeText; private set => Set(ref _maybeText, value); }

    /// <summary>Карточка, с которой этот мод, возможно, один.</summary>
    public string? MaybeOtherId { get; private set; }

    internal void Apply(Found f, bool downloads, bool installed, string? maybeSites, string? maybeId)
    {
        Data = f;
        Title = f.Title;
        var version = f.Listings.Select(l => l.ModVersion).FirstOrDefault(v => !string.IsNullOrWhiteSpace(v));
        var author = f.Listings.Select(l => l.Author).FirstOrDefault(a => !string.IsNullOrWhiteSpace(a));
        Subtitle = string.Join(" · ", new[] { version, author }.Where(x => !string.IsNullOrWhiteSpace(x)));
        SitesText = string.Join(" · ", f.Listings.Select(l => SiteNames.Of(l.Source)).Distinct());
        (Price, PriceText) = f.Price switch
        {
            PriceSummary.Free or PriceSummary.Mixed => (PriceKind.Free, "Бесплатно"),
            PriceSummary.Paid => (PriceKind.Paid, "Платный"),
            _ => (PriceKind.Unknown, "Цена не указана"),
        };
        // платный только у автора; бесплатный на каком-то из сайтов — туда и ведём
        var free = f.Listings.FirstOrDefault(l => l.Price == PriceStatus.Free);
        var notPaid = f.Listings.FirstOrDefault(l => l.Price != PriceStatus.Paid);
        PageUrl = (free ?? notPaid ?? f.Listings[0]).Url;
        Primary = f.Price == PriceSummary.Paid ? FoundAction.Buy : downloads ? FoundAction.Install : FoundAction.Page;
        PrimaryLabel = Primary switch { FoundAction.Buy => "Купить у автора", FoundAction.Install => "Установить", _ => "Открыть страницу" };
        PreviewUrl = f.Listings.Select(l => l.PreviewUrl).FirstOrDefault(u => u is not null);
        Placeholder = f.Listings.Any(l => l.Category is { } c && (c.Contains("карт", StringComparison.OrdinalIgnoreCase) || c.Contains("map", StringComparison.OrdinalIgnoreCase)))
            ? PlaceholderKind.Map : PlaceholderKind.Vehicle;
        Installed = installed;
        MaybeOtherId = maybeId;
        MaybeText = maybeSites is null ? null : $"Возможно, тот же мод, что на {maybeSites}";
    }
}

/// <summary>
/// Раздел «Поиск» без окна (DESIGN_FLOWS 3.2, DESIGN.md 5.0.1): запрос по Enter, результаты по мере ответов сайтов (сначала
/// скелеты), один мод с разных сайтов — одна карточка, «Возможно, тот же мод» с решением навсегда, фильтры, панель «Сайты».
/// </summary>
public sealed class SearchViewModel : Observable
{
    /// <summary>Скелетов, пока сайты не ответили (два ряда по три).</summary>
    public const int SkeletonSlots = 6;

    private readonly ISearchBackend _backend;
    private readonly Func<IReadOnlyList<Garage.ModCard>> _installed;
    private readonly Action<Action> _post;
    private readonly Dictionary<string, FoundCard> _cards = new();
    private readonly Dictionary<string, SiteRow> _siteRows = new();
    private SearchResponse? _response;
    private string _ranQuery = "";
    private CancellationTokenSource? _cts;
    private long _serial;

    /// <param name="installed">Карточки «Моды»: по ним фильтр «Установленные» и плашка «Установлен».</param>
    /// <param name="post">Выполнить в потоке окна (сайты отвечают из фона).</param>
    /// <summary>Сайты, проверку которых можно пройти во встроенном окне (см. <see cref="Updates.SiteRow.CanPassCheck"/>).</summary>
    public Func<string, bool> CanPassCheck { get; set; } = _ => false;

    public SearchViewModel(ISearchBackend backend, Func<IReadOnlyList<Garage.ModCard>> installed, Action<Action>? post = null)
    {
        _backend = backend;
        _installed = installed;
        _post = post ?? (a => a());
        Filters = new[]
        {
            new SearchFilterItem(SearchFilter.All, "Все") { IsActive = true },
            new SearchFilterItem(SearchFilter.Free, "Бесплатные"),
            new SearchFilterItem(SearchFilter.Installed, "Установленные"),
        };
        SelectFilterCommand = new RelayCommand(p => { if (p is SearchFilter f) Filter = f; else if (p is SearchFilterItem i) Filter = i.Filter; });
        RefreshSites();
    }

    public ObservableCollection<FoundCard> Cards { get; } = new();
    public ObservableCollection<SiteRow> Sites { get; } = new();
    public IReadOnlyList<SearchFilterItem> Filters { get; }
    public RelayCommand SelectFilterCommand { get; }

    /// <summary>Загрузка разрешена в настройках: у бесплатных «Установить», иначе «Страница» (DESIGN_FLOWS 3.2).</summary>
    public bool DownloadsAllowed
    {
        get => _downloads;
        set { if (_downloads == value) return; _downloads = value; Rebuild(); }
    }
    private bool _downloads;

    private SearchFilter _filter;
    public SearchFilter Filter
    {
        get => _filter;
        set { if (Set(ref _filter, value)) { foreach (var f in Filters) f.IsActive = f.Filter == value; Rebuild(); } }
    }

    private string _query = "";
    /// <summary>Текст поля. На сайты уходит по Enter (<see cref="RunAsync"/>), а не на каждую букву.</summary>
    public string Query
    {
        get => _query;
        set
        {
            if (!Set(ref _query, value ?? "")) return;
            // поле стёрли (Esc, крестик) — это новый пустой поиск: найденное и идущий поиск убираются, снова «Что ищем?»
            if (_query.Trim().Length == 0 && (_response is not null || Searching || _ranQuery.Length > 0)) Clear();
            else Rebuild();
        }
    }

    private void Clear()
    {
        _cts?.Cancel();
        _serial++;
        Searching = false;
        _response = null;
        _ranQuery = "";
        SitesNote = null;
        foreach (var r in _siteRows.Values) r.State = "";
        LoadSites();
        Rebuild();
    }

    private bool _searching;
    public bool Searching { get => _searching; private set { if (Set(ref _searching, value)) Raise(nameof(Skeletons)); } }

    /// <summary>Сколько скелетов показать: пока ищем и карточек ещё нет (DESIGN_COMPONENTS 3.10).</summary>
    public int Skeletons => Searching && Cards.Count == 0 && _response is not { Cards.Count: > 0 } ? SkeletonSlots : 0;

    private EmptyState? _empty;
    public EmptyState? Empty { get => _empty; private set => Set(ref _empty, value); }

    private string _headerNote = "", _countText = "", _searchNote = "";
    /// <summary>Под заголовком «Поиск»: «ищем на 3 сайтах…», «9 модов на 3 сайтах».</summary>
    public string HeaderNote { get => _headerNote; private set => Set(ref _headerNote, value); }
    /// <summary>Число в маленькой капсуле заголовка; до поиска пусто.</summary>
    public string CountText { get => _countText; private set => Set(ref _countText, value); }
    /// <summary>Справа от поля: «найдено 9», «ищем…».</summary>
    public string SearchNote { get => _searchNote; private set => Set(ref _searchNote, value); }

    private string? _sitesNote;
    /// <summary>Под списком сайтов янтарным: кто не ответил, поэтому показано не всё.</summary>
    public string? SitesNote { get => _sitesNote; private set => Set(ref _sitesNote, value); }

    /// <summary>Напоминание внизу панели «Сайты».</summary>
    public string PaidNote => DownloadsAllowed
        ? "Платные моды не скачиваются: вместо «Установить» кнопка ведёт на страницу автора"
        : "Скачайте мод на его странице и перетащите архив на окно. Платные — у автора";

    /// <summary>Найдено ли что-то для последнего запроса (нужно окну: показывать ли сетку).</summary>
    public bool HasResults => _response is { Cards.Count: > 0 };

    // ───────────── поиск ─────────────

    /// <summary>Enter в поле: ищем по запросу; прежний поиск отменяется, его поздние ответы не показываются.</summary>
    public async Task RunAsync()
    {
        var q = (_query ?? "").Trim();
        _cts?.Cancel();
        var serial = ++_serial;
        LoadSites();
        if (q.Length < _backend.MinQueryLength || EnabledSites().Count == 0)
        {
            _response = null;
            _ranQuery = q;
            Searching = false;
            Rebuild();
            return;
        }
        var cts = _cts = new CancellationTokenSource();
        _ranQuery = q;
        _response = null;
        Searching = true;
        foreach (var r in _siteRows.Values) { r.State = "ищем…"; r.Dot = SiteDot.Off; r.Note = null; r.CanPassCheck = false; }
        Rebuild();
        try
        {
            var final = await _backend.SearchAsync(q, partial => _post(() => { if (serial == _serial && Searching) Apply(partial); }), cts.Token)
                .ConfigureAwait(true);
            if (serial != _serial) return;
            Searching = false;
            Apply(final);
        }
        catch (OperationCanceledException) when (cts.IsCancellationRequested)
        {
            // новый запрос или закрытие окна: ничего не показываем
        }
        finally
        {
            if (serial == _serial) Searching = false;
        }
    }

    /// <summary>Окно закрывается или ушли из раздела с новым запросом: поиск прерывается без следов.</summary>
    public void Cancel()
    {
        _cts?.Cancel();
        _serial++;
        Searching = false;
    }

    private void Apply(SearchResponse r)
    {
        _response = r;
        foreach (var s in r.Sources)
        {
            if (!_siteRows.TryGetValue(s.SourceId, out var row)) continue;
            (row.State, row.Dot, row.Note) = s switch
            {
                { Pending: true } => ("ищем…", SiteDot.Off, null),
                { Status: SourceStatus.NeedsHumanCheck } => ("не пускает программу", SiteDot.Warn, s.Note),
                { Status: not SourceStatus.Ok } => ("не ответил", SiteDot.Warn, s.Note),
                { Count: 0 } => ("ничего", SiteDot.Ok, null),
                _ => ($"нашёл {s.Count}", SiteDot.Ok, null),
            };
            row.CanPassCheck = !s.Pending && s.Status == SourceStatus.NeedsHumanCheck && CanPassCheck(s.SourceId);
        }
        var human = r.Sources.Where(s => !s.Pending && s.Status == SourceStatus.NeedsHumanCheck && !CanPassCheck(s.SourceId)).Select(s => SiteNames.Of(s.SourceId)).ToList();
        var passable = r.Sources.Where(s => !s.Pending && s.Status == SourceStatus.NeedsHumanCheck && CanPassCheck(s.SourceId)).Select(s => SiteNames.Of(s.SourceId)).ToList();
        var failed = r.Sources.Where(s => !s.Pending && s.Status is not SourceStatus.Ok and not SourceStatus.NeedsHumanCheck).Select(s => SiteNames.Of(s.SourceId)).ToList();
        var waiting = r.Sources.Any(s => s.Pending);
        var notes = new List<string>();
        if (failed.Count > 0) notes.Add($"{string.Join(", ", failed)} {(failed.Count == 1 ? "не ответил" : "не ответили")}, поэтому показаны не все результаты" + (waiting ? ". Поиск продолжается" : ""));
        if (passable.Count > 0) notes.Add($"{string.Join(", ", passable)} {(passable.Count == 1 ? "просит" : "просят")} подтвердить, что вы человек: «Пройти проверку» в списке сайтов");
        if (human.Count > 0) notes.Add($"{string.Join(", ", human)} {(human.Count == 1 ? "не пускает" : "не пускают")} программу: просит подтвердить, что вы человек. Ищем без {(human.Count == 1 ? "него" : "них")}");
        SitesNote = notes.Count == 0 ? null : string.Join(". ", notes);
        Rebuild();
    }

    private IReadOnlyList<SearchSite> EnabledSites() => _sites.Where(s => s.Enabled && s.CanSearch).ToList();
    private IReadOnlyList<SearchSite> _sites = Array.Empty<SearchSite>();

    /// <summary>Сайты могли включить или выключить в настройках: строки панели и подписи — по текущему списку.</summary>
    public void RefreshSites()
    {
        LoadSites();
        Rebuild();
    }

    private void LoadSites()
    {
        _sites = _backend.Sites();
        var wanted = new List<SiteRow>();
        foreach (var s in _sites.Where(x => x.CanSearch))
        {
            if (!_siteRows.TryGetValue(s.Id, out var row)) _siteRows[s.Id] = row = new SiteRow(s.Id);
            row.Name = s.Name;
            if (!s.Enabled) { row.State = "выключен"; row.Dot = SiteDot.Off; row.Note = "Включается в «Настройки → Сайты»"; }
            else if (row.State is "" or "выключен") { row.State = "готов"; row.Dot = SiteDot.Ok; row.Note = null; }
            wanted.Add(row);
        }
        Sites.SyncTo(wanted);
    }

    // ───────────── карточки ─────────────

    private void Rebuild()
    {
        var installedKeys = InstalledKeys();
        var all = new List<FoundCard>();
        var byId = new Dictionary<string, Found>();
        if (_response is { } r)
        {
            foreach (var f in r.Cards) byId[f.Id] = f;
            foreach (var f in r.Cards)
            {
                if (!_cards.TryGetValue(f.Id, out var c)) _cards[f.Id] = c = new FoundCard(f.Id);
                var other = f.PossibleSameAs.Select(id => byId.GetValueOrDefault(id)).FirstOrDefault(x => x is not null);
                var otherSites = other is null ? null : string.Join(", ", other.Listings.Select(l => SiteNames.Of(l.Source)).Distinct());
                c.Apply(f, _downloads, IsInstalled(f, installedKeys), otherSites, other?.Id);
                all.Add(c);
            }
        }
        var alive = all.Select(c => c.Id).ToHashSet();
        foreach (var gone in _cards.Keys.Where(k => !alive.Contains(k)).ToList()) _cards.Remove(gone);

        Filters[0].Count = all.Count;
        Filters[1].Count = all.Count(c => c.Price == PriceKind.Free);
        Filters[2].Count = all.Count(c => c.Installed);
        var visible = all.Where(c => _filter switch
        {
            SearchFilter.Free => c.Price == PriceKind.Free,
            SearchFilter.Installed => c.Installed,
            _ => true,
        }).ToList();
        Cards.SyncTo(visible);
        Raise(nameof(Skeletons));
        Raise(nameof(HasResults));
        Raise(nameof(PaidNote));

        var sites = EnabledSites().Count;
        HeaderNote = Searching ? $"ищем на {sites} {Ru.Plural(sites, "сайте", "сайтах", "сайтах")}…"
            : _response is null ? (sites == 0 ? "все сайты выключены" : $"ищем сразу на {sites} {Ru.Plural(sites, "сайте", "сайтах", "сайтах")}")
            : all.Count == 0 ? "ничего не нашли" : Ru.Mods(all.Count);
        CountText = _response is null ? "" : all.Count.ToString(System.Globalization.CultureInfo.InvariantCulture);
        SearchNote = Searching ? "ищем…" : _response is null ? "" : all.Count == 0 ? "ничего не нашли" : $"найдено {all.Count}";
        Empty = EmptyFor(all.Count, visible.Count, sites);
    }

    private EmptyState? EmptyFor(int total, int visible, int sites)
    {
        if (sites == 0 && EnabledOrNot().Count > 0)
            return new("Все сайты выключены", "Включите хотя бы один", "Открыть настройки сайтов", EmptyAction.OpenSiteSettings);
        if (Searching) return null; // скелеты
        if (_response is null)
        {
            var q = (_query ?? "").Trim();
            if (_ranQuery.Length > 0 && _ranQuery.Length < _backend.MinQueryLength && q == _ranQuery)
                return new("Слишком короткий запрос", $"Нужно хотя бы {_backend.MinQueryLength} {Ru.Plural(_backend.MinQueryLength, "буква", "буквы", "букв")}", null, EmptyAction.None);
            return new("Что ищем?", "Ищем сразу на нескольких сайтах", null, EmptyAction.None);
        }
        if (total == 0)
        {
            // все сайты не ответили — это не «ничего нет»
            if (_response.Sources.Count > 0 && _response.Sources.All(s => s.Status != SourceStatus.Ok))
                return new("Сайты не ответили", "Проверьте соединение и попробуйте ещё раз", "Повторить", EmptyAction.Retry);
            return new("Ничего не нашли", "Проверьте написание или включите больше сайтов", "Открыть сайты", EmptyAction.OpenSiteSettings);
        }
        if (visible > 0) return null;
        return _filter switch
        {
            SearchFilter.Free => new("Бесплатных нет", "Среди найденного только платные и без указанной цены", "Показать все", EmptyAction.ShowAll),
            SearchFilter.Installed => new("Ничего из этого не установлено", "Найденного ещё нет в «Моды»", "Показать все", EmptyAction.ShowAll),
            _ => null,
        };
    }

    private IReadOnlyList<SearchSite> EnabledOrNot() => _sites.Where(s => s.CanSearch).ToList();

    /// <summary>Что уже стоит: страницы, за которыми следим, и названия модов.</summary>
    private (HashSet<(string, string)> Pages, HashSet<string> Urls, HashSet<string> Names) InstalledKeys()
    {
        var pages = new HashSet<(string, string)>();
        var urls = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var names = new HashSet<string>();
        foreach (var c in _installed())
        {
            if (c.Data.Binding is { } b) { pages.Add((b.Source, b.SourceId)); urls.Add(b.Url.AbsoluteUri.TrimEnd('/')); }
            var n = TitleNormalizer.Normalize(TitleParser.Parse(c.Name).Name);
            if (n.Length >= 3) names.Add(n);
        }
        return (pages, urls, names);
    }

    private static bool IsInstalled(Found f, (HashSet<(string, string)> Pages, HashSet<string> Urls, HashSet<string> Names) k) =>
        f.Listings.Any(l => k.Pages.Contains((l.Source, l.SourceId)) || k.Urls.Contains(l.Url.AbsoluteUri.TrimEnd('/')))
        || k.Names.Contains(TitleNormalizer.Normalize(TitleParser.Parse(f.Title).Name));

    /// <summary>«Моды» перечитаны (поставили или удалили мод): плашки «Установлен» и фильтр — по новому списку.</summary>
    public void InstalledChanged() => Rebuild();

    // ───────────── «Возможно, тот же мод» (DESIGN_FLOWS 6.6) ─────────────

    /// <summary>«Это один мод»: две карточки сливаются в одну и так запоминается навсегда.</summary>
    public void SameMod(FoundCard card) => Decide(card, ManualMatch.Same);

    /// <summary>«Это разные»: плашка пропадает, карточки остаются отдельными, и больше не спрашиваем.</summary>
    public void DifferentMods(FoundCard card) => Decide(card, ManualMatch.Different);

    private void Decide(FoundCard card, ManualMatch decision)
    {
        if (_response is not { } r || card.MaybeOtherId is not { } otherId) return;
        var other = r.Cards.FirstOrDefault(c => c.Id == otherId);
        if (other is null) return;
        var pairs = card.Data.Listings.SelectMany(a => other.Listings.Select(b => (a, b)))
            .Where(p => !string.Equals(p.a.Source, p.b.Source, StringComparison.OrdinalIgnoreCase))
            .Where(p => DuplicateMatcher.Compare(p.a, p.b).Verdict == MatchVerdict.Maybe)
            .ToList();
        if (pairs.Count == 0)
        {
            // сомнение было между другими записями этих карточек: решение — по первой паре с разных сайтов
            var any = card.Data.Listings.SelectMany(a => other.Listings.Select(b => (a, b)))
                .FirstOrDefault(p => !string.Equals(p.a.Source, p.b.Source, StringComparison.OrdinalIgnoreCase));
            if (any.a is null) return;
            pairs.Add(any);
        }
        // «один мод» — достаточно одной пары, она тянет за собой карточку; «разные» — все сомнительные пары
        foreach (var (a, b) in decision == ManualMatch.Same ? pairs.Take(1) : pairs) _backend.Decide(a, b, decision);
        _response = _backend.Regroup(r, _ranQuery);
        Rebuild();
    }
}
