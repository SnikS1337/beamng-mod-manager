using System.Net;
using System.Net.Http.Headers;
using BeamModManager.Core.Sources;

namespace BeamModManager.Core.Net;

/// <summary>Сырой ответ сайта. Сетевые сбои выбрасываются как HttpRequestException.</summary>
public sealed record RawResponse(int Status, string Body, TimeSpan? RetryAfter = null);

/// <summary>
/// Низкоуровневая загрузка страницы. На Windows для Modland будет реализация поверх WebView2,
/// для остальных сайтов подойдёт <see cref="HttpPageFetcher"/>.
/// </summary>
public interface IPageFetcher
{
    Task<RawResponse> GetAsync(Uri url, CancellationToken ct);
}

public sealed class HttpPageFetcher : IPageFetcher
{
    private readonly HttpClient _http;

    public HttpPageFetcher(HttpClient? http = null)
    {
        _http = http ?? new HttpClient(new HttpClientHandler { AutomaticDecompression = DecompressionMethods.All }) { Timeout = TimeSpan.FromSeconds(30) };
        if (_http.DefaultRequestHeaders.UserAgent.Count == 0)
            _http.DefaultRequestHeaders.UserAgent.ParseAdd("BeamModManager/0.1 (personal use; update checks)");
    }

    public async Task<RawResponse> GetAsync(Uri url, CancellationToken ct)
    {
        using var resp = await _http.GetAsync(url, ct).ConfigureAwait(false);
        var body = await resp.Content.ReadAsStringAsync(ct).ConfigureAwait(false);
        return new RawResponse((int)resp.StatusCode, body, ParseRetryAfter(resp.Headers.RetryAfter));
    }

    private static TimeSpan? ParseRetryAfter(RetryConditionHeaderValue? h)
    {
        if (h?.Delta is { } d) return d;
        if (h?.Date is { } date) { var w = date - DateTimeOffset.UtcNow; return w > TimeSpan.Zero ? w : TimeSpan.Zero; }
        return null;
    }
}

public sealed record FetchResult(int? Status, string? Body, SourceStatus Source, bool FromCache = false, string? Note = null)
{
    public bool IsSuccess => Source == SourceStatus.Ok && Status is >= 200 and < 300 && Body is not null;
    public bool IsNotFound => Status is 404 or 410;
}

public sealed class PoliteFetchOptions
{
    public TimeSpan MinDelay { get; init; } = TimeSpan.FromSeconds(1.5);
    public TimeSpan Jitter { get; init; } = TimeSpan.FromSeconds(1);
    public int MaxRetries { get; init; } = 2;
    public TimeSpan DefaultBackoff { get; init; } = TimeSpan.FromSeconds(5);
    public TimeSpan MaxBackoff { get; init; } = TimeSpan.FromSeconds(60);
    public TimeSpan CacheTtl { get; init; } = TimeSpan.FromMinutes(30);
    /// <summary>Подряд идущих сбоев, после которых сайт на время отключается.</summary>
    public int FailuresBeforeCooldown { get; init; } = 3;
    public TimeSpan Cooldown { get; init; } = TimeSpan.FromMinutes(10);
}

/// <summary>
/// Вежливая обёртка: один запрос за раз на сайт, пауза с разбросом, повтор после 429/503 с уважением Retry-After,
/// кэш, остановка сайта при странице проверки и после серии сбоев. Проверку «вы человек?» не обходит:
/// сообщает статус и ждёт <see cref="ResetHost"/> после того, как пользователь прошёл её сам.
/// </summary>
public sealed class PoliteFetcher
{
    private sealed class HostState
    {
        public readonly SemaphoreSlim Gate = new(1, 1);
        public DateTimeOffset? LastRequest;
        public bool NeedsHumanCheck;
        public int Failures;
        public DateTimeOffset? CooldownUntil;
    }

    private readonly IPageFetcher _inner;
    private readonly PoliteFetchOptions _o;
    private readonly Func<DateTimeOffset> _now;
    private readonly Func<TimeSpan, CancellationToken, Task> _delay;
    private readonly Random _rng;
    private readonly object _lock = new();
    private readonly Dictionary<string, HostState> _hosts = new();
    private readonly Dictionary<Uri, (DateTimeOffset At, FetchResult Result)> _cache = new();

    public PoliteFetcher(IPageFetcher inner, PoliteFetchOptions? options = null,
        Func<DateTimeOffset>? now = null, Func<TimeSpan, CancellationToken, Task>? delay = null, Random? rng = null)
    {
        _inner = inner;
        _o = options ?? new PoliteFetchOptions();
        _now = now ?? (() => DateTimeOffset.UtcNow);
        _delay = delay ?? ((t, ct) => Task.Delay(t, ct));
        _rng = rng ?? new Random();
    }

