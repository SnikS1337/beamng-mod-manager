using System.Net;
using System.Net.Http.Headers;
using System.Text;
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

    /// <summary>
    /// Отправка формы (application/x-www-form-urlencoded). Нужна поиску STMods: сортировку и число результатов он берёт
    /// только из формы, а не из адреса.
    /// </summary>
    Task<RawResponse> PostFormAsync(Uri url, string form, CancellationToken ct) => throw new NotSupportedException("форма не поддерживается");
}

public sealed class HttpPageFetcher : IPageFetcher
{
    /// <summary>Как программа представляется сайтам: страницам и картинкам поиска одинаково.</summary>
    public const string UserAgent = "BeamModManager/0.1 (personal use; update checks)";

    private readonly HttpClient _http;

    static HttpPageFetcher()
    {
        // Часть сайтов отдаёт страницы в windows-1251; без этого провайдера такая страница читалась бы как UTF-8 (кракозябры).
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
    }

    public HttpPageFetcher(HttpClient? http = null)
    {
        _http = http ?? new HttpClient(new SocketsHttpHandler
        {
            AutomaticDecompression = DecompressionMethods.All,
            // Приложение живёт долго: без этого соединение держалось бы вечно и не заметило бы смену адреса сайта (DNS).
            PooledConnectionLifetime = TimeSpan.FromMinutes(5),
            ConnectTimeout = TimeSpan.FromSeconds(15),
        }) { Timeout = TimeSpan.FromSeconds(30), MaxResponseContentBufferSize = 16 * 1024 * 1024 };
        if (_http.DefaultRequestHeaders.UserAgent.Count == 0)
            _http.DefaultRequestHeaders.UserAgent.ParseAdd(UserAgent);
    }

    public async Task<RawResponse> GetAsync(Uri url, CancellationToken ct)
    {
        using var resp = await _http.GetAsync(url, ct).ConfigureAwait(false);
        var body = await resp.Content.ReadAsStringAsync(ct).ConfigureAwait(false);
        return new RawResponse((int)resp.StatusCode, body, ParseRetryAfter(resp.Headers.RetryAfter));
    }

    public async Task<RawResponse> PostFormAsync(Uri url, string form, CancellationToken ct)
    {
        using var content = new StringContent(form, Encoding.ASCII, "application/x-www-form-urlencoded");
        using var resp = await _http.PostAsync(url, content, ct).ConfigureAwait(false);
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
    public int MaxCacheEntries { get; init; } = 64;
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
    /// <summary>Ключ — адрес, а для формы ещё и её содержимое (тот же адрес с другой формой — другая страница).</summary>
    private readonly Dictionary<string, (DateTimeOffset At, FetchResult Result)> _cache = new();

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
        if (InCooldown(s)) return SourceStatus.Unavailable;
        return SourceStatus.Ok;
    }

    public Task<FetchResult> GetAsync(Uri url, CancellationToken ct = default) => SendAsync(url, null, ct);

    /// <summary>Отправить форму с теми же правилами, что и у GET: очередь на сайт, пауза, повторы, кэш, проверка «вы человек?».</summary>
    public Task<FetchResult> PostFormAsync(Uri url, string form, CancellationToken ct = default) => SendAsync(url, form, ct);

    private bool FromCache(string key, out FetchResult result)
    {
        lock (_lock)
        {
            // Отрицательный возраст — часы перевели назад: сколько страница лежит, неизвестно, считаем устаревшей.
            if (_cache.TryGetValue(key, out var c) && _now() - c.At is { } age && age >= TimeSpan.Zero && age < _o.CacheTtl)
            {
                result = c.Result with { FromCache = true };
                return true;
            }
        }
        result = null!;
        return false;
    }

