using BeamModManager.Core.Library;
using BeamModManager.Core.Versioning;
using BeamModManager.Ui.Mvvm;

namespace BeamModManager.Ui.Garage;

/// <summary>Что написано под фото. Каждое состояние читается словами, а не только цветом (DESIGN.md раздел 2).</summary>
public enum CardStatus { Normal, UpdateAvailable, PossiblyUpdated, PageMissing, Disabled, Deleted }

/// <summary>Тип мода для заглушки, когда в архиве нет ни одной картинки.</summary>
public enum PlaceholderKind { Vehicle, Map, Sound, Script, Other }

/// <summary>Плашка в правом верхнем углу фото: голубая точка у обновления, янтарная у сомнительного, снежинка у выключенного.</summary>
public enum ChipKind { None, Update, Warning, Disabled, Deleted }

/// <summary>
/// Одна карточка гаража. Экземпляр живёт, пока мод в библиотеке, и обновляется на месте: так окно может плавно
/// показать изменение (фото «засыпает», номер версии перекатывается), а не перерисовать карточку заново.
/// </summary>
public sealed class ModCard : Observable
{
    public string Id { get; }

    internal ModCard(string id) => Id = id;

    private GarageMod _data = null!;
    public GarageMod Data { get => _data; private set => Set(ref _data, value); }

    private string _name = "";
    public string Name { get => _name; private set => Set(ref _name, value); }

    private string _fileName = "";
    public string FileName { get => _fileName; private set => Set(ref _fileName, value); }

    private CardStatus _status;
    public CardStatus Status { get => _status; private set => Set(ref _status, value); }

    private string _statusText = "";
    /// <summary>«1.0 → 1.5», «Страница пропала», «Выключен», версия или пусто.</summary>
    public string StatusText { get => _statusText; private set => Set(ref _statusText, value); }

    private string _subtitle = "";
    /// <summary>Строка под названием, как в макете: «1.6 · 88 МБ», «3.1 · не проверяется», «1.0 → 1.5» (белым).</summary>
    public string Subtitle { get => _subtitle; private set => Set(ref _subtitle, value); }

    private bool _subtitleStrong;
    /// <summary>Подпись выделена белым (переход версий при обновлении).</summary>
    public bool SubtitleStrong { get => _subtitleStrong; private set => Set(ref _subtitleStrong, value); }

    private ChipKind _chip;
    public ChipKind Chip { get => _chip; private set => Set(ref _chip, value); }

    private string? _chipText;
    public string? ChipText { get => _chipText; private set => Set(ref _chipText, value); }

    private bool _hasUpdateBadge;
    /// <summary>Белая плашка «Есть обновление» на фото.</summary>
    public bool HasUpdateBadge { get => _hasUpdateBadge; private set => Set(ref _hasUpdateBadge, value); }

    private bool _hasWarningDot;
    /// <summary>Янтарная точка у «Страница пропала» и «Возможно, обновлён».</summary>
    public bool HasWarningDot { get => _hasWarningDot; private set => Set(ref _hasWarningDot, value); }

    private bool _enabled;
    /// <summary>Как карточка показывается: с учётом ожидающего переключения (игра запущена).</summary>
    public bool Enabled { get => _enabled; private set => Set(ref _enabled, value); }

    private string? _pendingText;
    /// <summary>«Выключится, когда закроете игру» (DESIGN_FLOWS 3.5) или null.</summary>
    public string? PendingText { get => _pendingText; private set => Set(ref _pendingText, value); }

    private string? _previewPath;
    public string? PreviewPath { get => _previewPath; set => Set(ref _previewPath, value); }

    private PlaceholderKind _placeholder;
    public PlaceholderKind Placeholder { get => _placeholder; private set => Set(ref _placeholder, value); }

    private bool _isSelected;
    public bool IsSelected { get => _isSelected; internal set => Set(ref _isSelected, value); }

    private string? _previewKey;

    /// <summary>Фон дочитал название из архива: карточка меняет подпись сразу, не дожидаясь перечитывания гаража.</summary>
    internal void ApplyArchiveTitle(string? title)
    {
        if (IsDeleted) return;
        Data = Data with { ArchiveTitle = title, ArchiveTitleKnown = true };
        Name = Data.Title;
    }

