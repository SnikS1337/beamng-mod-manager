using BeamModManager.Core.Matching;
using BeamModManager.Core.Models;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Text;

namespace BeamModManager.Core.Search;

/// <summary>Итог цены по всем источникам карточки. Показывается рядом с ценой каждого источника, а не вместо неё.</summary>
public enum PriceSummary { Unknown, Free, Paid, Mixed }

public sealed record ModCard(
    string Id,
    string Title,
    IReadOnlyList<Listing> Listings,
    PriceSummary Price,
    /// <summary>Карточки, которые, возможно, тот же мод. Автоматически не объединяются, решает пользователь.</summary>
    IReadOnlyList<string> PossibleSameAs,
    double Relevance);

public sealed record SourceReport(string SourceId, string DisplayName, SourceStatus Status, int Count, string? Note)
{
    /// <summary>Сайт ещё не ответил (промежуточный ответ поиска): «ищем…».</summary>
    public bool Pending { get; init; }
}

public sealed record SearchResponse(IReadOnlyList<ModCard> Cards, IReadOnlyList<SourceReport> Sources)
{
    /// <summary>Все записи в порядке получения от сайтов. По ним <see cref="SearchService.Regroup"/> пересобирает карточки так же, как исходный поиск.</summary>
    public IReadOnlyList<Listing> Listings { get; init; } = Array.Empty<Listing>();
}

public sealed class SearchOptions
{
    public TimeSpan PerSourceTimeout { get; init; } = TimeSpan.FromSeconds(20);
    public int MaxQueryLength { get; init; } = 100;
    public int MaxPerSource { get; init; } = 30;
    public int MinQueryLength { get; init; } = 2;
}

/// <summary>
/// Общий поиск по всем источникам: один запрос, одна карточка на мод, источники и цены внутри.
/// Объединяем только при уверенном совпадении, сомнительное помечаем, чужое не склеиваем.
/// </summary>
public sealed class SearchService
{
    private readonly IReadOnlyList<IModSource> _sources;
    private readonly Func<string, SourceStatus> _status;
    private readonly SearchOptions _o;
    private readonly IMatchDecisions? _decisions;

    public SearchService(IEnumerable<IModSource> sources, Func<string, SourceStatus>? statusOf = null, SearchOptions? options = null,
        IMatchDecisions? decisions = null)
    {
        _sources = sources.ToList();
        _status = statusOf ?? (_ => SourceStatus.Ok);
        _o = options ?? new SearchOptions();
        _decisions = decisions;
    }

    /// <summary>Хранилище ручных решений («Это один мод» / «Это разные»), если оно подключено.</summary>
    public IMatchDecisions? Decisions => _decisions;

    /// <summary>
    /// Заново собирает карточки из уже найденных записей, не обращаясь к сайтам. Нужен сразу после ручного решения:
    /// карточки склеиваются или расходятся мгновенно, а отчёт по сайтам остаётся прежним.
    /// </summary>
    public SearchResponse Regroup(SearchResponse previous, string query)
    {
        var listings = previous.Listings.Count > 0 ? previous.Listings : previous.Cards.SelectMany(c => c.Listings).ToList();
        return new SearchResponse(Group(listings, (query ?? "").Trim()), previous.Sources) { Listings = listings };
    }

    public Task<SearchResponse> SearchAsync(string query, CancellationToken ct = default) => SearchAsync(query, null, ct);

    /// <summary>Запрос, который реально уйдёт на сайты: без пробелов по краям и не длиннее предела.</summary>
    public string Normalize(string? query)
    {
        var q = (query ?? "").Trim();
        return q.Length > _o.MaxQueryLength ? q[.._o.MaxQueryLength].TrimEnd() : q; // слишком длинный запрос ломает адрес (ошибка 414)
    }

    /// <summary>Запрос короче этого на сайты не уходит.</summary>
    public int MinQueryLength => _o.MinQueryLength;

    /// <param name="progress">
    /// Промежуточные ответы по мере того, как отвечают сайты (DESIGN_FLOWS 3.2: «результаты приходят по мере ответов»):
    /// карточки из уже ответивших, у остальных сайтов <see cref="SourceReport.Pending"/>. Последний ответ — сам результат.
    /// Порядок записей — порядок сайтов, а не ответов: карточки собираются так же, как в итоговом ответе.
    /// </param>
    public async Task<SearchResponse> SearchAsync(string query, Action<SearchResponse>? progress, CancellationToken ct = default)
    {
        query = Normalize(query);
        if (query.Length < _o.MinQueryLength)
            return new SearchResponse(Array.Empty<ModCard>(), Array.Empty<SourceReport>());

        var tasks = _sources.Select(s => RunOne(s, query, ct)).ToList();
        if (progress is not null)
        {
            var left = tasks.ToList();
            while (left.Count > 1)
            {
                var finished = await Task.WhenAny(left).ConfigureAwait(false);
                left.Remove(finished);
                ct.ThrowIfCancellationRequested();
                progress(Partial(tasks, query));
            }
        }
        var done = await Task.WhenAll(tasks).ConfigureAwait(false);

        var reports = done.Select(d => d.Report).ToList();
        var listings = done.SelectMany(d => d.Listings).ToList();
        return new SearchResponse(Group(listings, query), reports) { Listings = listings };
    }

