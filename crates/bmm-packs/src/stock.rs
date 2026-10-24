//! Папки машин и карт, которые есть в самой игре.
//!
//! Мод, который кладёт файлы в такую папку, меняет стандартную машину или карту
//! (покраска, тюнинг, конфиги): название из её `info.json` — название стандартной
//! машины, а не мода, поэтому карточке его не даём. Список неполный по природе:
//! новая машина игры, которой здесь нет, даст своё название — это лучше, чем ничего,
//! и поправится дописыванием строки (перенесено из legacy `StockContent`).

const VEHICLES: &[&str] = &[
    // машины
    "atv",
    "autobello",
    "barstow",
    "bastion",
    "bluebuck",
    "bolide",
    "burnside",
    "bx",
    "citybus",
    "covet",
    "etk800",
    "etkc",
    "etki",
    "fullsize",
    "hopper",
    "lansdale",
    "legran",
    "md_series",
    "midsize",
    "midtruck",
    "miramar",
    "moonhawk",
    "nine",
    "pessima",
    "pickup",
    "pigeon",
    "racetruck",
    "roamer",
    "rockbouncer",
    "sbr",
    "scintilla",
    "semi",
    "sunburst",
    "sunburst2",
    "us_semi",
    "utv",
    "van",
    "vivace",
    "wendover",
    "wigeon",
    "wl40",
    "lansdale_wagon",
    "unicycle",
    // прицепы и предметы
    "ball",
    "barrels",
    "barrier",
    "barrier_plastic",
    "blockwall",
    "bollard",
    "boxutility",
    "boxutility_large",
    "cannon",
    "caravan",
    "cardboard_box",
    "cargotrailer",
    "chair",
    "christmas_tree",
    "cones",
    "containertrailer",
    "couch",
    "crowdbarrier",
    "delineator",
    "dolly",
    "dryvan",
    "flail",
    "flatbed",
    "flipramp",
    "frameless_dump",
    "fridge",
    "gate",
    "haybale",
    "inflated_mat",
    "kickplate",
    "large_angletester",
    "large_bridge",
    "large_cannon",
    "large_crusher",
    "large_hamster_wheel",
    "large_roller",
    "large_spinner",
    "large_tilt",
    "large_tire",
    "log_trailer",
    "logs",
    "mattress",
    "metal_box",
    "metal_ramp",
    "piano",
    "porta_potty",
    "pressure_ball",
    "roadsigns",
    "rocks",
    "rollover",
    "sawhorse",
    "shipping_container",
    "steel_coil",
    "streetlight",
    "suspensionbridge",
    "tanker",
    "testroller",
    "tiltdeck",
    "tirestacks",
    "tirewall",
    "trafficbarrel",
    "tsfb",
    "tub",
    "tube",
    "wall",
    "weightpad",
    "woodcrate",
    "woodplanks",
    // добавлено по содержимому игры 0.40.1 (content/vehicles)
    "anticut",
    "dino",
    "dumptruck",
    "engine_props",
    "eu_trailers",
    "marble_block",
    "percheron",
    "rallyflags",
    "rallysigns",
    "rallytape",
    "rock_pile",
    "roof_crush_tester",
    "simple_traffic",
    "spikestrip",
    "trampoline",
    "trashbin",
    "tv",
];

const LEVELS: &[&str] = &[
    "automation_test_track",
    "autotest",
    "cliff",
    "derby",
    "driver_training",
    "east_coast_usa",
    "garage",
    "garage_v2",
    "glow_city",
    "gridmap",
    "gridmap_v2",
    "hirochi_raceway",
    "industrial",
    "italy",
    "johnson_valley",
    "jungle_rock_island",
    "port",
    "showroom_v2",
    "small_island",
    "smallgrid",
    "utah",
    "west_coast_usa",
    "showroom_v2_lighting_simple",
    "template",
];

pub fn is_stock_vehicle(folder: &str) -> bool {
    VEHICLES
        .iter()
        .any(|known| known.eq_ignore_ascii_case(folder))
}

pub fn is_stock_level(folder: &str) -> bool {
    LEVELS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(folder))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_stock_folders_case_insensitively() {
        assert!(is_stock_vehicle("Pessima"));
        assert!(is_stock_vehicle("containerTrailer"));
        assert!(is_stock_level("west_coast_usa"));
        // Есть в игре 0.40.1, раньше не было в списке.
        assert!(is_stock_vehicle("percheron"));
        assert!(is_stock_vehicle("eu_trailers"));
        assert!(is_stock_vehicle("dumptruck"));
        assert!(!is_stock_vehicle("sandero1"));
        assert!(!is_stock_level("my_map"));
    }
}
