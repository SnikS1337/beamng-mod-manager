using BeamModManager.Core.Models;

namespace BeamModManager.Core.Sources;

/// <summary>
/// Контракт адаптера сайта. Одна реализация на сайт; сбой одного адаптера не должен
/// ломать остальные. Реализации появятся после того, как у нас будут образцы реальных страниц.
/// </summary>
public interface IModSource
{
    /// <summary>Короткий стабильный идентификатор, например "modland".</summary>
    string Id { get; }

    string DisplayName { get; }

    /// <summary>Умеет ли адаптер искать (разметку результатов поиска мы видели не на всех сайтах).</summary>
    bool SupportsSearch { get; }

    Task<IReadOnlyList<Listing>> SearchAsync(string query, CancellationToken ct = default);

    Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default);

    /// <summary>Новые и обновлённые записи; since = null означает «последняя страница».</summary>
    /// <summary>Страница ленты «новое / обновлённое» (с 1). Нужна планировщику обновлений.</summary>
    Task<IReadOnlyList<Listing>> FeedPageAsync(int page, CancellationToken ct = default);

    Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? since, CancellationToken ct = default);

    Task<IReadOnlyList<Offer>> GetOffersAsync(Listing listing, CancellationToken ct = default);
}
