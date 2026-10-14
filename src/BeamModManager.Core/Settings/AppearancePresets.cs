namespace BeamModManager.Core.Settings;

/// <summary>Готовые наборы внешнего вида (DESIGN.md 5.1). Набор выставляет переключатели; после этого каждый можно менять отдельно.</summary>
public enum AppearancePreset { Ice, Glass, Matte, Light }

public static class AppearancePresets
{
    public static IReadOnlyList<AppearancePreset> All { get; } = new[] { AppearancePreset.Ice, AppearancePreset.Glass, AppearancePreset.Matte, AppearancePreset.Light };

    /// <summary>Применяет набор к настройкам. Остальные настройки (папки, проверки, окно) не затрагиваются.</summary>
    public static AppSettings Apply(AppSettings s, AppearancePreset preset) => (preset switch
    {
        // «Лёд»: стекло, иней, фон, всё включено (по умолчанию).
        AppearancePreset.Ice => s with
        {
            LiquidGlass = true, Frost = true, FrostStrength = 1, FrostSparks = true,
            Background = BackgroundMode.Photo, BackgroundDim = 35,
            UiAnimations = true, AnimatedBackground = true, Fireflies = true,
        },
        // «Стекло»: преломление есть, инея нет.
        AppearancePreset.Glass => s with
        {
            LiquidGlass = true, Frost = false, FrostStrength = 1, FrostSparks = true,
            Background = BackgroundMode.Photo, BackgroundDim = 35,
            UiAnimations = true, AnimatedBackground = true, Fireflies = true,
        },
        // «Матовое»: обычное матовое стекло без преломления и инея, движение остаётся.
        AppearancePreset.Matte => s with
        {
            LiquidGlass = false, Frost = false, FrostStrength = 1, FrostSparks = true,
            Background = BackgroundMode.Photo, BackgroundDim = 35,
            UiAnimations = true, AnimatedBackground = true, Fireflies = true,
        },
        // «Лёгкое»: ничего лишнего для слабых компьютеров.
        AppearancePreset.Light => s with
        {
            LiquidGlass = false, Frost = false, FrostStrength = 1, FrostSparks = false,
            Background = BackgroundMode.Photo, BackgroundDim = 35,
            UiAnimations = false, AnimatedBackground = false, Fireflies = false, BackgroundTilt = false,
        },
        _ => s,
    }).Normalize();

    /// <summary>Какой набор сейчас выставлен, или null, если настройки изменены вручную («свои»). Сила и искры инея при выключенном инее не сравниваются.</summary>
    public static AppearancePreset? Detect(AppSettings s)
    {
        s = s.Normalize();
        foreach (var p in All)
        {
            var e = Apply(s, p);
            var frostSame = !e.Frost ? !s.Frost : s.Frost && s.FrostStrength == e.FrostStrength && s.FrostSparks == e.FrostSparks;
            if (s.LiquidGlass == e.LiquidGlass && frostSame && s.Background == e.Background && s.BackgroundDim == e.BackgroundDim &&
                s.UiAnimations == e.UiAnimations && s.AnimatedBackground == e.AnimatedBackground && s.Fireflies == e.Fireflies)
                return p;
        }
        return null;
    }
}
