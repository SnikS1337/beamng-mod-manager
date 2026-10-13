using BeamModManager.Core.Sources;
using BeamModManager.Core.Updates;

namespace BeamModManager.Core.Library;

public sealed class UpdateCoordinatorOptions
{
    public UpdateCheckOptions Check { get; init; } = new();
    /// <summary>После сбоя сайта (недоступен) не обращаться к нему столько времени, сколько бы раз ни нажимали кнопку.</summary>
    public TimeSpan FailureBackoff { get; init; } = TimeSpan.FromMinutes(5);
    /// <summary>«Проверить заново» чаще этого срока возвращает прошлый отчёт, а не идёт на сайты.</summary>
    public TimeSpan MinForceInterval { get; init; } = TimeSpan.FromMinutes(1);
    /// <summary>
    /// Одна кнопка проверяет всё: если за один заход (ограниченный по времени) не успели, координатор сам продолжает следующей частью.
    /// Это предел числа частей, чтобы не крутиться вечно (5 минут × 12 = час).
    /// </summary>
    public int MaxBatches { get; init; } = 12;
}

/// <summary>
/// Единственная точка, через которую интерфейс запускает проверку. Защищает от серии нажатий:
/// - нажали, пока проверка идёт: получаете ту же самую проверку, второй не запускается;
/// - нажали сразу после завершённой: сайты не трогаются (срок давности), отчёт говорит, сколько модов свежие;
/// - «принудительно» подряд: слишком частые возвращают прошлый отчёт;
/// - сайт упал: следующие нажатия его не долбят в течение FailureBackoff;
/// - сайт просит проверку «вы человек?»: пропускается, пока вызвано ResolveHumanCheck.
/// Остановить проверку можно через Cancel: результат того, что успели, сохраняется.
/// </summary>
public sealed class UpdateCoordinator
{
    private readonly ILibraryStore _store;
    private readonly Func<IReadOnlyList<IModSource>> _sources;
    private readonly Func<DateTimeOffset> _clock;
    private readonly UpdateCoordinatorOptions _o;
    private readonly Action? _clearCache;
    private readonly Action<string>? _resetSource;
    private readonly object _gate = new();
    private readonly Dictionary<string, DateTimeOffset> _retryAfter = new();
    private Task<UpdateRunReport>? _current;
    private CancellationTokenSource? _cts;
    private DateTimeOffset? _lastFinished;

    public UpdateCoordinator(ILibraryStore store, Func<IReadOnlyList<IModSource>> sources,
        Func<DateTimeOffset>? clock = null, UpdateCoordinatorOptions? options = null,
        Action? clearCache = null, Action<string>? resetSource = null)
    {
        _store = store;
        _sources = sources;
        _clock = clock ?? (() => DateTimeOffset.UtcNow);
        _o = options ?? new UpdateCoordinatorOptions();
        _clearCache = clearCache;
        _resetSource = resetSource;
    }

    public bool IsRunning { get { lock (_gate) return _current is not null; } }
    public UpdateRunReport? LastReport { get; private set; }

    public Task<UpdateRunReport> CheckAsync(bool force = false, IProgress<UpdateProgress>? progress = null)
    {
        lock (_gate)
        {
            if (_current is not null)
            {
                // Идущую проверку уже отменили: присоединяться к ней бессмысленно (получим «прервано»).
                // Дожидаемся её конца и запускаем свежую; несколько ожидающих сойдутся в одной новой проверке.
                if (_cts is { IsCancellationRequested: true })
                    return _current.ContinueWith(_ => CheckAsync(force, progress), TaskScheduler.Default).Unwrap();
                return _current; // уже идёт: присоединяемся
            }
            if (force && _lastFinished is { } done && _clock() - done < _o.MinForceInterval && LastReport is { } last && !last.Cancelled)
                return Task.FromResult(last with { Reused = true });
            _cts = new CancellationTokenSource();
            var token = _cts.Token;
            _current = Task.Run(() => RunAsync(force, progress, token));
            return _current;
        }
    }