    /// <param name="pending">Нужное состояние, которое ещё не применено (игра запущена); null — ничего не ждёт.</param>
    /// <param name="live">Изменение уже передано помощнику в игре и вот-вот применится.</param>
    internal void Apply(GarageMod d, bool? pending, bool live = false)
    {
        Data = d;
        Name = d.Title;
        FileName = d.Mod.FileName;
        // картинку, догруженную в фоне, не сбрасываем, пока архив тот же; обновили архив — берём новую
        var key = $"{d.ArchivePath}|{d.Mod.SizeBytes}|{d.Mod.FileName}";
        if (key != _previewKey) { _previewKey = key; PreviewPath = d.PreviewPath; }
        else if (d.PreviewPath is not null) PreviewPath = d.PreviewPath;
        Placeholder = PlaceholderOf(d);

        var enabled = pending ?? d.Mod.Enabled;
        Enabled = enabled;
        PendingText = pending is null ? null
            : live ? (pending.Value ? "Включается…" : "Выключается…")
            : pending.Value ? "Включится, когда закроете игру" : "Выключится, когда закроете игру";

        var u = d.NeedsUpdate ? d.Update : null;
        HasUpdateBadge = u is { Status: UpdateStatus.Likely };
        if (u is { Status: UpdateStatus.Likely })
        {
            Status = CardStatus.UpdateAvailable;
            StatusText = $"{d.Mod.Version ?? "?"} → {u.LatestVersion ?? "?"}";
        }
        else if (u is { Status: UpdateStatus.Possible })
        {
            Status = CardStatus.PossiblyUpdated;
            StatusText = "Возможно, обновлён";
        }
        else if (d.PageMissing)
        {
            Status = CardStatus.PageMissing;
            StatusText = "Страница пропала";
        }
        else if (!enabled)
        {
            Status = CardStatus.Disabled;
            StatusText = "Выключен";
        }
        else
        {
            Status = CardStatus.Normal;
            StatusText = d.Mod.Version ?? "";
        }
        HasWarningDot = Status is CardStatus.PossiblyUpdated or CardStatus.PageMissing;

        (Chip, ChipText) = Status switch
        {
            CardStatus.UpdateAvailable => (ChipKind.Update, "Есть обновление"),
            CardStatus.PossiblyUpdated => (ChipKind.Warning, "Возможно, обновлён"),
            CardStatus.PageMissing => (ChipKind.Warning, "Страница пропала"),
            CardStatus.Disabled => (ChipKind.Disabled, "Выключен"),
            _ => (ChipKind.None, (string?)null),
        };
        if (Chip is not ChipKind.Disabled && !enabled) (Chip, ChipText) = (ChipKind.Disabled, "Выключен");

        SubtitleStrong = Status == CardStatus.UpdateAvailable && PendingText is null;
        if (PendingText is not null) Subtitle = PendingText;
        else if (SubtitleStrong) Subtitle = StatusText;
        else
        {
            var parts = new List<string>();
            if (d.Mod.Version is { Length: > 0 } v) parts.Add(v);
            if (!d.Tracked) parts.Add("не проверяется");
            else if (d.Mod.SizeBytes is { } s) parts.Add(Text.Ru.Size(s));
            Subtitle = string.Join(" · ", parts);
        }
    }

    private DeletedGarageMod? _deleted;
    /// <summary>Удалённый мод (фильтр «Удалённые»): файла нет, есть копия; null — обычный мод.</summary>
    public DeletedGarageMod? Deleted { get => _deleted; private set { if (Set(ref _deleted, value)) Raise(nameof(IsDeleted)); } }
    public bool IsDeleted => _deleted is not null;

    /// <summary>Карточка удалённого: серое фото, «Удалён», «удалён вчера · 88 МБ». Запись о моде забыта — данные из имени файла копии.</summary>
    internal void ApplyDeleted(DeletedGarageMod d, DateTimeOffset now)
    {
        var c = d.Copy;
        var when = c.LastSavedAtUtc ?? now;
        Data = new GarageMod(new InstalledMod(Id, d.Name, c.FileName, d.Version, c.LastSizeBytes, when, false),
            null, null, null, false, false, null, d.PreviewPath, null, null);
        Deleted = d;
        Name = d.Name;
        FileName = c.FileName;
        PreviewPath = d.PreviewPath;
        _previewKey = null;
        Placeholder = PlaceholderKind.Other;
        Enabled = false;
        PendingText = null;
        HasUpdateBadge = false;
        HasWarningDot = false;
        Status = CardStatus.Deleted;
        StatusText = "Удалён";
        (Chip, ChipText) = (ChipKind.Deleted, "Удалён");
        SubtitleStrong = false;
        var parts = new List<string> { "удалён " + Text.Ru.Ago(when, now) };
        if (d.Version is { Length: > 0 } v) parts.Insert(0, v);
        parts.Add(Text.Ru.Size(c.LastSizeBytes));
        Subtitle = string.Join(" · ", parts);
    }

    internal static PlaceholderKind PlaceholderOf(GarageMod d) => d.ModType?.ToLowerInvariant() switch
    {
        "vehicle" => PlaceholderKind.Vehicle,
        "terrain" or "level" => PlaceholderKind.Map,
        "sound" => PlaceholderKind.Sound,
        "scenario" or "app" or "unknown" or "script" => PlaceholderKind.Script,
        _ => PlaceholderKind.Other,
    };
}
