using System.IO;
using BeamModManager.Core.Library;
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

    public DemoBackend(string photos)
    {
        var now = DateTimeOffset.UtcNow;
        string? P(string n) { var p = Path.Combine(photos, n + ".jpg"); return File.Exists(p) ? Path.GetFullPath(p) : null; }
        void Add(string name, string ver, int mb, string photo, string? latest = null, bool on = true, bool tracked = true, bool missing = false,
            UpdateStatus st = UpdateStatus.Likely, string type = "vehicle", string? author = null)
        {
            var id = "demo-" + _mods.Count;
            var mod = new InstalledMod(id, name, id + ".zip", ver, mb * 1024L * 1024, now.AddDays(-60 + _mods.Count), on);
            var b = tracked ? new ModBinding(id, "modland", id, new Uri("https://modland.net/" + id), true) : null;
            var u = latest is null ? null : new UpdateFinding(id, name, "modland", st, ver, latest, b!.Url);
            _mods.Add(new GarageMod(mod, u, b, tracked ? now.AddMinutes(-18) : null, missing, false, null, P(photo), type, author));
        }
        Add("Honda Civic EK9", "1.6", 88, "dacia");
        Add("Subaru Impreza 22B", "1.3", 97, "truck");
        Add("Toyota AE86 Trueno", "2.1", 74, "lambo");
        Add("Dacia Sandero Stepway", "1.0", 142, "dacia", latest: "1.5", author: "kuro_garage");
        Add("Ford Mustang Shelby GT500", "1.2", 120, "mustang", missing: true);
        Add("New Holland T7", "2.3", 96, "tractor");
        Add("Kenworth T610 с цистерной", "1.4b", 210, "truck", latest: "1.5");
        Add("Traffic Reborn", "3.1", 2, "traffic", tracked: false, type: "unknown");
        Add("Заправка Diesel", "1.0", 12, "station", on: false, type: "terrain");
        Add("Lamborghini Huracán EVO", "1.6", 88, "lambo");
        Add("Lebedka Tow", "1.1", 8, "lebedka");
        Add("Falcon RS Coupe", "2.0", 140, "coupe");
        Add("Mazda RX-7 FD", "1.0", 83, "нет-картинки");
        Add("Alpine Pass", "2.0", 512, "нет-картинки", type: "terrain", tracked: false);
        Add("Volga 24 Classic", "1.0", 64, "нет-картинки", type: "sound");
    }

    public GarageSnapshot Load() => new(_mods.ToList(), true, DateTimeOffset.UtcNow.AddMinutes(-18));
    public bool IsGameRunning() => false;

    public ToggleResult SetEnabled(string fileName, bool enabled)
    {
        var i = _mods.FindIndex(m => m.Mod.FileName == fileName);
        if (i < 0) return new(false, ToggleWay.Failed, "Нет такого мода.");
        _mods[i] = _mods[i] with { Mod = _mods[i].Mod with { Enabled = enabled } };
        return new(true, ToggleWay.GameDb, enabled ? "Включён." : "Выключен.");
    }
}
