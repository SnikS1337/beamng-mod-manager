using BeamModManager.Core.Sources;

namespace BeamModManager.Core.Updates;

/// <summary>Привязка установленного мода к записи на сайте плюс то, что мы видели при прошлой проверке.</summary>
public sealed record TrackedBinding(
    string ModId,
    string Source,
    string SourceId,
    DateTimeOffset? LastCheckedUtc = null,
    DateTimeOffset? LastChangedUtc = null,
    string? LastVersion = null,
    long? LastSize = null,
    int MissCount = 0);

/// <summary>Строка из ленты «новое / обновлённое» сайта (дешёвый источник сведений).</summary>
public sealed record FeedEntry(string SourceId, string? Version, long? SizeBytes, DateTimeOffset? Updated);

public sealed class UpdateCheckOptions
{
    /// <summary>Пока результат свежее, повторно не проверяем.</summary>
    public TimeSpan Ttl { get; init; } = TimeSpan.FromHours(6);
    public int FeedPagesBase { get; init; } = 3;
    public int BindingsPerExtraFeedPage { get; init; } = 50;
    public int MaxFeedPagesPerSource { get; init; } = 10;
    /// <summary>
    /// Сколько карточек открывать на сайте за одну проверку. Сдерживают проверку не столько этот предел,
    /// сколько паузы между запросами и <see cref="MaxDuration"/>: после месяца без заходов нужно успеть проверить все моды за один раз.
    /// </summary>
    public int MaxCardChecksPerSource { get; init; } = 150;
    /// <summary>Игнорировать TTL (кнопка «проверить принудительно»).</summary>
    public bool Force { get; init; }
    /// <summary>Общий предел времени одной проверки; остаток переносится на следующий запуск.</summary>
    public TimeSpan MaxDuration { get; init; } = TimeSpan.FromMinutes(5);

    public UpdateCheckOptions With(bool force) => new()
    {
        Ttl = Ttl, FeedPagesBase = FeedPagesBase, BindingsPerExtraFeedPage = BindingsPerExtraFeedPage,
        MaxFeedPagesPerSource = MaxFeedPagesPerSource, MaxCardChecksPerSource = MaxCardChecksPerSource,
        MaxDuration = MaxDuration, Force = force,
    };
}

public sealed record SkippedSource(string Source, SourceStatus Status);

public sealed record FeedPlan(
    IReadOnlyDictionary<string, int> FeedPages,
    IReadOnlyList<TrackedBinding> Due,
    IReadOnlyList<SkippedSource> SkippedSources,
    int TotalRequests);

public sealed record CardPlan(
    IReadOnlyDictionary<string, IReadOnlyList<TrackedBinding>> ToFetch,
    IReadOnlyList<TrackedBinding> ConfirmedUnchanged,
    IReadOnlyList<TrackedBinding> Postponed)
{
    public int TotalRequests => ToFetch.Sum(kv => kv.Value.Count);
}

/// <summary>
/// Решает, сколько запросов потратить на проверку обновлений. Сети не касается.
/// Сначала читаем ленты сайтов (несколько страниц на сайт), потом открываем карточки
/// только тех модов, про которых лента сказала «изменилось» или ничего не сказала, а проверка пора.
/// </summary>
public static class UpdateCheckPlanner
{
    public static FeedPlan PlanFeeds(
        IEnumerable<TrackedBinding> bindings,
        IReadOnlyDictionary<string, SourceStatus> statuses,
        DateTimeOffset now,
        UpdateCheckOptions? options = null)
    {
        options ??= new UpdateCheckOptions();
        var due = new List<TrackedBinding>();
        var skipped = new Dictionary<string, SourceStatus>();

        foreach (var b in bindings)
        {
            var status = statuses.TryGetValue(b.Source, out var s) ? s : SourceStatus.Ok;
            if (status != SourceStatus.Ok) { skipped[b.Source] = status; continue; }
            // Время проверки «из будущего» (часы переводили назад) за свежее не считаем.
            if (!options.Force && b.LastCheckedUtc is { } t && t <= now && now - t < options.Ttl) continue;
            due.Add(b);
        }

        var pages = due.GroupBy(b => b.Source).ToDictionary(
            g => g.Key,
            g => Math.Min(options.MaxFeedPagesPerSource,
                options.FeedPagesBase + g.Count() / Math.Max(1, options.BindingsPerExtraFeedPage)));

        return new FeedPlan(
            pages,
            due,
            skipped.Select(kv => new SkippedSource(kv.Key, kv.Value)).ToList(),
            pages.Values.Sum());
    }

