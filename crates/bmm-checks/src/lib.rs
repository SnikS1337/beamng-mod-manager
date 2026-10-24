//! Проверка обновлений установленных модов.
//!
//! Имя крейта не содержит «update»: Windows UAC принимает такой exe за
//! установщик и требует повышение, из-за чего не запускаются тесты.
//!
//! Слои:
//! - [`UpdateCheckPlanner`] / [`FeedCoverage`] — сколько запросов потратить (сеть не трогают);
//! - [`UpdateRunner`] — выполняет план через [`bmm_sources::sources::ModSource`];
//! - [`UpdateCoordinator`] — одна проверка за раз, отмена, пауза после сбоя;
//! - [`UpdateCheckService`] — список обновлений, «Пропустить версию»;
//! - [`PageGuesser`], [`pages`], [`BindingSuggester`], [`BindingRepair`], [`BindingActions`] —
//!   подбор и починка привязок.
//!
//! Пример запуска из Tauri-команды:
//!
//! ```no_run
//! # use std::sync::{Arc, atomic::AtomicBool};
//! # use chrono::Utc;
//! # use bmm_domain::library::InMemoryLibraryStore;
//! # use bmm_checks::{UpdateCheckOptions, UpdateCheckService, UpdateRun, UpdateRunner};
//! # let store = InMemoryLibraryStore::new();
//! # let sources: Vec<Arc<dyn bmm_sources::sources::ModSource>> = Vec::new();
//! let cancel = AtomicBool::new(false);
//! let now = Utc::now().fixed_offset();
//! let run = UpdateRun::new(&store, &sources, now, UpdateCheckOptions::default(), &cancel);
//! let report = UpdateRunner::run(&run, None);
//! let visible = UpdateCheckService::pending(&store, false);
//! # let _ = (report, visible);
//! ```

pub mod bindings;
pub mod coordinator;
pub mod options;
pub mod page_guesser;
pub mod pages;
pub mod planner;
pub mod runner;
pub mod service;

pub use bindings::{
    BindingActions, BindingRepair, BindingSuggester, BindingSuggestion, DeadBinding, DeclineUndo,
    RepairProposal,
};
pub use coordinator::{UpdateCoordinator, UpdateCoordinatorOptions};
pub use options::UpdateCheckOptions;
pub use page_guesser::{PageGuess, PageGuesser};
pub use pages::{
    PageSearchReport, Unfollowed, bind_page, confirm_page, find_pages, refollow_page, reject_page,
    suggestion_for, suggestions, undo_page, unfollow_page,
};
pub use planner::{
    CardPlan, FeedCoverage, FeedEntry, FeedPlan, SkippedSource, SourceStatus, TrackedBinding,
    UpdateCheckPlanner,
};
pub use runner::{
    SourceRunReport, UpdateProgress, UpdateRun, UpdateRunReport, UpdateRunner,
    run as run_update_check,
};
pub use service::{UpdateCheckService, UpdateFinding};
