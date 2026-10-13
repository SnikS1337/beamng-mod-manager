using System.Numerics;
using System.Text.RegularExpressions;
using BeamModManager.Core.Hosts;
using BeamModManager.Core.Models;
using BeamModManager.Core.Text;

namespace BeamModManager.Core.Matching;

public enum MatchVerdict { Different, Maybe, Same }

public sealed record MatchResult(double Score, MatchVerdict Verdict, string Reason);

/// <summary>
/// Решает, одна ли это запись мода на разных сайтах. Точность важнее полноты:
/// по одному названию автоматически не склеиваем (Same только при подтверждающих признаках).
/// </summary>
public static class DuplicateMatcher
{
    public const double SameThreshold = 0.9;
    public const double MaybeThreshold = 0.7;

    private const double MinNameSimilarity = 0.6;

    // Веса признаков; сумма равна 1.
    private const double WName = 0.50, WAuthor = 0.20, WSize = 0.15, WPreview = 0.10, WGameVersion = 0.05;

    private static readonly Regex NonAlnum = new(@"[^\p{L}\p{Nd}]+", RegexOptions.Compiled);

    public static MatchResult Compare(Listing a, Listing b)
    {
        // Сильный признак: одинаковый файл на одном и том же файлообменнике.
        if (SharedHostFile(a, b) is { } shared)
            return new MatchResult(1.0, MatchVerdict.Same, $"один файл на {shared.Host} ({shared.FileId})");

        var name = TitleNormalizer.Similarity(a.Title, b.Title);
        if (name < MinNameSimilarity)
            return new MatchResult(name * 0.5, MatchVerdict.Different, "названия не совпадают");

        double matched = name * WName;
        double available = WName;

        var authorA = Key(a.Author);
        var authorB = Key(b.Author);
        var authorKnown = authorA.Length > 0 && authorB.Length > 0;
        var authorMismatch = authorKnown && authorA != authorB;
        if (authorKnown)
        {
            available += WAuthor;
            if (!authorMismatch) matched += WAuthor;
        }

        var sizeConflict = false;
        if (a.SizeBytes is > 0 && b.SizeBytes is > 0)
        {
            available += WSize;
            var ratio = (double)Math.Min(a.SizeBytes.Value, b.SizeBytes.Value) / Math.Max(a.SizeBytes.Value, b.SizeBytes.Value);
            if (ratio >= 0.97) matched += WSize;
            else if (ratio >= 0.90) matched += WSize * 0.5;
            else sizeConflict = true;
        }

        if (a.PreviewHash is { } ha && b.PreviewHash is { } hb)
        {
            available += WPreview;
            var distance = BitOperations.PopCount(ha ^ hb);
            if (distance <= 6) matched += WPreview;
            else if (distance <= 12) matched += WPreview * 0.5;
        }

        if (!string.IsNullOrWhiteSpace(a.GameVersion) && !string.IsNullOrWhiteSpace(b.GameVersion))
        {
            available += WGameVersion;
            if (string.Equals(a.GameVersion!.Trim(), b.GameVersion!.Trim(), StringComparison.OrdinalIgnoreCase))
                matched += WGameVersion;
        }

        // Чем меньше признаков, тем меньше уверенность: одно название даёт не больше 0.85.
        var confidence = matched / available * (0.7 + 0.3 * available);

        var reason = $"название {name:0.00}, признаков {available:0.00}";
        if (authorMismatch) { confidence = Math.Min(confidence, 0.69); reason += ", авторы разные"; }
        if (sizeConflict) { confidence = Math.Min(confidence, 0.75); reason += ", размер сильно отличается"; }

        var verdict = confidence >= SameThreshold ? MatchVerdict.Same
            : confidence >= MaybeThreshold ? MatchVerdict.Maybe
            : MatchVerdict.Different;

        return new MatchResult(confidence, verdict, reason);
    }

    private static string Key(string? s) =>
        string.IsNullOrWhiteSpace(s) ? "" : NonAlnum.Replace(s, "").ToLowerInvariant();

    private static HostLink? SharedHostFile(Listing a, Listing b)
    {
        var linksA = LinksOf(a).ToList();
        if (linksA.Count == 0) return null;

        var linksB = LinksOf(b).ToList();
        return linksA.FirstOrDefault(x => linksB.Any(y =>
            x.Host == y.Host && string.Equals(x.FileId, y.FileId, StringComparison.Ordinal)));
    }

    private static IEnumerable<HostLink> LinksOf(Listing l)
    {
        foreach (var offer in l.Offers)
        {
            if (offer.HostFileId is { Length: > 0 } id)
                yield return new HostLink(offer.Host.ToLowerInvariant(), id, offer.Url);
            else if (HostLinkParser.TryParse(offer.Url.ToString()) is { } parsed)
                yield return parsed;
        }
    }
}
