using BeamModManager.Core.Install;
using BeamModManager.Core.Text;
using BeamModManager.Core.Versioning;

namespace BeamModManager.Core.Library;

public sealed record SyncReport(
    IReadOnlyList<string> Added,
    IReadOnlyList<string> Removed,
    IReadOnlyList<string> Changed,
    IReadOnlyList<(string From, string To)> Renamed,
    string? SkippedReason)
{
    public bool Skipped => SkippedReason is not null;
    public bool AnyChange => Added.Count + Removed.Count + Changed.Count + Renamed.Count > 0;
}

/// <summary>
/// Связывает библиотеку с тем, что реально лежит в папке модов. Без этого запись о моде устаревает:
/// обновили мод (вручную или через менеджер), удалили, переименовали, отключили из игры, — а «обновление» висит вечно
/// или привязки пропадают. Запись в библиотеке всегда подстраивается под диск, а не наоборот.
/// </summary>
public static class LibrarySync
{
    // Сверка и запись «установлено» не должны перемешиваться: у обеих несколько обращений к хранилищу.
    private static readonly object Gate = new();

    /// <summary>Ключ мода — имя файла без учёта регистра (Windows их не различает).</summary>
    public static string KeyOf(string fileName) => fileName.ToLowerInvariant();

    public static SyncReport Reconcile(ILibraryStore store, ModInstaller installer)
    {
        lock (Gate)
        {
            // Снимок и применение под одной блокировкой: параллельная «запись установленного» не затрёт свежее старым снимком.
            var available = installer.ModsFolderAvailable;
            return ReconcileCore(store, available ? installer.ScanDetailed() : Array.Empty<DiskMod>(), available);
        }
    }

    public static SyncReport Reconcile(ILibraryStore store, IReadOnlyList<DiskMod> disk, bool folderAvailable = true)
    {
        lock (Gate) return ReconcileCore(store, disk, folderAvailable);
    }

    private static SyncReport ReconcileCore(ILibraryStore store, IReadOnlyList<DiskMod> disk, bool folderAvailable)
    {
        var none = Array.Empty<string>();
        var known = store.Mods();

        // Папка недоступна (диск отключён, переименована) или пуста при непустой библиотеке: это скорее сбой, чем «удалили всё».
        // Стирать привязки по такому снимку нельзя: пользователь потерял бы всё, что настраивал вручную.
        if (!folderAvailable)
            return new SyncReport(none, none, none, Array.Empty<(string, string)>(), "Папка модов недоступна.");
        if (disk.Count == 0 && known.Count > 0)
            return new SyncReport(none, none, none, Array.Empty<(string, string)>(), "Папка модов пуста, а в библиотеке есть моды: изменения не применены.");

        // Один и тот же файл мог оказаться и во включённых, и в отключённых: берём включённый.
        var onDisk = new Dictionary<string, DiskMod>();
        foreach (var d in disk.OrderByDescending(x => x.Enabled))
            onDisk.TryAdd(KeyOf(d.FileName), d);

        var byFile = new Dictionary<string, InstalledMod>();
        foreach (var m in known) byFile.TryAdd(KeyOf(m.FileName), m); // две записи на один файл: берём первую, вторую уберёт «исчезнувшие»

        var added = new List<string>(); var removed = new List<string>(); var changed = new List<string>();
        var renamed = new List<(string, string)>();

        // 1. Изменения у тех, что на месте.
        foreach (var (key, d) in onDisk)
        {
            if (!byFile.TryGetValue(key, out var mod)) continue;
            var updated = mod;
            if (mod.Enabled != d.Enabled) updated = updated with { Enabled = d.Enabled };
            if (mod.SizeBytes != d.SizeBytes)
            {
                updated = updated with { SizeBytes = d.SizeBytes, Version = VersionAfterExternalChange(store, mod, d) };
            }
            if (updated != mod)
            {
                store.UpsertMod(updated);
                changed.Add(d.FileName);
            }
        }

        // 2. Исчезнувшие и новые; пара «исчез + появился с тем же названием» — переименование (чаще всего: вручную поставили новую версию).
        var vanished = known.Where(m => !onDisk.ContainsKey(KeyOf(m.FileName)) || !ReferenceEquals(byFile[KeyOf(m.FileName)], m)).ToList();
        var fresh = onDisk.Where(kv => !byFile.ContainsKey(kv.Key)).Select(kv => kv.Value).OrderBy(d => d.FileName, StringComparer.OrdinalIgnoreCase).ToList();

        var vanishedByName = vanished.GroupBy(m => NameKey(m.FileName)).ToDictionary(g => g.Key, g => g.ToList());
        var freshByName = fresh.GroupBy(d => NameKey(d.FileName)).ToDictionary(g => g.Key, g => g.ToList());
        var handledFresh = new HashSet<string>();
        var handledVanished = new HashSet<string>();

        foreach (var (nk, vs) in vanishedByName)
        {
            if (nk.Length == 0 || vs.Count != 1) continue;
            if (!freshByName.TryGetValue(nk, out var fs) || fs.Count != 1) continue; // неоднозначно: не гадаем, привязки не переносим
            var old = vs[0]; var nw = fs[0];
            MigrateMod(store, old, nw);
            renamed.Add((old.FileName, nw.FileName));
            handledFresh.Add(KeyOf(nw.FileName)); handledVanished.Add(old.Id);
        }

        foreach (var m in vanished)
        {
            if (handledVanished.Contains(m.Id)) continue;
            store.RemoveMod(m.Id);
            removed.Add(m.FileName);
        }

        foreach (var d in fresh)
        {
            if (handledFresh.Contains(KeyOf(d.FileName))) continue;
            store.UpsertMod(NewMod(store, d, id: null));
            added.Add(d.FileName);
        }

        return new SyncReport(added, removed, changed, renamed, null);
    }

