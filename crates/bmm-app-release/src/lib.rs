//! Обновление самой программы: `latest.json`, загрузка, подмена exe, откат.
//!
//! Имя крейта не содержит «update»: Windows UAC принимает exe с таким словом
//! в имени за установщик и требует повышение, из-за чего не запускаются тесты.
//!
//! Слои:
//! - [`manifest`] — разбор и проверка `latest.json` (https, только GitHub,
//!   версия, размер, SHA-256, заметки и дата);
//! - [`download`] — описание и архив через [`HttpTransport`], поток в `.part`,
//!   сверка размера и хеша, прогресс и отмена;
//! - [`local_feed`] — те же проверки на сборочной машине, файлы из папки вместо GitHub;
//! - [`install`] — распаковка exe из zip, подготовка `*.new`, подмена с откатом,
//!   запись в тот же файл с копией `previous.exe`, `*.bad` и уборка;
//! - [`update`] — `check(local_version, manifest_url/bytes)` и состояние плашки.
//!
//! Порядок для Tauri:
//! 1. [`check`] с версией приложения и [`ManifestInput`] — получить [`UpdateState`]
//!    со стадией [`UpdateStage::Available`].
//! 2. [`UpdateDownloader::download_into`] — скачать архив с прогрессом и отменой.
//! 3. [`prepare_update`] — распаковать и проверить exe, получить [`PreparedUpdate`];
//!    рабочий exe не меняется.
//! 4. `PreparedUpdate::apply()` — после выхода прежнего процесса записать новую
//!    версию в тот же файл; `restore()` — вернуть прежнюю, если новая не запустилась.
//!
//! Запуск процесса после обновления и интерфейс — вне этого крейта.

mod paths;

pub mod download;
pub mod install;
pub mod local_feed;
pub mod manifest;
pub mod update;
pub mod version;

pub use download::{
    CancelToken, DEFAULT_USER_AGENT, DownloadError, DownloadedUpdate, HttpResponse, HttpTransport,
    MAX_MANIFEST_BYTES, MAX_REDIRECTS, ReqwestTransport, TransportError, UpdateDownloader,
};
pub use install::{
    BAD_SUFFIX, EXE_NAME, InstallError, NEW_SUFFIX, NO_RIGHTS, OLD_SUFFIX, PrepareOptions,
    PreparedUpdate, check_writable, cleanup, commit, commit_with, extract_exe,
    is_temporary_location, looks_like_exe, overwrite_file, prepare_update, read_skipped,
    restore_in_place, rollback, rollback_from_running, stage, swap, swap_with, write_in_place,
    write_in_place_with, write_skipped,
};
pub use local_feed::LocalFeedTransport;
pub use manifest::{
    MAX_NOTES_CHARS, MAX_SIZE, MIN_SIZE, ManifestError, UpdateManifest, is_allowed_host,
    parse_manifest,
};
pub use update::{
    ManifestInput, UpdateStage, UpdateState, check, check_bytes, check_json, check_url,
    check_with_cancel, evaluate,
};
pub use version::{AppVersion, VersionError};
