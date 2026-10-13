using BeamModManager.Core.Hosts;
using BeamModManager.Core.Models;

namespace BeamModManager.Core.Settings;

public sealed record DownloadDecision(bool Allowed, string? Reason = null, bool NeedsConfirmation = false)
{
    public static DownloadDecision Deny(string reason) => new(false, reason);
}

/// <summary>
/// Единственное место, которое решает, можно ли вообще предлагать загрузку. Окно загрузки и кнопки интерфейса спрашивают
/// его, а не проверяют настройку каждый по-своему: иначе одна забытая проверка открыла бы загрузку при выключенной настройке.
/// </summary>
public static class DownloadPolicy
{
    public static DownloadDecision Evaluate(AppSettings settings, Listing listing, Offer offer)
    {
        if (!settings.AllowDownloads) return DownloadDecision.Deny("Загрузка модов выключена в настройках.");
        if (!settings.IsSourceEnabled(listing.Source)) return DownloadDecision.Deny("Сайт отключён в настройках.");

        if (listing.Price == PriceStatus.Paid || offer.Kind == OfferKind.PaidLink || HostLinkParser.HostMatches(offer.Url, HostLinkParser.PaidHosts))
            return DownloadDecision.Deny("Мод платный: купите его у автора.");

        if (offer.Url.Scheme != Uri.UriSchemeHttps) return DownloadDecision.Deny("Ссылка не защищена (нужен https).");
        if (!HostLinkParser.HostMatches(offer.Url, HostLinkParser.KnownFileHosts))
            return DownloadDecision.Deny("Неизвестный файлообменник: скачайте вручную.");

        // Цена не доказана бесплатной: загрузку не запрещаем (страница могла просто не сказать), но просим подтверждения.
        return new DownloadDecision(true, null, NeedsConfirmation: listing.Price != PriceStatus.Free);
    }
}