    private HostState State(string host)
    {
        lock (_lock)
        {
            if (!_hosts.TryGetValue(host, out var s)) _hosts[host] = s = new HostState();
            return s;
        }
    }

    /// <summary>Сбрасывает кэш страниц (для «проверить заново», чтобы не получить копию получасовой давности).</summary>
    public void ClearCache() { lock (_lock) _cache.Clear(); }

    /// <summary>Вызывать после того, как пользователь прошёл проверку в окне браузера.</summary>
    public void ResetHost(string host)
    {
        var s = State(host.ToLowerInvariant());
        s.NeedsHumanCheck = false;
        s.Failures = 0;
        s.CooldownUntil = null;
    }

    public SourceStatus StatusOf(string host)
    {
        var s = State(host.ToLowerInvariant());
        if (s.NeedsHumanCheck) return SourceStatus.NeedsHumanCheck;
        if (s.CooldownUntil is { } u && _now() < u) return SourceStatus.Unavailable;
        return SourceStatus.Ok;
    }

    public async Task<FetchResult> GetAsync(Uri url, CancellationToken ct = default)
    {
        lock (_lock)
        {
            if (_cache.TryGetValue(url, out var c) && _now() - c.At < _o.CacheTtl)
                return c.Result with { FromCache = true };
        }

        var host = url.Host.ToLowerInvariant();
        var st = State(host);
        await st.Gate.WaitAsync(ct).ConfigureAwait(false);
        try
        {
            if (st.NeedsHumanCheck)
                return new FetchResult(null, null, SourceStatus.NeedsHumanCheck, Note: "сайт ждёт, пока пользователь пройдёт проверку");
            if (st.CooldownUntil is { } until && _now() < until)
                return new FetchResult(null, null, SourceStatus.Unavailable, Note: "сайт временно отключён после серии сбоев");

            for (var attempt = 0; ; attempt++)
            {
                await WaitTurnAsync(st, ct).ConfigureAwait(false);

                RawResponse raw;
                try
                {
                    raw = await _inner.GetAsync(url, ct).ConfigureAwait(false);
                }
                catch (HttpRequestException ex)
                {
                    st.LastRequest = _now();
                    if (attempt < _o.MaxRetries) { await _delay(Backoff(attempt, null), ct).ConfigureAwait(false); continue; }
                    return Failure(st, null, ex.Message);
                }
                st.LastRequest = _now();

                if (ChallengePageDetector.IsChallenge(raw.Body))
                {
                    st.NeedsHumanCheck = true;
                    return new FetchResult(raw.Status, null, SourceStatus.NeedsHumanCheck, Note: "страница проверки");
                }

                if (raw.Status is 429 or 503)
                {
                    if (attempt < _o.MaxRetries) { await _delay(Backoff(attempt, raw.RetryAfter), ct).ConfigureAwait(false); continue; }
                    return Failure(st, raw.Status, "слишком много запросов или сайт перегружен");
                }

                if (raw.Status is 404 or 410)
                {
                    st.Failures = 0;
                    return new FetchResult(raw.Status, null, SourceStatus.Ok, Note: "страницы нет");
                }

                if (raw.Status >= 400) return Failure(st, raw.Status, $"код {raw.Status}");

                st.Failures = 0;
                var ok = new FetchResult(raw.Status, raw.Body, SourceStatus.Ok);
                lock (_lock) _cache[url] = (_now(), ok);
                return ok;
            }
        }
        finally
        {
            st.Gate.Release();
        }
    }

    private FetchResult Failure(HostState st, int? status, string note)
    {
        st.Failures++;
        if (st.Failures >= _o.FailuresBeforeCooldown) st.CooldownUntil = _now() + _o.Cooldown;
        return new FetchResult(status, null, SourceStatus.Unavailable, Note: note);
    }

    private TimeSpan Backoff(int attempt, TimeSpan? retryAfter)
    {
        var wait = retryAfter ?? TimeSpan.FromTicks(_o.DefaultBackoff.Ticks * (1L << attempt));
        return wait > _o.MaxBackoff ? _o.MaxBackoff : wait;
    }

    private async Task WaitTurnAsync(HostState st, CancellationToken ct)
    {
        if (st.LastRequest is not { } last) return;
        var gap = _o.MinDelay + TimeSpan.FromTicks((long)(_o.Jitter.Ticks * _rng.NextDouble()));
        var wait = gap - (_now() - last);
        if (wait > TimeSpan.Zero) await _delay(wait, ct).ConfigureAwait(false);
    }
}
