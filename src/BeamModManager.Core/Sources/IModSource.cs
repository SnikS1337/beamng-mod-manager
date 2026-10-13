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

    Task<IReadOnlyList<Listing>> SearchAsync(string query, CancellationToken ct = default);

    Task<Listing?> GetListingAsync(Uri url, CancellationToken ct = default);

    /// <summary>Новые и обновлённые записи; since = null означает «последняя страница».</summary>
    Task<IReadOnlyList<Listing>> LatestAsync(DateTimeOffset? since, CancellationToken ct = default);

    Task<IReadOnlyList<Offer>> GetOffersAsync(Listing listing, CancellationToken ct = default);
}
