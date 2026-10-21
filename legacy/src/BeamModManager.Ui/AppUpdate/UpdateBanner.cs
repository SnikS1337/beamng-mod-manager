using BeamModManager.Core.AppUpdate;
using BeamModManager.Ui.Mvvm;

namespace BeamModManager.Ui.AppUpdate;

/// <summary>Что показывает плашка: от этого зависят полоса, галочка и кнопки.</summary>
public enum BannerPhase
{
    /// <summary>Просто сообщение: «Вышла новая версия», «У вас последняя версия», «Проверяем…».</summary>
    Message,
    /// <summary>Идёт загрузка: полоса едет, справа сколько скачано и сколько осталось, «Отменить».</summary>
    Downloading,
    /// <summary>Скачано, проверяется и распаковывается: полоса полная, по ней бежит блик.</summary>
    Verifying,
    /// <summary>Готово: галочка и отсчёт до перезапуска, «Перезапустить сейчас».</summary>
    Ready,
    /// <summary>Не вышло: полоса янтарная и стоит там, где оборвалось; причина и «Повторить».</summary>
    Failed,
}

/// <summary>
/// Плашка «Вышла новая версия» над сеткой. Одна кнопка: «Обновить» → скачивается (полоса, сколько скачано и сколько
/// осталось, «Отменить»), проверяется и распаковывается, через 3 с программа сама перезапускается уже новой («Перезапустить
/// сейчас» — не ждать). Пока идёт загрузка, «Обновить» нет (нажать дважды нельзя). Автоматическая проверка молчит, если
/// ничего не нашла или нет сети; по кнопке «Проверить» в настройках говорит результат.
/// </summary>
public sealed class UpdateBanner : Observable
{
    /// <summary>Сколько секунд от «готово» до перезапуска: человек успевает увидеть, что программа сейчас закроется.</summary>
    public const int RestartDelay = 3;

    /// <summary>
    /// Сколько секунд видно сообщение «У вас последняя версия» (и «У вас версия …: …») после «Проверить»: это просто
    /// ответ, кнопок у него нет, и он уходит сам (решено 2026-10-18 — «Позже» к нему не подходило).
    /// </summary>
    public const int NoticeSeconds = 5;

    private readonly IAppUpdater? _updater;
    private readonly Action<Action> _ui;
    private readonly Action _restart;
    private readonly Func<DateTimeOffset> _now;
    private readonly Func<TimeSpan, Task> _delay;
    private bool _dismissed, _manual, _restartRequested, _counting;
    private int _left;
    private int _check; // номер проверки по кнопке: сообщение прошлой проверки не прячет ответ новой
    private int _noticeFor = -1; // для какой проверки уже идёт отсчёт до скрытия ответа
    private double? _lastProgress;
    private (DateTimeOffset At, double P)? _start;
    private double _speed; // байт в секунду, сглаженная

    /// <param name="updater">null — обновление недоступно (сборка для разработки, демо).</param>
    /// <param name="ui">Выполнить в потоке окна.</param>
    /// <param name="restart">Сохранить своё и перезапуститься (новая версия уже на месте).</param>
    /// <param name="now">Часы (для «осталось ~10 с»); по умолчанию системные.</param>
    /// <param name="delay">Ожидание секунды отсчёта; по умолчанию Task.Delay (тесты подставляют мгновенное).</param>
    public UpdateBanner(IAppUpdater? updater, Action<Action> ui, Action restart, Func<DateTimeOffset>? now = null, Func<TimeSpan, Task>? delay = null)
    {
        _updater = updater;
        _ui = ui;
        _restart = restart;
        _now = now ?? (() => DateTimeOffset.UtcNow);
        _delay = delay ?? (t => Task.Delay(t));
        if (updater is not null) updater.Changed += () => _ui(Sync);
        Sync();
    }

    public bool Supported => _updater is not null;

    private bool _visible;
    public bool Visible { get => _visible; private set => Set(ref _visible, value); }

    private string _text = "";
    public string Text { get => _text; private set => Set(ref _text, value); }

    private string? _detail;
    /// <summary>Справа мелко: «23 из 57 МБ, ~10 с» во время загрузки; иначе null.</summary>
    public string? Detail { get => _detail; private set => Set(ref _detail, value); }

    private string? _action;
    /// <summary>«Обновить», «Повторить», «Перезапустить сейчас»; null — кнопки нет (идёт загрузка или проверка).</summary>
    public string? ActionLabel { get => _action; private set => Set(ref _action, value); }

    private bool _canDismiss;
    public bool CanDismiss { get => _canDismiss; private set => Set(ref _canDismiss, value); }