    private SearchResponse Partial(IReadOnlyList<Task<(SourceReport Report, IReadOnlyList<Listing> Listings)>> tasks, string query)
    {
        var reports = new List<SourceReport>();
        var listings = new List<Listing>();
        for (var i = 0; i < tasks.Count; i++)
        {
            if (tasks[i].IsCompletedSuccessfully)
            {
                reports.Add(tasks[i].Result.Report);
                listings.AddRange(tasks[i].Result.Listings);
            }
            else reports.Add(new SourceReport(_sources[i].Id, _sources[i].DisplayName, SourceStatus.Ok, 0, "ищем…") { Pending = true });
        }
        return new SearchResponse(Group(listings, query), reports) { Listings = listings };
    }

    private async Task<(SourceReport Report, IReadOnlyList<Listing> Listings)> RunOne(IModSource s, string query, CancellationToken ct)
    {
        if (!s.SupportsSearch)
            return (new SourceReport(s.Id, s.DisplayName, SourceStatus.Ok, 0, "поиск на этом сайте пока не поддерживается"), Array.Empty<Listing>());

        var st = _status(s.Id);
        if (st != SourceStatus.Ok)
            return (new SourceReport(s.Id, s.DisplayName, st, 0, st == SourceStatus.NeedsHumanCheck ? "нужно пройти проверку на сайте" : "сайт недоступен"), Array.Empty<Listing>());

        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(ct);
        timeout.CancelAfter(_o.PerSourceTimeout);
        try
        {
            var found = await s.SearchAsync(query, timeout.Token).ConfigureAwait(false);
            var list = found.Take(_o.MaxPerSource).ToList();
            return (new SourceReport(s.Id, s.DisplayName, SourceStatus.Ok, list.Count, null), list);
        }
        catch (OperationCanceledException) when (!ct.IsCancellationRequested)
        {
            return (new SourceReport(s.Id, s.DisplayName, SourceStatus.Unavailable, 0, "сайт не ответил вовремя"), Array.Empty<Listing>());
        }
        catch (SourceException ex)
        {
            return (new SourceReport(s.Id, s.DisplayName, ex.Status, 0, ex.Message), Array.Empty<Listing>());
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            return (new SourceReport(s.Id, s.DisplayName, SourceStatus.Unavailable, 0, ex.Message), Array.Empty<Listing>());
        }
    }

    internal IReadOnlyList<ModCard> Group(IReadOnlyList<Listing> listings, string query)
    {
        var clusters = new List<List<Listing>>();

        // Сайт мог вернуть одну и ту же запись дважды (разные категории, повтор на странице): оставляем первую.
        var seen = new HashSet<(string, string)>();
        listings = listings.Where(l => seen.Add((l.Source, l.SourceId))).ToList();

        foreach (var l in listings)
        {
            List<Listing>? best = null;
            var bestScore = -1.0;
            var bestForced = false;
            foreach (var c in clusters)
            {
                // Один сайт не даёт двух записей в одной карточке (кроме повтора той же записи).
                if (c.Any(m => m.Source == l.Source)) continue;
                // Ручное решение сильнее признаков: «разные» запрещает слияние, «один мод» даёт его,
                // даже если другой участник карточки по признакам не совпал.
                if (_decisions is not null && c.Any(m => _decisions.Get(m, l) == ManualMatch.Different)) continue;
                var results = c.Select(m => DuplicateMatcher.Compare(m, l, _decisions)).ToList();
                var forced = _decisions is not null && c.Any(m => _decisions.Get(m, l) == ManualMatch.Same);
                if (!forced && results.Any(r => r.Verdict != MatchVerdict.Same)) continue; // со всеми участниками «Same», иначе цепочки склеят чужое
                var score = results.Average(r => r.Score);
                // Карточка, с которой пользователь сам объединил запись, важнее любой автоматической склейки.
                if ((forced && !bestForced) || (forced == bestForced && score > bestScore)) { best = c; bestScore = score; bestForced = forced; }
            }
            if (best is not null) best.Add(l); else clusters.Add(new List<Listing> { l });
        }

        var cards = clusters.Select((c, i) => (Cluster: c, Id: CardId(c))).ToList();
        var result = new List<ModCard>();
        foreach (var (cluster, id) in cards)
        {
            var maybes = cards
                .Where(o => o.Id != id)
                .Where(o => o.Cluster.Any(b => cluster.Any(a =>
                    a.Source != b.Source && DuplicateMatcher.Compare(a, b, _decisions).Verdict == MatchVerdict.Maybe)))
                .Select(o => o.Id).ToList();

            var title = TitleParser.Parse(cluster[0].Title).Name;
            if (title.Length == 0) title = cluster[0].Title;
            var relevance = cluster.Max(m => TitleNormalizer.Similarity(query, m.Title)) + 0.05 * Math.Min(cluster.Count, 3);
            result.Add(new ModCard(id, title, cluster, Summarize(cluster), maybes, relevance));
        }

        return result.OrderByDescending(c => c.Relevance).ThenBy(c => c.Title, StringComparer.OrdinalIgnoreCase).ToList();
    }

    private static string CardId(List<Listing> c) => string.Join("+", c.Select(l => $"{l.Source}:{l.SourceId}").OrderBy(x => x, StringComparer.Ordinal));

    public static PriceSummary Summarize(IReadOnlyList<Listing> ls)
    {
        var free = ls.Any(l => l.Price == PriceStatus.Free);
        var paid = ls.Any(l => l.Price == PriceStatus.Paid);
        if (free && paid) return PriceSummary.Mixed;
        if (free) return PriceSummary.Free;
        if (paid) return PriceSummary.Paid;
        return PriceSummary.Unknown;
    }
}
