using BeamModManager.Core.AppUpdate;
using BeamModManager.Ui.Mvvm;

namespace BeamModManager.Ui.AppUpdate;

/// <summary>
/// Плашка «Вышла новая версия» над сеткой. Одна кнопка: «Обновить» → скачивается, проверяется, программа сама
/// перезапускается уже новой. Пока идёт загрузка, кнопки нет (нажать дважды нельзя). Автоматическая проверка молчит,
/// если ничего не нашла или нет сети; по кнопке «Проверить» (будущий раздел «Настройки») говорит результат.
/// </summary>
public sealed class UpdateBanner : Observable
{
    private readonly AppUpdater? _updater;
    private readonly Action<Action> _ui;
    private readonly Action _restart;
    private bool _dismissed, _manual, _restartRequested;

    /// <param name="updater">null — обновление недоступно (сборка для разработки, демо).</param>
    /// <param name="ui">Выполнить в потоке окна.</param>
    /// <param name="restart">Сохранить своё и перезапуститься (новая версия уже на месте).</param>
    public UpdateBanner(AppUpdater? updater, Action<Action> ui, Action restart)
    {
        _updater = updater;
        _ui = ui;
        _restart = restart;
        if (updater is not null) updater.Changed += () => _ui(Sync);
        Sync();
    }

    public bool Supported => _updater is not null;

    private bool _visible;
    public bool Visible { get => _visible; private set => Set(ref _visible, value); }

    private string _text = "";
    public string Text { get => _text; private set => Set(ref _text, value); }

    private string? _action;
    /// <summary>«Обновить», «Повторить»; null — кнопки нет (идёт загрузка или перезапуск).</summary>
    public string? ActionLabel { get => _action; private set => Set(ref _action, value); }

    private bool _canDismiss;
    public bool CanDismiss { get => _canDismiss; private set => Set(ref _canDismiss, value); }

    private double? _progress;
    /// <summary>0..1 во время загрузки, иначе null.</summary>
    public double? Progress { get => _progress; private set => Set(ref _progress, value); }

    /// <summary>Проверка при запуске (если не выключена в настройках): ничего не нашла — ничего не показывает.</summary>
    public Task CheckOnStartAsync(CancellationToken ct = default) => _updater?.CheckAsync(manual: false, ct) ?? Task.CompletedTask;

    /// <summary>«Проверить обновления» по кнопке: работает и при выключенной автопроверке, результат показывается.</summary>
    public Task CheckNowAsync(CancellationToken ct = default)
    {
        if (_updater is null) return Task.CompletedTask;
        _manual = true;
        _dismissed = false;
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
            case UpdateStage.Available: _ = _updater.InstallAsync(ct); break;
            case UpdateStage.Failed: _manual = true; _ = _updater.CheckAsync(manual: true, ct); break;
            case UpdateStage.ReadyToRestart: RequestRestart(); break;
        }
        Sync();
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
        _restart();
    }

    private void Sync()
    {
        if (_updater is null) { Visible = false; return; }
        var s = _updater.State;
        var ver = s.Manifest is { } m ? AppVersion.Display(m.Version) : "";
        Progress = s.Stage == UpdateStage.Downloading ? s.Progress : null;
        switch (s.Stage)
        {
            case UpdateStage.Available when s.Error is not null:
                Show($"Не удалось обновить до {ver}: {s.Error}", "Повторить", dismiss: true);
                break;
            case UpdateStage.Available:
                Show($"Вышла новая версия {ver}", "Обновить", dismiss: true);
                break;
            case UpdateStage.Downloading:
                Show($"Скачивается версия {ver}… {(int)Math.Round(s.Progress * 100)} %", null, dismiss: false);
                break;
            case UpdateStage.ReadyToRestart:
                Show($"Версия {ver} готова. Перезапускаем…", null, dismiss: false);
                RequestRestart(); // одно нажатие «Обновить» — дальше всё само, как договорились
                break;
            case UpdateStage.Checking when _manual:
                Show("Проверяем обновления…", null, dismiss: false);
                break;
            case UpdateStage.UpToDate when _manual && s.Error is not null:
                Show($"У вас версия {AppVersion.Display(_updater.Current)}: {s.Error}", null, dismiss: true);
                break;
            case UpdateStage.UpToDate when _manual:
                Show($"У вас последняя версия {AppVersion.Display(_updater.Current)}", null, dismiss: true);
                break;
            case UpdateStage.Failed when _manual:
                Show($"Не удалось проверить обновления: {s.Error}", "Повторить", dismiss: true);
                break;
            default:
                Visible = false; // автопроверка: «нет обновлений» и «нет сети» не показываем
                break;
        }
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
