namespace BeamModManager.Core.Packs;

/// <summary>Установленный мод и то, что в нём лежит.</summary>
public sealed record InstalledContent(string FileName, bool Enabled, ArchiveContent Content);

/// <summary>Пересечение одного установленного мода с набором (паком).</summary>
public sealed record OverlapItem(string FileName, bool Enabled, IReadOnlyList<string> Shared, bool FullyInsidePack);

public enum PackAdvice
{
    /// <summary>Совпадений нет: ставим как обычно.</summary>
    InstallFreely,
    /// <summary>Всё, что есть в паке, уже стоит отдельными модами: пак ничего не добавит.</summary>
    NothingNew,
    /// <summary>Часть машин уже стоит; новое в паке есть. Решает пользователь.</summary>
    PartialOverlap,
}

public sealed record PackOverlapReport(
    IReadOnlyList<string> PackItems,
    IReadOnlyList<string> AlreadyInstalled,
    IReadOnlyList<string> New,
    IReadOnlyList<OverlapItem> Overlaps,
    PackAdvice Advice)
{
    /// <summary>Отдельные включённые моды, целиком содержащиеся в паке: их можно выключить, если оставляем пак.</summary>
    public IReadOnlyList<string> RedundantSingles =>
        Overlaps.Where(o => o.Enabled && o.FullyInsidePack).Select(o => o.FileName).ToList();
}

public static class PackOverlapAnalyzer
{
    /// <summary>
    /// Сравнивает содержимое нового архива с установленными модами.
    /// Учитываются только включённые моды для «уже стоит»; выключенные показываются в Overlaps, но не считаются установленными.
    /// </summary>
    public static PackOverlapReport Analyze(ArchiveContent candidate, IEnumerable<InstalledContent> installed)
    {
        var packItems = candidate.Keys.OrderBy(x => x, StringComparer.Ordinal).ToList();
        var packSet = packItems.ToHashSet(StringComparer.Ordinal);

        var overlaps = new List<OverlapItem>();
        var covered = new HashSet<string>(StringComparer.Ordinal);
        foreach (var m in installed)
        {
            var keys = m.Content.Keys.ToList();
            var shared = keys.Where(packSet.Contains).OrderBy(x => x, StringComparer.Ordinal).ToList();
            if (shared.Count == 0) continue;
            overlaps.Add(new OverlapItem(m.FileName, m.Enabled, shared, keys.Count > 0 && keys.All(packSet.Contains)));
            if (m.Enabled) foreach (var s in shared) covered.Add(s);
        }

        var already = packItems.Where(covered.Contains).ToList();
        var fresh = packItems.Where(x => !covered.Contains(x)).ToList();
        var advice = packItems.Count == 0 || already.Count == 0 && overlaps.Count == 0 ? PackAdvice.InstallFreely
            : fresh.Count == 0 ? PackAdvice.NothingNew
            : already.Count == 0 ? PackAdvice.InstallFreely
            : PackAdvice.PartialOverlap;
        return new PackOverlapReport(packItems, already, fresh, overlaps.OrderBy(o => o.FileName, StringComparer.OrdinalIgnoreCase).ToList(), advice);
    }

    /// <summary>
    /// Две включённые копии одной машины (например, пак и отдельный мод): что именно дублируется.
    /// Нужен для предупреждения в библиотеке, пока не проверено, как игра ведёт себя при дубле.
    /// </summary>
    public static IReadOnlyDictionary<string, IReadOnlyList<string>> FindDuplicates(IEnumerable<InstalledContent> installed)
    {
        var map = new Dictionary<string, List<string>>(StringComparer.Ordinal);
        foreach (var m in installed.Where(i => i.Enabled))
            foreach (var k in m.Content.Keys)
            {
                if (!map.TryGetValue(k, out var l)) map[k] = l = new List<string>();
                l.Add(m.FileName);
            }
        return map.Where(kv => kv.Value.Count > 1)
            .ToDictionary(kv => kv.Key, kv => (IReadOnlyList<string>)kv.Value.OrderBy(x => x, StringComparer.OrdinalIgnoreCase).ToList(), StringComparer.Ordinal);
    }
}
