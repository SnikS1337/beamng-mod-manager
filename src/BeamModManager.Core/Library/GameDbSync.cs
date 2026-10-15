using BeamModManager.Core.Game;
using BeamModManager.Core.Install;
using BeamModManager.Core.Sources.BeamNg;

namespace BeamModManager.Core.Library;

/// <summary>Чем закончилось «Включить» / «Выключить».</summary>
public sealed record ToggleResult(bool Success, ToggleWay Way, string Message);

public enum ToggleWay
{
    /// <summary>Флажок active в db.json, как это делает сама игра (основной способ, решено 2026-10-14).</summary>
    GameDb,
    /// <summary>Файл перенесён в папку менеджера или обратно (запасной способ: игра этот файл ещё не видела).</summary>
    MoveFile,
    /// <summary>Ничего делать не нужно: новый файл игра при следующем запуске включит сама.</summary>
    None,
    /// <summary>Не получилось.</summary>
    Failed,
}

/// <summary>
/// Связь библиотеки менеджера с db.json игры: включён ли мод, данные модов из репозитория BeamNG, включение и выключение.
/// Формат и поведение игры проверены на 0.39.4 (PHASE0.md «Данные с Windows»): мод, положенный в mods, игра при
/// следующем запуске добавляет в db.json включённым; выключение в игре меняет только active, файл остаётся на месте.
/// </summary>
public static class GameDbSync
{
    /// <summary>Снимок папки с учётом db.json: архив в mods считается выключенным, если игра записала active: false.</summary>
    public static IReadOnlyList<DiskMod> Overlay(IReadOnlyList<DiskMod> disk, GameDbReadResult db) =>
        disk.Select(d => d.Enabled && db.Find(d.FileName) is { } e && e.IsAt(d.SubFolder) && !e.Active ? d with { Enabled = false } : d).ToList();

    /// <summary>Сверка библиотеки с папкой и db.json разом.</summary>
    public static SyncReport Reconcile(ILibraryStore store, ModInstaller installer) =>
        Reconcile(store, installer, GameModDb.ReadStable(GameModDb.PathIn(installer.Layout.ModsDir)));

    /// <summary>
    /// Сверка с уже прочитанным db.json. Если он не прочитался (игра как раз пишет), «включён ли мод» берётся из библиотеки,
    /// а не из пустого списка: иначе все выключенные на мгновение показались бы включёнными.
    /// </summary>
    public static SyncReport Reconcile(ILibraryStore store, ModInstaller installer, GameDbReadResult db)
    {
        var disk = installer.ScanDetailed();
        IReadOnlyList<DiskMod> view;
        if (db.Unreadable)
        {
            var known = store.Mods().ToDictionary(m => m.FileName, StringComparer.OrdinalIgnoreCase);
            view = disk.Select(d => d.Enabled && known.TryGetValue(d.FileName, out var m) && !m.Enabled ? d with { Enabled = false } : d).ToList();
        }
        else view = Overlay(disk, db);
        var report = LibrarySync.Reconcile(store, view, installer.ModsFolderAvailable);
        // мод, который уже лежал в папке: «Установлен» — когда его добавила игра, а не день первого запуска менеджера
        foreach (var f in report.Added)
            if (db.Find(f)?.DateAdded is { } added &&
                store.Mods().FirstOrDefault(m => string.Equals(m.FileName, f, StringComparison.OrdinalIgnoreCase)) is { } m && added < m.InstalledAtUtc)
                store.UpsertMod(m with { InstalledAtUtc = added });
        return report;
    }

    /// <summary>
    /// Моды, собранные репозиторием BeamNG (в db.json есть modID и modData): версия берётся с сайта, а не из имени файла;
    /// название — с сайта, если пользователь его не менял; привязка к странице на beamng.com ставится сразу подтверждённой.
    /// Своя привязка пользователя к этому же сайту и отказ «Не проверять этот мод» не трогаются. Возвращает число новых привязок.
    /// </summary>
    public static int ApplyRepoInfo(ILibraryStore store, GameDbReadResult db)
    {
        var added = 0;
        foreach (var mod in store.Mods())
        {
            if (db.Find(mod.FileName)?.Repo is not { } r) continue;
            var updated = mod;
            if (r.Version is not null && mod.Version != r.Version) updated = updated with { Version = r.Version };
            if (r.Title is not null && mod.Name != r.Title && IsGuessedName(mod)) updated = updated with { Name = r.Title };
            if (updated != mod) store.UpsertMod(updated);

            if (r.ResourceId is not { } rid || r.PageUrl is not { } url || store.IsCheckingDeclined(mod.Id)) continue;
            if (store.Bindings().Any(b => b.ModId == mod.Id && b.Source == BeamNgRepoParser.SourceId && b.Confirmed)) continue;
            store.UpsertBinding(new ModBinding(mod.Id, BeamNgRepoParser.SourceId, rid.ToString(System.Globalization.CultureInfo.InvariantCulture), url, true));
            added++;
        }
        return added;
    }

