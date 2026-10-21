using BeamModManager.Core.Library;
using BeamModManager.Core.Matching;
using BeamModManager.Core.Models;
using BeamModManager.Core.Sources;
using BeamModManager.Ui.Updates;

namespace BeamModManager.App.Demo;

/// <summary>
/// «Обновления» для снимков на сборочной машине: сайты не опрашиваются, проверка заканчивается сразу, найденное — то,
/// что уже лежит в демо-гараже (как на доске «Обновления»). Действия меняют демо-моды, чтобы окно вело себя живо.
/// </summary>
public sealed class DemoUpdatesBackend : IUpdatesBackend
{
    private readonly DemoBackend _garage;
    private readonly DateTimeOffset _last = DateTimeOffset.UtcNow.AddMinutes(-18);

    public DemoUpdatesBackend(DemoBackend garage)
    {
        _garage = garage;
        LastReport = Report();
    }

    private static UpdateRunReport Report() => new(Array.Empty<UpdateFinding>(), new[]
    {
        new SourceRunReport("modland", SourceStatus.Ok, 3, 6, 0, Array.Empty<string>(), null),
        new SourceRunReport("modsgaming", SourceStatus.Ok, 2, 1, 0, Array.Empty<string>(), null),
        new SourceRunReport("beamng", SourceStatus.Ok, 1, 0, 0, Array.Empty<string>(), null),
    }, 13, 9);

    public bool IsRunning => false;
    public UpdateRunReport? LastReport { get; private set; }

    public async Task<UpdateRunReport> CheckAsync(bool force, IProgress<UpdateProgress>? progress = null)
    {
        progress?.Report(new UpdateProgress("modland", "карточки", 3, 6));
        await Task.Delay(600);
        LastReport = Report();
        return LastReport;
    }

    public void Cancel() { }

    public IReadOnlyList<SiteInfo> Sites() => new SiteInfo[]
    {
        new("modland", "Modland", true, SourceStatus.Ok, 9, new Uri("https://www.modland.net/")),
        new("modsgaming", "ModsGaming", true, SourceStatus.Ok, 2, new Uri("https://modsgaming.ru/")),
        new("stmods", "STMods", true, SourceStatus.Ok, 0, new Uri("https://stmods.org/")),
        new("beamng", "Репозиторий BeamNG", true, SourceStatus.Ok, 1, new Uri("https://beamng.com/")),
    };

    public DateTimeOffset? LastCheckedUtc() => _last;
    public (int Checked, int Total) Coverage(DateTimeOffset since) => (12, 12);

    public bool Dismiss(string modId)
    {
        if (_garage.Find(modId)?.Update is null) return false;
        _garage.Edit(modId, m => m with { Update = m.Update! with { IsDismissed = true } });
        return true;
    }

    public void Restore(string modId)
    {
        if (_garage.Find(modId)?.Update is null) return;
        _garage.Edit(modId, m => m with { Update = m.Update! with { IsDismissed = false } });
    }

    public DeclineUndo? Decline(string modId)
    {
        var m = _garage.Find(modId);
        if (m is null) return null;
        _garage.Edit(modId, x => x with { Binding = null, PageMissing = false, CheckingDeclined = true, Update = null });
        return new DeclineUndo(modId, m.Binding is null ? Array.Empty<ModBinding>() : new[] { m.Binding }, Array.Empty<CheckState>(), m.PageMissing ? "missing" : null);
    }

    public bool UndoDecline(DeclineUndo undo)
    {
        _garage.Edit(undo.ModId, x => x with { Binding = undo.Bindings.FirstOrDefault(), PageMissing = undo.DismissedUpdate == "missing", CheckingDeclined = false });
        return true;
    }

    public void ResolveHumanCheck(string source) { }

    public Task<IReadOnlyList<BindingSuggestion>> SuggestAsync(string modId, bool replacing, CancellationToken ct = default)
    {
        var name = _garage.Find(modId)?.Mod.Name ?? "Мод";
        BindingSuggestion S(string src, string host, string title, string ver, int mb, string? author = null) =>
            new(new Listing(src, new Uri($"https://{host}/{Uri.EscapeDataString(title)}"), title, title) { ModVersion = ver, SizeBytes = mb * 1024L * 1024, Author = author },
                new MatchResult(0.8, MatchVerdict.Maybe, "похоже"));
        IReadOnlyList<BindingSuggestion> list = new[]
        {
            S("modland", "www.modland.net", $"{name} v3.1", "3.1", 2, "rk_mods"),
            S("stmods", "stmods.org", $"{name} версия 3.0 для BeamNG.drive", "3.0", 2),
            S("modsgaming", "modsgaming.ru", $"{name} (Free)", "2.8", 3),
        };
        return Task.FromResult(list);
    }

    public Task<PageCheck> CheckPageAsync(string text, CancellationToken ct = default) =>
        Task.FromResult(new PageCheck(null, string.IsNullOrWhiteSpace(text) ? null : "В демо ссылки не проверяются"));

    public bool Bind(string modId, Listing listing, string? replaceSource, bool verified)
    {
        _garage.Edit(modId, m => m with { Binding = new ModBinding(modId, listing.Source, listing.SourceId, listing.Url, true), PageMissing = false, CheckingDeclined = false });
        return true;
    }
}
