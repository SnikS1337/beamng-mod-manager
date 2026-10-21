using System.Text.RegularExpressions;

namespace BeamModManager.Core.Sources;

/// <summary>Состояние источника для интерфейса. Любой сбой одного сайта не ломает остальные.</summary>
public enum SourceStatus
{
    Ok,
    /// <summary>Сайт показал страницу проверки «вы человек?». Её проходит пользователь сам.</summary>
    NeedsHumanCheck,
    /// <summary>Сайт не отвечает или страница не похожа на ожидаемую.</summary>
    Unavailable,
}

/// <summary>
/// Распознаёт страницу проверки браузера. Ничего не обходит: нужно только вовремя показать
/// пользователю окно с этой страницей. Один лишь адрес /cdn-cgi/challenge-platform/ признаком не считается:
/// он есть и на обычных страницах за Cloudflare.
/// </summary>
public static class ChallengePageDetector
{
    private static readonly Regex TitleRx = new(
        @"<title>\s*(?:Just a moment|Один момент|Attention Required|Checking your browser|Подтвердите, что вы человек)",
        RegexOptions.IgnoreCase | RegexOptions.Compiled);

    private static readonly Regex BodyRx = new(
        @"Проверяем,\s+человек\s+ли\s+вы|Verify you are human|Checking if the site connection is secure|Needs to review the security of your connection|id=""challenge-(?:form|running|stage)""",
        RegexOptions.IgnoreCase | RegexOptions.Compiled);

    public static bool IsChallenge(string? html) =>
        !string.IsNullOrEmpty(html) && (TitleRx.IsMatch(html) || BodyRx.IsMatch(html));

    /// <summary>Итоговый статус по ответу сайта.</summary>
    public static SourceStatus Classify(int? httpStatus, string? html, bool pageLooksExpected)
    {
        if (IsChallenge(html)) return SourceStatus.NeedsHumanCheck;
        // 403/503 с пустым телом или без признаков проверки: сайт недоступен, а не «нужна проверка».
        if (httpStatus is null or >= 400) return SourceStatus.Unavailable;
        return pageLooksExpected ? SourceStatus.Ok : SourceStatus.Unavailable;
    }
}
