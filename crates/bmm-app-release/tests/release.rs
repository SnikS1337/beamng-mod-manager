//! Самообновление программы: описание версии, сеть (локальная подделка и локальный HTTP-сервер), распаковка,
//! подмена/откат, Unicode-пути, отмена. Процессы не запускаются.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Cursor, ErrorKind, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use bmm_app_release::{
    AppVersion, BAD_SUFFIX, CancelToken, DEFAULT_USER_AGENT, DownloadError, EXE_NAME, HttpResponse,
    HttpTransport, InstallError, LocalFeedTransport, MAX_MANIFEST_BYTES, MIN_SIZE, ManifestInput,
    NEW_SUFFIX, OLD_SUFFIX, PrepareOptions, ReqwestTransport, TransportError, UpdateDownloader,
    UpdateManifest, UpdateStage, check, check_bytes, check_json, check_url, cleanup, commit_with,
    evaluate, extract_exe, is_temporary_location, prepare_update, read_skipped, rollback,
    rollback_from_running, stage, swap, swap_with, write_in_place, write_in_place_with,
    write_skipped,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use url::Url;

const MANIFEST_URL: &str = "https://github.com/o/r/releases/latest/download/latest.json";
const ZIP_URL: &str = "https://github.com/o/r/releases/download/v0.1.30/ModManagerBeamNG.zip";
const BLOB_URL: &str = "https://objects.githubusercontent.com/blob/zip";

fn url(text: &str) -> Url {
    Url::parse(text).unwrap()
}

// ---------------------------------------------------------------------------
// Манифест и версии
// ---------------------------------------------------------------------------

#[test]
fn default_user_agent_uses_the_crate_version() {
    assert!(
        DEFAULT_USER_AGENT.contains(env!("CARGO_PKG_VERSION")),
        "{DEFAULT_USER_AGENT}"
    );
    assert!(
        !DEFAULT_USER_AGENT.contains("/0.4 "),
        "{DEFAULT_USER_AGENT}"
    );
}

#[test]
fn release_build_is_detected() {
    for text in ["0.3", "0.2", "0.3.1", "0.1.129"] {
        assert!(
            AppVersion::parse(text).unwrap().is_release(),
            "{text} — выпуск"
        );
    }
    assert!(!AppVersion::parse("1.0.0.0").unwrap().is_release());
    assert!(!AppVersion::parse("0.0.0.0").unwrap().is_release());
}

#[test]
fn parses_manifest_and_normalizes_version() {
    let zip = release_zip(true, true, EXE_NAME);
    let json = manifest_json("v0.1.30", &zip, ZIP_URL, None);
    let manifest = UpdateManifest::parse(&json).unwrap();

    assert_eq!(manifest.version, AppVersion::parse("0.1.30.0").unwrap());
    assert_eq!(manifest.version.display(), "0.1.30");
    assert_eq!(manifest.size, zip.len() as u64);
    assert_eq!(manifest.notes.as_deref(), Some("Что нового"));
    assert!(manifest.published.is_some(), "дата разобрана");
}

#[test]
fn short_versions_compare_and_display() {
    assert_eq!(
        AppVersion::parse("0.2").unwrap(),
        AppVersion::parse("v0.2.0.0").unwrap()
    );
    assert_eq!(AppVersion::parse("0.2").unwrap().display(), "0.2");
    assert_eq!(AppVersion::parse("0.2.1").unwrap().display(), "0.2.1");
    assert_eq!(AppVersion::parse("1.0.0").unwrap().display(), "1.0");
    assert_eq!(AppVersion::parse("0.2.0.4").unwrap().display(), "0.2.0.4");
    assert!(
        AppVersion::parse("0.2")
            .unwrap()
            .is_newer_than(&AppVersion::parse("0.1.129").unwrap()),
        "мост: 0.2 новее 0.1.129"
    );
    assert!(
        !AppVersion::parse("0.2.0")
            .unwrap()
            .is_newer_than(&AppVersion::parse("0.2").unwrap()),
        "та же версия"
    );
    let zip = release_zip(true, true, EXE_NAME);
    let manifest = UpdateManifest::parse(&manifest_json("0.2", &zip, ZIP_URL, None)).unwrap();
    assert_eq!(manifest.version, AppVersion::parse("0.2.0.0").unwrap());
}

#[test]
fn parses_real_latest_json_fixture() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/update/latest-0.1.30.json");
    let json = fs::read_to_string(&path).unwrap();
    let manifest = UpdateManifest::parse(&json).unwrap();

    assert_eq!(manifest.version.display(), "0.1.30");
    assert_eq!(
        manifest.url.as_str(),
        "https://github.com/SnikS1337/ModManagerBeamNG/releases/download/v0.1.30/ModManagerBeamNG.zip"
    );
    assert!(
        manifest.size > MIN_SIZE && manifest.size < bmm_app_release::MAX_SIZE,
        "размер правдоподобный"
    );
    assert!(manifest.published.is_some(), "дата из фикстуры");
    assert!(manifest.notes.is_some(), "заметки из фикстуры");
}

#[test]
fn rejects_untrusted_or_broken_manifest() {
    let zip = release_zip(true, true, EXE_NAME);
    let no_sha = format!(
        r#"{{"version":"0.1.30","url":"{ZIP_URL}","size":{}}}"#,
        zip.len()
    );
    let cases = [
        (
            manifest_json("0.1.30", &zip, "http://github.com/o/r/x.zip", None),
            "http",
        ),
        (
            manifest_json("0.1.30", &zip, "https://evil.example/x.zip", None),
            "чужой сайт",
        ),
        (
            manifest_json(
                "0.1.30",
                &zip,
                "https://github.com.evil.example/x.zip",
                None,
            ),
            "похожий адрес",
        ),
        (manifest_json("0.1.30", &zip, ZIP_URL, Some("abc")), "сумма"),
        (
            manifest_json("0.1.30", &[0u8; 100], ZIP_URL, None),
            "крошечный файл",
        ),
        (manifest_json("версия", &zip, ZIP_URL, None), "версия"),
        ("{ сломано".to_string(), "мусор"),
        ("[1]".to_string(), "не объект"),
        (no_sha, "нет суммы"),
    ];
    for (json, why) in cases {
        assert!(
            UpdateManifest::parse(&json).is_err(),
            "должно быть отклонено: {why}"
        );
    }
}

