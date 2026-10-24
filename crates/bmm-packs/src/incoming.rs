use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

use bmm_domain::filename;
use bmm_domain::text::TitleNormalizer;

use crate::extract::{ExtractBudget, archive_kind, extract_archive};
use crate::inspect::{ArchiveLimits, Finding, Severity, inspect_zip, is_known_root};
use crate::overview::read_overview;
use crate::zip_index::{Entry, ZipIndex, read_index};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingKind {
    New,
    Update,
    AlreadyInstalled,
}

#[derive(Debug, Clone)]
pub struct InstalledArchive {
    pub file_name: String,
    pub enabled: bool,
    pub size: i64,
    pub path: PathBuf,
    pub content: HashSet<String>,
    pub sub_folder: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IncomingItem {
    pub zip_path: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub title: String,
    pub kind: IncomingKind,
    pub replaces: Option<String>,
    pub replaces_enabled: bool,
    pub replaces_sub_folder: Option<String>,
    pub repacked: bool,
    pub blocked: bool,
    pub warnings: bool,
    pub findings: Vec<Finding>,
    pub content: HashSet<String>,
}

impl IncomingItem {
    pub fn locked(&self) -> bool {
        self.replaces_sub_folder.is_some() && self.kind == IncomingKind::Update
    }
}

#[derive(Debug, Clone)]
pub struct IncomingAnalysis {
    pub source: PathBuf,
    pub items: Vec<IncomingItem>,
    pub problem: Option<String>,
    pub work_dir: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub struct IncomingOptions {
    pub budget_bytes: u64,
    pub max_mods: usize,
}

impl Default for IncomingOptions {
    fn default() -> Self {
        Self {
            budget_bytes: 6 * 1024 * 1024 * 1024,
            max_mods: 64,
        }
    }
}

pub fn analyze(
    source: &Path,
    installed: &[InstalledArchive],
    work_dir: &Path,
    options: IncomingOptions,
    cancel: &AtomicBool,
) -> IncomingAnalysis {
    if !source.is_file() {
        return fail(source, work_dir, "Файл не найден.");
    }
    if archive_kind(source).is_none() {
        return fail(source, work_dir, "Это не архив ZIP, RAR или 7z.");
    }
    if let Err(error) = fs::create_dir_all(work_dir) {
        return fail(
            source,
            work_dir,
            &format!("Не удалось создать временную папку: {error}"),
        );
    }
    let mut budget = ExtractBudget::new(options.budget_bytes);
    let result = match archive_kind(source) {
        Some("zip") => analyze_zip(
            source,
            installed,
            work_dir,
            options,
            &mut budget,
            cancel,
            0,
            None,
        ),
        Some(_) => {
            let unpacked = work_dir.join("unpacked");
            match extract_archive(source, &unpacked, &mut budget, cancel) {
                Ok(()) => from_folder(
                    &unpacked,
                    source,
                    installed,
                    work_dir,
                    options,
                    &mut budget,
                    cancel,
                ),
                Err(error) => Err(error.to_string()),
            }
        }
        None => Err("RAR и 7z здесь не открываются.".to_string()),
    };
    match result {
        Ok(mut items) if !items.is_empty() => {
            dedupe_new_names(&mut items, installed);
            IncomingAnalysis {
                source: source.to_path_buf(),
                items,
                problem: None,
                work_dir: work_dir.to_path_buf(),
            }
        }
        Ok(_) => fail(
            source,
            work_dir,
            "В архиве нет мода BeamNG: нет папок vehicles, levels и других, которые ждёт игра.",
        ),
        Err(message) => fail(source, work_dir, &message),
    }
}

fn fail(source: &Path, work_dir: &Path, message: &str) -> IncomingAnalysis {
    IncomingAnalysis {
        source: source.to_path_buf(),
        items: Vec::new(),
        problem: Some(message.to_string()),
        work_dir: work_dir.to_path_buf(),
    }
}

/// Ошибка разбора, после которой нельзя продолжать и показывать частичный результат:
/// исчерпан общий бюджет распаковки или пришла отмена.
fn is_fatal_analysis_error(message: &str) -> bool {
    message == "отменено" || message.contains("слишком большой")
}

#[allow(clippy::too_many_arguments)]
fn analyze_zip(
    source: &Path,
    installed: &[InstalledArchive],
    work_dir: &Path,
    options: IncomingOptions,
    budget: &mut ExtractBudget,
    cancel: &AtomicBool,
    depth: usize,
    name: Option<&str>,
) -> Result<Vec<IncomingItem>, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("отменено".to_string());
    }
    let inspection = inspect_zip(source, &ArchiveLimits::default());
    let index = match read_index(source) {
        Ok(index) => index,
        Err(_) if inspection.is_blocked() => {
            return Ok(vec![blocked_item(
                source,
                "Архив повреждён и не читается.",
                inspection,
                name,
            )]);
        }
        Err(_) => return Err("Архив повреждён и не читается.".to_string()),
    };
    let overview = read_overview(source);
    let blocked = inspection.is_blocked();
    let file_name = display_name(name, source);

    // Запрещённый архив не распаковываем и не перепаковываем: окно покажет причину на исходном файле.
    if blocked {
        return Ok(vec![build_item(
            source,
            file_name,
            &index,
            overview.title.clone(),
            inspection,
            installed,
            work_dir,
            &[],
            false,
        )]);
    }

    let shape = shape_of(&index);
    if matches!(shape, Shape::Mod) {
        return Ok(vec![build_item(
            source,
            file_name,
            &index,
            overview.title.clone(),
            inspection,
            installed,
            work_dir,
            &[],
            false,
        )]);
    }
    let mapped = shape_map(&shape);
    if let Some(map) = mapped {
        let repacked = repack(source, map.as_ref(), work_dir, budget, cancel)?;
        let nested_index =
            read_index(&repacked).map_err(|_| "Архив повреждён и не читается.".to_string())?;
        let nested_overview = read_overview(&repacked);
        return Ok(vec![build_item(
            &repacked,
            file_name,
            &nested_index,
            nested_overview.title,
            inspection,
            installed,
            work_dir,
            &[],
            true,
        )]);
    }

    if depth >= 2 {
        return Err("В архиве нет мода BeamNG.".to_string());
    }

