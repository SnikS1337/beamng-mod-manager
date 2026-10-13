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
    /// <summary>Всё ли удалось проверить: ни один сайт не отказал и ничего не отложено.</summary>
    public bool Complete => Sources.All(s => s.Status == SourceStatus.Ok && s.Postponed == 0 && s.Note is null);
}

/// <summary>
/// Кнопка «проверить обновления»: ленты сайтов, затем карточки только тех модов, про которых лента не сказала «без изменений».
/// Сколько запросов тратить, решает <see cref="UpdateCheckPlanner"/>; паузы и повторы делает сетевой слой адаптеров.
/// Сбой одного сайта не ломает остальных, а результат прошлых проверок сохраняется.
/// </summary>
public static class UpdateRunner
{
    public static async Task<UpdateRunReport> RunAsync(
        ILibraryStore store,
        IEnumerable<IModSource> sources,
        DateTimeOffset now,
        UpdateCheckOptions? options = null,
        IProgress<UpdateProgress>? progress = null,
        CancellationToken ct = default)
    {
        options ??= new UpdateCheckOptions();
        var byId = sources.ToDictionary(s => s.Id);
        var plan = UpdateCheckService.PlanFeeds(store, now, options);
        var reports = new Dictionary<string, SourceRunReport>();
        var requests = 0;
        var upToDate = 0;

        foreach (var sk in plan.SkippedSources)
            reports[sk.Source] = new SourceRunReport(sk.Source, sk.Status, 0, 0, 0, Array.Empty<string>(),
                sk.Status == SourceStatus.NeedsHumanCheck ? "сайт просит пройти проверку «вы человек?»" : "сайт недоступен");

        // 1. Ленты.
        var feeds = new Dictionary<string, IReadOnlyDictionary<string, FeedEntry>>();
        var feedPagesDone = new Dictionary<string, int>();
        var failed = new Dictionary<string, (SourceStatus Status, string Note)>();

        foreach (var (source, pages) in plan.FeedPages)
        {
            if (!byId.TryGetValue(source, out var src))
            {
                failed[source] = (SourceStatus.Unavailable, "нет адаптера для этого сайта");
                continue;
            }
            var entries = new Dictionary<string, FeedEntry>();
            for (var p = 1; p <= pages; p++)
            {
                ct.ThrowIfCancellationRequested();
                progress?.Report(new UpdateProgress(source, "лента", p - 1, pages));
                try
                {
                    var page = await src.FeedPageAsync(p, ct).ConfigureAwait(false);
                    requests++;
                    feedPagesDone[source] = p;
                    if (page.Count == 0) break;
                    foreach (var l in page)
                        entries.TryAdd(l.SourceId, new FeedEntry(l.SourceId, l.ModVersion, l.SizeBytes, l.UpdatedAt));
                }
                catch (SourceException e)
                {
                    requests++;
                    failed[source] = (e.Status, e.Message);
                    if (e.Status == SourceStatus.NeedsHumanCheck) store.SetSourceStatus(source, SourceStatus.NeedsHumanCheck);
                    break;
                }
            }
            feeds[source] = entries;
        }

        // 2. Карточки: только у сайтов, которые ответили.
        var okDue = plan.Due.Where(b => !failed.ContainsKey(b.Source) && byId.ContainsKey(b.Source)).ToList();
        var cardPlan = UpdateCheckPlanner.PlanCards(okDue, feeds, now, options);
        foreach (var b in cardPlan.ConfirmedUnchanged)
        {
            UpdateCheckService.ApplyFeedUnchanged(store, b, now);
            upToDate++;
        }

        var cardsDone = new Dictionary<string, int>();
        var missing = new Dictionary<string, List<string>>();
        foreach (var (source, list) in cardPlan.ToFetch)
        {
            var src = byId[source];
            var done = 0;
            foreach (var b in list)
            {
                ct.ThrowIfCancellationRequested();
                progress?.Report(new UpdateProgress(source, "карточки", done, list.Count));
                try
                {
                    var current = await src.GetListingAsync(Binding(store, b)!.Url, ct).ConfigureAwait(false);
                    requests++;
                    done++;
                    if (current is null)
                    {
                        // Страницы нет (убрана автором или сменила адрес): прошлое состояние не трогаем, сообщаем.
                        (missing.TryGetValue(source, out var m) ? m : missing[source] = new()).Add(store.GetMod(b.ModId)?.Name ?? b.ModId);
                        continue;
                    }
                    UpdateCheckService.ApplyCard(store, b, current, now);
                }
                catch (SourceException e)
                {
                    requests++;
                    failed[source] = (e.Status, e.Message);
                    if (e.Status == SourceStatus.NeedsHumanCheck) store.SetSourceStatus(source, SourceStatus.NeedsHumanCheck);
                    break;
                }
            }
            cardsDone[source] = done;
        }

        // 3. Отчёт по сайтам.
        var postponedBy = cardPlan.Postponed.GroupBy(b => b.Source).ToDictionary(g => g.Key, g => g.Count());
        foreach (var source in plan.Due.Select(b => b.Source).Distinct())
        {
            if (reports.ContainsKey(source)) continue;
            failed.TryGetValue(source, out var f);
            var hasFail = failed.ContainsKey(source);
            reports[source] = new SourceRunReport(
                source,
                hasFail ? f.Status : SourceStatus.Ok,
                feedPagesDone.GetValueOrDefault(source),
                cardsDone.GetValueOrDefault(source),
                postponedBy.GetValueOrDefault(source),
                missing.TryGetValue(source, out var mm) ? mm : Array.Empty<string>(),
                hasFail ? f.Note : null);
        }

        progress?.Report(new UpdateProgress("", "готово", 1, 1));
        return new UpdateRunReport(
            UpdateCheckService.Pending(store),
            reports.Values.OrderBy(r => r.Source, StringComparer.Ordinal).ToList(),
            requests,
            upToDate);
    }

    private static ModBinding? Binding(ILibraryStore store, TrackedBinding b) =>
        store.Bindings().FirstOrDefault(x => x.ModId == b.ModId && x.Source == b.Source);
}