#[test]
fn newer_is_strictly_greater() {
    assert!(
        AppVersion::parse("0.1.30")
            .unwrap()
            .is_newer_than(&AppVersion::parse("0.1.29").unwrap())
    );
    assert!(
        !AppVersion::parse("0.1.30")
            .unwrap()
            .is_newer_than(&AppVersion::parse("0.1.30.0").unwrap())
    );
    assert!(
        !AppVersion::parse("0.1.9")
            .unwrap()
            .is_newer_than(&AppVersion::parse("0.1.30").unwrap()),
        "9 < 30 как числа"
    );
    for garbage in ["", " ", "версия", "1.2.3.4.5", "1..2", "v"] {
        assert!(
            AppVersion::parse(garbage).is_none(),
            "мусор не версия: {garbage:?}"
        );
    }
    assert_eq!(
        AppVersion::parse("1.2 ").unwrap().display(),
        "1.2",
        "хвостовой пробел срезается"
    );
}

#[test]
fn notes_are_truncated_and_bad_date_is_ignored() {
    let zip = release_zip(true, true, EXE_NAME);
    let notes = "я".repeat(9000);
    let json = format!(
        r#"{{"version":"0.1.30","url":"{ZIP_URL}","size":{},"sha256":"{}","notes":"{notes}","published":"не дата"}}"#,
        zip.len(),
        sha_hex(&zip)
    );
    let manifest = UpdateManifest::parse(&json).unwrap();
    assert_eq!(
        manifest.notes.as_deref().map(|notes| notes.chars().count()),
        Some(8000)
    );
    assert!(manifest.published.is_none());
}

#[test]
fn long_notes_are_kept_whole_lines() {
    // Было: заметки обрезались на 500 знаках, и «Что нового в версии 1.2»
    // заканчивалось на «Имя файла п».
    let zip = release_zip(true, true, EXE_NAME);
    let line = format!("- {}", "строка изменений ".repeat(10).trim_end());
    let lines: Vec<String> = (0..60).map(|_| line.clone()).collect();
    let notes = lines.join("\\n");
    let json = format!(
        r#"{{"version":"0.1.30","url":"{ZIP_URL}","size":{},"sha256":"{}","notes":"{notes}"}}"#,
        zip.len(),
        sha_hex(&zip)
    );
    let manifest = UpdateManifest::parse(&json).unwrap();
    let kept = manifest.notes.unwrap();
    assert!(kept.chars().count() <= 8000);
    assert!(kept.lines().all(|kept| kept == line), "строки целые");
    assert!(kept.lines().count() > 3);
}

// ---------------------------------------------------------------------------
// Сеть: локальная подделка и один настоящий локальный HTTP-сервер
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Route {
    Body { status: u16, body: Vec<u8> },
    BodyNoLength { body: Vec<u8> },
    BodyWithLength { body: Vec<u8>, length: u64 },
    Redirect(String),
    Status(u16),
    Error(TransportError),
    CancelAfter { body: Vec<u8>, token: CancelToken },
}

impl Route {
    fn body(status: u16, body: Vec<u8>) -> Self {
        Self::Body { status, body }
    }
}

#[derive(Default)]
struct MemoryTransport {
    routes: Mutex<HashMap<String, Route>>,
    calls: Mutex<Vec<String>>,
}

impl MemoryTransport {
    fn new() -> Self {
        Self::default()
    }

    fn route(&self, url: &str, route: Route) {
        self.routes.lock().unwrap().insert(url.to_string(), route);
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl HttpTransport for MemoryTransport {
    fn send(&self, url: &Url) -> Result<HttpResponse, TransportError> {
        self.calls.lock().unwrap().push(url.to_string());
        let route = self
            .routes
            .lock()
            .unwrap()
            .get(url.as_str())
            .cloned()
            .unwrap_or(Route::Status(404));
        match route {
            Route::Body { status, body } => Ok(HttpResponse {
                status,
                content_length: Some(body.len() as u64),
                location: None,
                body: Box::new(Cursor::new(body)),
            }),
            Route::BodyNoLength { body } => Ok(HttpResponse {
                status: 200,
                content_length: None,
                location: None,
                body: Box::new(Cursor::new(body)),
            }),
            Route::BodyWithLength { body, length } => Ok(HttpResponse {
                status: 200,
                content_length: Some(length),
                location: None,
                body: Box::new(Cursor::new(body)),
            }),
            Route::Redirect(location) => Ok(HttpResponse {
                status: 302,
                content_length: Some(0),
                location: Some(location),
                body: Box::new(Cursor::new(Vec::new())),
            }),
            Route::Status(status) => Ok(HttpResponse {
                status,
                content_length: Some(0),
                location: None,
                body: Box::new(Cursor::new(Vec::new())),
            }),
            Route::Error(error) => Err(error),
            Route::CancelAfter { body, token } => Ok(HttpResponse {
                status: 200,
                content_length: Some(body.len() as u64),
                location: None,
                body: Box::new(CancelAfterReader {
                    inner: Cursor::new(body),
                    token,
                    fired: false,
                }),
            }),
        }
    }
}

struct CancelAfterReader {
    inner: Cursor<Vec<u8>>,
    token: CancelToken,
    fired: bool,
}

impl Read for CancelAfterReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if !self.fired {
            self.fired = true;
            self.token.cancel();
        }
        self.inner.read(buffer)
    }
}

fn publish(net: &MemoryTransport, version: &str, zip: &[u8], sha: Option<&str>) -> UpdateManifest {
    let json = manifest_json(version, zip, ZIP_URL, sha);
    let manifest = UpdateManifest::parse(&json).unwrap();
    net.route(MANIFEST_URL, Route::body(200, json.into_bytes()));
    net.route(ZIP_URL, Route::Redirect(BLOB_URL.to_string()));
    net.route(BLOB_URL, Route::body(200, zip.to_vec()));
    manifest
}

