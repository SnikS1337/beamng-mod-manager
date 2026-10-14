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

    // CA2255 советует не применять ModuleInitializer в библиотеках. Здесь это сделано намеренно: таймаут должен стоять
    // до первого Regex ядра, чьи разборщики читают чужие страницы; отдельного места для вызова при старте приложения нет.
#pragma warning disable CA2255
    [ModuleInitializer]
#pragma warning restore CA2255
    internal static void Init()
    {
        // Действует на все Regex без явного таймаута и должно быть задано до первого использования.
        if (AppContext.GetData("REGEX_DEFAULT_MATCH_TIMEOUT") is null)
            AppContext.SetData("REGEX_DEFAULT_MATCH_TIMEOUT", MatchTimeout);
    }
}
