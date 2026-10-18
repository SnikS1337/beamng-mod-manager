using System.Text.RegularExpressions;

namespace BeamModManager.Core.Text;

/// <summary>
/// Какое название показывать на карточке мода (решено 2026-10-18): человек думает о машине, а не о файле. Берётся первое
/// известное: из репозитория игры (официальное), из собственного info.json мода, из заголовка страницы на сайте, где за
/// модом следим, и только потом — догадка по имени файла (она же <c>InstalledMod.Name</c>). Имя файла видно в панели мода.
/// </summary>
public static class ModTitles
{
    private const int MaxLength = 80;

    // «Ibishu Miramar Dirt Race [0.9.0]», «… (v1.2)» — версия в скобках в конце заголовка
    private static readonly Regex BracketVersion = new(@"\s*[\[(]\s*v?\d+(?:\.\d+)+[a-z]?\s*[\])]\s*$",
        RegexOptions.IgnoreCase | RegexOptions.CultureInvariant | RegexOptions.Compiled);

    public static string Choose(string? repoTitle, string? archiveTitle, string? siteTitle, string fallback) =>
        Usable(repoTitle) ?? Usable(archiveTitle) ?? Usable(Site(siteTitle)) ?? fallback;

    /// <summary>Заголовок страницы сайта без «Мод … для BeamNG.drive», версии мода и игры и метки платного.</summary>
    public static string? Site(string? raw)
    {
        if (string.IsNullOrWhiteSpace(raw)) return null;
        var name = TitleParser.Parse(raw).Name;
        name = BracketVersion.Replace(name, "").Trim();
        return name.Length == 0 ? null : name;
    }

    private static string? Usable(string? s)
    {
        var t = s?.Trim();
        return string.IsNullOrEmpty(t) || t.Length > MaxLength ? null : t;
    }
}
