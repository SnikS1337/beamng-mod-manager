using System.Collections.ObjectModel;
using BeamModManager.Core.Install;
using BeamModManager.Core.Safety;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Mvvm;
using BeamModManager.Ui.Text;

namespace BeamModManager.Ui.Incoming;

/// <summary>Откуда пришёл архив: перетащили на окно (показываем всё, даже «это не мод») или скачали (предлагаем только моды).</summary>
public enum IncomingOrigin { Dropped, Downloaded }

/// <summary>Строка проверки: ✓ в порядке, ! требует внимания, ✕ ставить нельзя.</summary>
public enum CheckLevel { Ok, Attention, Blocked }

public sealed record IncomingCheck(CheckLevel Level, string Text);

/// <summary>Мод из архива в окне проверки: что это, чем станет, что нашла проверка, ставить ли.</summary>
public sealed class IncomingItem : Observable
{
    private readonly Action _changed;

    public IncomingItem(IncomingMod mod, Action changed)
    {
        Mod = mod;
        _changed = changed;
        Checks = ChecksOf(mod);
        Facts = mod.Overview.Broken ? Array.Empty<Fact>() : GarageViewModel.Describe(mod.Overview).Facts;
        _include = CanInstall;
    }

    public IncomingMod Mod { get; }
    public string Title => Mod.Title;
    public string FileName => Mod.FileName;
    public IReadOnlyList<IncomingCheck> Checks { get; }
    public IReadOnlyList<Fact> Facts { get; }
    public bool CanInstall => !Mod.Blocked && Mod.Kind != IncomingKind.AlreadyInstalled;

    /// <summary>«Новый мод», «Обновление: заменит audi_v1.0.zip», «Уже установлен».</summary>
    public string KindText => Mod.Kind switch
    {
        IncomingKind.Update when string.Equals(Mod.Replaces, Mod.FileName, StringComparison.OrdinalIgnoreCase)
            => Mod.ReplacesEnabled ? "Обновление" : "Обновление выключенного: останется выключенным",
        IncomingKind.Update => Mod.ReplacesEnabled ? $"Обновление: заменит {Mod.Replaces}" : $"Обновление выключенного {Mod.Replaces}: останется выключенным",
        IncomingKind.AlreadyInstalled => $"Уже установлен ({Mod.Replaces})",
        _ => "Новый мод",
    };

    public bool IsUpdate => Mod.Kind == IncomingKind.Update;

    private bool _include;
    /// <summary>Ставить ли этот мод (галочка видна, когда модов в архиве больше одного).</summary>
    public bool Include
    {
        get => _include;
        set { if (CanInstall && Set(ref _include, value)) _changed(); }
    }

    private static IReadOnlyList<IncomingCheck> ChecksOf(IncomingMod m)
    {
        var list = new List<IncomingCheck>();
        if (m.Overview.Broken) return new[] { new IncomingCheck(CheckLevel.Blocked, "Архив повреждён и не читается") };
        foreach (var f in m.Overview.Findings.Where(f => f.Severity == Severity.Block).DistinctBy(f => f.Code))
            list.Add(new IncomingCheck(CheckLevel.Blocked, f.Message.TrimEnd('.')));
        if (list.Count == 0) list.Add(new IncomingCheck(CheckLevel.Ok, "Опасных файлов и путей нет"));
        if (m.Repacked) list.Add(new IncomingCheck(CheckLevel.Ok, "Файлы лежали в лишней папке — перепаковали, чтобы игра увидела мод"));
        foreach (var f in m.Overview.Findings.Where(f => f.Severity == Severity.Warning && f.Code is not "nested-root").DistinctBy(f => f.Code))
            list.Add(new IncomingCheck(CheckLevel.Attention, f.Message.TrimEnd('.')));
        if (m.Overview.LuaFiles > 0)
            list.Add(new IncomingCheck(CheckLevel.Attention,
                $"Есть код: {m.Overview.LuaFiles} {Ru.Plural(m.Overview.LuaFiles, "файл", "файла", "файлов")} Lua. Устанавливайте, если доверяете автору"));
        return list;
    }
}

/// <summary>
/// Установка из скачанного или перетащенного архива (2026-10-18). Перетащили — сразу окно проверки: «Проверяем…», потом
/// что внутри, чем станет каждый мод, что нашла проверка, «Установить» и «Отмена». Скачали (включено «Разрешить загрузку
/// модов», программа открыта) — плашка над сеткой «Скачан мод …» с «Посмотреть»; не моды и уже стоящие молча пропускаются.
/// Поставили — тост с «Отменить». Разбор идёт не в потоке окна.
/// </summary>
public sealed class IncomingViewModel : Observable
{
    private readonly IGarageBackend _backend;
    private readonly GarageViewModel _garage;
    private readonly Action<Action> _ui;
    private readonly Queue<IncomingArchive> _offers = new();
    private readonly HashSet<string> _seen = new(StringComparer.OrdinalIgnoreCase);
    private IncomingArchive? _current;
    private int _serial;

