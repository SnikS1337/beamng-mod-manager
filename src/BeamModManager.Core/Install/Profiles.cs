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

    /// <summary>Применяет план; сбой одного файла не останавливает остальные.</summary>
    public static ProfileApplyResult Apply(ModInstaller installer, ProfilePlan plan)
    {
        var enabled = new List<string>(); var disabled = new List<string>(); var failed = new List<string>();
        foreach (var f in plan.ToEnable) (installer.Enable(f).Success ? enabled : failed).Add(f);
        foreach (var f in plan.ToDisable) (installer.Disable(f).Success ? disabled : failed).Add(f);
        return new ProfileApplyResult(enabled, disabled, failed, plan.Missing);
    }
}