#[test]
fn downloads_manifest_and_archive_and_reports_progress() {
    let net = Arc::new(MemoryTransport::new());
    let zip = release_zip(true, true, EXE_NAME);
    let manifest = publish(&net, "0.1.30", &zip, None);
    let downloader = UpdateDownloader::new(net.clone());
    let cancel = CancelToken::new();
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("u.zip");
    let part = suffixed(&target, ".part");

    let mut progress = Vec::new();
    downloader
        .download(
            &manifest,
            &target,
            &mut |value| progress.push(value),
            &cancel,
        )
        .unwrap();

    assert_eq!(fs::read(&target).unwrap(), zip);
    assert!(!part.exists(), "частичного файла нет");
    assert!(
        progress.iter().all(|value| (0.0..=1.0).contains(value)),
        "{progress:?}"
    );
    assert_eq!(progress.last().copied(), Some(1.0));
    assert!(
        progress.windows(2).all(|pair| pair[0] <= pair[1]),
        "прогресс не идёт назад: {progress:?}"
    );
    assert!(net.calls().iter().any(|call| call == BLOB_URL));
    let _ = fs::remove_file(&target);
}

#[test]
fn download_into_builds_update_folder_for_version() {
    let net = Arc::new(MemoryTransport::new());
    let zip = release_zip(true, true, EXE_NAME);
    let manifest = publish(&net, "0.1.30", &zip, None);
    let downloader = UpdateDownloader::new(net);
    let root = tempfile::tempdir().unwrap();
    let downloaded = downloader
        .download_into(&manifest, root.path(), &mut |_| {}, &CancelToken::new())
        .unwrap();
    assert_eq!(downloaded.version.display(), "0.1.30");
    assert_eq!(
        downloaded.path,
        root.path().join("0.1.30").join("update.zip")
    );
    assert_eq!(fs::read(&downloaded.path).unwrap(), zip);
}

#[test]
fn foreign_redirect_is_not_followed() {
    let net = Arc::new(MemoryTransport::new());
    let zip = release_zip(true, true, EXE_NAME);
    let manifest = publish(&net, "0.1.30", &zip, None);
    net.route(
        ZIP_URL,
        Route::Redirect("https://evil.example/zip".to_string()),
    );
    let downloader = UpdateDownloader::new(net.clone());
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("u.zip");
    let error = downloader
        .download(&manifest, &target, &mut |_| {}, &CancelToken::new())
        .unwrap_err();
    assert_eq!(error, DownloadError::UntrustedUrl);
    assert!(
        net.calls().iter().all(|call| !call.contains("evil")),
        "к чужому сайту не ходили: {:?}",
        net.calls()
    );
    assert!(!target.exists());
    assert!(!suffixed(&target, ".part").exists());
}

#[test]
fn rejects_corrupted_truncated_oversized_and_mismatched_downloads() {
    let net = Arc::new(MemoryTransport::new());
    let zip = release_zip(true, true, EXE_NAME);
    let manifest = publish(&net, "0.1.30", &zip, None);
    let downloader = UpdateDownloader::new(net.clone());
    let root = tempfile::tempdir().unwrap();

    let target = root.path().join("bad-sha.zip");
    let mut bad = zip.clone();
    let last = bad.len() - 1;
    bad[last] ^= 0xff;
    net.route(BLOB_URL, Route::body(200, bad));
    let error = downloader
        .download(&manifest, &target, &mut |_| {}, &CancelToken::new())
        .unwrap_err();
    assert_eq!(error, DownloadError::ChecksumMismatch);
    assert!(!target.exists() && !suffixed(&target, ".part").exists());

    let target = root.path().join("short.zip");
    net.route(
        BLOB_URL,
        Route::BodyNoLength {
            body: zip[..zip.len() / 2].to_vec(),
        },
    );
    let error = downloader
        .download(&manifest, &target, &mut |_| {}, &CancelToken::new())
        .unwrap_err();
    assert_eq!(error, DownloadError::Truncated);
    assert!(!target.exists() && !suffixed(&target, ".part").exists());

    let target = root.path().join("long.zip");
    let mut oversized = zip.clone();
    oversized.extend_from_slice(&[0u8; 10]);
    net.route(BLOB_URL, Route::BodyNoLength { body: oversized });
    let error = downloader
        .download(&manifest, &target, &mut |_| {}, &CancelToken::new())
        .unwrap_err();
    assert_eq!(error, DownloadError::TooLarge);
    assert!(!target.exists() && !suffixed(&target, ".part").exists());

    let target = root.path().join("length.zip");
    net.route(
        BLOB_URL,
        Route::BodyWithLength {
            body: zip.clone(),
            length: zip.len() as u64 + 1,
        },
    );
    let error = downloader
        .download(&manifest, &target, &mut |_| {}, &CancelToken::new())
        .unwrap_err();
    assert_eq!(error, DownloadError::SizeMismatch);
    assert!(!target.exists() && !suffixed(&target, ".part").exists());
}

#[test]
fn no_releases_and_network_failures_are_clear() {
    let net = Arc::new(MemoryTransport::new());
    let downloader = UpdateDownloader::new(net.clone());
    let cancel = CancelToken::new();
    let error = downloader
        .get_manifest(&url(MANIFEST_URL), &cancel)
        .unwrap_err();
    assert_eq!(error, DownloadError::NoReleases);
    assert_eq!(error.to_string(), "выпусков пока нет");

    net.route(MANIFEST_URL, Route::Error(TransportError::Network));
    let error = downloader
        .get_manifest(&url(MANIFEST_URL), &cancel)
        .unwrap_err();
    assert_eq!(error.to_string(), "нет связи с GitHub");

    net.route(MANIFEST_URL, Route::Error(TransportError::Timeout));
    let error = downloader
        .get_manifest(&url(MANIFEST_URL), &cancel)
        .unwrap_err();
    assert_eq!(error.to_string(), "сервер не отвечает");
}

#[test]
fn oversized_manifest_is_rejected() {
    let net = Arc::new(MemoryTransport::new());
    net.route(
        MANIFEST_URL,
        Route::BodyNoLength {
            body: vec![b' '; MAX_MANIFEST_BYTES + 1],
        },
    );
    let downloader = UpdateDownloader::new(net);
    let error = downloader
        .get_manifest(&url(MANIFEST_URL), &CancelToken::new())
        .unwrap_err();
    assert_eq!(error, DownloadError::ManifestTooLarge);
}