    let inner_dir = work_dir.join(format!("inner-{depth}"));
    let mut items = Vec::new();
    let mut nested_count = 0;
    for entry in &index.entries {
        if nested_count >= options.max_mods {
            break;
        }
        if !entry.name.to_lowercase().ends_with(".zip") {
            continue;
        }
        if crate::inspect::is_unsafe_entry_path(&entry.name) {
            continue;
        }
        // Имя уникально на весь разбор: вложенные модпаки одного уровня иначе писали
        // в один и тот же файл и подменяли уже разобранный мод.
        let target = inner_dir.join(format!("{nested_count}-{}.zip", short_unique()));
        if let Err(error) = copy_entry(source, entry, &target, budget, cancel) {
            if is_fatal_analysis_error(&error) {
                return Err(error);
            }
            continue;
        }
        nested_count += 1;
        match analyze_zip(
            &target,
            installed,
            work_dir,
            options,
            budget,
            cancel,
            depth + 1,
            Some(&entry.name),
        ) {
            Ok(mut nested) => items.append(&mut nested),
            // Общий бюджет и отмена относятся ко всему разбору: это не «битый вложенный архив»,
            // а причина остановиться целиком.
            Err(message) if is_fatal_analysis_error(&message) => return Err(message),
            Err(message) if !message.starts_with("В архиве нет мода") => {
                items.push(blocked_item(
                    &target,
                    &message,
                    inspect_zip(&target, &ArchiveLimits::default()),
                    Some(&entry.name),
                ));
            }
            Err(_) => {}
        }
    }

    if items.is_empty() {
        return Err(
            "В архиве нет мода BeamNG: нет папок vehicles, levels и других, которые ждёт игра."
                .to_string(),
        );
    }
    Ok(items)
}

fn from_folder(
    folder: &Path,
    source: &Path,
    installed: &[InstalledArchive],
    work_dir: &Path,
    options: IncomingOptions,
    budget: &mut ExtractBudget,
    cancel: &AtomicBool,
) -> Result<Vec<IncomingItem>, String> {
    let mut zips = Vec::new();
    collect_zips(folder, 0, 4, &mut zips, cancel)?;
    zips.sort_by_key(|path| path.to_string_lossy().to_lowercase());
    if !zips.is_empty() {
        let mut items = Vec::new();
        for zip in zips.into_iter().take(options.max_mods) {
            match analyze_zip(&zip, installed, work_dir, options, budget, cancel, 1, None) {
                Ok(mut nested) => items.append(&mut nested),
                Err(message) if message.starts_with("В архиве нет мода") => continue,
                Err(message) => return Err(message),
            }
        }
        return Ok(items);
    }

    let Some(mod_folder) = find_mod_folder(folder, 3) else {
        return Err(
            "В архиве нет мода BeamNG: нет папок vehicles, levels и других, которые ждёт игра."
                .to_string(),
        );
    };
    let zip_path = work_dir.join(format!("folder-{}.zip", short_unique()));
    zip_folder(&mod_folder, &zip_path, budget, cancel)?;
    let index = read_index(&zip_path).map_err(|_| "Архив повреждён и не читается.".to_string())?;
    let overview = read_overview(&zip_path);
    let file_name = if mod_folder == folder {
        clean_name(source)
    } else {
        let name = mod_folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "mod".to_string());
        format!("{}.zip", clean_stem(&name))
    };
    let inspection = inspect_zip(&zip_path, &ArchiveLimits::default());
    Ok(vec![build_item(
        &zip_path,
        file_name,
        &index,
        overview.title,
        inspection,
        installed,
        work_dir,
        &[],
        true,
    )])
}

#[allow(clippy::too_many_arguments)]
fn build_item(
    zip_path: &Path,
    file_name: String,
    index: &ZipIndex,
    overview_title: Option<String>,
    inspection: crate::inspect::InspectionResult,
    installed: &[InstalledArchive],
    work_dir: &Path,
    existing: &[IncomingItem],
    repacked: bool,
) -> IncomingItem {
    let content = content_keys(index);
    let (kind, replaces, replaces_enabled, replaces_sub_folder) =
        match_item(zip_path, &file_name, &content, installed);
    let file_name = if kind == IncomingKind::New {
        unique_name(&file_name, installed, work_dir, existing)
    } else {
        file_name
    };
    let title = overview_title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| {
            Path::new(&file_name)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| file_name.clone())
        });
    IncomingItem {
        zip_path: zip_path.to_path_buf(),
        file_name,
        size: fs::metadata(zip_path).map(|meta| meta.len()).unwrap_or(0),
        title,
        kind,
        replaces,
        replaces_enabled,
        replaces_sub_folder,
        repacked,
        blocked: inspection.is_blocked(),
        warnings: inspection.has_warnings(),
        findings: inspection.findings,
        content,
    }
}

