using System.Text.RegularExpressions;

namespace BeamModManager.Core.Hosts;

/// <summary>Ссылка на файл на файлообменнике: хост и идентификатор файла на нём.</summary>
public sealed record HostLink(string Host, string FileId, Uri Url);

/// <summary>
/// Вытаскивает идентификатор файла из ссылки на известные файлообменники. Одинаковые
/// (хост, идентификатор) у двух записей разных сайтов означают один и тот же файл.
/// Идентификаторы регистрозависимы.
/// </summary>
public static class HostLinkParser
{
    private static readonly Regex IdPattern = new(@"^[A-Za-z0-9]{6,32}$", RegexOptions.Compiled);

    // Служебные страницы ModsFire, которые не являются файлами.
    private static readonly HashSet<string> ModsFireReserved = new(StringComparer.OrdinalIgnoreCase)
    {
        "get-premium", "login", "register", "points", "guide", "blog", "contact-us",
        "privacy-policy", "tos", "download", "auth", "alt", "img", "assets",
    };

    public static HostLink? TryParse(string? url)
    {
        if (string.IsNullOrWhiteSpace(url) || !Uri.TryCreate(url, UriKind.Absolute, out var uri)) return null;
        if (uri.Scheme != Uri.UriSchemeHttps && uri.Scheme != Uri.UriSchemeHttp) return null;

        var host = uri.Host.ToLowerInvariant();
        if (host.StartsWith("www.")) host = host[4..];

        var segments = uri.AbsolutePath.Split('/', StringSplitOptions.RemoveEmptyEntries);

        return host switch
        {
            "modsfire.com" => ParseModsFire(uri, segments),
            "mediafire.com" => ParseMediaFire(uri, segments),
            _ => null,
        };
    }

    private static HostLink? ParseModsFire(Uri uri, string[] segments)
    {
        string? id = null;

        if (segments.Length == 1 && !ModsFireReserved.Contains(segments[0]))
            id = segments[0];
        else if (segments.Length >= 2 && segments[0].Equals("download", StringComparison.OrdinalIgnoreCase))
            id = segments[1];

        return id is not null && IdPattern.IsMatch(id) ? new HostLink("modsfire", id, uri) : null;
    }

    private static HostLink? ParseMediaFire(Uri uri, string[] segments)
    {
        if (segments.Length >= 2 &&
            (segments[0].Equals("file", StringComparison.OrdinalIgnoreCase) ||
             segments[0].Equals("view", StringComparison.OrdinalIgnoreCase)) &&
            IdPattern.IsMatch(segments[1]))
        {
            return new HostLink("mediafire", segments[1], uri);
        }

        return null;
    }

    /// <summary>Файлообменники и каталоги, где файл раздаётся как есть (положительный признак бесплатности).</summary>
    public static readonly IReadOnlyCollection<string> KnownFileHosts = new[]
    {
        "modsfire.com", "mediafire.com", "mega.nz", "drive.google.com", "dropbox.com", "github.com", "beamng.com",
    };

    /// <summary>Страницы поддержки автора: платный контент или донаты.</summary>
    public static readonly IReadOnlyCollection<string> PaidHosts = new[]
    {
        "patreon.com", "boosty.to", "gumroad.com", "ko-fi.com", "buymeacoffee.com",
    };

    public static bool HostMatches(Uri uri, IEnumerable<string> domains)
    {
        var host = uri.Host.ToLowerInvariant();
        return domains.Any(d => host == d || host.EndsWith("." + d));
    }
}
