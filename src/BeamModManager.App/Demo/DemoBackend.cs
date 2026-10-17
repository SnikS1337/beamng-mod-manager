using System.IO;
using BeamModManager.Core.Install;
using BeamModManager.Core.Library;
using BeamModManager.Core.Packs;
using BeamModManager.Core.Safety;
using BeamModManager.Core.Versioning;
using BeamModManager.Ui.Garage;

namespace BeamModManager.App.Demo;

/// <summary>
/// Выдуманный гараж для снимков экрана на сборочной машине (--demo папка_с_картинками): те же моды и состояния,
/// что на доске «Гараж». Картинки — образцы из design/demo, в приложение они не входят.
/// </summary>
public sealed class DemoBackend : IGarageBackend
{
    private readonly List<GarageMod> _mods = new();

    /// <param name="many">Сколько ещё модов добавить к пятнадцати (замер большой библиотеки на сборочной машине, --many).</param>
    public DemoBackend(string photos, int many = 0)
    {
        var now = DateTimeOffset.UtcNow;
        string? P(string n) { var p = Path.Combine(photos, n + ".jpg"); return File.Exists(p) ? Path.GetFullPath(p) : null; }
        void Add(string name, string ver, int mb, string photo, string? latest = null, bool on = true, bool tracked = true, bool missing = false,
            UpdateStatus st = UpdateStatus.Likely, string type = "vehicle", string? author = null, bool hidden = false, int? siteMb = null,
            string site = "modland", int changedDays = 2)
        {
            var id = "demo-" + _mods.Count;
            var mod = new InstalledMod(id, name, id + ".zip", ver, mb * 1024L * 1024, now.AddDays(-60 + _mods.Count), on);
            var b = tracked ? new ModBinding(id, site, id, new Uri($"https://{(site == "modsgaming" ? "modsgaming.ru" : "modland.net")}/{id}"), true) : null;
            var u = latest is null ? null : new UpdateFinding(id, name, site, st, ver, latest, b!.Url) { IsDismissed = hidden };
            _mods.Add(new GarageMod(mod, u, b, tracked ? now.AddMinutes(-18) : null, missing, false, null, P(photo), type, author)
            {
                SiteSizeBytes = siteMb is { } s ? s * 1024L * 1024 : null,
                SiteChangedUtc = latest is null ? null : now.AddDays(-changedDays),
            });
        }
        Add("Honda Civic EK9", "1.6", 88, "dacia");
        Add("Subaru Impreza 22B", "1.3", 97, "truck");
        Add("Toyota AE86 Trueno", "2.1", 74, "lambo");
        Add("Dacia Sandero Stepway", "1.0", 142, "dacia", latest: "1.5", author: "kuro_garage", siteMb: 151);
        Add("Ford Mustang Shelby GT500", "1.2", 120, "mustang", missing: true);
        Add("New Holland T7", "2.3", 96, "tractor");
        Add("Kenworth T610 с цистерной", "1.4b", 210, "truck", latest: "1.5", site: "modsgaming", changedDays: 21);
        Add("Traffic Reborn", "3.1", 2, "traffic", tracked: false, type: "unknown");
        Add("Заправка Diesel", "1.0", 12, "station", on: false, type: "terrain");
        Add("Lamborghini Huracán EVO", "1.6", 88, "lambo");
        Add("Lebedka Tow", "1.1", 410, "lebedka", latest: "1.1", st: UpdateStatus.Possible, siteMb: 438);
        Add("Falcon RS Coupe", "2.0", 140, "coupe", latest: "2.1", hidden: true);
        Add("Mazda RX-7 FD", "1.0", 83, "нет-картинки");
        Add("Alpine Pass", "2.0", 512, "нет-картинки", type: "terrain", tracked: false);
        Add("Volga 24 Classic", "1.0", 64, "нет-картинки", type: "sound");
        string[] photosCycle = { "dacia", "truck", "lambo", "mustang", "tractor", "coupe", "station", "lebedka", "нет-картинки" };
        for (var i = 0; i < Math.Min(many, 20000); i++)
            Add($"Мод номер {i + 1} ({(i % 7 == 0 ? "карта" : "машина")})", $"1.{i % 10}", 20 + i % 300, photosCycle[i % photosCycle.Length],
                latest: i % 11 == 0 && i % 3 != 0 ? $"1.{i % 10 + 1}" : null, on: i % 4 != 0, tracked: i % 3 != 0, type: i % 7 == 0 ? "terrain" : "vehicle");
    }

