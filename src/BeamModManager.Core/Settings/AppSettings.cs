using BeamModManager.Core.Updates;

namespace BeamModManager.Core.Settings;

/// <summary>
/// Фон за панелями: одна из пяти тем (решено 2026-10-15 вместо фотографии; «Лес» и «Графит» — 2026-10-17). Цвета — в
/// BeamModManager.Ui.Look.BackgroundThemes; номера сохраняются в файле, их не меняем.
/// </summary>
public enum BackgroundTheme { Night = 0, Deep = 1, Dusk = 2, Forest = 3, Graphite = 4 }

/// <summary>Оформление: тёмное (как на доске), светлое или как в Windows («Режим приложения» в параметрах цвета).</summary>
public enum UiMode { Dark = 0, Light = 1, System = 2 }

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

    /// <summary>
    /// Проверять при запуске, не вышла ли новая версия самой программы (по умолчанию да). Выключатель — в «Настройки →
    /// Обновления»; кнопка «Проверить сейчас» там же работает и при выключенной настройке.
    /// </summary>
    public bool CheckAppUpdates { get; init; } = true;

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

    // «Иней на стекле», «Сила инея» и «Искры льда» убраны 2026-10-16: иней выглядел грязью, искры теперь часть «Огоньков».
    // Старые поля Frost / FrostStrength / FrostSparks в файле настроек просто не читаются.

    /// <summary>«Оформление»: тёмное, светлое или как в Windows.</summary>
    public UiMode Mode { get; init; } = UiMode.Dark;

    /// <summary>«Фон»: какой градиент за панелями.</summary>
    public BackgroundTheme Theme { get; init; } = BackgroundTheme.Night;

    /// <summary>«Анимации интерфейса»: капля-выбор, появление и подъём карточек.</summary>
    public bool UiAnimations { get; init; } = true;

    /// <summary>«Анимированный фон»: по градиенту медленно плывут пятна света и цветная полоса.</summary>
    public bool AnimatedBackground { get; init; } = true;

    /// <summary>«Огоньки»: светлые точки над фоном и редкие искры на кромках стекла.</summary>
    public bool Fireflies { get; init; } = true;

    /// <summary>«Наклон фона за мышкой» (DESIGN_MOTION.md 3.8). По умолчанию выключен.</summary>
    public bool BackgroundTilt { get; init; }

    // ---- Прочее

    /// <summary>Предупреждать, когда включённых модов больше этого числа (по умолчанию 30, PLAN.md), 0–1000; 0 — не предупреждать.</summary>
    public int ActiveModsWarnAbove { get; init; } = 30;

    /// <summary>Подсказка первого запуска уже показана.</summary>
    public bool FirstRunDone { get; init; }

    /// <summary>На «Интерфейс работает медленно. Включить «Быстро»?» ответили «Оставить»: больше не спрашиваем (2026-10-18).</summary>
    public bool SlowOfferDeclined { get; init; }

    /// <summary>
    /// Куда ещё смотреть, кроме «Загрузок», при включённом «Разрешить загрузку модов» (2026-10-18: «Добавить папку» —
    /// своя папка браузера, Telegram Desktop и т. п.). Без подпапок. Не больше <see cref="MaxWatchFolders"/>.
    /// </summary>
    public IReadOnlyList<string> WatchFolders { get; init; } = Array.Empty<string>();

    public const int MaxWatchFolders = 10;

    /// <summary>Размер окна при закрытии (0 — не сохранён, окно открывается по умолчанию). Меньше минимума окна 1000 × 680
    /// (DESIGN_FLOWS.md 2) не бывает: такое значение считается мусором.</summary>
    public int WindowWidth { get; init; }
    public int WindowHeight { get; init; }
    public bool WindowMaximized { get; init; }

    /// <summary>Приводит значения к допустимым и убирает мусор. Возвращает исправленное.</summary>
    public AppSettings Normalize() => this with
    {
        Theme = Enum.IsDefined(Theme) ? Theme : BackgroundTheme.Night,
        Mode = Enum.IsDefined(Mode) ? Mode : UiMode.Dark,
        ActiveModsWarnAbove = Math.Clamp(ActiveModsWarnAbove, 0, 1000),
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
        WatchFolders = (WatchFolders ?? Array.Empty<string>()).Select(CleanFolder).OfType<string>()
            .Distinct(StringComparer.OrdinalIgnoreCase).Take(MaxWatchFolders).ToList(),
    };

    /// <summary>Папка без пробелов по краям и без косой черты в конце («C:\Mods\» и «C:\Mods» — одна); пустое — null.</summary>
    public static string? CleanFolder(string? s)
    {
        if (string.IsNullOrWhiteSpace(s)) return null;
        var t = s.Trim();
        while (t.Length > 3 && (t.EndsWith('\\') || t.EndsWith('/'))) t = t[..^1];
        return t;
    }

    private static string? Clean(string? s) => string.IsNullOrWhiteSpace(s) ? null : s.Trim();

    public bool IsSourceEnabled(string id) => !DisabledSources.Contains(id.ToLowerInvariant());

    public UpdateCheckOptions ToCheckOptions() => new()
    {
        Ttl = TimeSpan.FromHours(Math.Clamp(CheckTtlHours, 1, 168)),
        MaxCardChecksPerSource = Math.Clamp(MaxCardChecksPerSource, 5, 500),
    };
}
