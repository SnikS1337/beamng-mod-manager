using BeamModManager.Core.Models;
using BeamModManager.Core.Updates;
using BeamModManager.Core.Versioning;

namespace BeamModManager.Core.Library;

/// <summary>Найденное обновление для одного установленного мода.</summary>
public sealed record UpdateFinding(
    string ModId,
    string ModName,
    string Source,
    UpdateStatus Status,
    string? InstalledVersion,
    string? LatestVersion,
    Uri Url)
{
    /// <summary>
    /// Что именно пользователь отклоняет: новая версия или, если версия неизвестна, состояние файла по размеру.
    /// Когда на сайте появится другая версия, ключ изменится и обновление покажется снова.
    /// </summary>
    public string DismissKey { get; init; } = LatestVersion ?? "";

    /// <summary>Пользователь пропустил это обновление (показывается только в списке «скрытые»).</summary>
    public bool IsDismissed { get; init; }
}

/// <summary>
/// Склеивает хранилище, планировщик и решение «есть обновление». Сеть сюда не входит:
/// слой выше получает ленты и карточки и передаёт сюда результат.
/// </summary>
public static class UpdateCheckService
{
    /// <summary>Привязки для проверки: только подтверждённые, с тем, что видели раньше.</summary>
    public static IReadOnlyList<TrackedBinding> Tracked(ILibraryStore store) =>
        store.Bindings().Where(b => b.Confirmed).Select(b =>
        {
            var c = store.GetCheck(b.ModId, b.Source);
            return new TrackedBinding(b.ModId, b.Source, b.SourceId, c?.LastCheckedUtc, c?.LastChangedUtc, c?.LastVersion, c?.LastSize);
        }).ToList();

    public static FeedPlan PlanFeeds(ILibraryStore store, DateTimeOffset now, UpdateCheckOptions? options = null) =>
        UpdateCheckPlanner.PlanFeeds(Tracked(store), store.SourceStatuses(), now, options);

    /// <summary>Лента подтвердила, что запись не менялась: обновляем только время проверки.</summary>
    public static void ApplyFeedUnchanged(ILibraryStore store, TrackedBinding b, DateTimeOffset now)
    {
        var c = store.GetCheck(b.ModId, b.Source);
        if (c is null) return; // без прошлых данных планировщик такие случаи не пропускает
        store.SaveCheck(c with { LastCheckedUtc = now });
    }

    /// <summary>Результат открытия карточки: запоминаем, что сайт показывает сейчас.</summary>
    public static void ApplyCard(ILibraryStore store, TrackedBinding b, Listing current, DateTimeOffset now)
    {
        var prev = store.GetCheck(b.ModId, b.Source);
        // Не разобранная версия (null) не затирает уже известную: сбой разбора не должен «забывать» данные.
        var version = current.ModVersion ?? prev?.LastVersion;
        var changed = prev is null
                      || (current.ModVersion is not null && prev.LastVersion != current.ModVersion)
                      || (prev.LastSize is not null && current.SizeBytes is not null && prev.LastSize != current.SizeBytes);
        store.SaveCheck(new CheckState(
            b.ModId, b.Source, now,
            changed ? now : prev!.LastChangedUtc,
            version,
            current.SizeBytes ?? prev?.LastSize));
    }

    /// <summary>
    /// Первое знакомство: версия известна из ленты, открывать карточку ради неё не нужно.
    /// Так первая проверка 200 модов обходится несколькими страницами ленты, а не 200 карточками.
    /// </summary>
    public static void ApplyFeedBaseline(ILibraryStore store, TrackedBinding b, FeedEntry entry, DateTimeOffset now)
    {
        var prev = store.GetCheck(b.ModId, b.Source);
        store.SaveCheck(new CheckState(b.ModId, b.Source, now, now, entry.Version, entry.SizeBytes ?? prev?.LastSize));
    }

    /// <summary>
    /// Список обновлений. Считается при чтении, поэтому установка новой версии убирает пункт сама.
    /// Отклонённые («пропустить эту версию») скрыты, пока на сайте не появится другая версия.
    /// </summary>
    public static IReadOnlyList<UpdateFinding> Pending(ILibraryStore store, bool includeDismissed = false)
    {
        var result = new List<UpdateFinding>();
        foreach (var mod in store.Mods())
        {
            var dismissed = store.GetDismissedUpdate(mod.Id);
            UpdateFinding? best = null;
            foreach (var b in store.Bindings().Where(x => x.ModId == mod.Id && x.Confirmed))
            {
                var c = store.GetCheck(mod.Id, b.Source);
                if (c is null) continue;
                var status = UpdateDecider.Decide(
                    new VersionSnapshot(mod.Version, mod.SizeBytes),
                    new VersionSnapshot(c.LastVersion, c.LastSize));
                if (status == UpdateStatus.None) continue;
                var key = c.LastVersion ?? $"size:{c.LastSize}";
                var isDismissed = dismissed is not null && string.Equals(dismissed, key, StringComparison.OrdinalIgnoreCase);
                if (isDismissed && !includeDismissed) continue;
                var f = new UpdateFinding(mod.Id, mod.Name, b.Source, status, mod.Version, c.LastVersion, b.Url) { DismissKey = key, IsDismissed = isDismissed };
                // Среди не скрытых берём более уверенное; скрытое не вытесняет нескрытое.
                if (best is null || (best.IsDismissed && !f.IsDismissed) || (best.IsDismissed == f.IsDismissed && status > best.Status))
                    best = f;
            }
            if (best is not null) result.Add(best);
        }
        return result;
    }

    /// <summary>«Пропустить эту версию».</summary>
    public static void Dismiss(ILibraryStore store, UpdateFinding finding) =>
        store.SetDismissedUpdate(finding.ModId, finding.DismissKey);

    /// <summary>Вернуть скрытое обновление в список.</summary>
    public static void Restore(ILibraryStore store, string modId) => store.SetDismissedUpdate(modId, null);
}
