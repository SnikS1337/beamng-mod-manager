using BeamModManager.Core.Models;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Updates;

namespace BeamModManager.Core.Library;

public sealed record UpdateProgress(string Source, string Stage, int Done, int Total);

/// <summary>Что произошло с одним сайтом за проверку.</summary>
public sealed record SourceRunReport(
    string Source,
    SourceStatus Status,
    int FeedPagesFetched,
    int CardsFetched,
    int Postponed,
    IReadOnlyList<string> MissingMods,
    string? Note);

public sealed record UpdateRunReport(
    IReadOnlyList<UpdateFinding> Findings,
    IReadOnlyList<SourceRunReport> Sources,
    int Requests,
    int UpToDate)
{
    /// <summary>Обновления, которых до этой проверки в списке не было (чтобы не показывать старое как новое при повторных нажатиях).</summary>
    public IReadOnlyList<UpdateFinding> NewFindings { get; init; } = Array.Empty<UpdateFinding>();
    /// <summary>Сколько модов не проверялось, потому что их проверили недавно (срок ещё не вышел).</summary>
    public int FreshSkipped { get; init; }
    public bool Cancelled { get; init; }
    public bool StoppedByTime { get; init; }
    /// <summary>Отчёт предыдущей проверки: новая не запускалась (слишком частое «принудительно»).</summary>
    public bool Reused { get; init; }

    /// <summary>Всё ли удалось проверить: ни один сайт не отказал, ничего не отложено и не прервано.</summary>
    public bool Complete => !Cancelled && !StoppedByTime && Sources.All(s => s.Status == SourceStatus.Ok && s.Postponed == 0 && s.Note is null);
}

