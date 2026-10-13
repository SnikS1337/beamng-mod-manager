namespace BeamModManager.Core.Updates;

/// <summary>
/// «Докуда дошла лента». Если лента отсортирована по дате обновления (новые сверху) и у каждой записи есть дата,
/// то всё, чего в прочитанной части нет, обновлялось раньше даты последней записи. Тогда мод, который мы проверяли
/// после этой даты, гарантированно не менялся, и открывать его карточку не нужно.
/// Любое сомнение (нет дат, порядок нарушен, лента пуста) даёт «покрытия нет», и работает обычная проверка карточек.
/// </summary>
public static class FeedCoverage
{
    /// <summary>Первые записи ленты могут быть «закреплёнными» и стоять вне порядка.</summary>
    public const int PinnedAllowance = 5;

    /// <summary>Запас на часовые пояса и округление дат сайта.</summary>
    public static readonly TimeSpan Margin = TimeSpan.FromDays(1);

    /// <summary>Дата, до которой прочитанная лента надёжно покрывает изменения; null — покрытия нет.</summary>
    public static DateTimeOffset? Compute(IReadOnlyList<FeedEntry> ordered)
    {
        if (ordered.Count < 20) return null; // слишком мало записей, чтобы судить о порядке
        if (ordered.Any(e => e.Updated is null)) return null;

        // Порядок «от новых к старым»: после закреплённых записей даты не должны расти.
        DateTimeOffset? prev = null;
        for (var i = PinnedAllowance; i < ordered.Count; i++)
        {
            var d = ordered[i].Updated!.Value;
            if (prev is { } p && d > p + Margin) return null;
            prev = d;
        }

        // Самая молодая из дат в хвосте ленты: всё непрочитанное не новее неё (консервативно).
        return ordered.Skip(Math.Max(PinnedAllowance, ordered.Count - 10)).Max(e => e.Updated!.Value);
    }

    /// <summary>Привязка проверена достаточно давно после границы покрытия, чтобы считать её неизменной.</summary>
    public static bool Proves(DateTimeOffset coverage, DateTimeOffset? lastChecked, DateTimeOffset now) =>
        lastChecked is { } c && c <= now && c - coverage >= Margin;
}
