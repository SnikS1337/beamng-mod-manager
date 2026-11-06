//! Связка хранилища, планировщика и решения «есть обновление».
//!
//! Сеть сюда не входит: [`crate::UpdateRunner`] получает ленты и карточки
//! и передаёт результат этим функциям.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset};
use url::Url;

use std::collections::HashSet;

use bmm_domain::file_check::{self, FileVerdict};
use bmm_domain::library::{
    CheckState, LibraryStore, ModBinding, SourceStatus as StoreSourceStatus,
};
use bmm_domain::versioning::{
    SizeComparer, UpdateDecider, UpdateStatus, VersionComparer, VersionOrder, VersionSnapshot,
};

use crate::options::UpdateCheckOptions;
use crate::planner::{FeedEntry, FeedPlan, SourceStatus, TrackedBinding, UpdateCheckPlanner};

/// Найденное обновление для одного установленного мода.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateFinding {
    pub mod_id: String,
    pub mod_name: String,
    pub source: String,
    pub status: UpdateStatus,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub url: Url,
    /// Что именно пользователь отклоняет: новая версия или, если версии нет, состояние файла по размеру.
    /// Когда на сайте появится другая версия, ключ изменится и обновление покажется снова.
    pub dismiss_key: String,
    /// Пользователь пропустил это обновление (показывается только в списке «скрытые»).
    pub is_dismissed: bool,
    /// Версия на странице автора в репозитории BeamNG, если она старее, чем пишет этот
    /// сайт, а у вас — как у автора. Тогда это скорее перезалив с неверной подписью
    /// («1.14.1» вместо «0.14.1»), и обновление показывается только как возможное.
    pub author_version: Option<String>,
    /// Что показал файл за кнопкой «Скачать»: тот же файл, подписан старой версией
    /// или новее, чем пишет сайт ([`FileVerdict::code`]).
    pub file_verdict: Option<String>,
    /// Имя этого файла и версия в нём, если есть.
    pub file_name: Option<String>,
    pub file_version: Option<String>,
    /// Файла по ссылке «Скачать» больше нет: файлообменник ответил «не найдено».
    pub file_gone: bool,
}

/// Сайт, на котором мод выкладывает сам автор.
const AUTHOR_SOURCE: &str = "beamng";

/// Склеивает хранилище, планировщик и решение «есть обновление».
pub struct UpdateCheckService;

