using System.Net;
using System.Security.Cryptography;

namespace BeamModManager.Core.AppUpdate;

/// <summary>
/// Скачивание описания и архива обновления. Переходы по перенаправлениям — вручную, только на GitHub и только по https;
/// размер ограничен; архив пишется в .part и становится файлом, только если совпали размер и SHA-256.
/// </summary>
public sealed class UpdateDownloader
{
    private const int MaxRedirects = 5;
    private const long MaxManifestBytes = 64 * 1024;
    private readonly HttpClient _http;

    /// <param name="http">HttpClient без автоматических перенаправлений (AllowAutoRedirect = false).</param>
    public UpdateDownloader(HttpClient http) => _http = http;

    public static HttpClient CreateClient(string userAgent)
    {
        var h = new HttpClient(new SocketsHttpHandler { AllowAutoRedirect = false, ConnectTimeout = TimeSpan.FromSeconds(15) })
        {
            Timeout = Timeout.InfiniteTimeSpan, // общий срок задаёт вызывающий через CancellationToken
        };
        h.DefaultRequestHeaders.UserAgent.ParseAdd(userAgent);
        return h;
    }

    public async Task<(UpdateManifest? Manifest, string? Error)> GetManifestAsync(Uri manifestUrl, CancellationToken ct)
    {
        try
        {
            using var resp = await SendFollowingAsync(manifestUrl, ct).ConfigureAwait(false);
            if (resp.StatusCode == HttpStatusCode.NotFound) return (null, "выпусков пока нет");
            if (!resp.IsSuccessStatusCode) return (null, $"сервер ответил {(int)resp.StatusCode}");
            if (resp.Content.Headers.ContentLength is > MaxManifestBytes) return (null, "описание обновления слишком большое");
            var bytes = await ReadLimitedAsync(resp, MaxManifestBytes, ct).ConfigureAwait(false);
            return UpdateManifest.Parse(System.Text.Encoding.UTF8.GetString(bytes));
        }
        catch (OperationCanceledException) when (!ct.IsCancellationRequested) { return (null, "сервер не отвечает"); }
        catch (Exception e) when (e is HttpRequestException or IOException or InvalidDataException) { return (null, "нет связи с GitHub"); }
    }

    /// <summary>Скачать архив в <paramref name="target"/>. Прогресс 0..1. Ошибка — понятная причина; частичный файл удаляется.</summary>
    public async Task<string?> DownloadAsync(UpdateManifest m, string target, IProgress<double>? progress, CancellationToken ct)
    {
        var part = target + ".part";
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(target))!);
            using var resp = await SendFollowingAsync(m.Url, ct).ConfigureAwait(false);
            if (!resp.IsSuccessStatusCode) return $"сервер ответил {(int)resp.StatusCode}";
            if (resp.Content.Headers.ContentLength is { } len && len != m.Size) return "размер файла на сервере не совпадает с описанием";

            using var sha = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
            long got = 0;
            await using (var src = await resp.Content.ReadAsStreamAsync(ct).ConfigureAwait(false))
            await using (var dst = new FileStream(part, FileMode.Create, FileAccess.Write, FileShare.None, 81920, useAsync: true))
            {
                var buf = new byte[81920];
                int n;
                while ((n = await ReadWithStallTimeoutAsync(src, buf, ct).ConfigureAwait(false)) > 0)
                {
                    got += n;
                    if (got > m.Size) return "файл на сервере больше, чем в описании";
                    sha.AppendData(buf, 0, n);
                    await dst.WriteAsync(buf.AsMemory(0, n), ct).ConfigureAwait(false);
                    progress?.Report((double)got / m.Size);
                }
            }
            if (got != m.Size) return "загрузка оборвалась";
            if (!Convert.ToHexString(sha.GetHashAndReset()).Equals(m.Sha256, StringComparison.OrdinalIgnoreCase))
                return "контрольная сумма не совпала: файл повреждён или подменён";
            File.Move(part, target, overwrite: true);
            return null;
        }
        catch (OperationCanceledException) when (!ct.IsCancellationRequested) { return "сервер перестал отвечать"; }
        catch (Exception e) when (e is HttpRequestException or IOException or UnauthorizedAccessException) { return "не удалось скачать: " + e.Message; }
        finally
        {
            try { if (File.Exists(part)) File.Delete(part); } catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
        }
    }

    /// <summary>Чтение с ограничением простоя: если 30 с не пришло ни байта, загрузка прерывается.</summary>
    private static async Task<int> ReadWithStallTimeoutAsync(Stream s, byte[] buf, CancellationToken ct)
    {
        using var stall = CancellationTokenSource.CreateLinkedTokenSource(ct);
        stall.CancelAfter(TimeSpan.FromSeconds(30));
        return await s.ReadAsync(buf, stall.Token).ConfigureAwait(false);
    }

    private async Task<HttpResponseMessage> SendFollowingAsync(Uri url, CancellationToken ct)
    {
        for (var i = 0; ; i++)
        {
            if (!UpdateManifest.IsAllowedHost(url)) throw new HttpRequestException("адрес не с GitHub");
            using var timeout = CancellationTokenSource.CreateLinkedTokenSource(ct);
            timeout.CancelAfter(TimeSpan.FromSeconds(30)); // ожидание заголовков ответа
            var resp = await _http.SendAsync(new HttpRequestMessage(HttpMethod.Get, url), HttpCompletionOption.ResponseHeadersRead, timeout.Token).ConfigureAwait(false);
            var code = (int)resp.StatusCode;
            if (code is < 300 or > 399) return resp;
            var loc = resp.Headers.Location;
            resp.Dispose();
            if (loc is null || i >= MaxRedirects) throw new HttpRequestException("слишком много перенаправлений");
            url = loc.IsAbsoluteUri ? loc : new Uri(url, loc);
        }
    }

    private static async Task<byte[]> ReadLimitedAsync(HttpResponseMessage resp, long limit, CancellationToken ct)
    {
        await using var s = await resp.Content.ReadAsStreamAsync(ct).ConfigureAwait(false);
        using var ms = new MemoryStream();
        var buf = new byte[8192];
        int n;
        while ((n = await s.ReadAsync(buf, ct).ConfigureAwait(false)) > 0)
        {
            if (ms.Length + n > limit) throw new InvalidDataException("слишком большой ответ");
            ms.Write(buf, 0, n);
        }
        return ms.ToArray();
    }
}
