using System.Globalization;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace BeamModManager.Core.AppUpdate;

/// <summary>
/// Описание свежей сборки (latest.json в выпуске публичного репозитория): версия, где архив, его размер и SHA-256.
/// Его пишет сборочная машина; программа не верит ему на слово и проверяет каждое поле.
/// </summary>
public sealed record UpdateManifest(Version Version, Uri Url, long Size, string Sha256, string? Notes, DateTimeOffset? Published)
{
    public const long MinSize = 1L * 1024 * 1024;     // меньше мегабайта — точно не наша программа
    public const long MaxSize = 400L * 1024 * 1024;   // защита от подмены огромным файлом

    /// <summary>Куда можно ходить за обновлением: GitHub и его хранилище файлов. Только https.</summary>
    public static bool IsAllowedHost(Uri u) =>
        u.Scheme == Uri.UriSchemeHttps &&
        (u.Host.Equals("github.com", StringComparison.OrdinalIgnoreCase) ||
         u.Host.EndsWith(".githubusercontent.com", StringComparison.OrdinalIgnoreCase));

    /// <summary>Разобрать и проверить. Ошибка — понятная причина, без исключений.</summary>
    public static (UpdateManifest? Manifest, string? Error) Parse(string json)
    {
        JsonObject? o;
        try { o = JsonNode.Parse(json) as JsonObject; }
        catch (JsonException) { return (null, "описание обновления повреждено"); }
        if (o is null) return (null, "описание обновления повреждено");

        string? S(string k) => o[k] is JsonValue v && v.TryGetValue<string>(out var s) ? s.Trim() : null;
        if (AppVersion.Parse(S("version")) is not { } ver) return (null, "в описании обновления нет версии");
        if (!Uri.TryCreate(S("url"), UriKind.Absolute, out var url) || !IsAllowedHost(url)) return (null, "адрес обновления не с GitHub или не https");
        long size = o["size"] is JsonValue sv && sv.TryGetValue<long>(out var l) ? l : -1;
        if (size is < MinSize or > MaxSize) return (null, "размер обновления неправдоподобный");
        var sha = S("sha256")?.ToLowerInvariant();
        if (sha is null || sha.Length != 64 || !sha.All(Uri.IsHexDigit)) return (null, "в описании обновления нет контрольной суммы");
        DateTimeOffset? published = DateTimeOffset.TryParse(S("published"), CultureInfo.InvariantCulture, DateTimeStyles.AssumeUniversal, out var p) ? p : null;
        var notes = S("notes");
        if (notes is { Length: > 500 }) notes = notes[..500];
        return (new UpdateManifest(ver, url, size, sha, notes, published), null);
    }
}

public static class AppVersion
{
    /// <summary>«0.1.30», «v0.1.30», «0.1.30.0», «0.1.30+abc» → 0.1.30.0. Мусор → null.</summary>
    public static Version? Parse(string? s)
    {
        if (string.IsNullOrWhiteSpace(s)) return null;
        var t = s.Trim().TrimStart('v', 'V');
        var plus = t.IndexOfAny(new[] { '+', '-', ' ' });
        if (plus >= 0) t = t[..plus];
        if (!Version.TryParse(t, out var v)) return null;
        return Normalize(v);
    }

    /// <summary>Недостающие части — нули: 0.1.30 и 0.1.30.0 одна версия.</summary>
    public static Version Normalize(Version v) =>
        new(v.Major, Math.Max(0, v.Minor), Math.Max(0, v.Build), Math.Max(0, v.Revision));

    /// <summary>Строго новее: на ту же или старую версию не «обновляемся».</summary>
    public static bool IsNewer(Version candidate, Version current) => Normalize(candidate) > Normalize(current);

    /// <summary>
    /// Сборка с машины сборки, которой можно обновляться: 0.x с номером («0.1.129», «0.2», «0.3.1»). 1.0.0.0 — без номера
    /// (dotnet run, папка разработки). Раньше требовался третий номер больше нуля, и выпуски 0.2 и 0.3 (0.2.0, 0.3.0)
    /// считали себя сборкой разработки: обновлений не искали вовсе (найдено после 0.3.1, 2026-10-18).
    /// </summary>
    public static bool IsRelease(Version v) => v.Major == 0 && (v.Minor > 0 || v.Build > 0);

    /// <summary>Как показывать человеку: без хвостовых нулей («0.1.30», «0.2», «0.2.1»; выпуски с 0.2 — две-три части).</summary>
    public static string Display(Version v) => v.Revision > 0 ? v.ToString(4) : v.Build > 0 ? v.ToString(3) : v.ToString(2);
}
