using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Core.Search;
using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources;
using BeamModManager.Core.Updates;

namespace BeamModManager.Ui.Updates;

/// <summary>Сайт в панели «Проверка»: включён ли он в настройках, что о нём известно и за сколькими модами на нём следим.</summary>
public sealed record SiteInfo(string Id, string Name, bool Enabled, SourceStatus Status, int Tracked, Uri? Home);

/// <summary>Проверка введённой ссылки в окне «Указать страницу»: найденная запись или понятная причина, почему нет.</summary>
public sealed record PageCheck(Listing? Listing, string? Problem)
{
    public bool Ok => Listing is not null;
}

/// <summary>
/// Чем раздел «Обновления» пользуется снаружи: проверка (через <see cref="UpdateCoordinator"/>), действия над найденным и
/// привязка страниц. В тестах подменяется, в приложении — <see cref="LibraryUpdatesBackend"/>, в демо — заглушка.
/// </summary>
public interface IUpdatesBackend
{
    bool IsRunning { get; }
    UpdateRunReport? LastReport { get; }
    Task<UpdateRunReport> CheckAsync(bool force, IProgress<UpdateProgress>? progress = null);
    void Cancel();

    IReadOnlyList<SiteInfo> Sites();
    /// <summary>Когда проверяли последний раз (самая свежая проверка любого мода); null — ни разу.</summary>
    DateTimeOffset? LastCheckedUtc();
    /// <summary>Сколько страниц на включённых сайтах проверено начиная с <paramref name="since"/> и сколько всего.</summary>
    (int Checked, int Total) Coverage(DateTimeOffset since);

    /// <summary>«Пропустить эту версию» / «Версия не изменилась»: скрывает текущее найденное обновление мода.</summary>
    bool Dismiss(string modId);
    void Restore(string modId);
    DeclineUndo? Decline(string modId);
    bool UndoDecline(DeclineUndo undo);
    /// <summary>Пользователь прошёл проверку «вы человек?» на сайте.</summary>
    void ResolveHumanCheck(string source);

    /// <summary>Похожие записи на сайтах для мода (поиск по названию); для пропавшей страницы — без неё самой.</summary>
    Task<IReadOnlyList<BindingSuggestion>> SuggestAsync(string modId, bool replacing, CancellationToken ct = default);
    /// <summary>Открыть ссылку, которую ввёл человек, и понять, страница ли это мода на знакомом сайте.</summary>
    Task<PageCheck> CheckPageAsync(string text, CancellationToken ct = default);
    /// <summary>
    /// Следить за модом на этой странице. <paramref name="replaceSource"/> — сайт пропавшей страницы, которую она заменяет.
    /// <paramref name="verified"/> — запись открыта целиком (ссылка проверена): её версия и размер сразу становятся известны.
    /// </summary>
    bool Bind(string modId, Listing listing, string? replaceSource, bool verified);
}

/// <summary>Короткие названия сайтов для интерфейса.</summary>
public static class SiteNames
{
    public static string Of(string id) => id switch
    {
        "modland" => "Modland",
        "modsgaming" => "ModsGaming",
        "stmods" => "STMods",
        "beamng" => "Репозиторий BeamNG",
        _ => id,
    };
}

/// <summary>Раздел «Обновления» поверх настоящих библиотеки и сайтов.</summary>
public sealed class LibraryUpdatesBackend : IUpdatesBackend
{
    private readonly ILibraryStore _store;
    private readonly IReadOnlyList<IModSource> _all;
    private readonly Func<AppSettings> _settings;
    private readonly Func<DateTimeOffset> _now;
    private readonly UpdateCoordinator _coordinator;
    private readonly Func<string, SourceStatus>? _hostStatus;

