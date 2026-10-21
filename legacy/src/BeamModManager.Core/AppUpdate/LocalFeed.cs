using System.Net;

namespace BeamModManager.Core.AppUpdate;

/// <summary>
/// Выпуски из папки вместо GitHub: для проверки самообновления на сборочной машине (переменная BMM_TEST_UPDATE_FEED).
/// Отвечает на любой разрешённый адрес файлом с тем же именем, что последняя часть адреса («latest.json»,
/// «ModManagerBeamNG.zip»); нет файла — 404. Всё остальное (адреса только GitHub, размер, SHA-256, версия exe)
/// проверяется как обычно: подменяется только сеть.
/// </summary>
public sealed class LocalFeedHandler(string dir) : HttpMessageHandler
{
    protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken ct)
    {
        ct.ThrowIfCancellationRequested();
        var name = request.RequestUri is { } u ? Uri.UnescapeDataString(u.Segments[^1]) : "";
        var path = Path.Combine(dir, name);
        // только файлы прямо в папке: «..» и подпапки из адреса не выходят за её пределы
        if (name.Length == 0 || name.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0 || name is "." or ".." || !File.Exists(path))
            return Task.FromResult(new HttpResponseMessage(HttpStatusCode.NotFound) { RequestMessage = request });
        var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read, 81920, useAsync: true);
        var content = new StreamContent(stream);
        content.Headers.ContentLength = stream.Length;
        return Task.FromResult(new HttpResponseMessage(HttpStatusCode.OK) { Content = content, RequestMessage = request });
    }
}