fn blocked_item(
    source: &Path,
    message: &str,
    inspection: crate::inspect::InspectionResult,
    name: Option<&str>,
) -> IncomingItem {
    let file_name = display_name(name, source);
    let title = Path::new(&file_name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Архив".to_string());
    IncomingItem {
        zip_path: source.to_path_buf(),
        file_name,
        size: fs::metadata(source).map(|meta| meta.len()).unwrap_or(0),
        title,
        kind: IncomingKind::New,
        replaces: None,
        replaces_enabled: false,
        replaces_sub_folder: None,
        repacked: false,
        blocked: true,
        warnings: false,
        findings: {
            let mut findings = inspection.findings;
            findings.push(Finding {
                severity: Severity::Block,
                code: "broken",
                message: message.to_string(),
                entry: None,
            });
            findings
        },
        content: HashSet::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Shape {
    Mod,
    Wrap(String),
    /// Папка машины без `vehicles/` (старый способ раздачи: «распакуйте в vehicles»):
    /// `dir` — путь к ней в архиве, `folder` — её имя.
    Loose {
        dir: String,
        folder: String,
    },
    None,
}

/// Почему игра не загрузит архив из папки модов.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackagingProblem {
    /// Всё лежит в лишней папке («Car/vehicles/…»): программа уберёт её.
    Wrapped,
    /// Папка машины без `vehicles/` («Acadia_16/acadia/*.jbeam»): программа переупакует.
    Loose,
    /// Внутри нет ничего, что ждёт игра (установщик, картинки, архив в архиве).
    NotMod,
}

impl PackagingProblem {
    pub fn code(self) -> &'static str {
        match self {
            Self::Wrapped => "wrapped",
            Self::Loose => "loose",
            Self::NotMod => "not_mod",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "wrapped" => Some(Self::Wrapped),
            "loose" => Some(Self::Loose),
            "not_mod" => Some(Self::NotMod),
            _ => None,
        }
    }

    pub fn fixable(self) -> bool {
        matches!(self, Self::Wrapped | Self::Loose)
    }
}

/// Загрузит ли игра этот архив из папки модов. `Err` — архив сейчас не открыть.
pub fn archive_problem(path: &Path) -> Result<Option<PackagingProblem>, std::io::Error> {
    let index = match read_index(path) {
        Ok(index) => index,
        Err(crate::zip_index::ZipError::Io(error)) => return Err(error),
        Err(crate::zip_index::ZipError::Zip(_)) => return Ok(Some(PackagingProblem::NotMod)),
    };
    Ok(match shape_of(&index) {
        Shape::Mod => None,
        Shape::Wrap(_) => Some(PackagingProblem::Wrapped),
        Shape::Loose { .. } => Some(PackagingProblem::Loose),
        Shape::None => Some(PackagingProblem::NotMod),
    })
}

/// Переупаковать архив так, как его ждёт игра (см. [`PackagingProblem::fixable`]).
/// Возвращает путь к новому архиву в `work_dir`.
pub fn fix_packaging(source: &Path, work_dir: &Path) -> Result<PathBuf, String> {
    let index = read_index(source).map_err(|_| "Архив повреждён и не читается.".to_string())?;
    let map = shape_map(&shape_of(&index))
        .ok_or_else(|| "Этот архив не переупаковать: в нём нет мода BeamNG.".to_string())?;
    fs::create_dir_all(work_dir).map_err(|error| error.to_string())?;
    let mut budget = ExtractBudget::new(IncomingOptions::default().budget_bytes);
    repack(
        source,
        map.as_ref(),
        work_dir,
        &mut budget,
        &AtomicBool::new(false),
    )
}

type EntryMap = Box<dyn Fn(&str) -> Option<String>>;

/// Как переименовать записи архива этой формы, чтобы игра его увидела.
fn shape_map(shape: &Shape) -> Option<EntryMap> {
    match shape {
        Shape::Wrap(folder) => {
            let folder = folder.clone();
            Some(Box::new(move |name: &str| {
                let (first, rest) = name.split_once('/')?;
                (first.eq_ignore_ascii_case(&folder) && !rest.is_empty()).then(|| rest.to_string())
            }))
        }
        Shape::Loose { dir, folder } => {
            let prefix = format!("{}/", dir.to_lowercase());
            let target = format!("vehicles/{folder}/");
            let cut = dir.len() + 1;
            Some(Box::new(move |name: &str| {
                // Сравниваем начало пути той же длины в байтах, что и исходная папка: строчные
                // буквы бывают длиннее заглавных («İ»), и срез по длине строчной строки
                // попадал в середину буквы.
                let name = name.trim_start_matches("./").trim_start_matches('/');
                let head = name.get(..cut)?;
                let rest = name.get(cut..)?;
                (head.to_lowercase() == prefix && !rest.is_empty())
                    .then(|| format!("{target}{rest}"))
            }))
        }
        _ => None,
    }
}

/// Папка машины, упакованная без `vehicles/`: в архиве нет ни одной папки, которую ждёт
/// игра, а файлы `.jbeam` лежат в одной папке (и её подпапках) не глубже трёх уровней.
/// Игра такой архив не загрузит; программа переупаковывает его в `vehicles/<папка>/`.
/// Возвращает путь к папке в архиве и её имя.
pub fn loose_vehicle(index: &ZipIndex) -> Option<(String, String)> {
    let has_known_root = index.entries.iter().any(|entry| {
        entry
            .name
            .trim_start_matches("./")
            .split('/')
            .any(is_known_root)
    });
    if has_known_root {
        return None;
    }
    let mut dirs: Vec<String> = index
        .entries
        .iter()
        .filter(|entry| entry.name.to_lowercase().ends_with(".jbeam"))
        .filter_map(|entry| {
            let name = entry.name.trim_start_matches("./").trim_start_matches('/');
            name.rsplit_once('/').map(|(dir, _)| dir.to_string())
        })
        .collect();
    if dirs.is_empty() {
        return None;
    }
    dirs.sort_by_key(|dir| dir.matches('/').count());
    let top = dirs[0].clone();
    let inside = |dir: &str| {
        dir.eq_ignore_ascii_case(&top)
            || dir
                .to_lowercase()
                .starts_with(&format!("{}/", top.to_lowercase()))
    };
    if !dirs.iter().all(|dir| inside(dir)) || top.matches('/').count() > 2 {
        return None;
    }
    let folder = top.rsplit('/').next().unwrap_or(&top).to_string();
    if folder.is_empty() || crate::inspect::is_unsafe_entry_path(&folder) {
        return None;
    }
    Some((top, folder))
}

fn shape_of(index: &ZipIndex) -> Shape {
    // Первые сегменты путей без учёта регистра: «Car/…» и «car/…» — одна и та же лишняя папка.
    let mut tops: Vec<&str> = Vec::new();
    let mut root_has_files = false;
    for entry in &index.entries {
        let name = entry.name.trim_start_matches("./").trim_start_matches('/');
        let first = name.split('/').find(|part| !part.is_empty());
        if let Some(first) = first
            && !tops.iter().any(|known| known.eq_ignore_ascii_case(first))
        {
            tops.push(first);
        }
        if entry.depth == 1 {
            root_has_files = true;
        }
    }

    // Файл с именем «vehicles» в корне — не папка мода: известный корень должен быть
    // первым сегментом пути, за которым что-то есть (как в legacy ShapeOf).
    let is_mod = index.entries.iter().any(|entry| {
        let name = entry.name.trim_start_matches("./").trim_start_matches('/');
        let mut parts = name.split('/').filter(|part| !part.is_empty());
        parts.next().map(is_known_root).unwrap_or(false) && parts.next().is_some()
    });
    if is_mod {
        return Shape::Mod;
    }

    if tops.len() == 1 && !root_has_files {
        let folder = tops[0];
        let has_known_child = index.entries.iter().any(|entry| {
            let name = entry.name.trim_start_matches("./").trim_start_matches('/');
            let mut parts = name.split('/').filter(|part| !part.is_empty());
            match (parts.next(), parts.next()) {
                (Some(first), Some(second)) if first.eq_ignore_ascii_case(folder) => {
                    is_known_root(second)
                }
                _ => false,
            }
        });
        if has_known_child {
            return Shape::Wrap(folder.to_string());
        }
    }
    if let Some((dir, folder)) = loose_vehicle(index) {
        return Shape::Loose { dir, folder };
    }
    Shape::None
}

pub fn content_keys(index: &ZipIndex) -> HashSet<String> {
    let mut keys = HashSet::new();
    for entry in &index.entries {
        let parts: Vec<&str> = entry
            .name
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        if parts.len() >= 2 && is_known_root(parts[0]) {
            keys.insert(format!(
                "{}/{}",
                parts[0].to_lowercase(),
                parts[1].to_lowercase()
            ));
        }
    }
    keys
}

fn match_item(
    zip_path: &Path,
    file_name: &str,
    content: &HashSet<String>,
    installed: &[InstalledArchive],
) -> (IncomingKind, Option<String>, bool, Option<String>) {
    let size = fs::metadata(zip_path)
        .map(|meta| meta.len() as i64)
        .unwrap_or(0);
    let hash = if installed.iter().any(|item| item.size == size) {
        sha256_file(zip_path).ok()
    } else {
        None
    };
    if let Some(hash) = &hash
        && let Some(found) = installed.iter().find(|item| {
            item.size == size && sha256_file(&item.path).ok().as_deref() == Some(hash.as_str())
        })
    {
        return (
            IncomingKind::AlreadyInstalled,
            Some(found.file_name.clone()),
            found.enabled,
            found.sub_folder.clone(),
        );
    }

    if let Some(found) = installed
        .iter()
        .find(|item| item.file_name.eq_ignore_ascii_case(file_name))
    {
        return (
            IncomingKind::Update,
            Some(found.file_name.clone()),
            found.enabled,
            found.sub_folder.clone(),
        );
    }

    // Сравниваем только то, что называет мод: свои машины и карты. Общие папки
    // («art/sound», «ui/…», «vehicles/common») есть у многих модов: по ним «Audi R8»
    // становился обновлением «Mercedes ML».
    let content = identity_keys(content);
    let candidate_count = content.len();
    let mut best: Option<(&InstalledArchive, f64)> = None;
    let mut tie = false;
    for item in installed {
        let item_content = identity_keys(&item.content);
        if item_content.is_empty() || candidate_count == 0 {
            continue;
        }
        let shared = content.intersection(&item_content).count();
        let score = shared as f64 / candidate_count.max(item_content.len()) as f64;
        if score < 0.5 {
            continue;
        }
        match best {
            None => best = Some((item, score)),
            Some((_, current)) if score > current + 1e-9 => {
                best = Some((item, score));
                tie = false;
            }
            Some((_, current)) if (score - current).abs() < 1e-9 => tie = true,
            _ => {}
        }
    }
    // Два одинаково похожих установленных — не гадаем: мод новый. К поиску по названию
    // в этом случае не переходим, иначе случайное совпадение выдало бы двусмысленное за обновление.
    if tie {
        return (IncomingKind::New, None, false, None);
    }
    if let Some((found, _)) = best {
        return (
            IncomingKind::Update,
            Some(found.file_name.clone()),
            found.enabled,
            found.sub_folder.clone(),
        );
    }

    let key = name_key(file_name);
    if !key.is_empty() {
        let matches: Vec<&InstalledArchive> = installed
            .iter()
            .filter(|item| name_key(&item.file_name) == key)
            .collect();
        if matches.len() == 1 {
            let found = matches[0];
            return (
                IncomingKind::Update,
                Some(found.file_name.clone()),
                found.enabled,
                found.sub_folder.clone(),
            );
        }
    }

    (IncomingKind::New, None, false, None)
}

/// Ключи содержимого, по которым мод узнаётся: `vehicles/<машина>` (кроме общей
/// `vehicles/common`) и `levels/<карта>`.
fn identity_keys(keys: &HashSet<String>) -> HashSet<String> {
    keys.iter()
        .filter(|key| {
            (key.starts_with("vehicles/") && key.as_str() != "vehicles/common")
                || key.starts_with("levels/")
        })
        .cloned()
        .collect()
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 81920];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn clean_name(source: &Path) -> String {
    // Не Path::file_name: на Windows «a:b.zip» он прочитал бы как диск «a:» и файл «b.zip»,
    // а имя записи внутри архива может содержать что угодно (в legacy было так же).
    let full = source.to_string_lossy();
    let raw = full.rsplit(['/', '\\']).next().unwrap_or("").to_string();
    let raw = if raw.is_empty() {
        "mod".to_string()
    } else {
        raw
    };
    let stem = raw.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(&raw);
    let stem = strip_copy_suffix(stem);
    format!("{}.zip", clean_stem(&stem))
}

fn display_name(name: Option<&str>, source: &Path) -> String {
    name.map(|value| clean_name(Path::new(value)))
        .unwrap_or_else(|| clean_name(source))
}

fn dedupe_new_names(items: &mut [IncomingItem], installed: &[InstalledArchive]) {
    let mut used: HashSet<String> = HashSet::new();
    for item in items {
        let mut candidate = item.file_name.clone();
        if item.kind == IncomingKind::New {
            let stem = candidate
                .strip_suffix(".zip")
                .unwrap_or(&candidate)
                .to_string();
            let mut index = 2;
            while used.contains(&candidate.to_lowercase())
                || installed
                    .iter()
                    .any(|entry| entry.file_name.eq_ignore_ascii_case(&candidate))
            {
                candidate = format!("{stem}_{index}.zip");
                index += 1;
            }
        }
        used.insert(candidate.to_lowercase());
        item.file_name = candidate;
    }
}

fn clean_stem(raw: &str) -> String {
    let mut text = raw.trim().trim_end_matches('.').to_string();
    if let Some(start) = text.rfind('(')
        && text.ends_with(')')
        && text[start + 1..text.len() - 1]
            .chars()
            .all(|ch| ch.is_ascii_digit())
    {
        text.truncate(start);
        text = text.trim_end().to_string();
    }
    let mut result: String = text
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                ch
            }
        })
        .collect();
    result = result.trim().trim_end_matches('.').to_string();
    if result.is_empty() || result.starts_with('.') {
        result = "mod".to_string();
    }
    result.chars().take(120).collect()
}