    private static bool IsGuessedName(InstalledMod mod)
    {
        var g = BindingSuggester.FromFileName(mod.FileName).Name;
        return string.Equals(mod.Name, string.IsNullOrWhiteSpace(g) ? Path.GetFileNameWithoutExtension(mod.FileName) : g, StringComparison.Ordinal);
    }

    /// <summary>
    /// «Включить» / «Выключить». Основной способ — флажок в db.json (только при закрытой игре). Если игра этот архив ещё не
    /// видела: выключение переносит файл в папку менеджера (иначе игра включила бы его сама), включение ничего не требует.
    /// Архив, выключенный раньше переносом, при включении возвращается в mods.
    /// </summary>
    public static ToggleResult SetEnabled(ModInstaller installer, string fileName, bool enabled, Func<bool>? gameRunning = null)
    {
        var name = ModInstaller.SafeFileName(fileName);
        if (name is null) return new(false, ToggleWay.Failed, "Недопустимое имя файла.");
        var dbPath = GameModDb.PathIn(installer.Layout.ModsDir);
        var onDisk = installer.ScanDetailed().FirstOrDefault(d => string.Equals(d.FileName, name, StringComparison.OrdinalIgnoreCase));
        if (onDisk is null) return new(false, ToggleWay.Failed, "Файл мода не найден.");
        gameRunning ??= GameProcess.IsRunning;

        if (enabled && !onDisk.Enabled)
        {
            // выключен раньше переносом: возвращаем файл; флажок в db.json тоже включаем, если игра его помнит
            var moved = installer.Enable(onDisk.FileName);
            if (!moved.Success) return new(false, ToggleWay.Failed, moved.Message ?? "Не удалось вернуть файл.");
            var r = GameModDb.SetActive(dbPath, onDisk.FileName, true, gameRunning);
            return r.Outcome == SetActiveOutcome.GameRunning
                ? new(true, ToggleWay.MoveFile, "Включён. Изменения применятся при следующем запуске игры.")
                : new(true, ToggleWay.MoveFile, "Включён.");
        }

        var res = GameModDb.SetActive(dbPath, onDisk.FileName, enabled, gameRunning);
        switch (res.Outcome)
        {
            case SetActiveOutcome.Done:
            case SetActiveOutcome.Unchanged:
                return new(true, ToggleWay.GameDb, enabled ? "Включён." : "Выключен.");
            case SetActiveOutcome.NotInDb:
            case SetActiveOutcome.NoDb:
                if (enabled) return new(true, ToggleWay.None, "Включён: игра подхватит его при запуске.");
                // папкой репозитория управляет игра: файлы оттуда не уносим
                if (onDisk.FromGameRepo) return new(false, ToggleWay.Failed, "Игра ещё не знает этот мод. Запустите игру один раз, потом выключите.");
                var mv = installer.Disable(onDisk.FileName);
                return mv.Success
                    ? new(true, ToggleWay.MoveFile, "Выключен. Игра ещё не видела этот мод, поэтому файл убран в папку менеджера.")
                    : new(false, ToggleWay.Failed, mv.Message ?? "Не удалось выключить.");
            case SetActiveOutcome.GameRunning:
                return new(false, ToggleWay.Failed, "Закройте игру: пока она запущена, список модов она держит у себя.");
            default:
                return new(false, ToggleWay.Failed, res.Message ?? "Не удалось изменить список модов игры.");
        }
    }

    /// <summary>
    /// После обновления под новым именем файла: если прежняя версия была выключена в игре, новая тоже должна остаться
    /// выключенной. Игра новый файл ещё не видела, поэтому он убирается в папку менеджера (запасной способ).
    /// </summary>
    public static ToggleResult KeepDisabledAfterUpdate(ModInstaller installer, GameDbReadResult dbBeforeUpdate, string oldFileName, string newFileName)
    {
        if (string.Equals(oldFileName, newFileName, StringComparison.OrdinalIgnoreCase)) return new(true, ToggleWay.None, "Имя то же: игра сохранит состояние.");
        if (dbBeforeUpdate.Find(oldFileName) is not { Active: false }) return new(true, ToggleWay.None, "Прежняя версия была включена.");
        var mv = installer.Disable(newFileName);
        return mv.Success ? new(true, ToggleWay.MoveFile, "Новая версия оставлена выключенной, как прежняя.") : new(false, ToggleWay.Failed, mv.Message ?? "Не удалось выключить новую версию.");
    }
}
