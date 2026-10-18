using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources.BeamNg;
using BeamModManager.Core.Sources.Modland;
using BeamModManager.Core.Sources.ModsGaming;
using BeamModManager.Core.Sources.StMods;
using BeamModManager.Ui.Look;
using BeamModManager.Ui.Mvvm;

namespace BeamModManager.Ui.Settings;

/// <summary>Разделы экрана «Настройки» (DESIGN.md 5.1), в порядке списка слева.</summary>
public enum SettingsSection { Appearance, Downloads, Folders, Updates, Sites }

/// <summary>Сайт в разделе «Сайты»: включатель пишет в <see cref="AppSettings.DisabledSources"/>.</summary>
public sealed class SiteToggle : Observable
{
    private readonly SettingsViewModel _owner;
    internal SiteToggle(SettingsViewModel owner, string id, string name, string text)
    {
        _owner = owner; Id = id; Name = name; Text = text;
    }

    public string Id { get; }
    public string Name { get; }
    public string Text { get; }

    public bool Enabled
    {
        get => _owner.Current.IsSourceEnabled(Id);
        set => _owner.SetSource(Id, value);
    }

    internal void Changed() => Raise(nameof(Enabled));
}

/// <summary>Плитка темы фона: выбор ставит тему, снятие отметки ничего не делает (как у переключателей-сегментов).</summary>
public sealed class ThemeTile : Observable
{
    private readonly SettingsViewModel _owner;

    private readonly BackgroundTheme _theme;
    internal ThemeTile(SettingsViewModel owner, BackgroundTheme theme) { _owner = owner; _theme = theme; }

    /// <summary>Палитра темы в текущем оформлении: плитка показывает именно то, что будет за панелями.</summary>
    public ThemePalette Palette => BackgroundThemes.Of(_theme, _owner.EffectiveLight);
    public string Name => Palette.Name;
    public string Note => Palette.Note;

    public bool IsSelected
    {
        get => _owner.Current.Theme == _theme;
        set { if (value) _owner.Theme = _theme; }
    }

    internal void Changed() { Raise(nameof(IsSelected)); Raise(nameof(Palette)); Raise(nameof(Name)); Raise(nameof(Note)); }
}

/// <summary>Пункт списка разделов слева.</summary>
public sealed record SectionItem(SettingsSection Section, string Title);

/// <summary>
/// Экран «Настройки» без окна. Всё применяется сразу, без кнопки «Сохранить»: каждое изменение — новое
/// <see cref="Current"/> и событие <see cref="Changed"/> (окно перерисовывает вид, программа пишет файл).
/// Значения проходят <see cref="AppSettings.Normalize"/>, поэтому мусор из окна в файл не попадает.
/// </summary>
public sealed class SettingsViewModel : Observable
{
    private AppSettings _current;

    public SettingsViewModel(AppSettings initial)
    {
        _current = initial.Normalize();
        Themes = Enum.GetValues<BackgroundTheme>().Select(t => new ThemeTile(this, t)).ToList();
        Sites = new[]
        {
            new SiteToggle(this, ModlandParser.SourceId, "Modland", "Большой каталог модов."),
            new SiteToggle(this, ModsGamingParser.SourceId, "ModsGaming", "Каталог на русском."),
            new SiteToggle(this, StModsParser.SourceId, "STMods", "Каталог модов."),
            new SiteToggle(this, BeamNgRepoParser.SourceId, "Репозиторий BeamNG", "Официальный каталог и обновления от авторов."),
        };
    }

    /// <summary>Настройки целиком; меняются через свойства ниже или <see cref="Update"/>.</summary>
    public AppSettings Current => _current;

    /// <summary>Что-то поменялось: новое значение уже в <see cref="Current"/>.</summary>
    public event Action<AppSettings>? Changed;

    /// <summary>
    /// Изменить настройки снаружи (выбрали папку, сохранили размер окна). Одинаковое значение ничего не вызывает:
    /// повторные нажатия и ползунок, вернувшийся на место, не пишут файл зря.
    /// </summary>
    public void Update(Func<AppSettings, AppSettings> change)
    {
        var next = change(_current).Normalize();
        if (SameAs(next, _current)) return;
        _current = next;
        Raise(string.Empty); // все привязки окна перечитываются: зависимые пункты, набор, подписи
        foreach (var s in Sites) s.Changed();
        foreach (var t in Themes) t.Changed();
        Changed?.Invoke(next);
    }

    private static bool SameAs(AppSettings a, AppSettings b) =>
        a with { DisabledSources = Array.Empty<string>(), WatchFolders = Array.Empty<string>() } ==
        b with { DisabledSources = Array.Empty<string>(), WatchFolders = Array.Empty<string>() } &&
        a.DisabledSources.SequenceEqual(b.DisabledSources) &&
        a.WatchFolders.SequenceEqual(b.WatchFolders, StringComparer.OrdinalIgnoreCase);

    // ───────────── разделы ─────────────