#[test]
fn cancellation_before_and_during_download_removes_partial_file() {
    let net = Arc::new(MemoryTransport::new());
    let zip = release_zip(true, true, EXE_NAME);
    let manifest = publish(&net, "0.1.30", &zip, None);
    let downloader = UpdateDownloader::new(net.clone());
    let root = tempfile::tempdir().unwrap();

    let target = root.path().join("cancelled.zip");
    let cancel = CancelToken::new();
    cancel.cancel();
    let error = downloader
        .download(&manifest, &target, &mut |_| {}, &cancel)
        .unwrap_err();
    assert_eq!(error, DownloadError::Cancelled);
    assert!(!target.exists() && !suffixed(&target, ".part").exists());

    let target = root.path().join("cancelled-mid.zip");
    let cancel = CancelToken::new();
    net.route(
        BLOB_URL,
        Route::CancelAfter {
            body: zip,
            token: cancel.clone(),
        },
    );
    let error = downloader
        .download(&manifest, &target, &mut |_| {}, &cancel)
        .unwrap_err();
    assert_eq!(error, DownloadError::Cancelled);
    assert!(!target.exists() && !suffixed(&target, ".part").exists());
}

#[test]
fn real_transport_reads_local_http_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        assert!(
            String::from_utf8_lossy(&request).starts_with("GET /file HTTP/1.1"),
            "{}",
            String::from_utf8_lossy(&request)
        );
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        stream.write_all(b"hello").unwrap();
    });

    let transport = ReqwestTransport::new("bmm-test").unwrap();
    let response = transport
        .send(&url(&format!("http://{address}/file")))
        .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.content_length, Some(5));
    let mut body = Vec::new();
    response.body.take(64).read_to_end(&mut body).unwrap();
    assert_eq!(body, b"hello");
    server.join().unwrap();
}

// ---------------------------------------------------------------------------
// Установка: распаковка, подмена, откат, Unicode
// ---------------------------------------------------------------------------

#[test]
fn extract_takes_only_program_and_verifies_its_version() {
    let root = tempfile::tempdir().unwrap();
    let zip_path = root.path().join("u.zip");
    fs::write(&zip_path, release_zip(true, true, EXE_NAME)).unwrap();
    let expected = AppVersion::parse("0.1.30").unwrap();

    let exe = extract_exe(&zip_path, &root.path().join("st"), expected, &|_| {
        Some(expected)
    })
    .unwrap();
    assert!(exe.is_file());
    assert!(bmm_app_release::looks_like_exe(&exe));

    let wrong = AppVersion::parse("0.1.31").unwrap();
    let error = extract_exe(&zip_path, &root.path().join("st2"), expected, &|_| {
        Some(wrong)
    })
    .unwrap_err();
    assert!(error.to_string().contains("версия"), "{error}");
    assert!(!root.path().join("st2").join(EXE_NAME).exists());

    fs::write(&zip_path, release_zip(false, true, EXE_NAME)).unwrap();
    let error = extract_exe(&zip_path, &root.path().join("st3"), expected, &|_| {
        Some(expected)
    })
    .unwrap_err();
    assert_eq!(error, InstallError::ProgramMissing);

    fs::write(&zip_path, release_zip(true, false, EXE_NAME)).unwrap();
    let error = extract_exe(&zip_path, &root.path().join("st4"), expected, &|_| {
        Some(expected)
    })
    .unwrap_err();
    assert_eq!(error, InstallError::NotWindowsExe);
}

#[test]
fn extract_rejects_unsafe_paths_and_duplicates() {
    let root = tempfile::tempdir().unwrap();
    let expected = AppVersion::parse("0.1.30").unwrap();

    for (index, unsafe_name) in [
        "../BeamModManager.exe",
        "sub/BeamModManager.exe",
        "..\\BeamModManager.exe",
    ]
    .into_iter()
    .enumerate()
    {
        let zip_path = root.path().join(format!("unsafe-{index}.zip"));
        fs::write(&zip_path, zip_entries(&[(unsafe_name, b"MZ fake program")])).unwrap();
        let staging = root.path().join(format!("st-{index}"));
        let error = extract_exe(&zip_path, &staging, expected, &|_| Some(expected)).unwrap_err();
        assert_eq!(error, InstallError::ProgramMissing);
        assert!(!staging.join(EXE_NAME).exists());
        assert!(!root.path().join(EXE_NAME).exists());
    }

    let zip_path = root.path().join("duplicate.zip");
    fs::write(
        &zip_path,
        zip_entries(&[
            (EXE_NAME, b"MZ first"),
            ("beammodmanager.exe", b"MZ second"),
        ]),
    )
    .unwrap();
    let error = extract_exe(&zip_path, &root.path().join("dup-st"), expected, &|_| {
        Some(expected)
    })
    .unwrap_err();
    assert_eq!(error, InstallError::ProgramMissing);
}

#[test]
fn extract_rejects_symlink_entry() {
    let root = tempfile::tempdir().unwrap();
    let zip_path = root.path().join("symlink.zip");
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer
            .add_symlink(EXE_NAME, "other-program.exe", options)
            .unwrap();
        writer.finish().unwrap();
    }
    fs::write(&zip_path, cursor.into_inner()).unwrap();
    let error = extract_exe(
        &zip_path,
        &root.path().join("st"),
        AppVersion::parse("0.1.30").unwrap(),
        &|_| Some(AppVersion::parse("0.1.30").unwrap()),
    )
    .unwrap_err();
    assert_eq!(error, InstallError::NotWindowsExe);
}