    private bool _canCancel;
    /// <summary>«Отменить» — только пока качается (проверку и распаковку не прервать).</summary>
    public bool CanCancel { get => _canCancel; private set => Set(ref _canCancel, value); }

    private BannerPhase _phase;
    public BannerPhase Phase { get => _phase; private set => Set(ref _phase, value); }

    private double? _progress;
    /// <summary>0..1 во время загрузки и проверки; после сбоя загрузки — где оборвалось; иначе null (полосы нет).</summary>
    public double? Progress { get => _progress; private set => Set(ref _progress, value); }

    /// <summary>Проверка при запуске (если не выключена в настройках): ничего не нашла — ничего не показывает.</summary>
    public Task CheckOnStartAsync(CancellationToken ct = default) => _updater?.CheckAsync(manual: false, ct) ?? Task.CompletedTask;

    /// <summary>«Проверить обновления» по кнопке: работает и при выключенной автопроверке, результат показывается.</summary>
    public Task CheckNowAsync(CancellationToken ct = default)
    {
        if (_updater is null) return Task.CompletedTask;
        _manual = true;
        _dismissed = false;
        _check++;
        var t = _updater.CheckAsync(manual: true, ct);
        Sync();
        return t;
    }

    /// <summary>Нажали кнопку на плашке. Повторные нажатия во время загрузки ничего не делают.</summary>
    public void Act(CancellationToken ct = default)
    {
        if (_updater is null) return;
        switch (_updater.State.Stage)
        {
            case UpdateStage.Available: _lastProgress = null; _start = null; _speed = 0; _ = _updater.InstallAsync(ct); break;
            case UpdateStage.Failed: _manual = true; _check++; _ = _updater.CheckAsync(manual: true, ct); break;
            case UpdateStage.ReadyToRestart: RequestRestart(); break;
        }
        Sync();
    }

    /// <summary>«Отменить» во время загрузки: плашка снова «Вышла новая версия».</summary>
    public void Cancel()
    {
        if (!CanCancel) return;
        _updater?.CancelInstall();
        _lastProgress = null;
    }

    /// <summary>«Позже»: до следующего запуска не показываем (загрузку не прерывает: во время неё кнопки нет).</summary>
    public void Dismiss()
    {
        if (_updater?.State.Stage is UpdateStage.Downloading or UpdateStage.ReadyToRestart) return;
        _dismissed = true;
        Sync();
    }

    private void RequestRestart()
    {
        if (_restartRequested) return; // перезапуск один, сколько бы раз ни нажали
        _restartRequested = true;
        Sync();
        _restart();
    }

    /// <summary>Отсчёт «Перезапуск через 3 с»: раз в секунду, потом перезапуск сам (если не нажали «сейчас»).</summary>
    private async Task CountdownAsync()
    {
        for (_left = RestartDelay; _left > 0 && !_restartRequested; _left--)
        {
            _ui(Sync);
            await _delay(TimeSpan.FromSeconds(1)).ConfigureAwait(false);
        }
        _ui(RequestRestart);
    }

    private readonly object _syncGate = new();

    /// <summary>
    /// Показать текущее состояние. Под замком: в окне всё идёт через его поток, но при синхронном <c>ui</c> (тесты) сообщение
    /// обновлятора из пула потоков могло записать новое состояние раньше, чем дописалось старое, и плашка застревала на
    /// «Проверяем…» (редкий сбой теста 2026-10-18).
    /// </summary>
    private void Sync()
    {
        lock (_syncGate) SyncCore();
    }

