namespace BeamModManager.Core.AppUpdate;

public enum UpdateStage
{
    /// <summary>Ничего не происходит (ещё не проверяли, проверка выключена или это сборка для разработки).</summary>
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    /// <summary>Новая версия скачана, проверена и поставлена на место: осталось перезапустить программу.</summary>
    ReadyToRestart,
    Failed,
}

public sealed record UpdateState(UpdateStage Stage, UpdateManifest? Manifest = null, double Progress = 0, string? Error = null, DateTimeOffset? CheckedAt = null);

/// <summary>
/// Самообновление: проверка, скачивание, проверка файла, подмена exe. Одна операция за раз: повторные нажатия во время
/// проверки или загрузки ничего не запускают заново (возвращают ту же задачу). Проверка по кнопке не чаще раза в 10 с.
/// Сеть и диск — через <see cref="UpdateDownloader"/> и <see cref="UpdateInstaller"/>; это место только решает, что когда делать.
/// </summary>
public sealed class AppUpdater
{
    public static readonly TimeSpan ManualCooldown = TimeSpan.FromSeconds(10);

    private readonly UpdateDownloader _net;
    private readonly Uri _manifestUrl;
    private readonly Version _current;
    private readonly string _currentExe;
    private readonly string _updateDir;
    private readonly Func<string, Version?> _versionOf;
    private readonly Func<DateTimeOffset> _now;
    private readonly object _g = new();
    private Task? _running;
    private UpdateState _state = new(UpdateStage.Idle);

    public AppUpdater(UpdateDownloader net, Uri manifestUrl, Version current, string currentExe, string updateDir,
        Func<string, Version?> versionOf, Func<DateTimeOffset>? now = null)
    {
        _net = net;
        _manifestUrl = manifestUrl;
        _current = AppVersion.Normalize(current);
        _currentExe = currentExe;
        _updateDir = updateDir;
        _versionOf = versionOf;
        _now = now ?? (() => DateTimeOffset.UtcNow);
    }

    public Version Current => _current;
    public UpdateState State { get { lock (_g) return _state; } }

    /// <summary>
    /// Состояние изменилось (из любого потока). Подписчик читает <see cref="State"/>: в нём всегда последнее, даже если
    /// сообщения пришли не по порядку.
    /// </summary>
    public event Action? Changed;

    /// <summary>Идёт проверка или загрузка.</summary>
    public bool Busy { get { lock (_g) return _running is { IsCompleted: false }; } }

    /// <summary>
    /// Проверить, вышла ли новая версия. Во время другой операции возвращает её задачу. По кнопке — не чаще раза в 10 с
    /// (повтор отдаёт прежний ответ); после найденного обновления повторная проверка не сбрасывает его.
    /// </summary>
    public Task CheckAsync(bool manual, CancellationToken ct = default)
    {
        lock (_g)
        {
            if (_running is { IsCompleted: false }) return _running;
            if (_state.Stage is UpdateStage.ReadyToRestart) return Task.CompletedTask;
            if (manual && _state.CheckedAt is { } at && _now() - at < ManualCooldown && _state.Stage is not UpdateStage.Failed)
                return Task.CompletedTask;
            Set(_state with { Stage = UpdateStage.Checking, Error = null });
            return _running = Task.Run(() => CheckCoreAsync(ct), CancellationToken.None);
        }
    }

    private async Task CheckCoreAsync(CancellationToken ct)
    {
        try
        {
            var (m, err) = await _net.GetManifestAsync(_manifestUrl, ct).ConfigureAwait(false);
            lock (_g)
            {
                if (m is null) Set(new UpdateState(UpdateStage.Failed, Error: err, CheckedAt: _now()));
                else if (AppVersion.IsNewer(m.Version, _current)) Set(new UpdateState(UpdateStage.Available, m, CheckedAt: _now()));
                else Set(new UpdateState(UpdateStage.UpToDate, m, CheckedAt: _now()));
            }
        }
        catch (OperationCanceledException) { lock (_g) Set(new UpdateState(UpdateStage.Idle)); }
    }

    /// <summary>
    /// Скачать, проверить и поставить найденное обновление. Повторное нажатие во время загрузки возвращает ту же задачу.
    /// В конце состояние <see cref="UpdateStage.ReadyToRestart"/>: окну остаётся сохранить своё и перезапуститься.
    /// </summary>
    public Task InstallAsync(CancellationToken ct = default)
    {
        lock (_g)
        {
            if (_running is { IsCompleted: false }) return _running;
            if (_state is not { Stage: UpdateStage.Available, Manifest: { } m }) return Task.CompletedTask;
            Set(_state with { Stage = UpdateStage.Downloading, Progress = 0, Error = null });
            return _running = Task.Run(() => InstallCoreAsync(m, ct), CancellationToken.None);
        }
    }

    private async Task InstallCoreAsync(UpdateManifest m, CancellationToken ct)
    {
        string? Fail(string why) { lock (_g) Set(new UpdateState(UpdateStage.Available, m, Error: why, CheckedAt: _state.CheckedAt)); return why; }
        try
        {
            var dir = Path.Combine(_updateDir, AppVersion.Display(m.Version));
            var zip = Path.Combine(dir, "update.zip");
            var lastReport = -1.0;
            var progress = new InlineProgress(p =>
            {
                if (p - lastReport < 0.01 && p < 1) return; // не чаще раза в процент
                lastReport = p;
                lock (_g) if (_state.Stage == UpdateStage.Downloading) Set(_state with { Progress = p });
            });
            var err = await _net.DownloadAsync(m, zip, progress, ct).ConfigureAwait(false);
            if (err is not null) { Fail(err); return; }

            var (exe, xerr) = UpdateInstaller.Extract(zip, Path.Combine(dir, "exe"), m.Version, _versionOf);
            if (exe is null) { Fail(xerr!); return; }
            var swap = UpdateInstaller.Swap(_currentExe, exe);
            if (swap is not null) { Fail(swap); return; }
            lock (_g) Set(new UpdateState(UpdateStage.ReadyToRestart, m, 1, CheckedAt: _state.CheckedAt));
        }
        catch (OperationCanceledException) { Fail("загрузка отменена"); }
        catch (Exception x) when (x is IOException or UnauthorizedAccessException) { Fail(x.Message); }
    }

    private void Set(UpdateState s)
    {
        _state = s;
        var h = Changed;
        if (h is not null) ThreadPool.QueueUserWorkItem(_ => h()); // подписчик не держит нашу блокировку
    }

    /// <summary>Прогресс сразу в том же потоке (Progress&lt;T&gt; без окна шлёт в пул потоков, не по порядку).</summary>
    private sealed class InlineProgress(Action<double> report) : IProgress<double>
    {
        public void Report(double value) => report(value);
    }
}
