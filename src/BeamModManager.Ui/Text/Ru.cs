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
        if (d < TimeSpan.FromDays(28)) return Count((long)d.TotalDays / 7, "неделю", "недели", "недель") + " назад";
        return Date(when);
    }

    public static string Date(DateTimeOffset when) => when.ToLocalTime().ToString("d MMMM yyyy", C);

    /// <summary>Когда было, по-человечески и с временем: «сегодня в 14:02», «вчера в 9:10», «12 октября в 9:10», «3 марта 2025».</summary>
    public static string When(DateTimeOffset when, DateTimeOffset now)
    {
        var w = when.ToLocalTime();
        var n = now.ToLocalTime();
        var time = w.ToString("H:mm", C);
        if (w.Date == n.Date) return "сегодня в " + time;
        if (w.Date == n.Date.AddDays(-1)) return "вчера в " + time;
        if (w.Year == n.Year && w <= n) return w.ToString("d MMMM", C) + " в " + time;
        return Date(when);
    }

    public static string Size(long bytes)
    {
        if (bytes < 1024) return $"{bytes} Б";
        if (bytes < 1024 * 1024) return $"{Math.Round(bytes / 1024.0):0} КБ";
        if (bytes < 1024L * 1024 * 1024) return $"{Math.Round(bytes / 1048576.0):0} МБ";
        return (bytes / 1073741824.0).ToString("0.0", C) + " ГБ";
    }

    /// <summary>Для поиска: регистр и «ё» не важны.</summary>
    public static string Fold(string s) => s.ToLowerInvariant().Replace('ё', 'е');

    /// <summary>
    /// Слова для поиска: разделители — всё, кроме букв и цифр, а также переход со строчной на заглавную
    /// («TrafficReborn_Free.zip» → traffic, reborn, free, zip). Регистр и «ё» не важны.
    /// </summary>
    public static IReadOnlyList<string> Words(string s)
    {
        var result = new List<string>();
        var cur = new System.Text.StringBuilder();
        void Flush() { if (cur.Length > 0) { result.Add(Fold(cur.ToString())); cur.Clear(); } }
        for (var i = 0; i < s.Length; i++)
        {
            var ch = s[i];
            if (!char.IsLetterOrDigit(ch)) { Flush(); continue; }
            if (i > 0 && char.IsUpper(ch) && char.IsLower(s[i - 1])) Flush();
            cur.Append(ch);
        }
        Flush();
        return result;
    }
}