    private void SyncCore()
    {
        if (_updater is null) { Visible = false; return; }
        var s = _updater.State;
        var ver = s.Manifest is { } m ? AppVersion.Display(m.Version) : "";
        Detail = null;
        CanCancel = false;
        switch (s.Stage)
        {
            case UpdateStage.Available when s.Error is not null:
                Phase = BannerPhase.Failed;
                Progress = _lastProgress ?? (s.Progress > 0 ? s.Progress : null); // полоса стоит там, где оборвалось (если загрузка была)
                Show($"Не удалось обновить до {ver}: {s.Error}", "Повторить", dismiss: true);
                break;
            case UpdateStage.Available:
                Phase = BannerPhase.Message;
                Progress = null;
                Show($"Вышла новая версия {ver}", "Обновить", dismiss: true);
                break;
            case UpdateStage.Downloading when s.Progress >= 1:
                Phase = BannerPhase.Verifying;
                Progress = _lastProgress = 1;
                Show($"Проверяем и распаковываем версию {ver}…", null, dismiss: false);
                break;
            case UpdateStage.Downloading:
                Phase = BannerPhase.Downloading;
                Progress = _lastProgress = Math.Clamp(s.Progress, 0, 1);
                CanCancel = true;
                Detail = s.Manifest is { } dm ? Downloaded(dm.Size, s.Progress) : null;
                Show($"Скачиваем версию {ver}", null, dismiss: false);
                break;
            case UpdateStage.ReadyToRestart:
                Phase = BannerPhase.Ready;
                Progress = null;
                if (!_counting) { _counting = true; _ = CountdownAsync(); } // одно «Обновить» — дальше всё само, как договорились
                if (_restartRequested) Show($"Версия {ver} готова. Перезапускаем…", null, dismiss: false);
                else Show($"Версия {ver} готова. Перезапуск через {Math.Max(1, _left)} с", "Перезапустить сейчас", dismiss: false);
                break;
            case UpdateStage.Checking when _manual:
                Phase = BannerPhase.Message;
                Progress = null;
                Show("Проверяем обновления…", null, dismiss: false);
                break;
            case UpdateStage.UpToDate when _manual && s.Error is not null:
                Phase = BannerPhase.Message;
                Progress = null;
                Show($"У вас версия {AppVersion.Display(_updater.Current)}: {s.Error}", null, dismiss: false);
                HideNoticeLater();
                break;
            case UpdateStage.UpToDate when _manual:
                Phase = BannerPhase.Message;
                Progress = null;
                Show($"У вас последняя версия {AppVersion.Display(_updater.Current)}", null, dismiss: false);
                HideNoticeLater();
                break;
            case UpdateStage.Failed when _manual:
                Phase = BannerPhase.Message;
                Progress = null;
                Show($"Не удалось проверить обновления: {s.Error}", "Повторить", dismiss: true);
                break;
            default:
                Progress = null;
                Visible = false; // автопроверка: «нет обновлений» и «нет сети» не показываем
                break;
        }
    }

    /// <summary>Ответ «У вас последняя версия» уходит сам через <see cref="NoticeSeconds"/> с, если за это время не проверили снова.</summary>
    private void HideNoticeLater()
    {
        if (_noticeFor == _check) return;
        var check = _noticeFor = _check;
        _ = HideAsync();

        async Task HideAsync()
        {
            await _delay(TimeSpan.FromSeconds(NoticeSeconds)).ConfigureAwait(false);
            _ui(() =>
            {
                lock (_syncGate)
                {
                    if (check != _check || _updater?.State.Stage != UpdateStage.UpToDate) return;
                    _manual = false; // дальше как после автопроверки: «нет обновлений» не показываем
                    SyncCore();
                }
            });
        }
    }

    /// <summary>«23 из 57 МБ, ~10 с»: сколько скачано и сколько примерно осталось (по сглаженной скорости, с 1,5 с загрузки).</summary>
    private string Downloaded(long size, double p)
    {
        var now = _now();
        _start ??= (now, p);
        var (t0, p0) = _start.Value;
        var secs = (now - t0).TotalSeconds;
        if (secs > 0.25 && p > p0)
        {
            var speed = (p - p0) * size / secs;
            _speed = _speed <= 0 ? speed : _speed * 0.7 + speed * 0.3;
        }
        var text = $"{Mb(p * size, size)} из {Mb(size, size)} МБ";
        if (secs < 1.5 || _speed <= 0 || p <= 0) return text;
        var left = (1 - p) * size / _speed;
        return text + ", " + Left(left);
    }

    private static string Mb(double bytes, long size) =>
        (bytes / 1048576).ToString(size < 10 * 1048576 ? "0.0" : "0", System.Globalization.CultureInfo.GetCultureInfo("ru-RU"));

    /// <summary>«~10 с», «~2 мин»; меньше 5 с — «несколько секунд». Секунды округляются до 5, чтобы не мелькали.</summary>
    public static string Left(double seconds)
    {
        if (double.IsNaN(seconds) || double.IsInfinity(seconds) || seconds < 0) return "";
        if (seconds < 5) return "несколько секунд";
        if (seconds < 57.5) return $"~{Math.Max(5, (int)Math.Round(seconds / 5) * 5)} с";
        return $"~{Math.Max(1, (int)Math.Round(seconds / 60))} мин";
    }

    private void Show(string text, string? action, bool dismiss)
    {
        Text = text;
        ActionLabel = action;
        CanDismiss = dismiss;
        // загрузку и перезапуск «Позже» не прячет: человек должен видеть, что программа сейчас закроется
        Visible = !_dismissed || _updater!.State.Stage is UpdateStage.Downloading or UpdateStage.ReadyToRestart;
    }
}