    private async Task<FetchResult> SendAsync(Uri url, string? form, CancellationToken ct)
    {
        var key = form is null ? url.AbsoluteUri : url.AbsoluteUri + "\n" + form;
        if (FromCache(key, out var cached)) return cached;

        var host = url.Host.ToLowerInvariant();
        var st = State(host);
        await st.Gate.WaitAsync(ct).ConfigureAwait(false);
        try
        {
            // пока ждали очереди, ту же страницу мог скачать соседний запрос (2026-10-18: раньше качали второй раз)
            if (FromCache(key, out cached)) return cached;
            if (st.NeedsHumanCheck)
                return new FetchResult(null, null, SourceStatus.NeedsHumanCheck, Note: "сайт ждёт, пока пользователь пройдёт проверку");
            if (InCooldown(st))
                return new FetchResult(null, null, SourceStatus.Unavailable, Note: "сайт временно отключён после серии сбоев");

            for (var attempt = 0; ; attempt++)
            {
                await WaitTurnAsync(st, ct).ConfigureAwait(false);

                RawResponse raw;
                try
                {
                    raw = form is null ? await _inner.GetAsync(url, ct).ConfigureAwait(false)
                        : await _inner.PostFormAsync(url, form, ct).ConfigureAwait(false);
                }
                catch (Exception ex) when (ex is HttpRequestException or IOException
                                           || (ex is OperationCanceledException && !ct.IsCancellationRequested)) // таймаут HttpClient, а не отмена пользователя
                {
                    st.LastRequest = _now();
                    if (attempt < _o.MaxRetries) { await _delay(Backoff(attempt, null), ct).ConfigureAwait(false); continue; }
                    return Failure(st, null, NetErrors.Describe(ex));
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

                // сбой по дороге к сайту (шлюз, Cloudflare 520–524) обычно мимолётный: один повтор после паузы (2026-10-18).
                // 500 не повторяем: это чаще постоянная ошибка самого сайта
                if (IsGatewayError(raw.Status))
                {
                    if (attempt < Math.Min(1, _o.MaxRetries)) { await _delay(Backoff(attempt, raw.RetryAfter), ct).ConfigureAwait(false); continue; }
                    return Failure(st, raw.Status, $"сайт временно не отвечает (код {raw.Status})");
                }

                if (raw.Status is 404 or 410)
                {
                    st.Failures = 0;
                    return new FetchResult(raw.Status, null, SourceStatus.Ok, Note: "страницы нет");
                }

                if (raw.Status >= 400) return Failure(st, raw.Status, $"код {raw.Status}");

                st.Failures = 0;
                var ok = new FetchResult(raw.Status, raw.Body, SourceStatus.Ok);
                lock (_lock) Remember(key, ok);
                return ok;
            }
        }
        finally
        {
            st.Gate.Release();
        }
    }

    /// <summary>Кэш ограничен: просроченное убирается, при переполнении уходят самые старые (страницы по сотни килобайт).</summary>
    private void Remember(string key, FetchResult result)
    {
        var now = _now();
        _cache[key] = (now, result);
        if (_cache.Count <= _o.MaxCacheEntries) return;
        foreach (var k in _cache.Where(kv => now - kv.Value.At >= _o.CacheTtl).Select(kv => kv.Key).ToList()) _cache.Remove(k);
        while (_cache.Count > _o.MaxCacheEntries)
        {
            var oldest = _cache.MinBy(kv => kv.Value.At).Key;
            _cache.Remove(oldest);
        }
    }

    /// <summary>Пауза после сбоев действует, пока не вышел срок. Срок дальше, чем длится сама пауза, значит часы перевели назад: пауза закончилась.</summary>
    private bool InCooldown(HostState st)
    {
        if (st.CooldownUntil is not { } until) return false;
        var left = until - _now();
        return left > TimeSpan.Zero && left <= _o.Cooldown;
    }

    private FetchResult Failure(HostState st, int? status, string note)
    {
        st.Failures++;
        if (st.Failures >= _o.FailuresBeforeCooldown) st.CooldownUntil = _now() + _o.Cooldown;
        return new FetchResult(status, null, SourceStatus.Unavailable, Note: note);
    }

    /// <summary>502, 504 и коды Cloudflare 520–524: сайт за посредником не ответил.</summary>
    public static bool IsGatewayError(int status) => status is 502 or 504 or (>= 520 and <= 524);

    private TimeSpan Backoff(int attempt, TimeSpan? retryAfter)
    {
        var wait = retryAfter ?? TimeSpan.FromTicks(_o.DefaultBackoff.Ticks * (1L << attempt));
        return wait > _o.MaxBackoff ? _o.MaxBackoff : wait;
    }

    private async Task WaitTurnAsync(HostState st, CancellationToken ct)
    {
        if (st.LastRequest is not { } last) return;
        // Random не для нескольких потоков: очереди разных сайтов идут параллельно, и без замка он мог сломаться
        // (отдаёт одни нули — паузы без разброса), 2026-10-18
        double r;
        lock (_rng) r = _rng.NextDouble();
        var gap = _o.MinDelay + TimeSpan.FromTicks((long)(_o.Jitter.Ticks * r));
        var elapsed = _now() - last;
        if (elapsed < TimeSpan.Zero) elapsed = TimeSpan.Zero; // часы перевели назад: ждём обычную паузу, а не часы
        var wait = gap - elapsed;
        if (wait > TimeSpan.Zero) await _delay(wait, ct).ConfigureAwait(false);
    }
}
