using System.Text.RegularExpressions;

namespace BeamModManager.Core.Text;

/// <summary>Результат разбора заголовка записи сайта.</summary>
public sealed record ParsedTitle(string Name, string? ModVersion, string? GameVersion, bool HasPaidMarker);

/// <summary>
/// Достаёт из заголовка название, версию мода и версию игры.
/// Шаблон ModsGaming: «МОД {название} v{версия} ({версия игры}) ДЛЯ BEAMNG.DRIVE».
/// Шаблон Modland (по наблюдению): версия в конце, PAID в названии; уточнить на реальных страницах.
/// </summary>
public static class TitleParser
{
    private const RegexOptions Opt = RegexOptions.IgnoreCase | RegexOptions.CultureInvariant | RegexOptions.Compiled;

    private static readonly Regex GameVersionParen = new(@"\(\s*v?(\d+\.\d+(?:\.(?:\d+|x))*)\s*\)", Opt);
    private static readonly Regex LeadingMod = new(@"^\s*(?:мод|mod)\s+", Opt);
    private static readonly Regex ForBeam = new(@"\s*\b(?:для|for)\s+beamng(?:\s*\.?\s*drive)?\s*$", Opt);
    // Версия с префиксом v только с точкой («v1.2»); голое «v8» оставляем в названии (Ford V8).
    private static readonly Regex VersionWithV = new(@"(?<![\p{L}\p{Nd}])v(\d+(?:\.\d+)+[a-z]?)(?![\p{L}\p{Nd}.])", Opt);
    // STMods: «Название версия 3.3 для BeamNG.drive (v0.39.x)».
    private static readonly Regex WordVersion = new(@"\s(?:версия|version)\s+(\d+(?:\.\d+)*[a-z]?)(?![\p{L}\p{Nd}.])", Opt);
    private static readonly Regex TrailingBareVersion = new(@"\s(\d{1,2}(?:\.\d+)+)\s*$", Opt);

    // Метка платного в названии: [PAID] в любой форме скобок или слово PAID заглавными.
    private static readonly Regex PaidBracketed = new(@"[\[(]\s*(?:paid|premium|vip|early\s+access)\s*[\])]", Opt);
    private static readonly Regex PaidUpper = new(@"(?<![\p{L}\p{Nd}])PAID(?![\p{L}\p{Nd}])", RegexOptions.CultureInvariant);

    public static bool HasPaidMarker(string title) =>
        PaidBracketed.IsMatch(title) || PaidUpper.IsMatch(title);

    public static ParsedTitle Parse(string raw)
    {
        var s = raw.Trim();
        var paid = HasPaidMarker(s);

        s = PaidBracketed.Replace(s, " ");
        s = PaidUpper.Replace(s, " ");

        string? gameVersion = null;
        var gv = GameVersionParen.Match(s);
        if (gv.Success)
        {
            gameVersion = gv.Groups[1].Value;
            s = s.Remove(gv.Index, gv.Length);
        }

        s = LeadingMod.Replace(s, "");
        s = ForBeam.Replace(s, "");

        string? modVersion = null;
        var wv = WordVersion.Match(s);
        var vm = VersionWithV.Match(s);
        if (wv.Success)
        {
            modVersion = wv.Groups[1].Value;
            s = s.Remove(wv.Index, wv.Length);
        }
        else if (vm.Success)
        {
            modVersion = vm.Groups[1].Value;
            s = s.Remove(vm.Index, vm.Length);
        }
        else
        {
            var bm = TrailingBareVersion.Match(s);
            if (bm.Success)
            {
                modVersion = bm.Groups[1].Value;
                s = s.Remove(bm.Index, bm.Length);
            }
        }

        s = Regex.Replace(s, @"\s+", " ").Trim(' ', '-', '–', '—', ',', ':', ';');
        return new ParsedTitle(s, modVersion, gameVersion, paid);
    }
}