    /// <param name="ui">Выполнить в потоке окна.</param>
    public IncomingViewModel(IGarageBackend backend, GarageViewModel garage, Action<Action> ui)
    {
        _backend = backend;
        _garage = garage;
        _ui = ui;
    }

    // ───────────── окно проверки ─────────────

    private bool _open;
    public bool Open { get => _open; private set => Set(ref _open, value); }

    private bool _busy;
    /// <summary>Архив ещё разбирается (распаковка, проверка, сравнение с установленными).</summary>
    public bool Busy { get => _busy; private set => Set(ref _busy, value); }

    private string _title = "";
    public string Title { get => _title; private set => Set(ref _title, value); }

    private string? _problem;
    /// <summary>Почему ставить нечего («В архиве нет мода BeamNG…»); тогда вместо «Установить» — «Понятно».</summary>
    public string? Problem { get => _problem; private set => Set(ref _problem, value); }

    public ObservableCollection<IncomingItem> Items { get; } = new();

    /// <summary>Есть что показать списком (при «ставить нечего» и во время разбора списка нет).</summary>
    public bool HasItems => Items.Count > 0;

    /// <summary>Галочки у модов — только когда их в архиве несколько.</summary>
    public bool Several => Items.Count > 1;

    /// <summary>«Прежняя версия уйдёт в копии…» — если среди выбранных есть обновление.</summary>
    public string? Note => Items.Any(i => i.Include && i.IsUpdate)
        ? "Прежняя версия уйдёт в копии — её можно вернуть в «Прошлых версиях»."
        : null;

    public bool CanInstall => !Busy && Problem is null && Items.Any(i => i.Include);

    public string InstallLabel
    {
        get
        {
            var n = Items.Count(i => i.Include);
            return n > 1 ? $"Установить {n}" : Items.Any(i => i.Include && i.IsUpdate) ? "Обновить" : "Установить";
        }
    }

    // ───────────── плашка «Скачан мод» ─────────────

    private string? _offerText;
    /// <summary>«Скачан мод «Honda Civic»: обновление» над сеткой; null — плашки нет.</summary>
    public string? OfferText { get => _offerText; private set => Set(ref _offerText, value); }

    /// <summary>
    /// Пришёл архив. Перетащенный — сразу в окно проверки (если оно занято — следом). Скачанный — разбирается молча и, если
    /// в нём есть что поставить, становится плашкой «Скачан мод …». Один и тот же файл (то же имя, размер и время) дважды не
    /// предлагается.
    /// </summary>
    public async Task OfferAsync(string path, IncomingOrigin origin, CancellationToken ct = default)
    {
        if (origin == IncomingOrigin.Downloaded && !_seen.Add(Stamp(path))) return;
        var serial = 0;
        if (origin == IncomingOrigin.Dropped)
        {
            if (Open) { await QueueDropAsync(path, ct).ConfigureAwait(false); return; }
            serial = ++_serial;
            ShowBusy(path);
        }
        IncomingArchive a;
        try { a = await Task.Run(() => _backend.InspectIncoming(path, ct), ct).ConfigureAwait(false); }
        catch (OperationCanceledException) { return; }
        _ui(() =>
        {
            if (origin == IncomingOrigin.Dropped)
            {
                if (serial != _serial || !Open) { _backend.DiscardIncoming(a); return; } // окно закрыли, пока разбирали
                Show(a);
                return;
            }
            // скачанное: предлагаем, только если есть что поставить
            if (!a.Mods.Any(m => !m.Blocked && m.Kind != IncomingKind.AlreadyInstalled)) { _backend.DiscardIncoming(a); return; }
            _offers.Enqueue(a);
            if (OfferText is null) NextOffer();
        });
    }

    private async Task QueueDropAsync(string path, CancellationToken ct)
    {
        IncomingArchive a;
        try { a = await Task.Run(() => _backend.InspectIncoming(path, ct), ct).ConfigureAwait(false); }
        catch (OperationCanceledException) { return; }
        // окно успели закрыть, пока разбирали, — открываем этот архив сразу (он перетащен, а не «скачан»)
        _ui(() => { _offers.Enqueue(a); if (!Open && OfferText is null) Review(); });
    }

    /// <summary>«Посмотреть» на плашке: окно проверки с этим архивом.</summary>
    public void Review()
    {
        if (_offers.Count == 0 || Open) return;
        var a = _offers.Dequeue();
        OfferText = null;
        _serial++;
        Show(a);
    }

    /// <summary>«Не сейчас» на плашке: этот архив больше не предлагаем (до перезапуска).</summary>
    public void DismissOffer()
    {
        if (_offers.Count == 0) return;
        _backend.DiscardIncoming(_offers.Dequeue());
        OfferText = null;
        NextOffer();
    }

