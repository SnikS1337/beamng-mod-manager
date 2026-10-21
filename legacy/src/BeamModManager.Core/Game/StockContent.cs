namespace BeamModManager.Core.Game;

/// <summary>
/// Папки машин и карт, которые есть в самой игре (BeamNG.drive 0.3x). Мод, который кладёт файлы в такую папку, меняет
/// стандартную машину или карту (покраска, тюнинг, конфиги): название из её info.json — название стандартной машины,
/// а не мода, поэтому карточке его не даём (2026-10-18). Список неполный по природе: новая машина игры, которой здесь нет,
/// у такого мода даст её название — это лучше, чем ничего, и поправится дописыванием строки.
/// </summary>
public static class StockContent
{
    private static readonly HashSet<string> Vehicles = new(StringComparer.OrdinalIgnoreCase)
    {
        // машины
        "atv", "autobello", "barstow", "bastion", "bluebuck", "bolide", "burnside", "bx", "citybus", "covet", "etk800", "etkc",
        "etki", "fullsize", "hopper", "lansdale", "legran", "md_series", "midsize", "midtruck", "miramar", "moonhawk", "nine",
        "pessima", "pickup", "pigeon", "racetruck", "roamer", "rockbouncer", "sbr", "scintilla", "semi", "sunburst", "sunburst2",
        "us_semi", "utv", "van", "vivace", "wendover", "wigeon", "wl40", "lansdale_wagon", "unicycle",
        // прицепы и предметы
        "ball", "barrels", "barrier", "barrier_plastic", "blockwall", "bollard", "boxutility", "boxutility_large", "cannon",
        "caravan", "cardboard_box", "cargotrailer", "chair", "christmas_tree", "cones", "containerTrailer", "couch",
        "crowdbarrier", "delineator", "dolly", "dryvan", "flail", "flatbed", "flipramp", "frameless_dump", "fridge", "gate",
        "haybale", "inflated_mat", "kickplate", "large_angletester", "large_bridge", "large_cannon", "large_crusher",
        "large_hamster_wheel", "large_roller", "large_spinner", "large_tilt", "large_tire", "log_trailer", "logs", "mattress",
        "metal_box", "metal_ramp", "piano", "porta_potty", "pressure_ball", "roadsigns", "rocks", "rollover", "sawhorse",
        "shipping_container", "steel_coil", "streetlight", "suspensionbridge", "tanker", "testroller", "tiltdeck", "tirestacks",
        "tirewall", "trafficbarrel", "tsfb", "tub", "tube", "wall", "weightpad", "woodcrate", "woodplanks",
    };

    private static readonly HashSet<string> Levels = new(StringComparer.OrdinalIgnoreCase)
    {
        "automation_test_track", "autotest", "cliff", "derby", "driver_training", "east_coast_usa", "garage", "garage_v2",
        "glow_city", "gridmap", "gridmap_v2", "hirochi_raceway", "industrial", "italy", "johnson_valley", "jungle_rock_island",
        "port", "showroom_v2", "small_island", "smallgrid", "utah", "west_coast_usa",
    };

    public static bool IsStockVehicle(string folder) => Vehicles.Contains(folder);

    public static bool IsStockLevel(string folder) => Levels.Contains(folder);
}