    /// <summary>Демо-«Обновления» меняют те же моды (скрыть версию, не проверять).</summary>
    internal void Edit(string id, Func<GarageMod, GarageMod> change)
    {
        var i = _mods.FindIndex(m => m.Mod.Id == id);
        if (i >= 0) _mods[i] = change(_mods[i]);
    }

    internal GarageMod? Find(string id) => _mods.FirstOrDefault(m => m.Mod.Id == id);

    public GarageSnapshot Load() => new(_mods.ToList(), true, DateTimeOffset.UtcNow.AddMinutes(-18));
    public bool IsGameRunning() => false;

    public ToggleResult SetEnabled(string fileName, bool enabled)
    {
        var i = _mods.FindIndex(m => m.Mod.FileName == fileName);
        if (i < 0) return new(false, ToggleWay.Failed, "Нет такого мода.");
        _mods[i] = _mods[i] with { Mod = _mods[i].Mod with { Enabled = enabled } };
        return new(true, ToggleWay.GameDb, enabled ? "Включён." : "Выключен.");
    }

    // ───────────── удаление, прошлые версии, «Внутри архива» (выдуманные, для снимков) ─────────────

    private readonly Dictionary<string, (int Index, GarageMod Mod)> _deleted = new();

    public DeleteResult Delete(GarageMod mod)
    {
        var i = _mods.FindIndex(m => m.Mod.Id == mod.Mod.Id);
        if (i < 0) return new(false, "Нет такого мода", null);
        _deleted[mod.Mod.Id] = (i, _mods[i]);
        _mods.RemoveAt(i);
        return new(true, null, new DeletedTicket(mod.Mod.Id, mod.Mod.FileName, "demo", false));
    }

    public OpResult Undelete(DeletedTicket ticket)
    {
        if (!_deleted.Remove(ticket.ModId, out var d)) return new(false, "Нечего возвращать");
        _mods.Insert(Math.Min(d.Index, _mods.Count), d.Mod);
        return new(true);
    }

    public void ForgetDeleted(DeletedTicket ticket) => _deleted.Remove(ticket.ModId);

    public IReadOnlyList<PastVersion> PastVersions(GarageMod mod)
    {
        // у обновлявшихся модов две прошлые версии, у остальных — ни одной
        if (mod.Mod.Version is not { } v || mod.Mod.SizeBytes is not { } size || !mod.Tracked || mod.Mod.Name.StartsWith("Мод номер")) return Array.Empty<PastVersion>();
        var now = DateTimeOffset.UtcNow;
        return new[] { (Older(v, 1), 9), (Older(v, 2), 40) }
            .Where(x => x.Item1 is not null)
            .Select((x, i) => new PastVersion(new BackupEntry($"demo-{i}", mod.Mod.FileName, 2 - i, now.AddDays(-x.Item2), size - (i + 1) * 3L * 1024 * 1024), x.Item1!))
            .ToList();
    }

    private static string? Older(string v, int back)
    {
        var dot = v.LastIndexOf('.');
        return dot > 0 && int.TryParse(v[(dot + 1)..], out var n) && n - back >= 0 ? v[..(dot + 1)] + (n - back) : null;
    }

    public OpResult RestoreVersion(GarageMod mod, PastVersion version)
    {
        Edit(mod.Mod.Id, m => m with { Mod = m.Mod with { Version = version.Version } });
        return new(true);
    }

    public ArchiveOverview? Inside(GarageMod mod) => mod.ModType switch
    {
        "terrain" => new(Array.Empty<string>(), new[] { mod.Mod.Name }, 0, 412, (mod.Mod.SizeBytes ?? 0) * 2, Array.Empty<Finding>()),
        "unknown" => new(Array.Empty<string>(), Array.Empty<string>(), 37, 58, 3L * 1024 * 1024,
            new[] { new Finding(Severity.Warning, "unusual-structure", "В корне архива нет привычных папок мода (vehicles, levels, art, lua, ui и т. п.).") }),
        "sound" => new(Array.Empty<string>(), Array.Empty<string>(), 0, 24, (mod.Mod.SizeBytes ?? 0) * 3 / 2, Array.Empty<Finding>()),
        _ => new(new[] { mod.Mod.Name }, Array.Empty<string>(), mod.Mod.Name.Contains("Kenworth") ? 6 : 0, 1240, (mod.Mod.SizeBytes ?? 0) * 3, Array.Empty<Finding>()),
    };
}