    /// <param name="sources">Все адаптеры сайтов; выключенные в настройках не опрашиваются ни проверкой, ни поиском.</param>
    /// <param name="clearCache">Сбросить кэш страниц перед принудительной проверкой.</param>
    /// <param name="resetSite">Снять паузу сайта после проверки «вы человек?» (по адресу главной страницы).</param>
    /// <param name="hostStatus">Состояние сайта в сетевом слое по адресу (пауза после сбоев, «вы человек?»): поиск его пропускает.</param>
    public LibraryUpdatesBackend(ILibraryStore store, IReadOnlyList<IModSource> sources, Func<AppSettings> settings,
        Func<DateTimeOffset>? now = null, Action? clearCache = null, Action<string>? resetSite = null,
        Func<string, SourceStatus>? hostStatus = null, UpdateCoordinatorOptions? options = null)
    {
        _store = store;
        _all = sources;
        _settings = settings;
        _now = now ?? (() => DateTimeOffset.UtcNow);
        _hostStatus = hostStatus;
        var s = settings();
        _coordinator = new UpdateCoordinator(store, Enabled, _now,
            options ?? new UpdateCoordinatorOptions { Check = s.ToCheckOptions() },
            clearCache, id => { if (_all.FirstOrDefault(x => x.Id == id) is SiteAdapter a) resetSite?.Invoke(a.Home.Host); });
    }

    private IReadOnlyList<IModSource> Enabled()
    {
        var s = _settings();
        return _all.Where(x => s.IsSourceEnabled(x.Id)).ToList();
    }

    public bool IsRunning => _coordinator.IsRunning;
    public UpdateRunReport? LastReport => _coordinator.LastReport;

    public async Task<UpdateRunReport> CheckAsync(bool force, IProgress<UpdateProgress>? progress = null)
    {
        // одна проверка пишет сотни состояний: на диск — одной записью в конце, а не на каждое
        using ((_store as JsonLibraryStore)?.Batch())
            return await _coordinator.CheckAsync(force, progress).ConfigureAwait(false);
    }

    public void Cancel() => _coordinator.Cancel();

    public IReadOnlyList<SiteInfo> Sites()
    {
        var s = _settings();
        var statuses = _store.SourceStatuses();
        var tracked = _store.Bindings().Where(b => b.Confirmed).GroupBy(b => b.Source).ToDictionary(g => g.Key, g => g.Count());
        return _all.Select(x => new SiteInfo(x.Id, SiteNames.Of(x.Id), s.IsSourceEnabled(x.Id),
            statuses.TryGetValue(x.Id, out var st) ? st : SourceStatus.Ok, tracked.GetValueOrDefault(x.Id),
            (x as SiteAdapter)?.Home)).ToList();
    }

    public DateTimeOffset? LastCheckedUtc()
    {
        var now = _now();
        // время «из будущего» (часы переводили назад) за последнюю проверку не считаем
        var times = _store.Checks().Select(c => c.LastCheckedUtc).Where(t => t <= now + TimeSpan.FromMinutes(5)).ToList();
        return times.Count == 0 ? null : times.Max();
    }

    public (int Checked, int Total) Coverage(DateTimeOffset since)
    {
        var s = _settings();
        var mods = _store.Mods().Select(m => m.Id).ToHashSet();
        var tracked = _store.Bindings().Where(b => b.Confirmed && mods.Contains(b.ModId) && s.IsSourceEnabled(b.Source)).ToList();
        var done = tracked.Count(b => _store.GetCheck(b.ModId, b.Source) is { } c && c.LastCheckedUtc >= since);
        return (done, tracked.Count);
    }

    public bool Dismiss(string modId)
    {
        var f = UpdateCheckService.Pending(_store).FirstOrDefault(x => x.ModId == modId);
        if (f is null) return false;
        UpdateCheckService.Dismiss(_store, f);
        return true;
    }

    public void Restore(string modId) => UpdateCheckService.Restore(_store, modId);
    public DeclineUndo? Decline(string modId) => BindingActions.Decline(_store, modId);
    public bool UndoDecline(DeclineUndo undo) => BindingActions.Undo(_store, undo);
    public void ResolveHumanCheck(string source) => _coordinator.ResolveHumanCheck(source);

    private SearchService Search()
    {
        var s = _settings();
        Func<string, SourceStatus>? status = _hostStatus is { } hs
            ? id => _all.FirstOrDefault(x => x.Id == id) is SiteAdapter a ? hs(a.Home.Host) : SourceStatus.Ok
            : null;
        return new SearchService(_all.Where(x => x.SupportsSearch && s.IsSourceEnabled(x.Id)), status);
    }

