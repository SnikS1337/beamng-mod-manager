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
        Some("zip") => analyze_zip(source, installed, work_dir, options, &mut budget, cancel, 0),
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
        Ok(items) if !items.is_empty() => IncomingAnalysis {
            source: source.to_path_buf(),
            items,
            problem: None,
            work_dir: work_dir.to_path_buf(),
        },
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

fn analyze_zip(
    source: &Path,
    installed: &[InstalledArchive],
    work_dir: &Path,
    options: IncomingOptions,
    budget: &mut ExtractBudget,
    cancel: &AtomicBool,
    depth: usize,
) -> Result<Vec<IncomingItem>, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("отменено".to_string());
    }
    let inspection = inspect_zip(source, &ArchiveLimits::default());
    let index = read_index(source).map_err(|_| "Архив повреждён и не читается.".to_string())?;
    let overview = read_overview(source);
    let blocked = inspection.is_blocked();

    let shape = shape_of(&index);
    if matches!(shape, Shape::Mod) {
        return Ok(vec![build_item(
            source,
            clean_name(source),
            &index,
            overview.title.clone(),
            inspection,
            installed,
            work_dir,
            &[],
            false,
        )]);
    }
    if let Shape::Wrap(folder) = shape {
        let repacked = repack(source, &folder, work_dir, budget, cancel)?;
        let nested_index =
            read_index(&repacked).map_err(|_| "Архив повреждён и не читается.".to_string())?;
        let nested_overview = read_overview(&repacked);
        return Ok(vec![build_item(
            &repacked,
            clean_name(source),
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
        let target = inner_dir.join(format!("{nested_count}.zip"));
        if copy_entry(source, entry, &target, budget, cancel).is_err() {
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
        ) {
            Ok(mut nested) => items.append(&mut nested),
            Err(message) if !message.starts_with("В архиве нет мода") => {
                items.push(blocked_item(&target, &message, inspection.clone()));
            }
            Err(_) => {}
        }
    }

    if items.is_empty() {
        if blocked {
            items.push(blocked_item(
                source,
                "Архив не прошёл проверку безопасности.",
                inspection,
            ));
        } else {
            return Err(
                "В архиве нет мода BeamNG: нет папок vehicles, levels и других, которые ждёт игра."
                    .to_string(),
            );
        }
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
            let mut nested = analyze_zip(&zip, installed, work_dir, options, budget, cancel, 1)?;
            items.append(&mut nested);
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
) -> IncomingItem {
    IncomingItem {
        zip_path: source.to_path_buf(),
        file_name: clean_name(source),
        size: fs::metadata(source).map(|meta| meta.len()).unwrap_or(0),
        title: Path::new(&clean_name(source))
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Архив".to_string()),
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
    None,
}

fn shape_of(index: &ZipIndex) -> Shape {
    let mut top = HashSet::new();
    let mut root_has_files = false;
    for entry in &index.entries {
        let name = entry.name.trim_start_matches("./").trim_start_matches('/');
        let parts: Vec<&str> = name.split('/').filter(|part| !part.is_empty()).collect();
        if let Some(first) = parts.first() {
            top.insert(first.to_string());
        }
        if entry.depth == 1 {
            root_has_files = true;
        }
    }
    if top.iter().any(|name| is_known_root(name)) {
        return Shape::Mod;
    }
    if top.len() == 1 && !root_has_files {
        let folder = top.into_iter().next().unwrap();
        let prefix = format!("{folder}/");
        let has_known_child = index.entries.iter().any(|entry| {
            entry
                .name
                .strip_prefix(&prefix)
                .and_then(|rest| rest.split('/').next())
                .map(is_known_root)
                .unwrap_or(false)
        });
        if has_known_child {
            return Shape::Wrap(folder);
        }
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

    let candidate_count = content.len();
    let mut best: Option<(&InstalledArchive, f64)> = None;
    let mut tie = false;
    for item in installed {
        if item.content.is_empty() || candidate_count == 0 {
            continue;
        }
        let shared = content.intersection(&item.content).count();
        let score = shared as f64 / candidate_count.max(item.content.len()) as f64;
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
    if let Some((found, _)) = best
        && !tie
    {
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
    let raw = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "mod".to_string());
    let stem = raw.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(&raw);
    let stem = strip_copy_suffix(stem);
    format!("{}.zip", clean_stem(&stem))
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

fn repack(
    source: &Path,
    wrapper: &str,
    work_dir: &Path,
    budget: &mut ExtractBudget,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let index = read_index(source).map_err(|_| "Архив повреждён и не читается.".to_string())?;
    let target = work_dir.join(format!("repacked-{}.zip", short_unique()));
    let file = File::create(&target).map_err(|error| error.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    let prefix = format!("{wrapper}/");
    for entry in &index.entries {
        if cancel.load(Ordering::Relaxed) {
            return Err("отменено".to_string());
        }
        let Some(rest) = entry.name.strip_prefix(&prefix) else {
            continue;
        };
        if rest.is_empty() || crate::inspect::is_unsafe_entry_path(rest) {
            continue;
        }
        let mut input = File::open(source).map_err(|error| error.to_string())?;
        let mut archive = zip::ZipArchive::new(&mut input).map_err(|error| error.to_string())?;
        let mut nested = archive
            .by_name(&entry.raw_name)
            .map_err(|error| error.to_string())?;
        writer
            .start_file(rest, options)
            .map_err(|error| error.to_string())?;
        let mut buffer = [0u8; 81920];
        loop {
            let read = nested
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            budget.add(read as u64).map_err(|error| error.to_string())?;
            writer
                .write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
        }
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
    let mut input = File::open(source).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(&mut input).map_err(|error| error.to_string())?;
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

fn short_unique() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
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
        InstalledArchive {
            file_name: file_name.to_string(),
            enabled: true,
            size: 0,
            path: PathBuf::from("missing"),
            content: content.iter().map(|value| value.to_string()).collect(),
            sub_folder: None,
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
        let analysis = analyze(
            &source,
            &[],
            &temp.path().join("work"),
            IncomingOptions::default(),
            &AtomicBool::new(false),
        );
        let item = &analysis.items[0];
        assert!(item.repacked);
        assert!(item.zip_path != source);
        let index = read_index(&item.zip_path).unwrap();
        assert!(
            index
                .entries
                .iter()
                .all(|entry| !entry.name.starts_with("Wrapper/"))
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
    }
}
