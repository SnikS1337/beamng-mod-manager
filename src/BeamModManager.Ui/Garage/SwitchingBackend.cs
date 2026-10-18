using System.Runtime.CompilerServices;
using BeamModManager.Core.Game;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Packs;

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

    // квитанция удаления помнит, какой папке она принадлежит: сменили папку, пока висел тост, — «Отменить» вернёт файл
    // туда, откуда он ушёл, а не в новую папку
    private readonly ConditionalWeakTable<DeletedTicket, IGarageBackend> _owners = new();

    public DeleteResult Delete(GarageMod mod)
    {
        var inner = Inner;
        var r = inner.Delete(mod);
        if (r.Ticket is { } t) _owners.AddOrUpdate(t, inner);
        return r;
    }

    public OpResult Undelete(DeletedTicket ticket) => OwnerOf(ticket).Undelete(ticket);
    public void ForgetDeleted(DeletedTicket ticket) => OwnerOf(ticket).ForgetDeleted(ticket);
    public IReadOnlyList<PastVersion> PastVersions(GarageMod mod) => Inner.PastVersions(mod);
    public OpResult RestoreVersion(GarageMod mod, PastVersion version) => Inner.RestoreVersion(mod, version);
    public ArchiveOverview? Inside(GarageMod mod) => Inner.Inside(mod);
    public OpResult RestoreDeleted(DeletedGarageMod mod) => Inner.RestoreDeleted(mod);

    private IGarageBackend OwnerOf(DeletedTicket t) => _owners.TryGetValue(t, out var b) ? b : Inner;

    // разбор и установка — у папки, где разбирали; «Отменить» — у той, куда поставили (папку могли сменить, пока висел тост)
    private readonly ConditionalWeakTable<IncomingArchive, IGarageBackend> _incoming = new();
    private readonly ConditionalWeakTable<IncomingTicket, IGarageBackend> _installed = new();

    public IncomingArchive InspectIncoming(string path, CancellationToken ct)
    {
        var inner = Inner;
        var a = inner.InspectIncoming(path, ct);
        _incoming.AddOrUpdate(a, inner);
        return a;
    }

    public IncomingResult InstallIncoming(IncomingMod mod)
    {
        var inner = Inner;
        var r = inner.InstallIncoming(mod);
        if (r.Ticket is { } t) _installed.AddOrUpdate(t, inner);
        return r;
    }

    public OpResult UndoIncoming(IncomingTicket ticket) => (_installed.TryGetValue(ticket, out var b) ? b : Inner).UndoIncoming(ticket);
    public void DiscardIncoming(IncomingArchive archive) => (_incoming.TryGetValue(archive, out var b) ? b : Inner).DiscardIncoming(archive);
}