    public async Task<IReadOnlyList<BindingSuggestion>> SuggestAsync(string modId, bool replacing, CancellationToken ct = default)
    {
        var mod = _store.GetMod(modId);
        if (mod is null) return Array.Empty<BindingSuggestion>();
        // человек сам попросил подобрать страницу: прежний отказ «Не проверять» здесь не мешает
        var response = await Search().SearchAsync(BindingSuggester.QueryFor(mod), ct).ConfigureAwait(false);
        var bound = _store.Bindings().Where(b => b.ModId == modId).ToList();
        var seen = new HashSet<(string, string)>();
        var listings = response.Cards.SelectMany(c => c.Listings)
            .Where(l => seen.Add((l.Source, l.SourceId)))
            // замена пропавшей: не предлагаем саму пропавшую страницу; уже отслеживаемую — тоже незачем
            .Where(l => !bound.Any(b => b.Source == l.Source && (b.SourceId == l.SourceId || b.Url == l.Url)))
            .ToList();
        return BindingSuggester.Rank(mod, listings, 5);
    }

    public async Task<PageCheck> CheckPageAsync(string text, CancellationToken ct = default)
    {
        var t = (text ?? "").Trim();
        if (t.Length == 0) return new PageCheck(null, null);
        if (!t.Contains("://", StringComparison.Ordinal)) t = "https://" + t;
        if (!Uri.TryCreate(t, UriKind.Absolute, out var url) || (url.Scheme != Uri.UriSchemeHttps && url.Scheme != Uri.UriSchemeHttp) || !url.Host.Contains('.'))
            return new PageCheck(null, "Это не похоже на ссылку");
        var src = _all.OfType<SiteAdapter>().FirstOrDefault(a => a.Owns(url));
        if (src is null)
            return new PageCheck(null, "Этот сайт менеджер не знает. Подойдут Modland, ModsGaming, STMods и репозиторий BeamNG");
        if (!_settings().IsSourceEnabled(src.Id))
            return new PageCheck(null, $"{SiteNames.Of(src.Id)} выключен в настройках. Включите его в «Настройки → Сайты»");
        try
        {
            var listing = await src.GetListingAsync(url, ct).ConfigureAwait(false);
            return listing is null
                ? new PageCheck(null, "Такой страницы на сайте нет. Проверьте ссылку")
                : new PageCheck(listing, null);
        }
        catch (SourceException e) when (e.Status == SourceStatus.NeedsHumanCheck)
        {
            return new PageCheck(null, $"{SiteNames.Of(src.Id)} просит подтвердить, что вы человек. Откройте сайт в браузере и попробуйте ещё раз");
        }
        catch (SourceException)
        {
            return new PageCheck(null, $"{SiteNames.Of(src.Id)} сейчас не отвечает. Попробуйте позже");
        }
        catch (HttpRequestException)
        {
            return new PageCheck(null, "Нет соединения с сайтом. Проверьте интернет");
        }
        catch (TaskCanceledException) when (!ct.IsCancellationRequested)
        {
            return new PageCheck(null, $"{SiteNames.Of(src.Id)} не ответил вовремя. Попробуйте позже");
        }
    }

    private static readonly object BindGate = new();

    public bool Bind(string modId, Listing listing, string? replaceSource, bool verified)
    {
        lock (BindGate)
        {
            var mod = _store.GetMod(modId);
            if (mod is null) return false;
            if (replaceSource is not null) _store.RemoveBinding(modId, replaceSource);
            // на одном сайте за модом следим по одной странице: прежняя (с её проверками) заменяется
            if (_store.Bindings().Any(b => b.ModId == modId && b.Source == listing.Source)) _store.RemoveBinding(modId, listing.Source);
            _store.UpsertBinding(new ModBinding(modId, listing.Source, listing.SourceId, listing.Url, Confirmed: true));
            _store.SetCheckingDeclined(modId, false);
            _store.SetDismissedUpdate(modId, null);
            if (verified)
            {
                // страницу только что открыли целиком: её версия и размер уже известны, обновление видно без проверки
                var t = new TrackedBinding(modId, listing.Source, listing.SourceId);
                UpdateCheckService.ApplyCard(_store, t, listing, _now());
            }
            return true;
        }
    }
}
