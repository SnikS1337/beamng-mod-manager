using System.Runtime.CompilerServices;

namespace BeamModManager.Core.Safety;

/// <summary>
/// Страница сайта может оказаться любой (сломанная вёрстка, мусор, подмена). Регулярные выражения разборщиков не должны
/// зависать на таком тексте: общий предел времени на одно совпадение задаётся при загрузке библиотеки.
/// Превышение предела приводит к RegexMatchTimeoutException, адаптер превращает его в отказ только этого сайта.
/// </summary>
internal static class RegexGuard
{
    public static readonly TimeSpan MatchTimeout = TimeSpan.FromSeconds(3);

    [ModuleInitializer]
    internal static void Init()
    {
        // Действует на все Regex без явного таймаута и должно быть задано до первого использования.
        if (AppContext.GetData("REGEX_DEFAULT_MATCH_TIMEOUT") is null)
            AppContext.SetData("REGEX_DEFAULT_MATCH_TIMEOUT", MatchTimeout);
    }
}
