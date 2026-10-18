using System.IO;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text;

namespace BeamModManager.App.Services;

/// <summary>
/// Картинки карточек поиска: с сайта (только https), уменьшаются до размера карточки (<see cref="Shrinker"/>) и лежат в
/// папке программы, чтобы повторный поиск не качал их снова. Не больше четырёх загрузок разом, файл больше 6 МБ не берём.
/// Не получилось (нет сети, формат не понят Windows) — null, карточка остаётся с заглушкой.
/// </summary>
public sealed class RemotePreviews
{
    private const long MaxBytes = 6L * 1024 * 1024;
    // представляемся так же, как при загрузке страниц: часть сайтов не отдаёт картинки запросу без User-Agent (2026-10-18)
    private static readonly HttpClient Http = CreateClient();

    private static HttpClient CreateClient()
    {
        var h = new HttpClient { Timeout = TimeSpan.FromSeconds(15) };
        h.DefaultRequestHeaders.UserAgent.ParseAdd(BeamModManager.Core.Net.HttpPageFetcher.UserAgent);
        return h;
    }
    private readonly string _dir;
    private readonly SemaphoreSlim _gate = new(4, 4);

    public RemotePreviews(string dir) => _dir = dir;

    public async Task<string?> FetchAsync(Uri url, CancellationToken ct)
    {
        if (url.IsFile) return File.Exists(url.LocalPath) ? url.LocalPath : null; // демо: образцы с диска
        if (url.Scheme != Uri.UriSchemeHttps) return null;
        var path = Path.Combine(_dir, Hash(url.AbsoluteUri) + ".jpg");
        if (File.Exists(path)) return path;
        await _gate.WaitAsync(ct).ConfigureAwait(false);
        try
        {
            if (File.Exists(path)) return path;
            using var resp = await Http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, ct).ConfigureAwait(false);
            if (!resp.IsSuccessStatusCode || resp.Content.Headers.ContentLength > MaxBytes) return null;
            var raw = await ReadLimitedAsync(await resp.Content.ReadAsStreamAsync(ct).ConfigureAwait(false), ct).ConfigureAwait(false);
            if (raw is null) return null;
            var jpeg = Shrinker.ToJpeg(raw);
            if (jpeg is null) return null;
            Directory.CreateDirectory(_dir);
            var tmp = path + ".tmp";
            await File.WriteAllBytesAsync(tmp, jpeg, ct).ConfigureAwait(false);
            File.Move(tmp, path, overwrite: true);
            return path;
        }
        catch (Exception e) when (e is HttpRequestException or IOException or UnauthorizedAccessException || (e is TaskCanceledException && !ct.IsCancellationRequested))
        {
            return null;
        }
        finally { _gate.Release(); }
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