    public IReadOnlyList<SectionItem> Sections { get; } = new SectionItem[]
    {
        new(SettingsSection.Appearance, "Внешний вид"),
        new(SettingsSection.Downloads, "Загрузка модов"),
        new(SettingsSection.Folders, "Папки"),
        new(SettingsSection.Updates, "Обновления"),
        new(SettingsSection.Sites, "Сайты"),
    };

    private SettingsSection _section;
    public SettingsSection Section
    {
        get => _section;
        set { if (Set(ref _section, value)) Raise(nameof(SectionTitle)); }
    }

    public string SectionTitle => Sections.First(s => s.Section == _section).Title;

    // ───────────── внешний вид ─────────────

    /// <summary>Выставленный набор или null, если переключатели движения поменяли вручную.</summary>
    public AppearancePreset? Preset => AppearancePresets.Detect(_current);

    // Плитки наборов — они же выбор стекла (отдельный включатель «Жидкое стекло» убран 2026-10-17: выключить его и было
    // «Быстро»). Отмечена та, чьё стекло стоит; выбор плитки выставляет набор целиком, снятие отметки ничего не делает.
    public bool IsBeautiful { get => _current.LiquidGlass; set { if (value) ApplyPreset(AppearancePreset.Beautiful); } }
    public bool IsFast { get => !_current.LiquidGlass; set { if (value) ApplyPreset(AppearancePreset.Fast); } }

    /// <summary>Движение поменяли вручную: под наборами «Вернуть как в наборе».</summary>
    public bool IsCustom => Preset is null;

    /// <summary>Подпись под наборами.</summary>
    public string PresetNote => IsCustom
        ? "Движение поменяно вручную."
        : "Набор выбирает стекло и выставляет движение. Движение можно поменять отдельно.";

    /// <summary>«Вернуть как в наборе»: движение — как в наборе, чьё стекло сейчас стоит.</summary>
    public void ReapplyPreset() => ApplyPreset(_current.LiquidGlass ? AppearancePreset.Beautiful : AppearancePreset.Fast);

    // ───────────── где искать скачанные моды (2026-10-18) ─────────────

    /// <summary>Папка «Загрузки» этого компьютера (её задаёт окно); смотрится всегда, убрать её нельзя.</summary>
    public string? DownloadsFolder { get; set; }

    /// <summary>Добавленные папки, в порядке добавления.</summary>
    public IReadOnlyList<string> WatchFolders => _current.WatchFolders;

    public bool CanAddWatchFolder => _current.WatchFolders.Count < AppSettings.MaxWatchFolders;

    /// <summary>«Добавить папку». null — добавлена; иначе почему нет («уже смотрим», «это и есть «Загрузки»»).</summary>
    public string? AddWatchFolder(string path)
    {
        var p = AppSettings.CleanFolder(path);
        if (p is null) return "Папка не выбрана";
        if (DownloadsFolder is { } d && string.Equals(AppSettings.CleanFolder(d), p, StringComparison.OrdinalIgnoreCase))
            return "«Загрузки» программа смотрит и так";
        if (_current.WatchFolders.Contains(p, StringComparer.OrdinalIgnoreCase)) return "Эту папку программа уже смотрит";
        if (!CanAddWatchFolder) return $"Больше {AppSettings.MaxWatchFolders} папок не добавить";
        Update(s => s with { WatchFolders = s.WatchFolders.Append(p).ToList() });
        return null;
    }

    public void RemoveWatchFolder(string path) =>
        Update(s => s with { WatchFolders = s.WatchFolders.Where(f => !string.Equals(f, path, StringComparison.OrdinalIgnoreCase)).ToList() });

    /// <summary>«Оставить» на «Интерфейс работает медленно…»: больше не спрашиваем (запоминается).</summary>
    public void DeclineSlowOffer() => Update(s => s with { SlowOfferDeclined = true });

    public void ApplyPreset(AppearancePreset p) => Update(s => AppearancePresets.Apply(s, p));

    /// <summary>Оформление: тёмное, светлое, как в Windows.</summary>
    public UiMode Mode { get => _current.Mode; set => Update(s => s with { Mode = value }); }
    public bool ModeDark { get => Mode == UiMode.Dark; set { if (value) Mode = UiMode.Dark; } }
    public bool ModeLight { get => Mode == UiMode.Light; set { if (value) Mode = UiMode.Light; } }
    public bool ModeSystem { get => Mode == UiMode.System; set { if (value) Mode = UiMode.System; } }

    /// <summary>В Windows сейчас светлый режим приложений (задаёт окно; нужен для «Как в Windows»).</summary>
    public bool SystemLight
    {
        get => _systemLight;
        set
        {
            if (!Set(ref _systemLight, value)) return;
            Raise(nameof(EffectiveLight));
            foreach (var t in Themes) t.Changed();
        }
    }
    private bool _systemLight;

    /// <summary>Светлое ли оформление на самом деле (с учётом «Как в Windows»).</summary>
    public bool EffectiveLight => LightFor(_current, _systemLight);

    public static bool LightFor(AppSettings s, bool systemLight) => s.Mode == UiMode.Light || (s.Mode == UiMode.System && systemLight);