fn strip_copy_suffix(stem: &str) -> String {
    let trimmed = stem.trim();
    if let Some(start) = trimmed.rfind('(')
        && trimmed.ends_with(')')
        && trimmed[start + 1..trimmed.len() - 1]
            .chars()
            .all(|ch| ch.is_ascii_digit())
    {
        return trimmed[..start].trim_end().to_string();
    }
    trimmed.to_string()
}

fn unique_name(
    base: &str,
    installed: &[InstalledArchive],
    work_dir: &Path,
    existing: &[IncomingItem],
) -> String {
    let base = if crate::safe_name::safe_file_name(Some(base)).is_some() {
        base.to_string()
    } else {
        "mod.zip".to_string()
    };
    let stem = base.strip_suffix(".zip").unwrap_or(&base);
    let mut name = base.clone();
    let mut index = 2;
    while installed
        .iter()
        .any(|item| item.file_name.eq_ignore_ascii_case(&name))
        || existing
            .iter()
            .any(|item| item.file_name.eq_ignore_ascii_case(&name))
        || work_dir.join(&name).exists()
    {
        name = format!("{stem}_{index}.zip");
        index += 1;
    }
    name
}

fn name_key(file_name: &str) -> String {
    TitleNormalizer::normalize(&filename::from_file_name(file_name).name)
}