#[test]
fn swap_rollback_and_cleanup() {
    let rig = Rig::new();
    let new = rig.write_new("новая");
    fs::write(suffixed(&rig.exe, OLD_SUFFIX), "совсем старая").unwrap();
    fs::write(suffixed(&rig.exe, NEW_SUFFIX), "обрывок").unwrap();

    assert!(swap(&rig.exe, &new).is_ok(), "встала");
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "новая");
    assert_eq!(
        fs::read_to_string(suffixed(&rig.exe, OLD_SUFFIX)).unwrap(),
        "старая версия"
    );
    assert!(rollback(&rig.exe), "откат");
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");

    assert!(swap(&rig.exe, &rig.root.path().join("нет.exe")).is_err());
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");

    fs::write(suffixed(&rig.exe, OLD_SUFFIX), "x").unwrap();
    fs::create_dir_all(rig.update_dir.join("0.1.30")).unwrap();
    cleanup(&rig.exe, &rig.update_dir);
    assert!(!suffixed(&rig.exe, OLD_SUFFIX).exists());
    assert!(!rig.update_dir.exists(), "папка обновления убрана");
}

#[test]
fn swap_copies_next_to_program_and_renames_only_there() {
    let rig = Rig::new();
    let elsewhere = rig.root.path().join("elsewhere").join(EXE_NAME);
    fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
    fs::write(&elsewhere, "новая").unwrap();

    let mut moves = Vec::new();
    let result = swap_with(&rig.exe, &elsewhere, &mut |from, to| {
        moves.push((from.to_path_buf(), to.to_path_buf()));
        fs::rename(from, to)
    });
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "новая");
    assert!(
        !suffixed(&rig.exe, NEW_SUFFIX).exists(),
        "копия стала программой"
    );
    assert_eq!(moves.len(), 2, "{moves:?}");
    let parent = rig.exe.parent().unwrap();
    assert!(
        moves
            .iter()
            .all(|(from, to)| from.parent() == Some(parent) && to.parent() == Some(parent)),
        "переименования только рядом с exe: {moves:?}"
    );
    assert_eq!(moves[0], (rig.exe.clone(), suffixed(&rig.exe, OLD_SUFFIX)));
    assert_eq!(moves[1], (suffixed(&rig.exe, NEW_SUFFIX), rig.exe.clone()));
    assert!(elsewhere.exists(), "подготовленный файл не тронут");
}

#[test]
fn swap_failure_returns_previous_version_and_removes_new() {
    let rig = Rig::new();
    let new = rig.write_new("новая");
    let result = swap_with(&rig.exe, &new, &mut |from, to| {
        if from.to_string_lossy().ends_with(NEW_SUFFIX) {
            return Err(io::Error::other("файл занят"));
        }
        fs::rename(from, to)
    });
    let error = result.unwrap_err();
    assert!(error.to_string().contains("не установилась"), "{error}");
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");
    assert!(!suffixed(&rig.exe, NEW_SUFFIX).exists(), "копия убрана");
}

#[test]
fn swap_without_rights_leaves_no_traces() {
    let rig = Rig::new();
    let new = rig.write_new("новая");
    let error = swap_with(&rig.exe, &new, &mut |_, _| {
        Err(io::Error::from(ErrorKind::PermissionDenied))
    })
    .unwrap_err();
    assert!(error.to_string().contains("нет прав"), "{error}");
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");
    assert!(!suffixed(&rig.exe, NEW_SUFFIX).exists());
    assert!(!suffixed(&rig.exe, OLD_SUFFIX).exists());
}

#[test]
fn failed_swap_reports_how_to_rename_back() {
    let rig = Rig::new();
    let new = rig.write_new("новая");
    let mut first = true;
    let error = swap_with(&rig.exe, &new, &mut |from, to| {
        if first {
            first = false;
            return fs::rename(from, to);
        }
        Err(io::Error::other("занято"))
    })
    .unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("BeamModManager.exe.old") && message.contains("обратно"),
        "{message}"
    );
    assert_eq!(
        fs::read_to_string(suffixed(&rig.exe, OLD_SUFFIX)).unwrap(),
        "старая версия"
    );
}

#[test]
fn swap_refuses_when_staged_place_is_a_directory() {
    let rig = Rig::new();
    let new = rig.write_new("новая");
    fs::create_dir_all(suffixed(&rig.exe, NEW_SUFFIX)).unwrap();

    let error = swap(&rig.exe, &new).unwrap_err();
    assert!(
        matches!(error, InstallError::NoRights | InstallError::StageFailed(_)),
        "{error}"
    );
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");
    assert!(
        !suffixed(&rig.exe, OLD_SUFFIX).exists(),
        "текущую не отодвигали"
    );
}

#[test]
fn swap_refuses_when_current_program_is_missing() {
    let rig = Rig::new();
    let new = rig.write_new("новая");
    fs::remove_file(&rig.exe).unwrap();
    assert!(swap(&rig.exe, &new).is_err());
    assert!(!rig.exe.exists());
    assert!(!suffixed(&rig.exe, NEW_SUFFIX).exists());
}

#[test]
fn unicode_paths_survive_swap_and_rollback() {
    let rig = Rig::new();
    let directory = rig
        .root
        .path()
        .join("Мои программы (2)")
        .join("Mod Manager — BeamNG");
    fs::create_dir_all(&directory).unwrap();
    let exe = directory.join(EXE_NAME);
    fs::write(&exe, "старая").unwrap();
    let new = rig.root.path().join("обновление").join(EXE_NAME);
    fs::create_dir_all(new.parent().unwrap()).unwrap();
    fs::write(&new, "новая").unwrap();

    assert!(swap(&exe, &new).is_ok(), "встала");
    assert_eq!(fs::read_to_string(&exe).unwrap(), "новая");
    assert!(rollback(&exe), "откат");
    assert_eq!(fs::read_to_string(&exe).unwrap(), "старая");
    assert!(is_temporary_location(&exe), "путь лежит во временной папке");
}

#[test]
fn write_in_place_replaces_same_file_and_restores_backup() {
    let rig = Rig::new();
    let new = rig.write_new("новая");
    let backup = rig.update_dir.join("previous.exe");

    assert!(write_in_place(&new, &rig.exe, &backup).is_ok(), "записано");
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "новая");
    assert_eq!(fs::read_to_string(&backup).unwrap(), "старая версия");
    assert_eq!(
        fs::read_dir(rig.exe.parent().unwrap()).unwrap().count(),
        1,
        "рядом с программой ничего лишнего"
    );
    assert!(
        bmm_app_release::restore_in_place(&backup, &rig.exe),
        "вернули"
    );
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");
    assert!(!bmm_app_release::restore_in_place(
        &backup.with_file_name("нет.exe"),
        &rig.exe
    ));
}

