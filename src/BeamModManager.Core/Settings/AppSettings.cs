using BeamModManager.Core.Updates;

namespace BeamModManager.Core.Settings;

/// <summary>«Что за панелями»: фото, размытое фото, один цвет.</summary>
public enum BackgroundMode { Photo = 0, BlurredPhoto = 1, SolidColor = 2 }

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

    // ---- Внешний вид (DESIGN.md, раздел 5.1). Всё независимо друг от друга; зависимые пункты показываются тусклыми, но значения хранятся.

    /// <summary>«Жидкое стекло»: края панелей преломляют фон. Выключено: обычное матовое стекло.</summary>
    public bool LiquidGlass { get; init; } = true;

    /// <summary>«Иней на стекле».</summary>
    public bool Frost { get; init; } = true;

    /// <summary>«Сила инея»: 0 слабо, 1 средне, 2 сильно.</summary>
    public int FrostStrength { get; init; } = 1;

    /// <summary>«Искры льда»: блики на кромках. Работают только при включённом инее.</summary>
    public bool FrostSparks { get; init; } = true;

    /// <summary>«Что за панелями»: фото, размытое фото или один цвет.</summary>
    public BackgroundMode Background { get; init; } = BackgroundMode.Photo;

    /// <summary>«Затемнение фона», 0–100 процентов (по умолчанию 35).</summary>
    public int BackgroundDim { get; init; } = 35;

    /// <summary>«Анимации интерфейса»: капля-выбор, появление и подъём карточек.</summary>
    public bool UiAnimations { get; init; } = true;

    /// <summary>«Анимированный фон»: фото медленно меняет цвет.</summary>
    public bool AnimatedBackground { get; init; } = true;

    /// <summary>«Огоньки»: светлые точки над фоном.</summary>
    public bool Fireflies { get; init; } = true;

    /// <summary>«Наклон фона за мышкой» (DESIGN_MOTION.md 3.8). По умолчанию выключен.</summary>
    public bool BackgroundTilt { get; init; }

    // ---- Прочее

    /// <summary>Предупреждать, когда включённых модов больше этого числа (по умолчанию 30, PLAN.md), 1–1000.</summary>
    public int ActiveModsWarnAbove { get; init; } = 30;

    /// <summary>Подсказка первого запуска уже показана.</summary>
    public bool FirstRunDone { get; init; }

    /// <summary>Размер окна при закрытии (0 — не сохранён, окно открывается по умолчанию). Меньше минимума окна 1000 × 680
    /// (DESIGN_FLOWS.md 2) не бывает: такое значение считается мусором.</summary>
    public int WindowWidth { get; init; }
    public int WindowHeight { get; init; }
    public bool WindowMaximized { get; init; }

    /// <summary>Приводит значения к допустимым и убирает мусор. Возвращает исправленное.</summary>
    public AppSettings Normalize() => this with
    {
        FrostStrength = Math.Clamp(FrostStrength, 0, 2),
        Background = Enum.IsDefined(Background) ? Background : BackgroundMode.Photo,
        BackgroundDim = Math.Clamp(BackgroundDim, 0, 100),
        ActiveModsWarnAbove = Math.Clamp(ActiveModsWarnAbove, 1, 1000),
        WindowWidth = WindowWidth is >= 1000 and <= 20000 ? WindowWidth : 0,       // обрывок или мусор: окно откроется по умолчанию
        WindowHeight = WindowHeight is >= 680 and <= 20000 ? WindowHeight : 0,
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