/// <summary>
/// Кнопка «проверить обновления»: ленты сайтов, затем карточки только тех модов, про которых лента не сказала «без изменений».
/// Сколько запросов тратить, решает <see cref="UpdateCheckPlanner"/>; паузы и повторы делает сетевой слой адаптеров.
/// Любой сбой сайта (отказ, проверка «вы человек?», исключение адаптера) касается только этого сайта.
/// Отмена и лимит времени не теряют результат: возвращается то, что успели.
/// Одновременные запуски и частые нажатия обрабатывает <see cref="UpdateCoordinator"/>.
/// </summary>
public static class UpdateRunner
{
    public static async Task<UpdateRunReport> RunAsync(
        ILibraryStore store,
        IEnumerable<IModSource> sources,
        DateTimeOffset now,
        UpdateCheckOptions? options = null,
        IProgress<UpdateProgress>? progress = null,
        CancellationToken ct = default,
        Func<DateTimeOffset>? clock = null,
        IReadOnlyDictionary<string, string>? holdOff = null)
    {
        options ??= new UpdateCheckOptions();
        holdOff ??= new Dictionary<string, string>();
        var started = clock?.Invoke() ?? now;
        bool TimeUp() => clock is not null && clock() - started > options.MaxDuration;

        var before = UpdateCheckService.Pending(store).Select(f => (f.ModId, f.LatestVersion)).ToHashSet();
        var byId = sources.GroupBy(s => s.Id).ToDictionary(g => g.Key, g => g.First());
        var plan = UpdateCheckService.PlanFeeds(store, now, options);
        var dueSet = plan.Due.Select(b => (b.ModId, b.Source)).ToHashSet();
        var tracked = UpdateCheckService.Tracked(store);
        var statuses = store.SourceStatuses();
        var freshSkipped = tracked.Count(b => (!statuses.TryGetValue(b.Source, out var st) || st == SourceStatus.Ok) && !dueSet.Contains((b.ModId, b.Source)));

        var reports = new Dictionary<string, SourceRunReport>();
        var failed = new Dictionary<string, (SourceStatus Status, string Note)>();
        var notes = new Dictionary<string, string>();
        var requests = 0;
        var upToDate = 0;
        var cancelled = false;
        var stoppedByTime = false;

        foreach (var sk in plan.SkippedSources)
            reports[sk.Source] = new SourceRunReport(sk.Source, sk.Status, 0, 0, 0, Array.Empty<string>(),
                sk.Status == SourceStatus.NeedsHumanCheck ? "сайт просит пройти проверку «вы человек?»" : "сайт недоступен");
        foreach (var (src, why) in holdOff)
            if (plan.Due.Any(b => b.Source == src) && !reports.ContainsKey(src))
                reports[src] = new SourceRunReport(src, SourceStatus.Unavailable, 0, 0, 0, Array.Empty<string>(), why);

        void Fail(string source, Exception e)
        {
            var st = e is SourceException se ? se.Status : SourceStatus.Unavailable;
            failed[source] = (st, e.Message);
            // Проверка «вы человек?» запоминается, пока пользователь её не пройдёт; недоступность — нет, она может пройти сама.
            if (st == SourceStatus.NeedsHumanCheck) store.SetSourceStatus(source, SourceStatus.NeedsHumanCheck);
        }

        var feeds = new Dictionary<string, IReadOnlyDictionary<string, FeedEntry>>();
        var feedPagesDone = new Dictionary<string, int>();
        var cardsDone = new Dictionary<string, int>();
        var missing = new Dictionary<string, List<string>>();
        IReadOnlyList<TrackedBinding> postponedAll = Array.Empty<TrackedBinding>();

        try
        {
            // 1. Ленты.
            foreach (var (source, pages) in plan.FeedPages)
            {
                if (reports.ContainsKey(source)) continue;
                if (!byId.TryGetValue(source, out var src))
                {
                    notes[source] = "нет адаптера для этого сайта";
                    failed[source] = (SourceStatus.Unavailable, notes[source]);
                    continue;
                }
                var entries = new Dictionary<string, FeedEntry>();
                for (var p = 1; p <= pages; p++)
                {
                    ct.ThrowIfCancellationRequested();
                    if (TimeUp()) { stoppedByTime = true; break; }
                    progress?.Report(new UpdateProgress(source, "лента", p - 1, pages));
                    try
                    {
                        var page = await src.FeedPageAsync(p, ct).ConfigureAwait(false);
                        requests++;
                        feedPagesDone[source] = p;
                        if (page.Count == 0)
                        {
                            if (p == 1) notes[source] = "лента пуста: возможно, изменилась вёрстка сайта";
                            break;
                        }
                        foreach (var l in page)
                            entries.TryAdd(l.SourceId, new FeedEntry(l.SourceId, l.ModVersion, l.SizeBytes, l.UpdatedAt));
                    }
                    catch (Exception e) when (e is not OperationCanceledException || !ct.IsCancellationRequested)
                    {
                        requests++;
                        Fail(source, e);
                        break;
                    }
                }
                feeds[source] = entries;
            }

            // 2. Знакомство: версия есть в ленте, прошлых данных нет, карточка не нужна.
            var cardCandidates = new List<TrackedBinding>();
            foreach (var b in plan.Due.Where(b => !failed.ContainsKey(b.Source) && !reports.ContainsKey(b.Source) && byId.ContainsKey(b.Source)))
            {
                if (b.LastVersion is null && feeds.TryGetValue(b.Source, out var f)
                    && f.TryGetValue(b.SourceId, out var entry) && entry.Version is not null)
                    UpdateCheckService.ApplyFeedBaseline(store, b, entry, now);
                else
                    cardCandidates.Add(b);
            }

            // 3. Карточки: только у сайтов, которые ответили.
            var cardPlan = UpdateCheckPlanner.PlanCards(cardCandidates, feeds, now, options);
            postponedAll = cardPlan.Postponed;
            foreach (var b in cardPlan.ConfirmedUnchanged)
            {
                UpdateCheckService.ApplyFeedUnchanged(store, b, now);
                upToDate++;
            }

            foreach (var (source, list) in cardPlan.ToFetch)
            {
                var src = byId[source];
                var done = 0;
                var attempted = 0;
                foreach (var b in list)
                {
                    ct.ThrowIfCancellationRequested();
                    if (TimeUp()) { stoppedByTime = true; break; }
                    // Мод или привязку могли удалить, пока шла проверка.
                    var bind = store.Bindings().FirstOrDefault(x => x.ModId == b.ModId && x.Source == b.Source);
                    if (bind is null) continue;
                    progress?.Report(new UpdateProgress(source, "карточки", done, list.Count));
                    try
                    {
                        var current = await src.GetListingAsync(bind.Url, ct).ConfigureAwait(false);
                        requests++;
                        done++;
                        attempted++;
                        if (current is null)
                        {
                            // Страницы нет (убрана автором или сменила адрес): прошлое состояние не трогаем, сообщаем.
                            (missing.TryGetValue(source, out var m) ? m : missing[source] = new()).Add(store.GetMod(b.ModId)?.Name ?? b.ModId);
                            continue;
                        }
                        UpdateCheckService.ApplyCard(store, b, current, now);
                    }
                    catch (Exception e) when (e is not OperationCanceledException || !ct.IsCancellationRequested)
                    {
                        requests++;
                        Fail(source, e);
                        break;
                    }
                }
                cardsDone[source] = done;
                // Не нашлись сразу много карточек подряд: скорее сломались адреса или вёрстка, чем всё удалили.
                if (attempted >= 3 && missing.TryGetValue(source, out var mm) && mm.Count == attempted)
                    notes[source] = "карточки не найдены ни у одного мода: возможно, изменились адреса или вёрстка сайта";
            }
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            cancelled = true;
        }

        // 4. Отчёт по сайтам.
        var postponedBy = postponedAll.GroupBy(b => b.Source).ToDictionary(g => g.Key, g => g.Count());
        foreach (var source in plan.Due.Select(b => b.Source).Distinct())
        {
            if (reports.ContainsKey(source)) continue;
            var hasFail = failed.TryGetValue(source, out var f);
            var note = hasFail ? f.Note : notes.GetValueOrDefault(source);
            if (note is null && (cancelled || stoppedByTime) && !feedPagesDone.ContainsKey(source))
                note = cancelled ? "проверка прервана" : "не хватило времени, продолжится при следующей проверке";
            reports[source] = new SourceRunReport(
                source,
                hasFail ? f.Status : SourceStatus.Ok,
                feedPagesDone.GetValueOrDefault(source),
                cardsDone.GetValueOrDefault(source),
                postponedBy.GetValueOrDefault(source),
                missing.TryGetValue(source, out var mm) ? mm : Array.Empty<string>(),
                note);
        }

        progress?.Report(new UpdateProgress("", "готово", 1, 1));
        var findings = UpdateCheckService.Pending(store);
        return new UpdateRunReport(findings, reports.Values.OrderBy(r => r.Source, StringComparer.Ordinal).ToList(), requests, upToDate)
        {
            NewFindings = findings.Where(f => !before.Contains((f.ModId, f.LatestVersion))).ToList(),
            FreshSkipped = freshSkipped,
            Cancelled = cancelled,
            StoppedByTime = stoppedByTime,
        };
    }
}