#[test]
fn write_in_place_cuts_longer_previous_version() {
    let rig = Rig::new();
    fs::write(&rig.exe, "x".repeat(5000)).unwrap();
    let new = rig.write_new("коротко");
    let backup = rig.root.path().join("b.exe");
    assert!(write_in_place(&new, &rig.exe, &backup).is_ok(), "записано");
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "коротко");
}

#[test]
fn interrupted_write_restores_backup_and_permission_error_is_clear() {
    let rig = Rig::new();
    let new = rig.write_new("новая версия программы");
    let backup = rig.root.path().join("b.exe");
    let error = write_in_place_with(&new, &rig.exe, &backup, &mut |_, target| {
        fs::write(target, "обры").unwrap();
        Err(io::Error::other("диск заполнен"))
    })
    .unwrap_err();
    assert!(error.to_string().contains("не записалась"), "{error}");
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");

    let denied = write_in_place_with(&new, &rig.exe, &backup, &mut |_, _| {
        Err(io::Error::from(ErrorKind::PermissionDenied))
    })
    .unwrap_err();
    assert!(denied.to_string().starts_with("нет прав"), "{denied}");
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");

    let missing = rig.exe.with_extension("нет");
    assert_eq!(
        write_in_place(&new, &missing, &backup).unwrap_err(),
        InstallError::CurrentMissing
    );
}

#[test]
fn commit_failure_keeps_previous_and_removes_staged() {
    let rig = Rig::new();
    let new = rig.write_new("новая");
    assert!(stage(&rig.exe, &new).is_ok(), "подготовлено");
    let error = commit_with(&rig.exe, &mut |from, to| {
        if from.to_string_lossy().ends_with(NEW_SUFFIX) {
            return Err(io::Error::other("занято"));
        }
        fs::rename(from, to)
    })
    .unwrap_err();
    assert!(error.to_string().contains("не установилась"), "{error}");
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");
    assert!(!suffixed(&rig.exe, NEW_SUFFIX).exists());
}

#[test]
fn rollback_from_running_puts_itself_into_bad() {
    let rig = Rig::new();
    let new = rig.write_new("новая");
    assert!(swap(&rig.exe, &new).is_ok(), "встала");
    fs::write(suffixed(&rig.exe, BAD_SUFFIX), "старый мусор").unwrap();

    assert!(rollback_from_running(&rig.exe), "вернула");
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");
    assert_eq!(
        fs::read_to_string(suffixed(&rig.exe, BAD_SUFFIX)).unwrap(),
        "новая"
    );
    assert!(!suffixed(&rig.exe, OLD_SUFFIX).exists());

    cleanup(&rig.exe, &rig.update_dir);
    assert!(!suffixed(&rig.exe, BAD_SUFFIX).exists());
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");
}

#[test]
fn rollback_without_old_files_does_nothing() {
    let rig = Rig::new();
    assert!(!rollback_from_running(&rig.exe));
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");
    assert!(!suffixed(&rig.exe, BAD_SUFFIX).exists());
    assert!(!rollback(&rig.exe));
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");
}

#[test]
fn cleanup_removes_leftovers_and_update_folder() {
    let rig = Rig::new();
    for suffix in [OLD_SUFFIX, NEW_SUFFIX, BAD_SUFFIX] {
        fs::write(suffixed(&rig.exe, suffix), "x").unwrap();
    }
    fs::create_dir_all(rig.update_dir.join("0.1.30").join("exe")).unwrap();
    cleanup(&rig.exe, &rig.update_dir);
    assert_eq!(fs::read_dir(rig.exe.parent().unwrap()).unwrap().count(), 1);
    assert!(!rig.update_dir.exists());
    cleanup(&rig.exe, &rig.update_dir);
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");
}

#[test]
fn skipped_version_roundtrip_and_garbage() {
    let rig = Rig::new();
    let path = rig.root.path().join("cfg").join("update-skip.txt");
    assert!(read_skipped(&path).is_none(), "файла нет");
    write_skipped(&path, AppVersion::parse("0.1.58").unwrap()).unwrap();
    assert_eq!(
        read_skipped(&path),
        Some(AppVersion::parse("0.1.58").unwrap())
    );
    fs::write(&path, "сломано").unwrap();
    assert!(read_skipped(&path).is_none(), "мусор");

    // Пересобранная версия когда-то должна снова предлагаться, а не никогда.
    write_skipped(&path, AppVersion::parse("0.1.58").unwrap()).unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(8 * 24 * 60 * 60);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old)
        .unwrap();
    assert!(read_skipped(&path).is_none(), "старая отметка не вечная");
}

#[test]
fn writable_check() {
    let rig = Rig::new();
    assert!(bmm_app_release::check_writable(&rig.exe).is_ok());
    assert_eq!(
        bmm_app_release::check_writable(&rig.exe.with_extension("нет")),
        Err(InstallError::CurrentMissing)
    );
}

// ---------------------------------------------------------------------------
// Проверка версии и подготовка к перезапуску
// ---------------------------------------------------------------------------

#[test]
fn check_reports_available_up_to_date_and_skipped() {
    let zip = release_zip(true, true, EXE_NAME);
    let json = manifest_json("0.1.30", &zip, ZIP_URL, None);
    let manifest = UpdateManifest::parse(&json).unwrap();

    let older = AppVersion::parse("0.1.29").unwrap();
    assert_eq!(
        evaluate(&older, manifest.clone(), None).stage,
        UpdateStage::Available
    );
    let same = AppVersion::parse("0.1.30").unwrap();
    assert_eq!(
        evaluate(&same, manifest.clone(), None).stage,
        UpdateStage::UpToDate
    );
    let newer = AppVersion::parse("0.2.0").unwrap();
    assert_eq!(
        evaluate(&newer, manifest.clone(), None).stage,
        UpdateStage::UpToDate
    );

    let skipped = AppVersion::parse("0.1.30").unwrap();
    let state = evaluate(&older, manifest.clone(), Some(&skipped));
    assert_eq!(state.stage, UpdateStage::UpToDate);
    assert!(
        state
            .error
            .as_deref()
            .is_some_and(|text| text.contains("0.1.30")),
        "{:?}",
        state.error
    );
    let other_skipped = AppVersion::parse("0.1.29").unwrap();
    assert_eq!(
        evaluate(&older, manifest.clone(), Some(&other_skipped)).stage,
        UpdateStage::Available
    );
}

