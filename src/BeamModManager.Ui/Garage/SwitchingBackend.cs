using BeamModManager.Core.Game;
using BeamModManager.Core.Library;

namespace BeamModManager.Ui.Garage;

/// <summary>
/// Гараж поверх бэкенда, который можно заменить (пользователь указал другую папку). Передаёт ВСЕ методы, в том числе
/// со значением по умолчанию в интерфейсе: иначе сработала бы заглушка — так было с переключением на ходу, пока его не
/// поймал тест (GarageTests: «обёртка передаёт каждый метод»).
/// </summary>
public sealed class SwitchingBackend : IGarageBackend
{
    public IGarageBackend Inner { get; set; } = null!;
    public GarageSnapshot Load() => Inner.Load();
    public bool IsGameRunning() => Inner.IsGameRunning();
    public ToggleResult SetEnabled(string fileName, bool enabled) => Inner.SetEnabled(fileName, enabled);
    public bool LiveAvailable => Inner.LiveAvailable;
    public bool TrySendLive(IReadOnlyDictionary<string, bool> wanted) => Inner.TrySendLive(wanted);
    public BridgeAnswer? LiveAnswer() => Inner.LiveAnswer();
}
