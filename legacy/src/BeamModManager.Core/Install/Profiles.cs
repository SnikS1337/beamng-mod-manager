namespace BeamModManager.Core.Install;

/// <summary>Набор модов для одной «сборки»: перечисленные файлы включены, остальные отключены.</summary>
public sealed record ModProfile(string Name, IReadOnlySet<string> EnabledFiles);

public sealed record ProfilePlan(
    IReadOnlyList<string> ToEnable,
    IReadOnlyList<string> ToDisable,
    IReadOnlyList<string> Missing)
{
    public bool IsEmpty => ToEnable.Count == 0 && ToDisable.Count == 0;
}

public sealed record ProfileApplyResult(IReadOnlyList<string> Enabled, IReadOnlyList<string> Disabled, IReadOnlyList<string> Failed, IReadOnlyList<string> Missing);

public static class ProfileService
{
    /// <summary>Что изменится при применении профиля. Ничего не трогает.</summary>
    public static ProfilePlan Plan(ModProfile profile, IReadOnlyList<(string FileName, bool Enabled)> installed)
    {
        var wanted = new HashSet<string>(profile.EnabledFiles, StringComparer.OrdinalIgnoreCase);
        // Одно имя могло оказаться в списке дважды (разный регистр, копия во включённых и отключённых): без падения.
        var have = installed.GroupBy(x => x.FileName, StringComparer.OrdinalIgnoreCase)
            .ToDictionary(g => g.Key, g => g.Any(x => x.Enabled), StringComparer.OrdinalIgnoreCase);

        var enable = installed.Where(x => !x.Enabled && wanted.Contains(x.FileName) && !have[x.FileName]).Select(x => x.FileName).Distinct(StringComparer.OrdinalIgnoreCase).ToList();
        var disable = installed.Where(x => x.Enabled && !wanted.Contains(x.FileName)).Select(x => x.FileName).Distinct(StringComparer.OrdinalIgnoreCase).ToList();
        var missing = wanted.Where(w => !have.ContainsKey(w)).OrderBy(w => w, StringComparer.OrdinalIgnoreCase).ToList();
        return new ProfilePlan(enable, disable, missing);
    }

    /// <summary>
    /// Профиль хранит имена файлов, а имя меняется при обновлении («audi_v1.0.zip» → «audi_v1.5.zip»). Без переноса профиль
    /// счёл бы старый файл «пропавшим», а новый отключил бы. Переименования берутся из сверки библиотеки или из обновления.
    /// </summary>
    public static ModProfile ApplyRenames(ModProfile profile, IEnumerable<(string From, string To)> renames)
    {
        var map = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (var (from, to) in renames) map[from] = to;
        if (map.Count == 0) return profile;
        var files = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var f in profile.EnabledFiles)
        {
            // Цепочка a→b→c за один вызов тоже сворачивается; зацикливание ограничено числом шагов.
            var cur = f;
            for (var i = 0; i < map.Count && map.TryGetValue(cur, out var next) && !string.Equals(next, cur, StringComparison.OrdinalIgnoreCase); i++) cur = next;
            files.Add(cur);
        }
        return profile with { EnabledFiles = files };
    }

    /// <summary>Применяет план; сбой одного файла не останавливает остальные.</summary>
    public static ProfileApplyResult Apply(ModInstaller installer, ProfilePlan plan)
    {
        var enabled = new List<string>(); var disabled = new List<string>(); var failed = new List<string>();
        foreach (var f in plan.ToEnable) (installer.Enable(f).Success ? enabled : failed).Add(f);
        foreach (var f in plan.ToDisable) (installer.Disable(f).Success ? disabled : failed).Add(f);
        return new ProfileApplyResult(enabled, disabled, failed, plan.Missing);
    }
}