#[test]
fn check_accepts_json_and_bytes_without_network() {
    let zip = release_zip(true, true, EXE_NAME);
    let json = manifest_json("0.1.30", &zip, ZIP_URL, None);
    let local = AppVersion::parse("0.1.29").unwrap();

    let state = check_json(&local, &json, None);
    assert_eq!(state.stage, UpdateStage::Available);
    assert!(state.manifest.is_some());
    assert!(state.checked_at.is_some());

    let state = check_bytes(&local, json.as_bytes(), None);
    assert_eq!(state.stage, UpdateStage::Available);

    let state = check(&local, ManifestInput::Json("{ сломано"), None, None);
    assert_eq!(state.stage, UpdateStage::Failed);
    assert!(state.error.is_some());
    assert!(state.manifest.is_none());
}

#[test]
fn prepare_stages_without_touching_current_and_applies_later() {
    let rig = Rig::new();
    let zip = release_zip(true, true, EXE_NAME);
    let archive = rig.update_dir.join("0.1.30").join("update.zip");
    fs::create_dir_all(archive.parent().unwrap()).unwrap();
    fs::write(&archive, &zip).unwrap();
    let manifest = UpdateManifest::parse(&manifest_json("0.1.30", &zip, ZIP_URL, None)).unwrap();
    let local = AppVersion::parse("0.1.29").unwrap();
    let expected = AppVersion::parse("0.1.30").unwrap();
    let options = PrepareOptions {
        allow_temporary_location: true,
        ..PrepareOptions::default()
    };

    let prepared = prepare_update(
        &archive,
        &rig.update_dir,
        &manifest,
        &local,
        &rig.exe,
        &|_| Some(expected),
        &options,
    )
    .unwrap();

    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");
    assert_eq!(
        fs::read_dir(rig.exe.parent().unwrap()).unwrap().count(),
        1,
        "рядом с программой ни *.new, ни *.old"
    );
    assert!(prepared.new_exe.starts_with(&rig.update_dir));
    assert!(fs::read(&prepared.new_exe).unwrap().starts_with(b"MZ"));

    prepared.apply().unwrap();
    assert!(fs::read(&rig.exe).unwrap().starts_with(b"MZ"));
    assert_eq!(
        fs::read_to_string(&prepared.backup).unwrap(),
        "старая версия"
    );
    assert!(prepared.restore(), "прежняя вернулась");
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");
}

#[test]
fn failed_prepare_removes_unpacked_exe_and_keeps_archive() {
    let rig = Rig::new();
    let zip = release_zip(true, true, EXE_NAME);
    let archive = rig.update_dir.join("0.1.30").join("update.zip");
    fs::create_dir_all(archive.parent().unwrap()).unwrap();
    fs::write(&archive, &zip).unwrap();
    let manifest = UpdateManifest::parse(&manifest_json("0.1.30", &zip, ZIP_URL, None)).unwrap();
    let local = AppVersion::parse("0.1.29").unwrap();
    let wrong = AppVersion::parse("0.1.28").unwrap();
    let options = PrepareOptions {
        allow_temporary_location: true,
        ..PrepareOptions::default()
    };

    let error = prepare_update(
        &archive,
        &rig.update_dir,
        &manifest,
        &local,
        &rig.exe,
        &|_| Some(wrong),
        &options,
    )
    .unwrap_err();

    assert!(error.to_string().contains("версия"), "{error}");
    assert!(
        !rig.update_dir.join("0.1.30").join("exe").exists(),
        "распакованный exe убран"
    );
    assert!(archive.is_file(), "сам архив остался");
}

#[test]
fn prepare_forbids_temporary_location_by_default() {
    let rig = Rig::new();
    let zip = release_zip(true, true, EXE_NAME);
    let archive = rig.update_dir.join("0.1.30").join("update.zip");
    fs::create_dir_all(archive.parent().unwrap()).unwrap();
    fs::write(&archive, &zip).unwrap();
    let manifest = UpdateManifest::parse(&manifest_json("0.1.30", &zip, ZIP_URL, None)).unwrap();
    let local = AppVersion::parse("0.1.29").unwrap();
    let expected = AppVersion::parse("0.1.30").unwrap();

    let error = prepare_update(
        &archive,
        &rig.update_dir,
        &manifest,
        &local,
        &rig.exe,
        &|_| Some(expected),
        &PrepareOptions::default(),
    )
    .unwrap_err();
    assert_eq!(error, InstallError::TemporaryLocation);
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");

    let allowed = PrepareOptions {
        allow_temporary_location: true,
        ..PrepareOptions::default()
    };
    assert!(
        prepare_update(
            &archive,
            &rig.update_dir,
            &manifest,
            &local,
            &rig.exe,
            &|_| Some(expected),
            &allowed,
        )
        .is_ok()
    );
}

#[test]
fn prepare_rejects_non_newer_and_wrong_archive_version() {
    let rig = Rig::new();
    let zip = release_zip(true, true, EXE_NAME);
    let archive = rig.update_dir.join("0.1.30").join("update.zip");
    fs::create_dir_all(archive.parent().unwrap()).unwrap();
    fs::write(&archive, &zip).unwrap();
    let manifest = UpdateManifest::parse(&manifest_json("0.1.30", &zip, ZIP_URL, None)).unwrap();
    let local = AppVersion::parse("0.1.29").unwrap();
    let same = AppVersion::parse("0.1.30").unwrap();
    let options = PrepareOptions {
        allow_temporary_location: true,
        ..PrepareOptions::default()
    };

    let error = prepare_update(
        &archive,
        &rig.update_dir,
        &manifest,
        &same,
        &rig.exe,
        &|_| Some(same),
        &options,
    )
    .unwrap_err();
    assert_eq!(
        error,
        InstallError::NotNewer {
            candidate: "0.1.30".to_string(),
            current: "0.1.30".to_string()
        }
    );

    let wrong = AppVersion::parse("0.1.28").unwrap();
    let error = prepare_update(
        &archive,
        &rig.update_dir,
        &manifest,
        &local,
        &rig.exe,
        &|_| Some(wrong),
        &options,
    )
    .unwrap_err();
    assert!(error.to_string().contains("версия"), "{error}");
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");

    // Без ресурса версии (как в текущем приложении) обновление не блокируется:
    // архив уже сверен по размеру и SHA-256.
    assert!(
        prepare_update(
            &archive,
            &rig.update_dir,
            &manifest,
            &local,
            &rig.exe,
            &|_| None,
            &options,
        )
        .is_ok(),
        "неизвестная версия exe не должна ломать самообновление"
    );
}

