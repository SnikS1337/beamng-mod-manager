using BeamModManager.Core.Settings;
using BeamModManager.Core.Sources.BeamNg;
using BeamModManager.Core.Sources.Modland;
using BeamModManager.Core.Sources.ModsGaming;
using BeamModManager.Core.Sources.StMods;
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
        Changed?.Invoke(next);
    }

    private static bool SameAs(AppSettings a, AppSettings b) =>
        a with { DisabledSources = Array.Empty<string>() } == b with { DisabledSources = Array.Empty<string>() } &&
        a.DisabledSources.SequenceEqual(b.DisabledSources);

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

    /// <summary>Выставленный набор или null, если переключатели поменяли вручную.</summary>
    public AppearancePreset? Preset => AppearancePresets.Detect(_current);

    // плитки наборов в окне — переключатели: выбор плитки выставляет набор, снятие ничего не делает
    public bool IsIce { get => Preset == AppearancePreset.Ice; set { if (value) ApplyPreset(AppearancePreset.Ice); } }
    public bool IsGlass { get => Preset == AppearancePreset.Glass; set { if (value) ApplyPreset(AppearancePreset.Glass); } }
    public bool IsMatte { get => Preset == AppearancePreset.Matte; set { if (value) ApplyPreset(AppearancePreset.Matte); } }
    public bool IsLight { get => Preset == AppearancePreset.Light; set { if (value) ApplyPreset(AppearancePreset.Light); } }

    /// <summary>Подпись под наборами: какой выбран или что набор свой.</summary>
    public string PresetNote => Preset is null
        ? "Свой набор: переключатели поменяны вручную. Выберите готовый, чтобы вернуть его."
        : "Набор выставляет переключатели. Дальше каждый можно поменять отдельно.";

    public void ApplyPreset(AppearancePreset p) => Update(s => AppearancePresets.Apply(s, p));

    /// <summary>«Сбросить»: набор «Лёд», как при первом запуске.</summary>
    public void ResetAppearance() => ApplyPreset(AppearancePreset.Ice);

    public bool LiquidGlass { get => _current.LiquidGlass; set => Update(s => s with { LiquidGlass = value }); }
    public bool Frost { get => _current.Frost; set => Update(s => s with { Frost = value }); }

    /// <summary>0 слабо, 1 средне, 2 сильно.</summary>
    public int FrostStrength { get => _current.FrostStrength; set => Update(s => s with { FrostStrength = value }); }
    public bool FrostWeak { get => FrostStrength == 0; set { if (value) FrostStrength = 0; } }
    public bool FrostMedium { get => FrostStrength == 1; set { if (value) FrostStrength = 1; } }
    public bool FrostStrong { get => FrostStrength == 2; set { if (value) FrostStrength = 2; } }

    public BackgroundMode Background { get => _current.Background; set => Update(s => s with { Background = value }); }
    public bool BackgroundPhoto { get => Background == BackgroundMode.Photo; set { if (value) Background = BackgroundMode.Photo; } }
    public bool BackgroundBlurred { get => Background == BackgroundMode.BlurredPhoto; set { if (value) Background = BackgroundMode.BlurredPhoto; } }
    public bool BackgroundColor { get => Background == BackgroundMode.SolidColor; set { if (value) Background = BackgroundMode.SolidColor; } }

    /// <summary>Своё фото выбрано (иначе за панелями градиент).</summary>
    public bool HasBackgroundImage => _current.BackgroundImage is not null;

    public string BackgroundImageNote => HasBackgroundImage
        ? "Фото с вашего компьютера. Программа хранит свою копию."
        : "Своего фото нет: за панелями тёмный градиент с цветными пятнами.";

    public void RemoveBackgroundImage() => Update(s => s with { BackgroundImage = null });

    public int BackgroundDim { get => _current.BackgroundDim; set => Update(s => s with { BackgroundDim = value }); }
    public string BackgroundDimText => $"{BackgroundDim} %";

    public bool UiAnimations { get => _current.UiAnimations; set => Update(s => s with { UiAnimations = value }); }
    public bool AnimatedBackground { get => _current.AnimatedBackground; set => Update(s => s with { AnimatedBackground = value }); }
    public bool Fireflies { get => _current.Fireflies; set => Update(s => s with { Fireflies = value }); }

    /// <summary>Сила инея зависит от инея: при выключенном пункт тускнеет и не нажимается (значение хранится).</summary>
    public bool FrostOptionsEnabled => _current.Frost;

    /// <summary>Затемнение, движение фона и огоньки не имеют смысла при фоне «Цвет».</summary>
    public bool PhotoOptionsEnabled => _current.Background != BackgroundMode.SolidColor;

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
    public string WarnAboveText => $"{WarnAbove}";
    public bool CanLessWarn => WarnAbove > 1;
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