    /// <summary>Как часто нужно открывать карточку мода, про который лента молчит.</summary>
    /// <summary>Столько проверок подряд страница мода не находится, прежде чем привязку считают потерянной.</summary>
    public const int DeadAfterMisses = 2;

    private static readonly TimeSpan FirstMissRecheck = TimeSpan.FromDays(1);
    private static readonly TimeSpan DeadRecheck = TimeSpan.FromDays(7);

    public static TimeSpan FallbackInterval(TrackedBinding b, DateTimeOffset now)
    {
        if (b.LastChangedUtc is not { } changed) return TimeSpan.FromDays(1);
        var age = now - changed;
        if (age < TimeSpan.FromDays(30)) return TimeSpan.FromDays(1);
        if (age < TimeSpan.FromDays(180)) return TimeSpan.FromDays(3);
        return TimeSpan.FromDays(14);
    }

    public static CardPlan PlanCards(
        IReadOnlyList<TrackedBinding> due,
        IReadOnlyDictionary<string, IReadOnlyDictionary<string, FeedEntry>> feedBySource,
        DateTimeOffset now,
        UpdateCheckOptions? options = null,
        IReadOnlyDictionary<string, DateTimeOffset>? coverage = null,
        Func<TrackedBinding, bool>? inFeed = null)
    {
        options ??= new UpdateCheckOptions();
        var unchanged = new List<TrackedBinding>();
        var postponed = new List<TrackedBinding>();
        var toFetch = new Dictionary<string, IReadOnlyList<TrackedBinding>>();

        foreach (var group in due.GroupBy(b => b.Source))
        {
            feedBySource.TryGetValue(group.Key, out var feed);
            var changed = new List<TrackedBinding>();
            var fallback = new List<(TrackedBinding B, TimeSpan Overdue)>();

            foreach (var b in group)
            {
                if (feed is not null && feed.TryGetValue(b.SourceId, out var entry))
                {
                    // Сравниваем с тем, что видели раньше. Без прошлых данных уверенности нет, открываем карточку.
                    var same = b.LastVersion is not null && entry.Version == b.LastVersion
                               && !Versioning.SizeComparer.Differs(entry.SizeBytes, b.LastSize);
                    if (same) unchanged.Add(b); else changed.Add(b);
                    continue;
                }

                // Лента датирована и дошла глубже последней проверки мода: его в ленте нет, значит, он не менялся.
                // Принудительная проверка этого доказательства не использует — пользователь просит перепроверить.
                // Только если мод вообще мог попасть в эту ленту (у ModsGaming лента — один раздел, inFeed).
                if (!options.Force && coverage is not null && coverage.TryGetValue(group.Key, out var cov)
                    && b.LastVersion is not null && b.MissCount == 0 && FeedCoverage.Proves(cov, b.LastCheckedUtc, now)
                    && (inFeed?.Invoke(b) ?? true))
                {
                    unchanged.Add(b);
                    continue;
                }

                var since = b.LastCheckedUtc is { } t && t <= now ? now - t : TimeSpan.MaxValue;
                var interval = FallbackInterval(b, now);
                // Первый промах быстро перепроверяем (сбой сайта или правда удалили?), а подтверждённо мёртвую страницу — раз в неделю.
                if (b.MissCount == 1 && interval > FirstMissRecheck) interval = FirstMissRecheck;
                if (b.MissCount >= DeadAfterMisses && interval < DeadRecheck) interval = DeadRecheck;
                if (options.Force || since >= interval)
                    fallback.Add((b, since == TimeSpan.MaxValue ? TimeSpan.MaxValue : since - interval));
                else
                    postponed.Add(b);
            }

            var ordered = changed
                .Concat(fallback.OrderByDescending(x => x.Overdue).Select(x => x.B))
                .ToList();
            var take = ordered.Take(options.MaxCardChecksPerSource).ToList();
            postponed.AddRange(ordered.Skip(options.MaxCardChecksPerSource));
            if (take.Count > 0) toFetch[group.Key] = take;
        }

        return new CardPlan(toFetch, unchanged, postponed);
    }
}