    private void NextOffer()
    {
        if (_offers.Count == 0) { OfferText = null; return; }
        var a = _offers.Peek();
        var mods = a.Mods.Where(m => !m.Blocked && m.Kind != IncomingKind.AlreadyInstalled).ToList();
        if (mods.Count == 0) { OfferText = null; return; } // перетащенный «не мод» стоит в очереди: окно покажет его само
        OfferText = mods.Count == 1
            ? $"Скачан мод {Ru.Quoted(mods[0].Title, 48)}" + (mods[0].Kind == IncomingKind.Update ? ": обновление" : "")
            : $"Скачан архив {Ru.Quoted(Path.GetFileName(a.SourcePath), 48)}: модов {mods.Count}";
    }

    private void ShowBusy(string path)
    {
        Items.Clear();
        Problem = null;
        Title = $"Проверяем {Ru.Quoted(Path.GetFileName(path), 48)}…";
        Busy = true;
        Open = true;
        RaiseAll();
    }

    private void Show(IncomingArchive a)
    {
        _current = a;
        Items.Clear();
        foreach (var m in a.Mods) Items.Add(new IncomingItem(m, RaiseAll));
        Problem = a.Problem;
        var name = Ru.Quoted(Path.GetFileName(a.SourcePath), 48);
        Title = a.Problem is not null ? $"{name}: ставить нечего"
            : a.Mods.Count == 1 ? $"Архив {name} проверен"
            : $"В архиве {name} модов: {a.Mods.Count}";
        Busy = false;
        Open = true;
        RaiseAll();
    }

    /// <summary>«Отмена» или «Понятно»: окно закрывается, следующий перетащенный архив (если ждёт) открывается сам.</summary>
    public void Close()
    {
        if (_current is { } a) _backend.DiscardIncoming(a);
        _current = null;
        _serial++;
        Open = false;
        Busy = false;
        Items.Clear();
        Problem = null;
        RaiseAll();
        // ждущий перетащенный архив — сразу окном; скачанные остаются плашкой
        if (_offers.Count > 0 && OfferText is null) Review();
    }

    /// <summary>«Установить»: выбранные моды по очереди; тост с «Отменить» (отмена — в обратном порядке).</summary>
    public void Install()
    {
        if (!CanInstall) return;
        var picked = Items.Where(i => i.Include).ToList();
        var tickets = new List<IncomingTicket>();
        var failed = new List<(IncomingItem Item, string Why)>();
        foreach (var it in picked)
        {
            var r = _backend.InstallIncoming(it.Mod);
            if (r.Success && r.Ticket is { } t) tickets.Add(t);
            else failed.Add((it, r.Message ?? "поставить не получилось"));
        }
        Close();
        _garage.Refresh();
        if (tickets.Count > 0) SelectInstalled(tickets[^1].FileName);
        _garage.Notify(ToastText(picked, tickets.Count, failed), tickets.Count == 0 ? null : () => Undo(tickets));
    }

    private void SelectInstalled(string fileName)
    {
        if (_garage.Cards.FirstOrDefault(c => string.Equals(c.Data.Mod.FileName, fileName, StringComparison.OrdinalIgnoreCase)) is { } card)
            _garage.Selected = card;
    }

    private void Undo(List<IncomingTicket> tickets)
    {
        string? why = null;
        for (var i = tickets.Count - 1; i >= 0; i--)
        {
            var r = _backend.UndoIncoming(tickets[i]);
            if (!r.Success) why ??= r.Message;
        }
        _garage.Refresh();
        if (why is not null) _garage.Notify("Вернуть как было не получилось: " + why.TrimEnd('.'));
    }

    private static string ToastText(List<IncomingItem> picked, int ok, List<(IncomingItem Item, string Why)> failed)
    {
        if (failed.Count == 0)
        {
            if (ok == 1) return picked[0].IsUpdate ? $"{Ru.Quoted(picked[0].Title, 40)} обновлён" : $"Установлен {Ru.Quoted(picked[0].Title, 40)}";
            return $"Установлено {ok} {Ru.Plural(ok, "мод", "мода", "модов")}";
        }
        var (item, why) = failed[0];
        if (ok == 0) return $"Не удалось поставить {Ru.Quoted(item.Title, 40)}: {Lower(why)}";
        return $"Установлено {ok} из {picked.Count}. {Ru.Quoted(item.Title, 32)}: {Lower(why)}";
    }

    private static string Lower(string s) => s.Length > 1 && char.IsUpper(s[0]) && !char.IsUpper(s[1]) ? char.ToLowerInvariant(s[0]) + s[1..] : s;

    private static string Stamp(string path)
    {
        try { var fi = new FileInfo(path); return $"{fi.FullName}|{fi.Length}|{fi.LastWriteTimeUtc.Ticks}"; }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or ArgumentException) { return path; }
    }

    private void RaiseAll()
    {
        Raise(nameof(Several));
        Raise(nameof(HasItems));
        Raise(nameof(Note));
        Raise(nameof(CanInstall));
        Raise(nameof(InstallLabel));
    }
}