// ---------------------------------------------------------------------------
// Выпуски из папки (проверка на сборочной машине)
// ---------------------------------------------------------------------------

#[test]
fn local_feed_serves_last_segment_and_hides_everything_else() {
    let root = tempfile::tempdir().unwrap();
    let feed = root.path().join("feed");
    fs::create_dir_all(&feed).unwrap();
    fs::write(feed.join("latest.json"), "{}").unwrap();
    fs::write(root.path().join("secret.txt"), "нельзя").unwrap();

    let transport = LocalFeedTransport::new(feed.clone());
    let response = transport.send(&url(MANIFEST_URL)).unwrap();
    assert_eq!(response.status, 200);
    let mut body = Vec::new();
    response.body.take(64).read_to_end(&mut body).unwrap();
    assert_eq!(body, b"{}");

    assert_eq!(transport.send(&url(ZIP_URL)).unwrap().status, 404);
    fs::write(feed.join("обновление.zip"), b"zip").unwrap();
    assert_eq!(
        transport
            .send(&url(
                "https://github.com/o/r/%D0%BE%D0%B1%D0%BD%D0%BE%D0%B2%D0%BB%D0%B5%D0%BD%D0%B8%D0%B5.zip"
            ))
            .unwrap()
            .status,
        200,
        "Unicode-имя из адреса раскодируется"
    );
    assert_eq!(
        transport
            .send(&url("https://github.com/o/r/..%2Fsecret.txt"))
            .unwrap()
            .status,
        404
    );
    assert_eq!(
        transport
            .send(&url("https://github.com/o/r/%2e%2e"))
            .unwrap()
            .status,
        404
    );
    assert_eq!(
        transport.send(&url("https://github.com/")).unwrap().status,
        404
    );
    assert_eq!(transport.dir(), feed.as_path());
}

#[test]
fn full_cycle_through_local_feed() {
    let rig = Rig::new();
    let feed = rig.root.path().join("feed");
    fs::create_dir_all(&feed).unwrap();
    let zip = release_zip(true, true, EXE_NAME);
    fs::write(feed.join("ModManagerBeamNG.zip"), &zip).unwrap();
    fs::write(
        feed.join("latest.json"),
        manifest_json("0.1.30", &zip, ZIP_URL, None),
    )
    .unwrap();

    let downloader = UpdateDownloader::new(Arc::new(LocalFeedTransport::new(feed)));
    let local = AppVersion::parse("0.1.29").unwrap();
    let state = check_url(&local, &url(MANIFEST_URL), &downloader, None);
    assert_eq!(state.stage, UpdateStage::Available);
    let manifest = state.manifest.unwrap();

    let downloaded = downloader
        .download_into(&manifest, &rig.update_dir, &mut |_| {}, &CancelToken::new())
        .unwrap();
    let expected = AppVersion::parse("0.1.30").unwrap();
    let options = PrepareOptions {
        allow_temporary_location: true,
        ..PrepareOptions::default()
    };
    let prepared = prepare_update(
        &downloaded.path,
        &rig.update_dir,
        &manifest,
        &local,
        &rig.exe,
        &|_| Some(expected),
        &options,
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&rig.exe).unwrap(), "старая версия");

    prepared.apply().unwrap();
    assert!(fs::read(&rig.exe).unwrap().starts_with(b"MZ"));
    assert_eq!(
        fs::read_to_string(&prepared.backup).unwrap(),
        "старая версия"
    );
}

// ---------------------------------------------------------------------------
// Фикстуры и помощники
// ---------------------------------------------------------------------------

struct Rig {
    root: TempDir,
    exe: PathBuf,
    update_dir: PathBuf,
}

impl Rig {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("app");
        fs::create_dir_all(&app).unwrap();
        let exe = app.join(EXE_NAME);
        fs::write(&exe, "старая версия").unwrap();
        let update_dir = root.path().join("update");
        Self {
            root,
            exe,
            update_dir,
        }
    }

    fn write_new(&self, contents: &str) -> PathBuf {
        let path = self.root.path().join("new.exe");
        fs::write(&path, contents).unwrap();
        path
    }
}

fn release_exe() -> &'static [u8] {
    static EXE: OnceLock<Vec<u8>> = OnceLock::new();
    EXE.get_or_init(|| {
        let mut exe = vec![0u8; 1_200_000];
        let mut state = 0x1234_5678u32;
        for byte in exe.iter_mut() {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *byte = (state >> 24) as u8;
        }
        exe[0] = b'M';
        exe[1] = b'Z';
        exe
    })
}

fn release_zip(with_exe: bool, mz: bool, exe_name: &str) -> Vec<u8> {
    let mut exe = release_exe().to_vec();
    if !mz {
        exe[0] = b'X';
    }
    let mut entries: Vec<(&str, &[u8])> = Vec::new();
    if with_exe {
        entries.push((exe_name, &exe));
    }
    entries.push(("Как запустить.txt", b"x"));
    zip_entries(&entries)
}

fn zip_entries(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    cursor.into_inner()
}

fn manifest_json(version: &str, zip: &[u8], url: &str, sha: Option<&str>) -> String {
    let sha = sha.map(str::to_string).unwrap_or_else(|| sha_hex(zip));
    format!(
        "{{\"version\":\"{version}\",\"url\":\"{url}\",\"size\":{},\"sha256\":\"{sha}\",\"notes\":\"Что нового\",\"published\":\"2026-10-15T12:00:00Z\"}}",
        zip.len()
    )
}

fn sha_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}