    /// <summary>Тема фона (три градиента, DESIGN.md 3.1). Плитки в окне — <see cref="Themes"/>.</summary>
    public BackgroundTheme Theme { get => _current.Theme; set => Update(s => s with { Theme = value }); }

    public IReadOnlyList<ThemeTile> Themes { get; }

    public bool UiAnimations { get => _current.UiAnimations; set => Update(s => s with { UiAnimations = value }); }
    public bool AnimatedBackground { get => _current.AnimatedBackground; set => Update(s => s with { AnimatedBackground = value }); }
    /// <summary>«Огоньки»: светлые точки над фоном и редкие искры на кромках стекла (бывшие «Искры льда»).</summary>
    public bool Fireflies { get => _current.Fireflies; set => Update(s => s with { Fireflies = value }); }

    // ───────────── загрузка модов ─────────────

    public bool AllowDownloads { get => _current.AllowDownloads; set => Update(s => s with { AllowDownloads = value }); }

    // ───────────── папки ─────────────

    /// <summary>Где сейчас моды (заполняет программа: папка могла найтись сама). null — не нашли.</summary>
    public string? ModsFolderPath
    {
        get => _modsPath;
        set { if (Set(ref _modsPath, value)) { Raise(nameof(ModsFolderText)); Raise(nameof(HasModsFolder)); } }
    }
    private string? _modsPath;

    public bool HasModsFolder => _modsPath is not null;

    public string ModsFolderText => _modsPath ?? "Не нашли. Укажите папку mods или папку пользователя BeamNG.";

    /// <summary>Папка указана вручную: можно вернуться к поиску самой программой.</summary>
    public bool FolderIsManual => _current.ModsFolder is not null || _current.UserFolder is not null;

    public string FolderNote => FolderIsManual ? "Указана вручную." : "Программа нашла её сама.";

    public void UseAutoFolder() => Update(s => s with { ModsFolder = null, UserFolder = null });

    public int KeepBackups { get => _current.KeepBackups; set => Update(s => s with { KeepBackups = value }); }
    public string KeepBackupsText => $"{KeepBackups}";
    public bool CanLessBackups => KeepBackups > 1;
    public bool CanMoreBackups => KeepBackups < 20;
    public void LessBackups() => KeepBackups--;
    public void MoreBackups() => KeepBackups++;

    public int WarnAbove { get => _current.ActiveModsWarnAbove; set => Update(s => s with { ActiveModsWarnAbove = value }); }
    /// <summary>0 — не предупреждать.</summary>
    public string WarnAboveText => WarnAbove == 0 ? "нет" : $"{WarnAbove}";
    public bool CanLessWarn => WarnAbove > 0;
    public bool CanMoreWarn => WarnAbove < 1000;
    /// <summary>Шаг 5: порог обычно круглый (30, 40), а щёлкать по одному долго.</summary>
    public void LessWarn() => WarnAbove = WarnAbove > 5 ? (WarnAbove - 1) / 5 * 5 : WarnAbove - 1;
    public void MoreWarn() => WarnAbove = WarnAbove < 5 ? WarnAbove + 1 : WarnAbove / 5 * 5 + 5;

    // ───────────── обновления ─────────────

    public bool AutoCheckOnStart { get => _current.AutoCheckOnStart; set => Update(s => s with { AutoCheckOnStart = value }); }

    /// <summary>Повторная проверка, пока программа открыта: 0 — нет, иначе раз в N часов.</summary>
    public int AutoCheckEveryHours { get => _current.AutoCheckEveryHours; set => Update(s => s with { AutoCheckEveryHours = value }); }
    public bool RepeatOff { get => AutoCheckEveryHours == 0; set { if (value) AutoCheckEveryHours = 0; } }
    public bool Repeat3 { get => AutoCheckEveryHours == 3; set { if (value) AutoCheckEveryHours = 3; } }
    public bool Repeat6 { get => AutoCheckEveryHours == 6; set { if (value) AutoCheckEveryHours = 6; } }
    public bool Repeat12 { get => AutoCheckEveryHours == 12; set { if (value) AutoCheckEveryHours = 12; } }

    public bool CheckAppUpdates { get => _current.CheckAppUpdates; set => Update(s => s with { CheckAppUpdates = value }); }

    /// <summary>«Версия 0.1.36» или пояснение, что эта сборка сама не обновляется.</summary>
    public string VersionText { get => _version; set => Set(ref _version, value); }
    private string _version = "";

    /// <summary>Самообновление доступно (настоящая сборка с номером версии).</summary>
    public bool CanUpdateApp { get => _canUpdate; set => Set(ref _canUpdate, value); }
    private bool _canUpdate;

    // ───────────── сайты ─────────────

    public IReadOnlyList<SiteToggle> Sites { get; }

    internal void SetSource(string id, bool on) => Update(s => s with
    {
        DisabledSources = on
            ? s.DisabledSources.Where(x => !string.Equals(x, id, StringComparison.OrdinalIgnoreCase)).ToList()
            : s.DisabledSources.Append(id).ToList(),
    });
}