/// Новый архив из записей `source`, переименованных `map` (`None` — запись не нужна):
/// снять лишнюю папку или положить папку машины в `vehicles/`.
pub(crate) fn repack(
    source: &Path,
    map: &dyn Fn(&str) -> Option<String>,
    work_dir: &Path,
    budget: &mut ExtractBudget,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let index = read_index(source).map_err(|_| "Архив повреждён и не читается.".to_string())?;
    let target = work_dir.join(format!("repacked-{}.zip", short_unique()));
    let file = File::create(&target).map_err(|error| error.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    // Архив открываем один раз, а записи переносим как есть, без распаковки и нового
    // сжатия: переупаковка 36 МБ занимала ~40 с, теперь — доли секунды.
    let mut archive = crate::zip_index::open_archive(source).map_err(|error| error.to_string())?;
    for entry in &index.entries {
        if cancel.load(Ordering::Relaxed) {
            return Err("отменено".to_string());
        }
        // Лишняя папка могла быть записана в разном регистре в разных записях.
        let Some(rest) = map(&entry.name) else {
            continue;
        };
        let rest = rest.as_str();
        if rest.is_empty() || crate::inspect::is_unsafe_entry_path(rest) {
            continue;
        }
        budget.add(entry.size).map_err(|error| error.to_string())?;
        let nested = archive
            .by_name(&entry.raw_name)
            .map_err(|error| error.to_string())?;
        writer
            .raw_copy_file_rename(nested, rest)
            .map_err(|error| error.to_string())?;
    }
    writer.finish().map_err(|error| error.to_string())?;
    Ok(target)
}

fn zip_folder(
    folder: &Path,
    target: &Path,
    budget: &mut ExtractBudget,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let file = File::create(target).map_err(|error| error.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            if cancel.load(Ordering::Relaxed) {
                return Err("отменено".to_string());
            }
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if is_reparse(&metadata) {
                continue;
            }
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            let Ok(relative) = path.strip_prefix(folder) else {
                continue;
            };
            let name = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if crate::inspect::is_unsafe_entry_path(&name) {
                continue;
            }
            writer
                .start_file(&name, options)
                .map_err(|error| error.to_string())?;
            let mut input = File::open(&path).map_err(|error| error.to_string())?;
            let mut buffer = [0u8; 81920];
            loop {
                let read = input.read(&mut buffer).map_err(|error| error.to_string())?;
                if read == 0 {
                    break;
                }
                budget.add(read as u64).map_err(|error| error.to_string())?;
                writer
                    .write_all(&buffer[..read])
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    writer.finish().map_err(|error| error.to_string())?;
    Ok(())
}

fn collect_zips(
    folder: &Path,
    depth: usize,
    max_depth: usize,
    result: &mut Vec<PathBuf>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if depth > max_depth {
        return Ok(());
    }
    let Ok(entries) = fs::read_dir(folder) else {
        return Ok(());
    };
    for entry in entries.filter_map(Result::ok) {
        if cancel.load(Ordering::Relaxed) {
            return Err("отменено".to_string());
        }
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if is_reparse(&metadata) {
            continue;
        }
        if metadata.is_dir() {
            collect_zips(&path, depth + 1, max_depth, result, cancel)?;
        } else if metadata.is_file()
            && path
                .extension()
                .map(|value| value.to_string_lossy().eq_ignore_ascii_case("zip"))
                .unwrap_or(false)
        {
            result.push(path);
        }
    }
    Ok(())
}

fn find_mod_folder(root: &Path, max_depth: usize) -> Option<PathBuf> {
    let mut queue = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = queue.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut children = Vec::new();
        let mut has_known = false;
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if is_reparse(&metadata) {
                continue;
            }
            if metadata.is_dir() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if is_known_root(&name) {
                    has_known = true;
                }
                children.push(path);
            }
        }
        if has_known {
            return Some(dir);
        }
        if depth < max_depth {
            for child in children {
                queue.insert(0, (child, depth + 1));
            }
        }
    }
    None
}

