using BeamModManager.Core.Install;

namespace BeamModManager.Core.Library;

/// <summary>Прошлая версия мода: резервная копия и под каким именем файла она лежала.</summary>
public sealed record PastVersion(BackupEntry Backup, string Version);

/// <summary>
/// «Прошлые версии» мода: копии под текущим именем файла и под всеми прежними (после обновления имя обычно другое),
/// плюс возврат выбранной версии вместе с записью в библиотеке.
/// </summary>
public static class VersionHistory
{
    /// <summary>
    /// Все копии мода от новых к старым. Версия берётся из имени файла копии, если она там есть (иначе пусто).
    /// Копии ищутся под текущим именем, под прежними именами из библиотеки и под именами с тем же названием мода
    /// («car_v1.0.zip» для «car_v2.0.zip»): так прошлые версии видны, даже если история имён не сохранилась.
    /// Имя, которым сейчас называется файл другого мода, не берётся.
    /// </summary>
    public static IReadOnlyList<PastVersion> For(ModInstaller installer, ILibraryStore store, InstalledMod mod)
    {
        var mods = store.Mods();
        var otherCurrent = new HashSet<string>(mods.Where(m => m.Id != mod.Id).Select(m => m.FileName), StringComparer.OrdinalIgnoreCase);
        var key = LibrarySync.NameKey(mod.FileName);
        var names = new List<string>(store.FileHistory(mod.Id)) { mod.FileName };
        if (key.Length > 0)
            names.AddRange(installer.BackedUpFileNames().Where(n => !otherCurrent.Contains(n) && LibrarySync.NameKey(n) == key));
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var all = new List<PastVersion>();
        foreach (var name in names)
        {
            if (!seen.Add(name)) continue;
            var guessed = BindingSuggester.FromFileName(name).Version ?? "";
            foreach (var b in installer.ListBackups(name)) all.Add(new PastVersion(b, guessed));
        }
        // Порядковые номера считаются внутри папки одного имени файла, поэтому между именами сравниваем по времени.
        return all.OrderByDescending(p => p.Backup.SavedAtUtc ?? DateTimeOffset.MinValue).ThenByDescending(p => p.Backup.Sequence).ToList();
    }

    /// <summary>
    /// «Удалённые моды»: копии, которые не принадлежат ни одному моду из библиотеки. Прежние имена файлов (после обновления под
    /// новым именем) сюда не попадают: они в «Прошлых версиях» своего мода, иначе «Вернуть» поставило бы старую версию рядом с новой.
    /// </summary>
    public static IReadOnlyList<DeletedMod> Deleted(ModInstaller installer, ILibraryStore store)
    {
        // копий нет — и сверять не с чем: гараж зовёт это при каждом перечитывании, а ключи названий для тысяч модов не даром
        var raw = installer.DeletedMods();
        if (raw.Count == 0) return raw;
        var taken = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var keys = new HashSet<string>();
        foreach (var m in store.Mods())
        {
            taken.Add(m.FileName);
            foreach (var h in store.FileHistory(m.Id)) taken.Add(h);
            var k = LibrarySync.NameKey(m.FileName);
            if (k.Length > 0) keys.Add(k);
        }
        return raw.Where(d => !taken.Contains(d.FileName) && !keys.Contains(LibrarySync.NameKey(d.FileName))).ToList();
    }

    /// <summary>Возвращает выбранную версию: файл ставится на место текущего, запись в библиотеке следует за ним.</summary>
    public static OpResult Restore(ModInstaller installer, ILibraryStore store, InstalledMod mod, PastVersion version, DateTimeOffset now)
    {
        var r = installer.RestoreVersion(mod.FileName, version.Backup.FileName, version.Backup.Id);
        if (!r.Success || r.Path is null) return r;
        var restoredName = Path.GetFileName(r.Path);
        var disk = installer.ScanDetailed().FirstOrDefault(d => string.Equals(d.FileName, restoredName, StringComparison.OrdinalIgnoreCase));
        LibrarySync.RecordInstalled(store, restoredName, disk?.SizeBytes ?? version.Backup.SizeBytes, null, disk?.Enabled ?? mod.Enabled, now, replacesFileName: mod.FileName);
        return r;
    }

    /// <summary>Возвращает удалённый мод: копия встаёт в папку модов включённой, запись в библиотеке создастся при следующей сверке или сразу здесь.</summary>
    public static OpResult RestoreDeleted(ModInstaller installer, ILibraryStore store, DeletedMod deleted, DateTimeOffset now, string? backupId = null)
    {
        var id = backupId ?? installer.ListBackups(deleted.FileName).FirstOrDefault()?.Id;
        if (id is null) return new OpResult(false, "Резервных копий нет.");
        var r = installer.RestoreBackup(deleted.FileName, id);
        if (!r.Success || r.Path is null) return r;
        var name = Path.GetFileName(r.Path);
        var disk = installer.ScanDetailed().FirstOrDefault(d => string.Equals(d.FileName, name, StringComparison.OrdinalIgnoreCase));
        LibrarySync.RecordInstalled(store, name, disk?.SizeBytes ?? deleted.LastSizeBytes, null, disk?.Enabled ?? true, now);
        return r;
    }
}
