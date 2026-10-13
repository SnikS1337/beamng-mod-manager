using BeamModManager.Core.Hosts;
using BeamModManager.Core.Models;
using BeamModManager.Core.Text;

namespace BeamModManager.Core.Pricing;

/// <summary>
/// Определяет статус цены. Принцип: «бесплатно» только при положительных признаках,
/// всё неопознанное остаётся Unknown.
/// </summary>
public static class PaidDetector
{
    private static readonly HashSet<string> PaidTags = new(StringComparer.OrdinalIgnoreCase)
    {
        "paid", "premium", "vip", "early access", "early-access", "платный", "платно",
    };

    public static PriceStatus Detect(string title, IEnumerable<string>? tags, IEnumerable<Uri>? downloadUrls)
    {
        // 1. Явные метки платного: в названии или в тегах сайта.
        if (TitleParser.HasPaidMarker(title)) return PriceStatus.Paid;
        if (tags is not null && tags.Any(t => PaidTags.Contains(t.Trim()))) return PriceStatus.Paid;

        var urls = downloadUrls?.ToList() ?? new List<Uri>();
        if (urls.Count == 0) return PriceStatus.Unknown;

        // 2. Если есть известный хост файлов, значит файл раздаётся; ссылка на Patreon рядом
        //    чаще всего просто поддержка автора, а не платная стена.
        if (urls.Any(u => HostLinkParser.HostMatches(u, HostLinkParser.KnownFileHosts)))
            return PriceStatus.Free;

        // 3. Единственные ссылки ведут на страницы поддержки автора.
        if (urls.All(u => HostLinkParser.HostMatches(u, HostLinkParser.PaidHosts)))
            return PriceStatus.Paid;

        return PriceStatus.Unknown;
    }
}