fn copy_entry(
    source: &Path,
    entry: &Entry,
    target: &Path,
    budget: &mut ExtractBudget,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut archive = crate::zip_index::open_archive(source).map_err(|error| error.to_string())?;
    let mut nested = archive
        .by_name(&entry.raw_name)
        .map_err(|error| error.to_string())?;
    let mut output = File::create(target).map_err(|error| error.to_string())?;
    let mut buffer = [0u8; 81920];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("отменено".to_string());
        }
        let read = nested
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        budget.add(read as u64).map_err(|error| error.to_string())?;
        output
            .write_all(&buffer[..read])
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn is_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Уникальная часть имени временного файла. Часы Windows идут шагом 100 нс, поэтому
/// к времени добавлен счётчик: два вызова подряд не дадут одно имя.
fn short_unique() -> String {
    use std::sync::atomic::AtomicU64;
    use std::time::{SystemTime, UNIX_EPOCH};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn build_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn installed(file_name: &str, content: &[&str]) -> InstalledArchive {
        installed_with(file_name, true, content, None)
    }

    fn installed_with(
        file_name: &str,
        enabled: bool,
        content: &[&str],
        sub_folder: Option<&str>,
    ) -> InstalledArchive {
        InstalledArchive {
            file_name: file_name.to_string(),
            enabled,
            size: 0,
            path: PathBuf::from("missing"),
            content: content.iter().map(|value| value.to_string()).collect(),
            sub_folder: sub_folder.map(str::to_string),
        }
    }

    #[test]
    fn zip_mod_is_analyzed_as_new() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("BMW M3 (1).ZIP");
        build_zip(&source, &[("vehicles/bmw/a.jbeam", b"x")]);
        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert!(analysis.problem.is_none(), "{:?}", analysis.problem);
        assert_eq!(analysis.items.len(), 1);
        let item = &analysis.items[0];
        assert_eq!(item.kind, IncomingKind::New);
        assert_eq!(item.file_name, "BMW M3.zip");
        assert_eq!(item.title, "bmw");
        assert!(!item.repacked);
    }

    #[test]
    fn wrapper_is_repacked() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("Car.zip");
        build_zip(
            &source,
            &[
                ("Wrapper/vehicles/car/a.jbeam", b"x"),
                ("Wrapper/vehicles/car/info.json", b"{}"),
            ],
        );
        let before = fs::read(&source).unwrap();
        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        let item = &analysis.items[0];
        assert!(item.repacked);
        assert_eq!(item.file_name, "Car.zip");
        assert!(item.zip_path != source);
        assert_eq!(fs::read(&source).unwrap(), before, "исходник не тронут");
        let index = read_index(&item.zip_path).unwrap();
        assert!(
            index
                .entries
                .iter()
                .all(|entry| !entry.name.starts_with("Wrapper/"))
        );
    }

    #[test]
    fn wrapper_folder_is_matched_case_insensitively() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("Mixed.zip");
        build_zip(
            &source,
            &[
                ("Car/vehicles/car/a.jbeam", b"x"),
                ("car/levels/town/info.json", b"{}"),
                ("CAR/art/x.dds", b"dds"),
            ],
        );
        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert!(analysis.problem.is_none(), "{:?}", analysis.problem);
        let item = &analysis.items[0];
        assert!(item.repacked);
        let index = read_index(&item.zip_path).unwrap();
        let names: Vec<&str> = index
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert!(names.contains(&"vehicles/car/a.jbeam"));
        assert!(names.contains(&"levels/town/info.json"));
        assert!(names.contains(&"art/x.dds"));
    }

    #[test]
    fn root_file_named_like_known_folder_is_not_a_mod() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("odd.zip");
        build_zip(&source, &[("vehicles", b"x"), ("readme.txt", b"y")]);
        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert!(analysis.items.is_empty());
        assert!(
            analysis
                .problem
                .as_deref()
                .unwrap()
                .starts_with("В архиве нет мода BeamNG")
        );
    }

    #[test]
    fn update_by_name_and_content() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("new.zip");
        build_zip(&source, &[("vehicles/car/a.jbeam", b"x")]);
        let by_name = analyze(
            &source,
            &[installed("new.zip", &["vehicles/car"])],
            &temp.path().join("work1"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert_eq!(by_name.items[0].kind, IncomingKind::Update);

        let by_content = analyze(
            &source,
            &[installed("old.zip", &["vehicles/car"])],
            &temp.path().join("work2"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert_eq!(by_content.items[0].kind, IncomingKind::Update);
        assert_eq!(by_content.items[0].replaces.as_deref(), Some("old.zip"));
    }

    #[test]
    fn shared_generic_folders_do_not_make_an_update() {
        // Как в паке Audi: «Audi R8 2020 Update» и установленный Mercedes ML делят только art/sound.
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("Audi R8 2020 Update.zip");
        build_zip(
            &source,
            &[("art/sound/r8.ogg", b"x"), ("vehicles/r8/r8.jbeam", b"x")],
        );
        let analysis = analyze(
            &source,
            &[installed(
                "MB_w164_ken_3.0_f4rza.zip",
                &["art/sound", "vehicles/ml55"],
            )],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert_eq!(analysis.items[0].kind, IncomingKind::New);
        assert_eq!(analysis.items[0].replaces, None);

        let common = analyze(
            &source,
            &[installed(
                "other.zip",
                &["vehicles/common", "vehicles/ml55"],
            )],
            &temp.path().join("work2"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert_eq!(common.items[0].kind, IncomingKind::New);
    }

    #[test]
    fn loose_vehicle_folder_is_repacked_into_vehicles() {
        // Как Acadia с Modland: «Acadia_16/acadia/*.jbeam» без vehicles/.
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("Acadia_16_modland.zip");
        build_zip(
            &source,
            &[
                ("Acadia_16/acadia/acadia.jbeam", b"{}"),
                ("Acadia_16/acadia/wheels/wheels.jbeam", b"{}"),
                ("Acadia_16/acadia/acadia.dae", b"x"),
                ("Acadia_16/readme.txt", b"x"),
            ],
        );
        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert_eq!(analysis.items.len(), 1);
        let item = &analysis.items[0];
        assert!(item.repacked);
        assert!(!item.blocked);
        assert!(item.content.contains("vehicles/acadia"));
        let names: Vec<String> = read_index(&item.zip_path)
            .unwrap()
            .entries
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert!(
            names.contains(&"vehicles/acadia/acadia.jbeam".to_string()),
            "{names:?}"
        );
        assert!(names.contains(&"vehicles/acadia/wheels/wheels.jbeam".to_string()));
        assert!(!names.iter().any(|name| name.contains("readme")));
    }

    #[test]
    fn packaging_problems_are_found_and_fixed_in_place() {
        let temp = tempfile::tempdir().unwrap();
        let loose = temp.path().join("acadia.zip");
        build_zip(&loose, &[("Acadia_16/acadia/acadia.jbeam", b"{}")]);
        assert_eq!(
            archive_problem(&loose).unwrap(),
            Some(PackagingProblem::Loose)
        );
        let fixed = fix_packaging(&loose, &temp.path().join("work")).unwrap();
        assert_eq!(archive_problem(&fixed).unwrap(), None);

        let wrapped = temp.path().join("wrapped.zip");
        build_zip(&wrapped, &[("Car/vehicles/car/car.jbeam", b"{}")]);
        assert_eq!(
            archive_problem(&wrapped).unwrap(),
            Some(PackagingProblem::Wrapped)
        );
        let fixed = fix_packaging(&wrapped, &temp.path().join("work")).unwrap();
        assert_eq!(archive_problem(&fixed).unwrap(), None);

        let installer = temp.path().join("RealSmoke_3D_Setup.zip");
        build_zip(
            &installer,
            &[
                ("install.bat", b"x"),
                ("RealSmoke_3D.zip", b"x"),
                ("README_INSTALL.txt", b"x"),
            ],
        );
        assert_eq!(
            archive_problem(&installer).unwrap(),
            Some(PackagingProblem::NotMod)
        );
        assert!(fix_packaging(&installer, &temp.path().join("work")).is_err());

        let good = temp.path().join("good.zip");
        build_zip(&good, &[("vehicles/car/car.jbeam", b"{}")]);
        assert_eq!(archive_problem(&good).unwrap(), None);
    }

    #[test]
    fn loose_vehicle_needs_one_folder_and_no_game_roots() {
        let temp = tempfile::tempdir().unwrap();
        let two = temp.path().join("two.zip");
        build_zip(&two, &[("a/a.jbeam", b"{}"), ("b/b.jbeam", b"{}")]);
        assert_eq!(
            loose_vehicle(&read_index(&two).unwrap()),
            None,
            "две машины — не гадаем"
        );

        let normal = temp.path().join("normal.zip");
        build_zip(&normal, &[("vehicles/car/car.jbeam", b"{}")]);
        assert_eq!(loose_vehicle(&read_index(&normal).unwrap()), None);

        let flat = temp.path().join("flat.zip");
        build_zip(&flat, &[("car/car.jbeam", b"{}"), ("car/info.json", b"{}")]);
        assert_eq!(
            loose_vehicle(&read_index(&flat).unwrap()),
            Some(("car".to_string(), "car".to_string()))
        );
    }

    #[test]
    fn not_a_mod_is_reported() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("photos.zip");
        build_zip(&source, &[("readme.txt", b"x")]);
        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert!(analysis.items.is_empty());
        assert!(analysis.problem.unwrap().contains("нет мода BeamNG"));
    }

    #[test]
    fn clean_names() {
        assert_eq!(clean_stem("a:b"), "a_b");
        assert_eq!(clean_stem("mod (1)"), "mod");
        assert_eq!(clean_stem(".hidden"), "mod");
        assert_eq!(clean_name(Path::new("mod (1).zip")), "mod.zip");
        assert_eq!(clean_name(Path::new("mod.rar")), "mod.zip");
        assert_eq!(clean_name(Path::new("a:b.zip")), "a_b.zip");
        assert_eq!(clean_name(Path::new(".zip")), "mod.zip");
    }

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default();
            for (name, bytes) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn stored_zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for (name, bytes) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn build_zip_stored(path: &Path, entries: &[(&str, &[u8])]) {
        fs::write(path, stored_zip_bytes(entries)).unwrap();
    }

    /// Детерминированный генератор для пачки битых архивов (без внешних зависимостей).
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) as u32
        }
    }

    #[test]
    fn pack_keeps_inner_names_and_dedupes() {
        let temp = tempfile::tempdir().unwrap();
        let audi = zip_bytes(&[("vehicles/audi/a.jbeam", b"a")]);
        let bmw = zip_bytes(&[("vehicles/bmw/b.jbeam", b"b")]);
        let source = temp.path().join("cars.zip");
        build_zip(
            &source,
            &[
                ("cars/audi_r8.zip", &audi),
                ("cars/bmw_m3.zip", &bmw),
                ("other/audi_r8.zip", &audi),
            ],
        );
        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert!(analysis.problem.is_none(), "{:?}", analysis.problem);
        let names: Vec<&str> = analysis
            .items
            .iter()
            .map(|item| item.file_name.as_str())
            .collect();
        assert_eq!(names, vec!["audi_r8.zip", "bmw_m3.zip", "audi_r8_2.zip"]);
    }

    #[test]
    fn corrupt_archive_is_a_blocked_item() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("broken.zip");
        fs::write(&source, [0x50, 0x4B, 3, 4, 1, 2, 3]).unwrap();
        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert!(analysis.problem.is_none(), "{:?}", analysis.problem);
        assert_eq!(analysis.items.len(), 1);
        assert!(analysis.items[0].blocked);
        assert!(
            analysis.items[0]
                .findings
                .iter()
                .any(|finding| finding.severity == Severity::Block)
        );
    }

    #[test]
    fn blocked_wrapper_is_not_repacked() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("evil.zip");
        build_zip(
            &source,
            &[
                ("Wrapper/vehicles/car/a.jbeam", b"x"),
                ("Wrapper/setup.exe", b"x"),
            ],
        );
        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        let item = &analysis.items[0];
        assert!(item.blocked);
        assert!(!item.repacked);
        assert_eq!(item.zip_path, source);
    }

    #[test]
    fn already_installed_is_detected_by_exact_hash() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("map.zip");
        build_zip(
            &source,
            &[("levels/town/info.json", br#"{"title":"Town"}"#)],
        );
        let installed_path = temp.path().join("installed-map.zip");
        fs::copy(&source, &installed_path).unwrap();
        let size = fs::metadata(&installed_path).unwrap().len() as i64;
        let same = InstalledArchive {
            file_name: "map_v1.zip".to_string(),
            enabled: false,
            size,
            path: installed_path,
            content: HashSet::new(),
            sub_folder: None,
        };
        let analysis = analyze(
            &source,
            &[same],
            &temp.path().join("work1"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        let item = &analysis.items[0];
        assert_eq!(item.kind, IncomingKind::AlreadyInstalled);
        assert_eq!(item.replaces.as_deref(), Some("map_v1.zip"));
        assert!(!item.replaces_enabled);

        // Тот же размер, другое содержимое — это не «уже установлен».
        let other_path = temp.path().join("other_v1.zip");
        fs::write(&other_path, vec![0u8; size as usize]).unwrap();
        let different = InstalledArchive {
            file_name: "other_v1.zip".to_string(),
            enabled: true,
            size,
            path: other_path,
            content: HashSet::new(),
            sub_folder: None,
        };
        let analysis = analyze(
            &source,
            &[different],
            &temp.path().join("work2"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert_eq!(analysis.items[0].kind, IncomingKind::New);
        assert!(analysis.items[0].replaces.is_none());
    }

    #[test]
    fn ambiguous_content_match_is_new_not_update() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("car_a_v9.zip");
        build_zip(&source, &[("vehicles/car/a.jbeam", b"x")]);
        let first = installed_with("car_a_v1.zip", true, &["vehicles/car"], None);
        let second = installed_with("other.zip", true, &["vehicles/car"], None);

        let analysis = analyze(
            &source,
            &[first, second],
            &temp.path().join("work1"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        let item = &analysis.items[0];
        assert_eq!(
            item.kind,
            IncomingKind::New,
            "два равных кандидата — не гадаем"
        );
        assert!(item.replaces.is_none());

        // Единственное совпадение по составу — обычное обновление.
        let unique = installed_with("car_a_v1.zip", true, &["vehicles/car"], None);
        let analysis = analyze(
            &source,
            &[unique],
            &temp.path().join("work2"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert_eq!(analysis.items[0].kind, IncomingKind::Update);
        assert_eq!(analysis.items[0].replaces.as_deref(), Some("car_a_v1.zip"));
    }

    #[test]
    fn update_by_normalized_name_when_content_does_not_match() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("speedo_v1.5.zip");
        build_zip(&source, &[("ui/modules/app/app.js", b"x")]);
        let old = installed_with("speedo_v1.0.zip", true, &["lua/ge"], None);
        let analysis = analyze(
            &source,
            &[old],
            &temp.path().join("work1"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert_eq!(analysis.items[0].kind, IncomingKind::Update);
        assert_eq!(
            analysis.items[0].replaces.as_deref(),
            Some("speedo_v1.0.zip")
        );

        // Два установленных с тем же названием — тоже не гадаем.
        let first = installed_with("speedo_v1.0.zip", true, &["lua/ge"], None);
        let second = installed_with("speedo_v1.1.zip", true, &["lua/ge"], None);
        let analysis = analyze(
            &source,
            &[first, second],
            &temp.path().join("work2"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert_eq!(analysis.items[0].kind, IncomingKind::New);
    }

    #[test]
    fn update_from_game_repo_subfolder_is_locked() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("car_v2.zip");
        build_zip(&source, &[("vehicles/car/a.jbeam", b"x")]);
        let repo = installed_with("car_v1.zip", true, &["vehicles/car"], Some("repo"));
        let analysis = analyze(
            &source,
            &[repo],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        let item = &analysis.items[0];
        assert_eq!(item.kind, IncomingKind::Update);
        assert_eq!(item.replaces.as_deref(), Some("car_v1.zip"));
        assert_eq!(item.replaces_sub_folder.as_deref(), Some("repo"));
        assert!(item.locked(), "мод из подпапки repo не устанавливается");
    }

    #[test]
    fn budget_overflow_is_a_clear_problem() {
        let temp = tempfile::tempdir().unwrap();
        let big = vec![b'x'; 3000];
        let inner = stored_zip_bytes(&[("vehicles/a/a.jbeam", big.as_slice())]);
        let source = temp.path().join("pack.zip");
        build_zip_stored(
            &source,
            &[("a.zip", &inner), ("b.zip", &inner), ("c.zip", &inner)],
        );

        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work-small"),
            IncomingOptions {
                budget_bytes: 5000,
                ..IncomingOptions::default()
            },
            &AtomicBool::new(false),
        );
        assert_eq!(
            analysis.problem.as_deref(),
            Some("Архив слишком большой после распаковки")
        );
        assert!(analysis.items.is_empty());

        let normal = analyze(
            &source,
            &[],
            &temp.path().join("work-normal"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert!(normal.problem.is_none(), "{:?}", normal.problem);
        let names: Vec<&str> = normal
            .items
            .iter()
            .map(|item| item.file_name.as_str())
            .collect();
        assert_eq!(names, vec!["a.zip", "b.zip", "c.zip"]);
    }

    #[test]
    fn budget_overflow_inside_nested_repack_is_a_clear_problem() {
        let temp = tempfile::tempdir().unwrap();
        let big = vec![b'x'; 6000];
        // Вложенный архив маленький (данные сжаты), но распаковка превышает общий бюджет.
        let inner = zip_bytes(&[("Car/vehicles/car/big.bin", big.as_slice())]);
        let source = temp.path().join("pack.zip");
        build_zip_stored(&source, &[("car.zip", &inner)]);

        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions {
                budget_bytes: 5000,
                ..IncomingOptions::default()
            },
            &AtomicBool::new(false),
        );
        assert_eq!(
            analysis.problem.as_deref(),
            Some("Архив слишком большой после распаковки")
        );
        assert!(analysis.items.is_empty());
    }

    #[test]
    fn pack_skips_nested_zips_that_are_not_mods() {
        let temp = tempfile::tempdir().unwrap();
        let audi = zip_bytes(&[("vehicles/audi/a.jbeam", b"a")]);
        let readme = zip_bytes(&[("readme.txt", b"x")]);
        let source = temp.path().join("pack.zip");
        build_zip(
            &source,
            &[("cars/audi.zip", &audi), ("readme.zip", &readme)],
        );
        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert!(analysis.problem.is_none(), "{:?}", analysis.problem);
        let names: Vec<&str> = analysis
            .items
            .iter()
            .map(|item| item.file_name.as_str())
            .collect();
        assert_eq!(names, vec!["audi.zip"]);
    }

    #[test]
    fn random_broken_and_truncated_archives_never_panic() {
        let temp = tempfile::tempdir().unwrap();
        let good = zip_bytes(&[
            ("Car/vehicles/car/a.jbeam", b"a"),
            ("Car/vehicles/car/info.json", b"{}"),
            ("inner/x.zip", b"x"),
        ]);
        let mut rng = Lcg(18);
        for i in 0..100u32 {
            let mut data: Vec<u8> = if i % 2 == 0 {
                let len = (rng.next() % 400) as usize;
                (0..len).map(|_| (rng.next() & 0xFF) as u8).collect()
            } else {
                let len = 1 + (rng.next() as usize % (good.len() - 1));
                good[..len].to_vec()
            };
            if i % 7 == 0 && data.len() > 4 {
                data = good.clone();
                let index = rng.next() as usize % data.len();
                data[index] ^= 0xFF;
            }
            let path = temp.path().join(format!("f{i}.zip"));
            fs::write(&path, &data).unwrap();
            let analysis = analyze(
                &path,
                &[],
                &temp.path().join(format!("w{i}")),
                IncomingOptions::default(),
                &AtomicBool::new(false),
            );
            assert!(
                analysis.problem.is_some() || !analysis.items.is_empty(),
                "случай {i}: ни причины, ни модов"
            );
        }
    }

    #[test]
    fn packs_inside_a_pack_do_not_overwrite_each_other() {
        // Модпак из двух модпаков: в каждом свой мод. Раньше оба вложенных мода
        // распаковывались в один и тот же файл inner-1/0.zip, и первый пункт ставил
        // чужой мод.
        let temp = tempfile::tempdir().unwrap();
        let alpha = zip_bytes(&[("vehicles/alpha/a.jbeam", b"alpha")]);
        let beta = zip_bytes(&[("vehicles/beta/b.jbeam", b"beta")]);
        let pack_one = stored_zip_bytes(&[("alpha.zip", &alpha)]);
        let pack_two = stored_zip_bytes(&[("beta.zip", &beta)]);
        let source = temp.path().join("Big pack.zip");
        build_zip_stored(&source, &[("one.zip", &pack_one), ("two.zip", &pack_two)]);
        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        assert!(analysis.problem.is_none(), "{:?}", analysis.problem);
        assert_eq!(analysis.items.len(), 2, "{:?}", analysis.items);
        for item in &analysis.items {
            let keys = content_keys(&read_index(&item.zip_path).unwrap());
            assert_eq!(
                keys, item.content,
                "{}: файл не тот, что разобран",
                item.file_name
            );
        }
        let paths: HashSet<_> = analysis
            .items
            .iter()
            .map(|item| item.zip_path.clone())
            .collect();
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn loose_folder_with_unusual_letters_is_repacked_without_panic() {
        // «İ» в нижнем регистре занимает больше байт: срез по длине строчного префикса
        // попадал в середину буквы.
        let map = shape_map(&Shape::Loose {
            dir: "İstanbul_Taxi".to_string(),
            folder: "İstanbul_Taxi".to_string(),
        })
        .unwrap();
        assert_eq!(
            map("İstanbul_Taxi/body.jbeam").as_deref(),
            Some("vehicles/İstanbul_Taxi/body.jbeam")
        );
        assert_eq!(map("other/body.jbeam"), None);
    }

    #[test]
    fn broken_nested_archive_does_not_borrow_outer_warnings() {
        let temp = tempfile::tempdir().unwrap();
        let good = zip_bytes(&[("vehicles/good/a.jbeam", b"x")]);
        let source = temp.path().join("pack.zip");
        build_zip_stored(
            &source,
            &[
                ("good.zip", &good),
                ("bad.zip", b"PK\x03\x04 not really a zip"),
            ],
        );
        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        let bad = analysis
            .items
            .iter()
            .find(|item| item.file_name == "bad.zip")
            .expect("битый вложенный архив показан");
        assert!(bad.blocked);
        assert!(
            bad.findings
                .iter()
                .all(|finding| finding.entry.as_deref() != Some("good.zip")),
            "{:?}",
            bad.findings
        );
    }

    #[test]
    fn temporary_names_never_repeat() {
        let names: HashSet<String> = (0..10_000).map(|_| short_unique()).collect();
        assert_eq!(names.len(), 10_000);
    }
}
