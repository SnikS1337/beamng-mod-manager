using System.Net.Sockets;
using System.Security.Authentication;

namespace BeamModManager.Core.Net;

/// <summary>
/// Причина сетевого сбоя словами для человека (2026-10-18). Раньше в строку сайта в «Обновлениях» и в поиске попадал текст
/// исключения — системный и часто английский («No such host is known. (www.beamng.com:443)»).
/// </summary>
public static class NetErrors
{
    /// <summary>
    /// Текст для строки сайта: сетевой сбой — словами (<see cref="Describe"/>), остальное — как есть (адаптеры пишут свои
    /// причины по-русски: «разметка изменилась», «лента пуста»).
    /// </summary>
    public static string ForUser(Exception e) =>
        e is HttpRequestException or IOException or SocketException or AuthenticationException ? Describe(e) : e.Message;

    public static string Describe(Exception e)
    {
        if (e is OperationCanceledException) return "сайт не ответил вовремя";
        if (e is HttpRequestException h)
        {
            switch (h.HttpRequestError)
            {
                case HttpRequestError.NameResolutionError: return "сайт не найден: нет сети или адрес изменился";
                case HttpRequestError.ConnectionError: return "нет связи с сайтом";
                case HttpRequestError.SecureConnectionError: return "не удалось установить защищённое соединение";
                case HttpRequestError.ResponseEnded or HttpRequestError.InvalidResponse: return "сайт оборвал ответ";
                case HttpRequestError.ProxyTunnelError: return "прокси не пропустил запрос";
            }
        }
        for (var x = e; x is not null; x = x.InnerException)
        {
            switch (x)
            {
                case SocketException { SocketErrorCode: SocketError.HostNotFound or SocketError.NoData or SocketError.TryAgain }:
                    return "сайт не найден: нет сети или адрес изменился";
                case SocketException { SocketErrorCode: SocketError.TimedOut }:
                    return "сайт не ответил вовремя";
                case SocketException:
                    return "нет связи с сайтом";
                case AuthenticationException:
                    return "не удалось установить защищённое соединение";
            }
        }
        return e is IOException ? "соединение оборвалось" : "сеть недоступна или сайт не отвечает";
    }
}
