namespace BeamModManager.Core.Settings;

/// <summary>Готовые наборы внешнего вида (DESIGN.md 5.1). Набор выставляет переключатели; после этого каждый можно менять отдельно.</summary>
public enum AppearancePreset { Beautiful, Fast }

public static class AppearancePresets
{
    public static IReadOnlyList<AppearancePreset> All { get; } = new[] { AppearancePreset.Beautiful, AppearancePreset.Fast };

    /// <summary>Применяет набор к настройкам. Остальные настройки (фон, папки, проверки, окно) не затрагиваются: фон — выбор цвета, а не «тяжести».</summary>
    public static AppSettings Apply(AppSettings s, AppearancePreset preset) => (preset switch
    {
        // «Красиво»: жидкое стекло, живой фон, огоньки, анимации — всё включено (по умолчанию).
        AppearancePreset.Beautiful => s with
        {
            LiquidGlass = true, UiAnimations = true, AnimatedBackground = true, Fireflies = true,
        },
        // «Быстро»: матовое стекло без капель-линз, фон стоит, огоньков нет, и анимации интерфейса тоже выключены
        // (2026-10-17, после боя: «Быстро» должно быть быстрым целиком, а не «всё стоит, кроме анимаций»). Включаются отдельно.
        AppearancePreset.Fast => s with
        {
            LiquidGlass = false, UiAnimations = false, AnimatedBackground = false, Fireflies = false, BackgroundTilt = false,
        },
        _ => s,
    }).Normalize();

    /// <summary>Какой набор сейчас выставлен, или null, если настройки изменены вручную («свой»). Наклон фона на набор не влияет.</summary>
    public static AppearancePreset? Detect(AppSettings s)
    {
        s = s.Normalize();
        foreach (var p in All)
        {
            var e = Apply(s, p);
            if (s.LiquidGlass == e.LiquidGlass && s.UiAnimations == e.UiAnimations && s.AnimatedBackground == e.AnimatedBackground && s.Fireflies == e.Fireflies)
                return p;
        }
        return null;
    }
}
