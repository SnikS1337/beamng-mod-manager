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

    /// <summary>
    /// Попадает ли страница мода в ленту сайта. Если нет, по ленте нельзя заключить, что мод не менялся: его проверяем по
    /// карточке (2026-10-18: лента ModsGaming — только раздел «Машины», и обновления модов из других разделов не находились).
    /// </summary>
    bool FeedCovers(Uri page) => true;

    Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? since, CancellationToken ct = default);

    Task<IReadOnlyList<Offer>> GetOffersAsync(Listing listing, CancellationToken ct = default);
}
