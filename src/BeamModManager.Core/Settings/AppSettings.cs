using BeamModManager.Core.Updates;

namespace BeamModManager.Core.Settings;

/// <summary>
/// Настройки менеджера. Любое неизвестное, повреждённое или отсутствующее значение заменяется безопасным
/// по умолчанию, в первую очередь загрузка модов: она включается только явным значением true в файле.
/// </summary>
public sealed record AppSettings
{
    public const int CurrentSchema = 1;

    public int SchemaVersion { get; init; } = CurrentSchema;

    /// <summary>«Разрешить загрузку модов». По умолчанию выключено: кнопки скачивания и окно ModsFire скрыты.</summary>
    public bool AllowDownloads { get; init; }

    /// <summary>Папка mods, которую указал пользователь (имеет приоритет).</summary>
    public string? ModsFolder { get; init; }

    /// <summary>Папка пользователя BeamNG (из неё подбирается mods последней версии), если ModsFolder не задана.</summary>
    public string? UserFolder { get; init; }

    /// <summary>Не проверять мод повторно, пока не прошло столько часов (1–168).</summary>
    public int CheckTtlHours { get; init; } = 6;

    /// <summary>Проверять обновления при запуске (по умолчанию да: сценарий «открыл раз в неделю, посмотрел, что обновилось»).</summary>
    public bool AutoCheckOnStart { get; init; } = true;

    /// <summary>Автопроверка раз в N часов; 0 — выключена (1–168).</summary>
    public int AutoCheckEveryHours { get; init; }

    /// <summary>Сайты, которые пользователь отключил (их не опрашивают ни в поиске, ни в проверке).</summary>
    public IReadOnlyList<string> DisabledSources { get; init; } = Array.Empty<string>();

    /// <summary>Сколько резервных копий каждого мода хранить (1–20).</summary>
    public int KeepBackups { get; init; } = 3;

    /// <summary>Сколько карточек открывать за одну проверку на сайт (5–500).</summary>
    public int MaxCardChecksPerSource { get; init; } = 150;

    /// <summary>Приводит значения к допустимым и убирает мусор. Возвращает исправленное.</summary>
    public AppSettings Normalize() => this with
    {
        SchemaVersion = CurrentSchema,
        ModsFolder = Clean(ModsFolder),
        UserFolder = Clean(UserFolder),
        CheckTtlHours = Math.Clamp(CheckTtlHours, 1, 168),
        AutoCheckEveryHours = AutoCheckEveryHours <= 0 ? 0 : Math.Clamp(AutoCheckEveryHours, 1, 168),
        KeepBackups = Math.Clamp(KeepBackups, 1, 20),
        MaxCardChecksPerSource = Math.Clamp(MaxCardChecksPerSource, 5, 500),
        DisabledSources = DisabledSources.Select(s => s?.Trim().ToLowerInvariant() ?? "")
            .Where(s => s.Length > 0).Distinct().OrderBy(s => s, StringComparer.Ordinal).ToList(),
    };

    private static string? Clean(string? s) => string.IsNullOrWhiteSpace(s) ? null : s.Trim();

    public bool IsSourceEnabled(string id) => !DisabledSources.Contains(id.ToLowerInvariant());

    public UpdateCheckOptions ToCheckOptions() => new()
    {
        Ttl = TimeSpan.FromHours(Math.Clamp(CheckTtlHours, 1, 168)),
        MaxCardChecksPerSource = Math.Clamp(MaxCardChecksPerSource, 5, 500),
    };
}
