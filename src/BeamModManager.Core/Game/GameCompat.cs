using System.Text.RegularExpressions;

namespace BeamModManager.Core.Game;

/// <summary>Мод и игра: та же версия игры, мод для более старой или для более новой.</summary>
public enum CompatLevel { Unknown, Same, ModOlder, ModNewer }

/// <summary>
/// Совместимость с версией игры (DESIGN_FLOWS 6.8, сделано 2026-10-18): для какой игры мод (сайт: «Game Version: 0.38»,
/// «… (0.38) ДЛЯ BEAMNG.DRIVE») и какая стоит (<see cref="GameInfo.DetectVersion"/>). Сравниваются первые два числа:
/// 0.38.3 и 0.38.5 — одна игра, 0.38 и 0.39 — разные. Ничего не блокирует, только подсказывает.
/// </summary>
public static class GameCompat
{
    // целое число версии «0.39.4.0» — одно совпадение; из него берутся первые два числа
    private static readonly Regex Ver = new(@"(?<![\d.])(?<a>\d{1,3})\.(?<b>\d{1,3})(?:\.\d+)*(?![\d])", RegexOptions.Compiled);

    /// <summary>«0.38», «v0.38.3», «0.38.x», «0.36–0.38» (берётся последняя, самая новая) → (0, 38); мусор — null.</summary>
    public static (int Major, int Minor)? Parse(string? v)
    {
        if (string.IsNullOrWhiteSpace(v)) return null;
        // несколько версий («0.36–0.38») — самая новая
        (int, int)? best = null;
        foreach (Match m in Ver.Matches(v))
        {
            var p = (int.Parse(m.Groups["a"].Value), int.Parse(m.Groups["b"].Value));
            if (best is null || p.CompareTo(best.Value) > 0) best = p;
        }
        return best;
    }

    public static CompatLevel Compare(string? modFor, string? game)
    {
        if (Parse(modFor) is not { } mod || Parse(game) is not { } g) return CompatLevel.Unknown;
        var c = mod.CompareTo(g);
        return c == 0 ? CompatLevel.Same : c < 0 ? CompatLevel.ModOlder : CompatLevel.ModNewer;
    }

    /// <summary>Два числа для показа: «0.39.4.0» → «0.39».</summary>
    public static string? Short(string? v) => Parse(v) is { } p ? $"{p.Major}.{p.Minor}" : null;

    /// <summary>
    /// Строка для правой панели: «0.39, как у вас»; «0.38 — у вас 0.39, может не работать»; «0.40 — у вас 0.39, нужна
    /// игра новее». null — не знаем версию мода или игры (строки нет).
    /// </summary>
    public static string? Describe(string? modFor, string? game) => Compare(modFor, game) switch
    {
        CompatLevel.Same => $"{Short(modFor)}, как у вас",
        CompatLevel.ModOlder => $"{Short(modFor)} — у вас {Short(game)}, может не работать",
        CompatLevel.ModNewer => $"{Short(modFor)} — у вас {Short(game)}, нужна игра новее",
        _ => null,
    };
}