impl UpdateCheckService {
    /// Привязки для проверки: только подтверждённые, с тем, что видели раньше.
    pub fn tracked(store: &dyn LibraryStore) -> Vec<TrackedBinding> {
        let mut tracked: Vec<TrackedBinding> = store
            .bindings()
            .into_iter()
            .filter(|binding| binding.confirmed)
            .map(|binding| {
                let check = store.get_check(&binding.mod_id, &binding.source);
                TrackedBinding {
                    enabled: store
                        .get_mod(&binding.mod_id)
                        .map(|installed| installed.enabled)
                        .unwrap_or(true),
                    mod_id: binding.mod_id,
                    source: binding.source,
                    source_id: binding.source_id,
                    last_checked_utc: check.as_ref().map(|state| state.last_checked_utc),
                    last_changed_utc: check.as_ref().and_then(|state| state.last_changed_utc),
                    last_version: check.as_ref().and_then(|state| state.last_version.clone()),
                    last_size: check.as_ref().and_then(|state| state.last_size),
                    miss_count: check.as_ref().map_or(0, |state| state.miss_count),
                }
            })
            .collect();
        // Детерминированный порядок: хранилища на HashMap не гарантируют его между запусками.
        tracked.sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then_with(|| a.mod_id.cmp(&b.mod_id))
                .then_with(|| a.source_id.cmp(&b.source_id))
        });
        tracked
    }

    /// Статусы сайтов в терминах сетевого слоя (типы домена и источников совпадают по смыслу).
    pub fn source_statuses(store: &dyn LibraryStore) -> BTreeMap<String, SourceStatus> {
        store
            .source_statuses()
            .into_iter()
            .map(|(source, status)| (source, status_from_store(status)))
            .collect()
    }

    pub fn plan_feeds(
        store: &dyn LibraryStore,
        now: DateTime<FixedOffset>,
        options: &UpdateCheckOptions,
    ) -> FeedPlan {
        UpdateCheckPlanner::plan_feeds(
            &Self::tracked(store),
            &Self::source_statuses(store),
            now,
            options,
        )
    }

    /// Лента подтвердила, что запись не менялась: обновляем только время проверки.
    pub fn apply_feed_unchanged(
        store: &dyn LibraryStore,
        binding: &TrackedBinding,
        now: DateTime<FixedOffset>,
    ) {
        let Some(check) = store.get_check(&binding.mod_id, &binding.source) else {
            return; // без прошлых данных планировщик такие случаи не пропускает
        };
        store.save_check(CheckState {
            last_checked_utc: now,
            miss_count: 0, // раз лента его показала, страница есть
            ..check
        });
    }

    /// Находки, у которых сведений о файле за ссылкой нет или они о прежней подписи
    /// страницы: их страницу стоит открыть в ближайшую проверку вне очереди — иначе
    /// «тот же файл» или «файл подписан старой версией» не увидеть до следующей смены.
    pub fn stale_file_findings(store: &dyn LibraryStore) -> HashSet<(String, String)> {
        Self::pending(store, false)
            .into_iter()
            .filter(|finding| {
                let Some(check) = store.get_check(&finding.mod_id, &finding.source) else {
                    return false;
                };
                let key = file_check::page_key(check.last_version.as_deref(), check.last_size);
                match &check.last_file {
                    None => true,
                    Some(file) => {
                        file.page_key.as_deref() != Some(key.as_str())
                            || (file.name.is_none() && !file.gone)
                    }
                }
            })
            .map(|finding| (finding.mod_id, finding.source))
            .collect()
    }

    /// Что лежит за кнопкой «Скачать» — к той подписи страницы, что записана сейчас.
    pub fn apply_file(
        store: &dyn LibraryStore,
        mod_id: &str,
        source: &str,
        mut file: bmm_domain::library::FileSnapshot,
    ) {
        let Some(mut check) = store.get_check(mod_id, source) else {
            return;
        };
        file.page_key = Some(file_check::page_key(
            check.last_version.as_deref(),
            check.last_size,
        ));
        check.last_file = Some(file);
        store.save_check(check);
    }

    /// Карточка не нашлась (404). Прошлые версия и размер сохраняются, время проверки
    /// обновляется (иначе страницу дёргали бы при каждом нажатии), счётчик промахов растёт.
    pub fn apply_missing(
        store: &dyn LibraryStore,
        binding: &TrackedBinding,
        now: DateTime<FixedOffset>,
    ) {
        let check = store.get_check(&binding.mod_id, &binding.source);
        store.save_check(match check {
            None => CheckState {
                last_file: None,
                mod_id: binding.mod_id.clone(),
                source: binding.source.clone(),
                last_checked_utc: now,
                last_changed_utc: None,
                last_version: None,
                last_size: None,
                miss_count: 1,
                last_game_version: None,
                last_title: None,
            },
            Some(check) => CheckState {
                last_checked_utc: now,
                miss_count: check.miss_count + 1,
                ..check
            },
        });
    }

    /// Результат открытия карточки: запоминаем, что сайт показывает сейчас.
    pub fn apply_card(
        store: &dyn LibraryStore,
        binding: &TrackedBinding,
        current: &bmm_domain::models::Listing,
        now: DateTime<FixedOffset>,
    ) {
        let previous = store.get_check(&binding.mod_id, &binding.source);
        // Не разобранная версия (None) не затирает уже известную: сбой разбора не должен «забывать» данные.
        let version = current.mod_version.clone().or_else(|| {
            previous
                .as_ref()
                .and_then(|state| state.last_version.clone())
        });
        let changed = previous.is_none()
            || (current.mod_version.is_some()
                && previous
                    .as_ref()
                    .and_then(|state| state.last_version.as_ref())
                    != current.mod_version.as_ref())
            || current.size_bytes == Some(0)
            || previous.as_ref().and_then(|state| state.last_size) == Some(0)
            || SizeComparer::differs(
                previous.as_ref().and_then(|state| state.last_size),
                current.size_bytes,
            );
        let last_changed = if changed {
            Some(now)
        } else {
            previous.as_ref().and_then(|state| state.last_changed_utc)
        };
        store.save_check(CheckState {
            // Сведения о файле живут дольше подписи: к какой подписи они относятся,
            // помнит сам снимок (`page_key`).
            last_file: previous.as_ref().and_then(|state| state.last_file.clone()),
            mod_id: binding.mod_id.clone(),
            source: binding.source.clone(),
            last_checked_utc: now,
            last_changed_utc: last_changed,
            last_version: version,
            last_size: current
                .size_bytes
                .or_else(|| previous.as_ref().and_then(|state| state.last_size)),
            miss_count: 0,
            last_game_version: current.game_version.clone().or_else(|| {
                previous
                    .as_ref()
                    .and_then(|state| state.last_game_version.clone())
            }),
            last_title: if current.title.trim().is_empty() {
                previous.as_ref().and_then(|state| state.last_title.clone())
            } else {
                Some(current.title.trim().to_string())
            },
        });
    }

    /// Первое знакомство: версия известна из ленты, открывать карточку ради неё не нужно.
    /// Так первая проверка 200 модов обходится несколькими страницами ленты, а не 200 карточками.
    pub fn apply_feed_baseline(
        store: &dyn LibraryStore,
        binding: &TrackedBinding,
        entry: &FeedEntry,
        now: DateTime<FixedOffset>,
    ) {
        let previous = store.get_check(&binding.mod_id, &binding.source);
        store.save_check(CheckState {
            // Сведения о файле живут дольше подписи: к какой подписи они относятся,
            // помнит сам снимок (`page_key`).
            last_file: previous.as_ref().and_then(|state| state.last_file.clone()),
            mod_id: binding.mod_id.clone(),
            source: binding.source.clone(),
            last_checked_utc: now,
            last_changed_utc: Some(now),
            last_version: entry.version.clone(),
            last_size: entry
                .size_bytes
                .or_else(|| previous.as_ref().and_then(|state| state.last_size)),
            miss_count: 0,
            last_game_version: previous
                .as_ref()
                .and_then(|state| state.last_game_version.clone()),
            last_title: previous.and_then(|state| state.last_title),
        });
    }

    /// Список обновлений. Считается при чтении, поэтому установка новой версии
    /// убирает пункт сама. Отклонённые («пропустить эту версию») скрыты, пока
    /// на сайте не появится другая версия.
    pub fn pending(store: &dyn LibraryStore, include_dismissed: bool) -> Vec<UpdateFinding> {
        let mut by_mod: BTreeMap<String, Vec<ModBinding>> = BTreeMap::new();
        for binding in store
            .bindings()
            .into_iter()
            .filter(|binding| binding.confirmed)
        {
            by_mod
                .entry(binding.mod_id.clone())
                .or_default()
                .push(binding);
        }

        let mut result = Vec::new();
        for installed in store.mods() {
            let dismissed = store.get_dismissed_update(&installed.id);
            // У автора та же версия, что у вас (или старее): более высокий номер на
            // сайте-перезаливщике — скорее неверная подпись, чем новая версия.
            let author_version = by_mod
                .get(&installed.id)
                .into_iter()
                .flatten()
                .find(|binding| binding.source == AUTHOR_SOURCE)
                .and_then(|binding| store.get_check(&installed.id, &binding.source))
                .and_then(|check| check.last_version)
                .filter(|author| {
                    matches!(
                        VersionComparer::compare(installed.version.as_deref(), Some(author)),
                        VersionOrder::Same | VersionOrder::Older
                    )
                });
            let mut best: Option<UpdateFinding> = None;
            for binding in by_mod.get(&installed.id).into_iter().flatten() {
                let Some(check) = store.get_check(&installed.id, &binding.source) else {
                    continue;
                };
                let page_status = UpdateDecider::decide(
                    &VersionSnapshot::new(installed.version.as_deref(), installed.size_bytes, None),
                    &VersionSnapshot::new(check.last_version.as_deref(), check.last_size, None),
                );
                // Файл за кнопкой «Скачать» уточняет решение по подписи страницы.
                let page_key = file_check::page_key(check.last_version.as_deref(), check.last_size);
                let verdict = check.last_file.as_ref().and_then(|file| {
                    file_check::judge(
                        file_check::Installed {
                            version: installed.version.as_deref(),
                            size: installed.size_bytes,
                        },
                        page_status,
                        &page_key,
                        file,
                    )
                });
                let mut status = match &verdict {
                    // Сайт пишет новую версию, а лежит ваш файл: это не обновление.
                    Some(FileVerdict::SameFile) => UpdateStatus::None,
                    Some(FileVerdict::LabelOlder { .. }) => UpdateStatus::Possible,
                    Some(FileVerdict::FileNewer { .. }) => UpdateStatus::Possible,
                    None => page_status,
                };
                let same_file = verdict == Some(FileVerdict::SameFile);
                let file_gone = check
                    .last_file
                    .as_ref()
                    .is_some_and(|file| file.gone && file.page_key.as_deref() == Some(&page_key));
                if status == UpdateStatus::None && !same_file {
                    continue;
                }
                if same_file {
                    // Показываем только среди скрытых — с пояснением, почему.
                    if !include_dismissed {
                        continue;
                    }
                    status = UpdateStatus::Possible;
                }
                let mut against_author = None;
                if binding.source != AUTHOR_SOURCE
                    && status == UpdateStatus::Likely
                    && let Some(author) = author_version.as_deref()
                    && VersionComparer::compare(Some(author), check.last_version.as_deref())
                        == VersionOrder::Newer
                {
                    status = UpdateStatus::Possible;
                    against_author = Some(author.to_string());
                }
                // Новый файл при прежней подписи пропускают до следующего файла.
                let key = match &verdict {
                    Some(FileVerdict::FileNewer { file_version }) => format!("file:{file_version}"),
                    _ => page_key,
                };
                let is_dismissed = same_file
                    || dismissed
                        .as_deref()
                        .is_some_and(|dismissed| dismissed.eq_ignore_ascii_case(&key));
                if is_dismissed && !include_dismissed {
                    continue;
                }
                let finding = UpdateFinding {
                    mod_id: installed.id.clone(),
                    mod_name: installed.name.clone(),
                    source: binding.source.clone(),
                    status,
                    installed_version: installed.version.clone(),
                    latest_version: check.last_version.clone(),
                    url: binding.url.clone(),
                    dismiss_key: key,
                    is_dismissed,
                    author_version: against_author,
                    file_verdict: verdict.as_ref().map(|verdict| verdict.code().to_string()),
                    file_name: verdict
                        .as_ref()
                        .and(check.last_file.as_ref())
                        .and_then(|file| file.name.clone()),
                    file_version: verdict
                        .as_ref()
                        .and_then(|verdict| verdict.file_version().map(str::to_string)),
                    file_gone,
                };
                // Среди не скрытых берём более уверенное; скрытое не вытесняет нескрытое.
                let replace = match &best {
                    None => true,
                    Some(best) => {
                        (best.is_dismissed && !finding.is_dismissed)
                            || (best.is_dismissed == finding.is_dismissed
                                && (status_rank(finding.status) > status_rank(best.status)
                                    // При равной уверенности — страница автора: качать
                                    // лучше оттуда, а не с перезаливщика.
                                    || (status_rank(finding.status)
                                        == status_rank(best.status)
                                        && finding.source == AUTHOR_SOURCE
                                        && best.source != AUTHOR_SOURCE)))
                    }
                };
                if replace {
                    best = Some(finding);
                }
            }
            if let Some(best) = best {
                result.push(best);
            }
        }
        result
    }

    /// Все обновления, включая скрытые пользователем.
    pub fn pending_including_dismissed(store: &dyn LibraryStore) -> Vec<UpdateFinding> {
        Self::pending(store, true)
    }

    /// «Пропустить эту версию».
    pub fn dismiss(store: &dyn LibraryStore, finding: &UpdateFinding) {
        store.set_dismissed_update(&finding.mod_id, Some(&finding.dismiss_key));
    }

    /// Вернуть скрытое обновление в список.
    pub fn restore(store: &dyn LibraryStore, mod_id: &str) {
        store.set_dismissed_update(mod_id, None);
    }

    /// Удобный поиск обновления мода (для «Пропустить версию» из интерфейса).
    pub fn finding_for(store: &dyn LibraryStore, mod_id: &str) -> Option<UpdateFinding> {
        store
            .mods()
            .into_iter()
            .find(|installed| installed.id == mod_id)?;
        Self::pending(store, false)
            .into_iter()
            .find(|finding| finding.mod_id == mod_id)
    }

    /// Переводит статус сайта из домена в статус сетевого слоя.
    pub fn status_from_store(status: StoreSourceStatus) -> SourceStatus {
        status_from_store(status)
    }

    /// Переводит статус сайта из сетевого слоя в домен (для записи в хранилище).
    pub fn status_to_store(status: SourceStatus) -> StoreSourceStatus {
        status_to_store(status)
    }
}