    /// <summary>
    /// Менеджер поставил файл: запись обновляется сразу. Версия берётся из того, что менеджер знает (страница сайта),
    /// иначе из имени файла, иначе неизвестна: старую версию не оставляем, она была бы неверной.
    /// </summary>
    public static InstalledMod RecordInstalled(ILibraryStore store, string fileName, long sizeBytes, string? version, bool enabled, DateTimeOffset now)
    {
        lock (Gate)
        {
            var key = KeyOf(fileName);
            var existing = store.Mods().FirstOrDefault(m => KeyOf(m.FileName) == key);
            var guessed = BindingSuggester.FromFileName(fileName);
            var v = !string.IsNullOrWhiteSpace(version) ? version : guessed.Version;
            var mod = existing is null
                ? NewMod(store, new DiskMod(fileName, enabled, sizeBytes), id: null, now) with { Version = v }
                : existing with { FileName = fileName, Version = v, SizeBytes = sizeBytes, Enabled = enabled };
            store.UpsertMod(mod);
            return mod;
        }
    }

    private static string? VersionAfterExternalChange(ILibraryStore store, InstalledMod mod, DiskMod d)
    {
        // Версия в имени файла — самый прямой признак.
        var fromName = BindingSuggester.FromFileName(d.FileName).Version;
        if (fromName is not null) return fromName;

        // Файл заменили вручную, имя то же. Если теперь размер совпадает с тем, что показывает сайт, — это та версия.
        foreach (var b in store.Bindings().Where(x => x.ModId == mod.Id && x.Confirmed))
        {
            var c = store.GetCheck(mod.Id, b.Source);
            if (c?.LastVersion is not null && c.LastSize is not null && !SizeComparer.Differs(d.SizeBytes, c.LastSize))
                return c.LastVersion;
        }
        // Иначе старая версия точно неверна: честнее «неизвестно», дальше решит размер.
        return null;
    }

    private static string NameKey(string fileName) => TitleNormalizer.Normalize(BindingSuggester.FromFileName(fileName).Name);

    private static InstalledMod NewMod(ILibraryStore store, DiskMod d, string? id, DateTimeOffset? now = null)
    {
        var g = BindingSuggester.FromFileName(d.FileName);
        var baseId = id ?? KeyOf(d.FileName);
        var final = baseId;
        for (var i = 2; store.GetMod(final) is not null; i++) final = $"{baseId}#{i}";
        var name = string.IsNullOrWhiteSpace(g.Name) ? Path.GetFileNameWithoutExtension(d.FileName) : g.Name;
        return new InstalledMod(final, name, d.FileName, g.Version, d.SizeBytes, now ?? DateTimeOffset.UtcNow, d.Enabled);
    }

    private static void MigrateMod(ILibraryStore store, InstalledMod old, DiskMod nw)
    {
        var created = NewMod(store, nw, id: null);
        // Время установки и имя, которое пользователь мог поправить, сохраняем.
        var mod = created with { Name = old.Name, InstalledAtUtc = old.InstalledAtUtc };
        store.UpsertMod(mod);
        foreach (var b in store.Bindings().Where(x => x.ModId == old.Id))
        {
            store.UpsertBinding(b with { ModId = mod.Id });
            var c = store.GetCheck(old.Id, b.Source);
            if (c is not null) store.SaveCheck(c with { ModId = mod.Id });
        }
        var dismissed = store.GetDismissedUpdate(old.Id);
        if (dismissed is not null) store.SetDismissedUpdate(mod.Id, dismissed);
        store.RemoveMod(old.Id);
    }
}
