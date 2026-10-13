using System.Text.RegularExpressions;

namespace BeamModManager.Core.Packs;

/// <summary>
/// Подсказка «это, похоже, пак» по названию на сайте (до скачивания содержимое неизвестно).
/// Точный ответ даёт только состав архива: <see cref="PackContentReader"/>.
/// </summary>
public static class PackTitleHint
{
    private static readonly Regex Rx = new(@"(?<![\p{L}\p{N}])((?:wheel|sound|skin|paint|texture)s?pack|pack|collection|bundle|сборка|пак|набор)(?![\p{L}\p{N}])|\[pack\]",
        RegexOptions.IgnoreCase | RegexOptions.Compiled);

    // Паки колёс, звуков и т.п. — не наборы машин.
    private static readonly Regex NotVehicles = new(@"(?<![\p{L}\p{N}])(wheel|wheels|sound|sounds|skin|skins|paint|livery|liveries|texture|textures)\s*(pack)?",
        RegexOptions.IgnoreCase | RegexOptions.Compiled);

    public static bool Looks(string title) => Rx.IsMatch(title);

    /// <summary>Пак деталей (колёса, звуки, краски), а не машин: пересечение по папкам vehicles с ним не ищем.</summary>
    public static bool LooksLikePartsPack(string title) => Looks(title) && NotVehicles.IsMatch(title);
}