    /// <summary>Останавливает идущую проверку (если она есть). Сделанное сохраняется.</summary>
    public void Cancel()
    {
        lock (_gate) _cts?.Cancel();
    }

    /// <summary>Пользователь прошёл проверку «вы человек?» на сайте: снимаем пометку, следующая проверка его опросит.</summary>
    public void ResolveHumanCheck(string source)
    {
        _store.SetSourceStatus(source, SourceStatus.Ok);
        lock (_gate) _retryAfter.Remove(source);
        _resetSource?.Invoke(source);
    }

    private async Task<UpdateRunReport> RunAsync(bool force, IProgress<UpdateProgress>? progress, CancellationToken ct)
    {
        try
        {
            if (force) _clearCache?.Invoke(); // иначе «заново» вернёт копию из кэша
            var before = UpdateCheckService.Pending(_store).Select(f => (f.ModId, f.LatestVersion)).ToHashSet();

            UpdateRunReport? first = null, last = null;
            var requests = 0;
            var upToDate = 0;

            // Одно нажатие — вся работа. Заход ограничен по времени, поэтому после долгого перерыва проверка идёт несколькими частями
            // подряд; человеку не нужно нажимать кнопку снова. Продолжаем, только пока это идёт на пользу: часть что-то проверила
            // (число моделей «к проверке» уменьшилось) и ещё есть отложенное.
            for (var batch = 0; ; batch++)
            {
                var now = _clock();
                var batchForce = force && batch == 0;
                var options = _o.Check.With(batchForce);
                var dueBefore = UpdateCheckService.PlanFeeds(_store, now, options).Due.Count;

                Dictionary<string, string> hold;
                lock (_gate)
                {
                    // Принудительная проверка отменяет паузу после сбоя: пользователь настаивает, а частоту ограничивает MinForceInterval.
                    if (batchForce) _retryAfter.Clear();
                    hold = _retryAfter.Where(kv => kv.Value > now)
                        .ToDictionary(kv => kv.Key, kv => $"сайт недавно не отвечал, повторим после {kv.Value.ToLocalTime():HH:mm}");
                }

                var report = await UpdateRunner.RunAsync(_store, _sources(), now, options, progress, ct, _clock, hold)
                    .ConfigureAwait(false);
                first ??= report;
                last = report;
                requests += report.Requests;
                upToDate += report.UpToDate;

                lock (_gate)
                {
                    foreach (var s in report.Sources)
                    {
                        if (s.Status == SourceStatus.Unavailable && !hold.ContainsKey(s.Source)) _retryAfter[s.Source] = now + _o.FailureBackoff;
                        else if (s.Status == SourceStatus.Ok && s.Note is null) _retryAfter.Remove(s.Source);
                    }
                }

                var unfinished = report.StoppedByTime || report.Sources.Any(s => s.Postponed > 0);
                if (report.Cancelled || !unfinished || batch + 1 >= _o.MaxBatches) break;
                var dueAfter = UpdateCheckService.PlanFeeds(_store, _clock(), _o.Check.With(false)).Due.Count;
                if (dueAfter >= dueBefore) break; // ничего не продвинулось (сайты отказывают): не крутимся впустую
            }

            var final = last!;
            var result = new UpdateRunReport(final.Findings, final.Sources, requests, upToDate)
            {
                NewFindings = final.Findings.Where(f => !before.Contains((f.ModId, f.LatestVersion))).ToList(),
                FreshSkipped = first!.FreshSkipped,
                Cancelled = final.Cancelled,
                StoppedByTime = final.StoppedByTime,
            };

            lock (_gate)
            {
                LastReport = result;
                if (!result.Cancelled) _lastFinished = _clock();
            }
            return result;
        }
        finally
        {
            lock (_gate)
            {
                _current = null;
                _cts?.Dispose();
                _cts = null;
            }
        }
    }
}
