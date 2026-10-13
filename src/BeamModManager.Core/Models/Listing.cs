namespace BeamModManager.Core.Models;

/// <summary>Статус цены. Free ставится только при положительных признаках, остальное Unknown.</summary>
public enum PriceStatus { Unknown, Free, Paid }

/// <summary>Как можно получить файл мода.</summary>
public enum OfferKind { Direct, ExternalHost, PaidLink }

/// <summary>Один способ получить файл (ссылка на хост или страницу автора).</summary>
public sealed record Offer(OfferKind Kind, string Host, Uri Url, string? HostFileId = null, long? SizeBytes = null);

/// <summary>Запись об одном моде на одном сайте.</summary>
public sealed record Listing(string Source, Uri Url, string SourceId, string Title)
{
    public string? ModVersion { get; init; }
    public string? GameVersion { get; init; }
    public string? Author { get; init; }
    public string? Category { get; init; }
    public long? SizeBytes { get; init; }

    /// <summary>64-битный перцептивный хеш превью (если посчитан).</summary>
    public ulong? PreviewHash { get; init; }

    public PriceStatus Price { get; init; } = PriceStatus.Unknown;
    public IReadOnlyList<Offer> Offers { get; init; } = Array.Empty<Offer>();
    /// <summary>Дата публикации на сайте, если она видна в выдаче.</summary>
    public DateTimeOffset? PublishedAt { get; init; }
    /// <summary>Дата последнего обновления на сайте, если она видна в ленте.</summary>
    public DateTimeOffset? UpdatedAt { get; init; }

    /// <summary>Устойчивый идентификатор мода у источника (у официального репозитория — Unique ID, как в игре).</summary>
    public string? ExternalId { get; init; }
    public DateTimeOffset? FirstSeen { get; init; }
    public DateTimeOffset? LastSeen { get; init; }
}
