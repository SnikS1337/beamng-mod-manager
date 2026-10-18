using BeamModManager.Core.Matching;
using BeamModManager.Core.Models;
using BeamModManager.Core.Search;
using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources;
using BeamModManager.Ui.Updates;

namespace BeamModManager.Ui.Search;

/// <summary>Сайт в панели «Сайты» раздела «Поиск»: включён ли он в настройках и умеет ли искать.</summary>
public sealed record SearchSite(string Id, string Name, bool Enabled, bool CanSearch);

/// <summary>Чем раздел «Поиск» пользуется снаружи. В тестах подменяется, в приложении — <see cref="LibrarySearchBackend"/>.</summary>
public interface ISearchBackend
{
    IReadOnlyList<SearchSite> Sites();

    /// <summary>Поиск по включённым сайтам; промежуточные ответы приходят из фона по мере ответов сайтов.</summary>
    Task<SearchResponse> SearchAsync(string query, Action<SearchResponse>? progress, CancellationToken ct);

    /// <summary>Пересобрать карточки из уже найденного (после «Это один мод» / «Это разные»), не обращаясь к сайтам.</summary>
    SearchResponse Regroup(SearchResponse previous, string query);

    /// <summary>Запомнить решение по паре записей навсегда.</summary>
    void Decide(Listing a, Listing b, ManualMatch decision);

    /// <summary>Самый короткий запрос, который уходит на сайты.</summary>
    int MinQueryLength => 2;

    /// <summary>
    /// Картинка мода по его странице — для выдачи без картинок (ModsGaming, 2026-10-18). Страница грузится той же вежливой
    /// очередью, что и поиск. null — картинки нет или сайт не наш.
    /// </summary>
    Task<Uri?> PreviewOfAsync(Uri page, CancellationToken ct) => Task.FromResult<Uri?>(null);
}

/// <summary>Поиск поверх настоящих сайтов, настроек («Настройки → Сайты») и ручных решений по дублям.</summary>
public sealed class LibrarySearchBackend : ISearchBackend
{
    private readonly IReadOnlyList<IModSource> _all;
    private readonly Func<AppSettings> _settings;
    private readonly IMatchDecisions _decisions;
    private readonly Func<string, SourceStatus>? _hostStatus;

    /// <param name="hostStatus">Состояние сайта в сетевом слое по адресу (пауза после сбоев, «вы человек?»): такой сайт не опрашиваем.</param>
    public LibrarySearchBackend(IReadOnlyList<IModSource> sources, Func<AppSettings> settings, IMatchDecisions decisions,
        Func<string, SourceStatus>? hostStatus = null)
    {
        _all = sources;
        _settings = settings;
        _decisions = decisions;
        _hostStatus = hostStatus;
    }

    public IReadOnlyList<SearchSite> Sites()
    {
        var s = _settings();
        return _all.Select(x => new SearchSite(x.Id, SiteNames.Of(x.Id), s.IsSourceEnabled(x.Id), x.SupportsSearch)).ToList();
    }

    /// <summary>Картинка со страницы мода: страницу открывает только её сайт (остальные отвечают null без запроса).</summary>
    public async Task<Uri?> PreviewOfAsync(Uri page, CancellationToken ct)
    {
        foreach (var src in _all)
        {
            try
            {
                if (await src.GetListingAsync(page, ct).ConfigureAwait(false) is { PreviewUrl: { } img }) return img;
            }
            catch (Exception e) when (e is not OperationCanceledException) { return null; } // сайт не ответил — без картинки
        }
        return null;
    }

    private SearchService Service()
    {
        var s = _settings();
        Func<string, SourceStatus>? status = _hostStatus is { } hs
            ? id => _all.FirstOrDefault(x => x.Id == id) is SiteAdapter a ? hs(a.Home.Host) : SourceStatus.Ok
            : null;
        return new SearchService(_all.Where(x => x.SupportsSearch && s.IsSourceEnabled(x.Id)), status, decisions: _decisions);
    }

    public Task<SearchResponse> SearchAsync(string query, Action<SearchResponse>? progress, CancellationToken ct) =>
        Service().SearchAsync(query, progress, ct);

    public SearchResponse Regroup(SearchResponse previous, string query) => Service().Regroup(previous, query);

    public void Decide(Listing a, Listing b, ManualMatch decision)
    {
        if (string.Equals(a.Source, b.Source, StringComparison.OrdinalIgnoreCase)) return; // записи одного сайта не объединяются
        _decisions.Set(a, b, decision);
    }
}

/// <summary>Искать негде (демо без сайтов, раздел ещё не подключён): сайтов нет, раздел скажет об этом словами.</summary>
public sealed class NoSearch : ISearchBackend
{
    public IReadOnlyList<SearchSite> Sites() => Array.Empty<SearchSite>();
    public Task<SearchResponse> SearchAsync(string query, Action<SearchResponse>? progress, CancellationToken ct) =>
        Task.FromResult(new SearchResponse(Array.Empty<ModCard>(), Array.Empty<SourceReport>()));
    public SearchResponse Regroup(SearchResponse previous, string query) => previous;
    public void Decide(Listing a, Listing b, ManualMatch decision) { }
}
