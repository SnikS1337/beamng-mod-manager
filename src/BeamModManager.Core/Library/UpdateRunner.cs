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

        var feedPagesDone = new Dictionary<string, int>();
        var cardsDone = new Dictionary<string, int>();
        var missing = new Dictionary<string, List<string>>();
        var postponedAll = new List<TrackedBinding>();

        // Сайты независимы (у каждого своя очередь запросов и свои паузы), поэтому идут параллельно:
        // проверка занимает время самого медленного сайта, а не сумму. Общее состояние каждый сайт пишет
        // только в свой результат; хранилище защищено блокировкой, а привязки разных сайтов не пересекаются.
        var progressGate = new object();
        void Report(UpdateProgress p) { if (progress is null) return; lock (progressGate) progress.Report(p); }

        var jobs = plan.FeedPages
            .Where(kv => !reports.ContainsKey(kv.Key))
            .Select(kv => RunSourceAsync(kv.Key, kv.Value))
            .ToList();
        var outcomes = await Task.WhenAll(jobs).ConfigureAwait(false);

        foreach (var o in outcomes)
        {
            requests += o.Requests;
            upToDate += o.UpToDate;
            cancelled |= o.Cancelled;
            stoppedByTime |= o.StoppedByTime;
            if (o.FeedPages > 0) feedPagesDone[o.Source] = o.FeedPages;
            if (o.CardsDone is { } cd) cardsDone[o.Source] = cd;
            if (o.Missing.Count > 0) missing[o.Source] = o.Missing;
            if (o.Note is not null) notes[o.Source] = o.Note;
            if (o.Failure is { } fl) failed[o.Source] = fl;
            postponedAll.AddRange(o.Postponed);
        }

        async Task<SourceOutcome> RunSourceAsync(string source, int pages)
        {
            var o = new SourceOutcome(source);
            try
            {
                if (!byId.TryGetValue(source, out var src))
                {
                    o.Note = "нет адаптера для этого сайта";
                    o.Failure = (SourceStatus.Unavailable, o.Note);
                    return o;
                }

                // 1. Лента.
                var entries = new Dictionary<string, FeedEntry>();
                for (var p = 1; p <= pages; p++)
                {
                    ct.ThrowIfCancellationRequested();
                    if (TimeUp()) { o.StoppedByTime = true; break; }
                    Report(new UpdateProgress(source, "лента", p - 1, pages));
                    try
                    {
                        var page = await src.FeedPageAsync(p, ct).ConfigureAwait(false);
                        o.Requests++;
                        o.FeedPages = p;
                        if (page.Count == 0)
                        {
                            if (p == 1) o.Note = "лента пуста: возможно, изменилась вёрстка сайта";
                            break;
                        }
                        foreach (var l in page)
                            entries.TryAdd(l.SourceId, new FeedEntry(l.SourceId, l.ModVersion, l.SizeBytes, l.UpdatedAt));
                    }
                    catch (Exception e) when (e is not OperationCanceledException || !ct.IsCancellationRequested)
                    {
                        o.Requests++;
                        o.Failure = FailureOf(source, e);
                        return o; // у сайта, отказавшего на ленте, карточки не открываем
                    }
                }

                // 2. Знакомство: версия есть в ленте, прошлых данных нет, карточка не нужна.
                var cardCandidates = new List<TrackedBinding>();
                foreach (var b in plan.Due.Where(b => b.Source == source))
                {
                    if (b.LastVersion is null && entries.TryGetValue(b.SourceId, out var entry) && entry.Version is not null)
                        UpdateCheckService.ApplyFeedBaseline(store, b, entry, now);
                    else
                        cardCandidates.Add(b);
                }

                // 3. Карточки.
                var feeds = new Dictionary<string, IReadOnlyDictionary<string, FeedEntry>> { [source] = entries };
                var cardPlan = UpdateCheckPlanner.PlanCards(cardCandidates, feeds, now, options);
                o.Postponed.AddRange(cardPlan.Postponed);
                foreach (var b in cardPlan.ConfirmedUnchanged)
                {
                    UpdateCheckService.ApplyFeedUnchanged(store, b, now);
                    o.UpToDate++;
                }

                if (!cardPlan.ToFetch.TryGetValue(source, out var list)) return o;
                var done = 0;
                var attempted = 0;
                o.CardsDone = 0;
                foreach (var b in list)
                {
                    ct.ThrowIfCancellationRequested();
                    if (TimeUp()) { o.StoppedByTime = true; break; }
                    // Мод или привязку могли удалить, пока шла проверка.
                    var bind = store.Bindings().FirstOrDefault(x => x.ModId == b.ModId && x.Source == b.Source);
                    if (bind is null) continue;
                    Report(new UpdateProgress(source, "карточки", done, list.Count));
                    try
                    {
                        var current = await src.GetListingAsync(bind.Url, ct).ConfigureAwait(false);
                        o.Requests++;
                        done++;
                        attempted++;
                        o.CardsDone = done;
                        if (current is null)
                        {
                            // Страницы нет (убрана автором или сменила адрес): прошлое состояние не трогаем, сообщаем.
                            o.Missing.Add(store.GetMod(b.ModId)?.Name ?? b.ModId);
                            continue;
                        }
                        UpdateCheckService.ApplyCard(store, b, current, now);
                    }
                    catch (Exception e) when (e is not OperationCanceledException || !ct.IsCancellationRequested)
                    {
                        o.Requests++;
                        o.Failure = FailureOf(source, e);
                        break;
                    }
                }
                // Не нашлись сразу много карточек подряд: скорее сломались адреса или вёрстка, чем всё удалили.
                if (attempted >= 3 && o.Missing.Count == attempted)
                    o.Note = "карточки не найдены ни у одного мода: возможно, изменились адреса или вёрстка сайта";
            }
            catch (OperationCanceledException) when (ct.IsCancellationRequested)
            {
                o.Cancelled = true;
            }
            return o;
        }

        (SourceStatus Status, string Note) FailureOf(string source, Exception e)
        {
            var st = e is SourceException se ? se.Status : SourceStatus.Unavailable;
            // Проверка «вы человек?» запоминается, пока пользователь её не пройдёт; недоступность — нет, она может пройти сама.
            if (st == SourceStatus.NeedsHumanCheck) store.SetSourceStatus(source, SourceStatus.NeedsHumanCheck);
            return (st, e.Message);
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

/// <summary>Итог работы с одним сайтом внутри проверки (собирается отдельно, чтобы сайты могли идти параллельно).</summary>
internal sealed class SourceOutcome
{
    public SourceOutcome(string source) { Source = source; }
    public string Source { get; }
    public int Requests, UpToDate, FeedPages;
    public int? CardsDone;
    public bool Cancelled, StoppedByTime;
    public string? Note;
    public (SourceStatus Status, string Note)? Failure;
    public List<string> Missing { get; } = new();
    public List<TrackedBinding> Postponed { get; } = new();
}
