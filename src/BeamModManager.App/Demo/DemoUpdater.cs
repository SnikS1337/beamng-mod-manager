using BeamModManager.Core.AppUpdate;

namespace BeamModManager.App.Demo;

/// <summary>
/// Плашка обновления в демо и на снимках (--update available|downloading|verifying|ready|failed): состояние стоит на месте,
/// сеть и диск не трогаются. Перезапуск в демо ничего не делает.
/// </summary>
public sealed class DemoUpdater : IAppUpdater
{
    private readonly UpdateState _state;

    private DemoUpdater(UpdateState state) => _state = state;

    /// <summary>null — такого состояния нет (плашки в демо не будет).</summary>
    public static DemoUpdater? From(string? phase)
    {
        var m = new UpdateManifest(new Version(0, 3, 0, 0), new Uri("https://github.com/SnikS1337/beamng-mod-manager/releases/download/v0.3/ModManagerBeamNG.zip"),
            57L * 1048576, new string('0', 64), "Демо", null);
        UpdateState? s = phase?.ToLowerInvariant() switch
        {
            "available" => new(UpdateStage.Available, m),
            "downloading" => new(UpdateStage.Downloading, m, 0.42),
            "verifying" => new(UpdateStage.Downloading, m, 1),
            "ready" => new(UpdateStage.ReadyToRestart, m, 1),
            "failed" => new(UpdateStage.Available, m, 0.63, "загрузка оборвалась"),
            _ => null,
        };
        return s is null ? null : new DemoUpdater(s);
    }

    public Version Current => new(0, 2, 0, 0);
    public UpdateState State => _state;
#pragma warning disable CS0067 // состояние в демо не меняется
    public event Action? Changed;
#pragma warning restore CS0067
    public Task CheckAsync(bool manual, CancellationToken ct = default) => Task.CompletedTask;
    public Task InstallAsync(CancellationToken ct = default) => Task.CompletedTask;
    public void CancelInstall() { }
}
