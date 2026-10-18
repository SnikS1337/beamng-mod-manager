using System.IO;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text;

namespace BeamModManager.App.Services;

/// <summary>
/// Картинки карточек поиска: с сайта (только https), уменьшаются до размера карточки (<see cref="Shrinker"/>) и лежат в
/// папке программы, чтобы повторный поиск не качал их снова. Файл больше 6 МБ не берём. Не получилось (нет сети, формат
/// не понят Windows) — null, карточка остаётся с заглушкой.
/// 2026-10-18: у STMods картинок почти не было — по четыре разом и без повторов сайт отвечал «слишком много запросов».
/// Теперь с одного сайта не больше двух разом и с паузой, после «слишком много» и временного сбоя — повтор, с заголовком
/// Referer (откуда пришли) — часть сайтов отдаёт картинки только своим страницам. Картинки Modland — встроенным браузером:
/// они за той же защитой Cloudflare, что и страницы.
/// </summary>
public sealed class RemotePreviews
{
    private const long MaxBytes = 6L * 1024 * 1024;
    private const int Retries = 2;
    private static readonly TimeSpan Gap = TimeSpan.FromMilliseconds(250);
    // представляемся так же, как при загрузке страниц
    private static readonly HttpClient Http = CreateClient();

    private static HttpClient CreateClient()
    {
        var h = new HttpClient { Timeout = TimeSpan.FromSeconds(15) };
        h.DefaultRequestHeaders.UserAgent.ParseAdd(BeamModManager.Core.Net.HttpPageFetcher.UserAgent);
        return h;
    }

    private readonly string _dir;
    private readonly SemaphoreSlim _gate = new(4, 4);
    private readonly Dictionary<string, SemaphoreSlim> _hosts = new(StringComparer.OrdinalIgnoreCase);
    private readonly Func<Uri, bool>? _viaBrowser;
    private readonly Func<Uri, CancellationToken, Task<byte[]?>>? _browser;

    /// <param name="viaBrowser">Чьи картинки качать встроенным браузером (сайт за защитой Cloudflare).</param>
    /// <param name="browser">Скачать встроенным браузером: байты или null.</param>
    public RemotePreviews(string dir, Func<Uri, bool>? viaBrowser = null, Func<Uri, CancellationToken, Task<byte[]?>>? browser = null)
    {
        _dir = dir;
        _viaBrowser = viaBrowser;
        _browser = browser;
    }

    public async Task<string?> FetchAsync(Uri url, CancellationToken ct)
    {
        if (url.IsFile) return File.Exists(url.LocalPath) ? url.LocalPath : null; // демо: образцы с диска
        if (url.Scheme != Uri.UriSchemeHttps) return null;
        var path = Path.Combine(_dir, Hash(url.AbsoluteUri) + ".jpg");
        if (File.Exists(path)) return path;
        var host = HostGate(url.Host);
        await _gate.WaitAsync(ct).ConfigureAwait(false);
        try
        {
            await host.WaitAsync(ct).ConfigureAwait(false);
            try
            {
                if (File.Exists(path)) return path;
                var raw = _browser is not null && _viaBrowser?.Invoke(url) == true
                    ? await _browser(url, ct).ConfigureAwait(false)
                    : await DownloadAsync(url, ct).ConfigureAwait(false);
                if (raw is null || raw.Length > MaxBytes) return null;
                var jpeg = Shrinker.ToJpeg(raw);
                if (jpeg is null) return null;
                Directory.CreateDirectory(_dir);
                var tmp = path + ".tmp";
                await File.WriteAllBytesAsync(tmp, jpeg, ct).ConfigureAwait(false);
                File.Move(tmp, path, overwrite: true);
                return path;
            }
            finally
            {
                // пауза перед следующей картинкой с того же сайта: не залпом
                try { await Task.Delay(Gap, ct).ConfigureAwait(false); } catch (OperationCanceledException) { }
                host.Release();
            }
        }
        catch (Exception e) when (e is HttpRequestException or IOException or UnauthorizedAccessException || (e is TaskCanceledException && !ct.IsCancellationRequested))
        {
            return null;
        }
        finally { _gate.Release(); }
    }

    private SemaphoreSlim HostGate(string host)
    {
        lock (_hosts)
        {
            if (!_hosts.TryGetValue(host, out var g)) _hosts[host] = g = new SemaphoreSlim(2, 2);
            return g;
        }
    }

    /// <summary>Скачать с повтором после «слишком много запросов» (с уважением Retry-After) и временного сбоя сайта.</summary>
    private static async Task<byte[]?> DownloadAsync(Uri url, CancellationToken ct)
    {
        for (var attempt = 0; ; attempt++)
        {
            using var req = new HttpRequestMessage(HttpMethod.Get, url);
            req.Headers.Referrer = new Uri(url.GetLeftPart(UriPartial.Authority) + "/");
            req.Headers.Accept.ParseAdd("image/avif,image/webp,image/png,image/jpeg,image/*;q=0.8");
            using var resp = await Http.SendAsync(req, HttpCompletionOption.ResponseHeadersRead, ct).ConfigureAwait(false);
            var code = (int)resp.StatusCode;
            if (code is 429 or 502 or 503 or 504 && attempt < Retries)
            {
                var wait = resp.Headers.RetryAfter?.Delta ?? TimeSpan.FromSeconds(1.5 * (attempt + 1));
                await Task.Delay(wait > TimeSpan.FromSeconds(10) ? TimeSpan.FromSeconds(10) : wait, ct).ConfigureAwait(false);
                continue;
            }
            if (!resp.IsSuccessStatusCode || resp.Content.Headers.ContentLength > MaxBytes) return null;
            return await ReadLimitedAsync(await resp.Content.ReadAsStreamAsync(ct).ConfigureAwait(false), ct).ConfigureAwait(false);
        }
    }

    /// <summary>Картинки, которые не открывали месяц, убираются (раз за запуск, в фоне).</summary>
    public void Prune(TimeSpan olderThan)
    {
        try
        {
            if (!Directory.Exists(_dir)) return;
            var edge = DateTime.UtcNow - olderThan;
            foreach (var f in Directory.EnumerateFiles(_dir))
                if (File.GetLastWriteTimeUtc(f) < edge) { try { File.Delete(f); } catch (IOException) { } }
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
    }

    private static async Task<byte[]?> ReadLimitedAsync(Stream s, CancellationToken ct)
    {
        using var ms = new MemoryStream();
        var buf = new byte[81920];
        int n;
        while ((n = await s.ReadAsync(buf, ct).ConfigureAwait(false)) > 0)
        {
            ms.Write(buf, 0, n);
            if (ms.Length > MaxBytes) return null;
        }
        return ms.ToArray();
    }

    private static string Hash(string s) => Convert.ToHexString(SHA1.HashData(Encoding.UTF8.GetBytes(s)))[..20].ToLowerInvariant();
}