fn status_rank(status: UpdateStatus) -> i32 {
    match status {
        UpdateStatus::None => 0,
        // «Обновление» увереннее, чем «возможно»: выбирать между сайтами надо его.
        UpdateStatus::Possible => 1,
        UpdateStatus::Likely => 2,
    }
}

fn status_from_store(status: StoreSourceStatus) -> SourceStatus {
    match status {
        StoreSourceStatus::Ok => SourceStatus::Ok,
        StoreSourceStatus::NeedsHumanCheck => SourceStatus::NeedsHumanCheck,
        StoreSourceStatus::Unavailable => SourceStatus::Unavailable,
    }
}

pub(crate) fn status_to_store(status: SourceStatus) -> StoreSourceStatus {
    match status {
        SourceStatus::Ok => StoreSourceStatus::Ok,
        SourceStatus::NeedsHumanCheck => StoreSourceStatus::NeedsHumanCheck,
        SourceStatus::Unavailable => StoreSourceStatus::Unavailable,
    }
}

/// Подпись мода для отчёта (имя, иначе идентификатор).
pub(crate) fn mod_label(store: &dyn LibraryStore, mod_id: &str) -> String {
    store
        .get_mod(mod_id)
        .map_or_else(|| mod_id.to_string(), |installed| installed.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmm_domain::library::{FileSnapshot, InMemoryLibraryStore, InstalledMod, PageSearch};

    fn now() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-13T12:00:00+00:00").unwrap()
    }

    fn add(
        store: &InMemoryLibraryStore,
        id: &str,
        version: Option<&str>,
        source: &str,
        checked: Option<&str>,
        last_version: Option<&str>,
        last_size: Option<i64>,
    ) {
        store.upsert_mod(InstalledMod {
            id: id.to_string(),
            name: id.to_string(),
            file_name: format!("{id}.zip"),
            version: version.map(str::to_string),
            size_bytes: None,
            installed_at_utc: now(),
            enabled: true,
        });
        store
            .upsert_binding(ModBinding {
                mod_id: id.to_string(),
                source: source.to_string(),
                source_id: id.to_string(),
                url: Url::parse(&format!("https://{source}.test/{id}")).unwrap(),
                confirmed: true,
            })
            .unwrap();
        if let Some(checked) = checked {
            store.save_check(CheckState {
                last_file: None,
                mod_id: id.to_string(),
                source: source.to_string(),
                last_checked_utc: DateTime::parse_from_rfc3339(checked).unwrap(),
                last_changed_utc: Some(DateTime::parse_from_rfc3339(checked).unwrap()),
                last_version: last_version.map(str::to_string),
                last_size,
                miss_count: 0,
                last_game_version: None,
                last_title: None,
            });
        }
    }

    /// Мод `id` версии `ours` и размера `size`, сайт пишет `site`, за ссылкой — `file`.
    fn with_file(
        store: &InMemoryLibraryStore,
        id: &str,
        ours: Option<&str>,
        size: i64,
        site: Option<&str>,
        file: Option<(&str, i64, Option<&str>)>,
    ) {
        add(
            store,
            id,
            ours,
            "stmods",
            Some("2026-10-12T12:00:00+00:00"),
            site,
            None,
        );
        let mut installed = store.get_mod(id).unwrap();
        installed.size_bytes = Some(size);
        store.upsert_mod(installed);
        let mut check = store.get_check(id, "stmods").unwrap();
        check.last_file = file.map(|(name, file_size, key)| FileSnapshot {
            gone: false,
            offer: None,
            seen_at: None,
            url: "https://modsfire.com/abc".to_string(),
            name: Some(name.to_string()),
            size: Some(file_size),
            exact: true,
            page_key: key.map(str::to_string),
        });
        store.save_check(check);
    }

    #[test]
    fn findings_without_fresh_file_details_are_listed_for_a_reread() {
        let store = InMemoryLibraryStore::new();
        with_file(&store, "none", Some("1.0"), 5, Some("1.1"), None);
        with_file(
            &store,
            "old",
            Some("1.0"),
            5,
            Some("1.1"),
            Some(("old.zip", 9, Some("1.0"))),
        );
        with_file(
            &store,
            "fresh",
            Some("1.0"),
            5,
            Some("1.1"),
            Some(("f_v1.1.zip", 9, Some("1.1"))),
        );
        with_file(&store, "calm", Some("1.0"), 5, Some("1.0"), None);
        let stale = UpdateCheckService::stale_file_findings(&store);
        assert!(stale.contains(&("none".to_string(), "stmods".to_string())));
        assert!(
            stale.contains(&("old".to_string(), "stmods".to_string())),
            "снимок о прежней подписи"
        );
        assert!(!stale.contains(&("fresh".to_string(), "stmods".to_string())));
        assert!(
            !stale.contains(&("calm".to_string(), "stmods".to_string())),
            "находки нет"
        );

        // Адрес файла есть, а имени нет (страницу файлообменника ещё не читали).
        let mut check = store.get_check("fresh", "stmods").unwrap();
        check.last_file.as_mut().unwrap().name = None;
        store.save_check(check);
        assert!(
            UpdateCheckService::stale_file_findings(&store)
                .contains(&("fresh".to_string(), "stmods".to_string()))
        );
        // Имени нет, потому что файл удалён: перечитывать нечего.
        let mut check = store.get_check("fresh", "stmods").unwrap();
        check.last_file.as_mut().unwrap().gone = true;
        store.save_check(check);
        assert!(
            !UpdateCheckService::stale_file_findings(&store)
                .contains(&("fresh".to_string(), "stmods".to_string()))
        );
        let finding = UpdateCheckService::pending(&store, false)
            .into_iter()
            .find(|finding| finding.mod_id == "fresh")
            .unwrap();
        assert!(finding.file_gone);
    }

    #[test]
    fn file_details_are_tied_to_the_current_page_label() {
        let store = InMemoryLibraryStore::new();
        with_file(&store, "a", Some("1.0"), 5, Some("1.1"), None);
        let file = FileSnapshot {
            gone: false,
            offer: None,
            seen_at: None,
            url: "https://modsfire.com/x".to_string(),
            name: Some("a.zip".to_string()),
            size: Some(5),
            exact: true,
            page_key: Some("старая".to_string()),
        };
        UpdateCheckService::apply_file(&store, "a", "stmods", file.clone());
        let saved = store.get_check("a", "stmods").unwrap().last_file.unwrap();
        assert_eq!(saved.page_key.as_deref(), Some("1.1"));
        // Записи проверки нет — снимок некуда класть, его и не создаём.
        UpdateCheckService::apply_file(&store, "нет", "stmods", file);
        assert!(store.get_check("нет", "stmods").is_none());
    }

    #[test]
    fn same_file_behind_the_link_is_not_an_update() {
        // Smoke Dust FX: сайт пишет 1.14.1, а по ссылке ваш файл того же размера.
        let store = InMemoryLibraryStore::new();
        with_file(
            &store,
            "smoke",
            Some("0.14.1"),
            4_000_000,
            Some("1.14.1"),
            Some(("smoke_dust_fx.zip", 4_000_000, Some("1.14.1"))),
        );
        assert!(UpdateCheckService::pending(&store, false).is_empty());
        // Среди скрытых — с причиной, чтобы было видно, почему его нет.
        let hidden = UpdateCheckService::pending(&store, true);
        assert_eq!(hidden.len(), 1);
        assert!(hidden[0].is_dismissed);
        assert_eq!(hidden[0].file_verdict.as_deref(), Some("same_file"));
        assert_eq!(hidden[0].file_name.as_deref(), Some("smoke_dust_fx.zip"));
    }

    #[test]
    fn new_label_over_an_old_file_name_is_only_possible() {
        let store = InMemoryLibraryStore::new();
        with_file(
            &store,
            "dummy",
            Some("1.1"),
            10_000_000,
            Some("1.2"),
            Some(("Dummy_v1.1.zip", 12_000_000, Some("1.2"))),
        );
        let pending = UpdateCheckService::pending(&store, false);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].status, UpdateStatus::Possible);
        assert_eq!(pending[0].file_verdict.as_deref(), Some("label_older"));
        assert_eq!(pending[0].file_version.as_deref(), Some("1.1"));
        assert_eq!(pending[0].dismiss_key, "1.2");
    }

    #[test]
    fn newer_file_behind_an_unchanged_label_is_possible() {
        let store = InMemoryLibraryStore::new();
        with_file(
            &store,
            "dummy",
            Some("1.1"),
            10_000_000,
            Some("1.1"),
            Some(("Dummy_v1.2.zip", 12_000_000, Some("1.1"))),
        );
        let pending = UpdateCheckService::pending(&store, false);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].status, UpdateStatus::Possible);
        assert_eq!(pending[0].file_verdict.as_deref(), Some("file_newer"));
        assert_eq!(pending[0].file_version.as_deref(), Some("1.2"));
        // «Пропустить» прячет этот файл, а не подпись страницы.
        assert_eq!(pending[0].dismiss_key, "file:1.2");
        UpdateCheckService::dismiss(&store, &pending[0]);
        assert!(UpdateCheckService::pending(&store, false).is_empty());
    }

    #[test]
    fn stale_file_snapshot_does_not_hide_a_new_version() {
        // Файл смотрели при подписи 1.1; сайт уже пишет 1.2, а новый файл ещё не видели.
        let store = InMemoryLibraryStore::new();
        with_file(
            &store,
            "dummy",
            Some("1.1"),
            10_000_000,
            Some("1.2"),
            Some(("dummy.zip", 10_000_000, Some("1.1"))),
        );
        let pending = UpdateCheckService::pending(&store, false);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].status, UpdateStatus::Likely);
        assert_eq!(pending[0].file_verdict, None);
    }

    #[test]
    fn without_file_details_everything_works_as_before() {
        let store = InMemoryLibraryStore::new();
        with_file(&store, "dummy", Some("1.1"), 10_000_000, Some("1.2"), None);
        let pending = UpdateCheckService::pending(&store, false);
        assert_eq!(pending[0].status, UpdateStatus::Likely);
        assert_eq!(pending[0].file_verdict, None);
        assert_eq!(pending[0].file_name, None);
    }

    #[test]
    fn same_file_on_one_site_does_not_hide_an_update_on_another() {
        // STMods: тот же файл; BeamNG автора: настоящая новая версия.
        let store = InMemoryLibraryStore::new();
        with_file(
            &store,
            "car",
            Some("1.0"),
            5_000_000,
            Some("1.1"),
            Some(("car.zip", 5_000_000, Some("1.1"))),
        );
        store
            .upsert_binding(ModBinding {
                mod_id: "car".to_string(),
                source: "beamng".to_string(),
                source_id: "car".to_string(),
                url: Url::parse("https://beamng.test/car").unwrap(),
                confirmed: true,
            })
            .unwrap();
        let mut author = store.get_check("car", "stmods").unwrap();
        author.source = "beamng".to_string();
        author.last_version = Some("1.2".to_string());
        author.last_file = None;
        store.save_check(author);
        let pending = UpdateCheckService::pending(&store, false);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].source, "beamng");
        assert_eq!(pending[0].status, UpdateStatus::Likely);
    }

    #[test]
    fn pending_uses_version_then_size_and_hides_dismissed() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            Some("1.0"),
            "stmods",
            Some("2026-10-12T12:00:00+00:00"),
            Some("1.1"),
            None,
        );
        let pending = UpdateCheckService::pending(&store, false);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].status, UpdateStatus::Likely);
        assert_eq!(pending[0].dismiss_key, "1.1");

        UpdateCheckService::dismiss(&store, &pending[0]);
        assert!(UpdateCheckService::pending(&store, false).is_empty());
        let hidden = UpdateCheckService::pending(&store, true);
        assert_eq!(hidden.len(), 1);
        assert!(hidden[0].is_dismissed);

        UpdateCheckService::restore(&store, "a");
        assert_eq!(UpdateCheckService::pending(&store, false).len(), 1);
    }

    #[test]
    fn dismissed_version_does_not_hide_newer_one() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            Some("1.0"),
            "stmods",
            Some("2026-10-12T12:00:00+00:00"),
            Some("1.1"),
            None,
        );
        store.set_dismissed_update("a", Some("1.1"));
        store.save_check(CheckState {
            last_file: None,
            mod_id: "a".to_string(),
            source: "stmods".to_string(),
            last_checked_utc: now(),
            last_changed_utc: Some(now()),
            last_version: Some("1.2".to_string()),
            last_size: None,
            miss_count: 0,
            last_game_version: None,
            last_title: None,
        });
        assert_eq!(UpdateCheckService::pending(&store, false).len(), 1);
    }

    #[test]
    fn new_version_is_chosen_over_a_size_change() {
        // «Обновление» (новая версия на STMods) увереннее, чем «возможно» (на Modland
        // та же версия, но другой размер). Раньше выигрывало «возможно», и мод показывал
        // то один сайт, то другой.
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            Some("1.0"),
            "stmods",
            Some("2026-10-12T12:00:00+00:00"),
            Some("1.1"),
            None,
        );
        let installed = store.get_mod("a").unwrap();
        store.upsert_mod(InstalledMod {
            size_bytes: Some(100 * 1024 * 1024),
            ..installed
        });
        store
            .upsert_binding(ModBinding {
                mod_id: "a".to_string(),
                source: "modland".to_string(),
                source_id: "a".to_string(),
                url: Url::parse("https://modland.test/a").unwrap(),
                confirmed: true,
            })
            .unwrap();
        store.save_check(CheckState {
            last_file: None,
            mod_id: "a".to_string(),
            source: "modland".to_string(),
            last_checked_utc: now(),
            last_changed_utc: Some(now()),
            last_version: Some("1.0".to_string()),
            last_size: Some(120 * 1024 * 1024),
            miss_count: 0,
            last_game_version: None,
            last_title: None,
        });
        let pending = UpdateCheckService::pending(&store, false);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].status, UpdateStatus::Likely);
        assert_eq!(pending[0].source, "stmods");
    }

    fn check(store: &InMemoryLibraryStore, id: &str, source: &str, version: &str, size: i64) {
        store
            .upsert_binding(ModBinding {
                mod_id: id.to_string(),
                source: source.to_string(),
                source_id: id.to_string(),
                url: Url::parse(&format!("https://{source}.test/{id}")).unwrap(),
                confirmed: true,
            })
            .unwrap();
        store.save_check(CheckState {
            last_file: None,
            mod_id: id.to_string(),
            source: source.to_string(),
            last_checked_utc: now(),
            last_changed_utc: Some(now()),
            last_version: Some(version.to_string()),
            last_size: Some(size),
            miss_count: 0,
            last_game_version: None,
            last_title: None,
        });
    }

    #[test]
    fn rehost_version_above_the_author_is_only_possible() {
        // Smoke Dust FX: у автора на BeamNG 0.14.1 (стоит она), на ModsGaming тот же
        // файл подписан «1.14.1». Это не «Обновление», а в лучшем случае «возможно».
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(InstalledMod {
            id: "smoke".to_string(),
            name: "Smoke Dust FX".to_string(),
            file_name: "smokeDustFX_mihaits.zip".to_string(),
            version: Some("0.14.1".to_string()),
            size_bytes: Some(20_457_482),
            installed_at_utc: now(),
            enabled: true,
        });
        check(&store, "smoke", "beamng", "0.14.1", 20_447_232);
        check(&store, "smoke", "modsgaming", "1.14.1", 20_457_717);
        let pending = UpdateCheckService::pending(&store, false);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].status, UpdateStatus::Possible);
        assert_eq!(pending[0].source, "modsgaming");
        assert_eq!(pending[0].author_version.as_deref(), Some("0.14.1"));

        // Автор сам выпустил новую версию — это обновление, без оговорок.
        check(&store, "smoke", "beamng", "0.15.0", 21_000_000);
        let pending = UpdateCheckService::pending(&store, false);
        assert_eq!(pending[0].status, UpdateStatus::Likely);
        assert_eq!(pending[0].source, "beamng");
        assert_eq!(pending[0].author_version, None);
    }

    #[test]
    fn without_an_author_page_rehost_versions_count_as_before() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(InstalledMod {
            id: "meo".to_string(),
            name: "Meo".to_string(),
            file_name: "meo.zip".to_string(),
            version: Some("4.0.0".to_string()),
            size_bytes: Some(65_000_000),
            installed_at_utc: now(),
            enabled: true,
        });
        check(&store, "meo", "modland", "4.0.2", 68_000_000);
        let pending = UpdateCheckService::pending(&store, false);
        assert_eq!(pending[0].status, UpdateStatus::Likely);
        assert_eq!(pending[0].author_version, None);
    }

    #[test]
    fn tracked_ignores_unconfirmed_and_sorts() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            Some("1.0"),
            "stmods",
            Some("2026-10-12T12:00:00+00:00"),
            Some("1.0"),
            Some(10),
        );
        store
            .upsert_binding(ModBinding {
                mod_id: "a".to_string(),
                source: "modland".to_string(),
                source_id: "a".to_string(),
                url: Url::parse("https://modland.test/a").unwrap(),
                confirmed: false,
            })
            .unwrap();
        let tracked = UpdateCheckService::tracked(&store);
        assert_eq!(tracked.len(), 1);
        assert_eq!(tracked[0].source, "stmods");
        assert_eq!(tracked[0].last_version.as_deref(), Some("1.0"));
    }

    #[test]
    fn dismissed_update_on_one_source_does_not_hide_newer_on_another() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            Some("1.0"),
            "stmods",
            Some("2026-10-12T12:00:00+00:00"),
            Some("1.1"),
            Some(100),
        );
        store
            .upsert_binding(ModBinding {
                mod_id: "a".to_string(),
                source: "modland".to_string(),
                source_id: "a".to_string(),
                url: Url::parse("https://modland.test/a").unwrap(),
                confirmed: true,
            })
            .unwrap();
        store.save_check(CheckState {
            last_file: None,
            mod_id: "a".to_string(),
            source: "modland".to_string(),
            last_checked_utc: now(),
            last_changed_utc: Some(now()),
            last_version: Some("1.2".to_string()),
            last_size: Some(100),
            miss_count: 0,
            last_game_version: None,
            last_title: None,
        });
        store.set_dismissed_update("a", Some("1.1"));

        let pending = UpdateCheckService::pending(&store, false);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].source, "modland");
        assert_eq!(pending[0].latest_version.as_deref(), Some("1.2"));
        assert!(!pending[0].is_dismissed);

        // Скрытая версия остаётся видна в списке «скрытые».
        let all = UpdateCheckService::pending(&store, true);
        assert_eq!(all.len(), 1, "одно обновление на мод: более уверенное");
        assert_eq!(all[0].source, "modland");
    }

    #[test]
    fn large_library_pending_and_plan_are_fast() {
        use std::time::Instant;

        let store = InMemoryLibraryStore::new();
        let t0 = DateTime::parse_from_rfc3339("2026-10-13T12:00:00+00:00").unwrap();
        for index in 0i64..5000 {
            let id = format!("m{index}");
            let source = if index % 2 == 0 { "a" } else { "b" };
            store.upsert_mod(InstalledMod {
                id: id.clone(),
                name: format!("mod {index}"),
                file_name: format!("m{index}.zip"),
                version: Some("1.0".to_string()),
                size_bytes: Some(1000 + index),
                installed_at_utc: t0,
                enabled: true,
            });
            store
                .upsert_binding(ModBinding {
                    mod_id: id.clone(),
                    source: source.to_string(),
                    source_id: format!("s{index}"),
                    url: Url::parse(&format!("https://x.test/{index}")).unwrap(),
                    confirmed: true,
                })
                .unwrap();
            store.save_check(CheckState {
                last_file: None,
                mod_id: id.clone(),
                source: source.to_string(),
                last_checked_utc: t0 - chrono::TimeDelta::days(3),
                last_changed_utc: Some(t0 - chrono::TimeDelta::days(3)),
                last_version: Some(if index % 10 == 0 { "1.1" } else { "1.0" }.to_string()),
                last_size: Some(1000 + index),
                miss_count: 0,
                last_game_version: None,
                last_title: None,
            });
        }
        let started = Instant::now();
        let pending = UpdateCheckService::pending(&store, false);
        let plan = UpdateCheckService::plan_feeds(&store, t0, &UpdateCheckOptions::default());
        assert_eq!(pending.len(), 500);
        assert_eq!(plan.due.len(), 5000);
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn page_search_is_not_used_by_pending() {
        let store = InMemoryLibraryStore::new();
        add(&store, "a", Some("1.0"), "stmods", None, None, None);
        store.set_page_search(
            "a",
            Some(PageSearch {
                at: now(),
                rejected: false,
            }),
        );
        assert!(UpdateCheckService::pending(&store, false).is_empty());
    }
}
