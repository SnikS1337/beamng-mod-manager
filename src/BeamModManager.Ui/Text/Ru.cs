using System.Globalization;

namespace BeamModManager.Ui.Text;

/// <summary>Русские числа и даты для интерфейса: «3 мода», «18 минут назад», «312 МБ».</summary>
public static class Ru
{
    private static readonly CultureInfo C = CultureInfo.GetCultureInfo("ru-RU");

    public static string Plural(long n, string one, string few, string many)
    {
        var a = Math.Abs(n) % 100;
        var b = a % 10;
        if (a is >= 11 and <= 14) return many;
        return b == 1 ? one : b is >= 2 and <= 4 ? few : many;
    }

    public static string Count(long n, string one, string few, string many) => $"{n} {Plural(n, one, few, many)}";

    public static string Mods(long n) => Count(n, "мод", "мода", "модов");

    public static string Ago(DateTimeOffset when, DateTimeOffset now)
    {
        var d = now - when;
        if (d < TimeSpan.FromMinutes(1)) return "только что";
        if (d < TimeSpan.FromHours(1)) return Count((long)d.TotalMinutes, "минуту", "минуты", "минут") + " назад";
        if (d < TimeSpan.FromDays(1)) return Count((long)d.TotalHours, "час", "часа", "часов") + " назад";
        if (d < TimeSpan.FromDays(2)) return "вчера";
        if (d < TimeSpan.FromDays(7)) return Count((long)d.TotalDays, "день", "дня", "дней") + " назад";
        return Date(when);
    }

    public static string Date(DateTimeOffset when) => when.ToLocalTime().ToString("d MMMM yyyy", C);

    public static string Size(long bytes)
    {
        if (bytes < 1024) return $"{bytes} Б";
        if (bytes < 1024 * 1024) return $"{Math.Round(bytes / 1024.0):0} КБ";
        if (bytes < 1024L * 1024 * 1024) return $"{Math.Round(bytes / 1048576.0):0} МБ";
        return (bytes / 1073741824.0).ToString("0.0", C) + " ГБ";
    }

    /// <summary>Для поиска: регистр и «ё» не важны.</summary>
    public static string Fold(string s) => s.ToLowerInvariant().Replace('ё', 'е');
}
