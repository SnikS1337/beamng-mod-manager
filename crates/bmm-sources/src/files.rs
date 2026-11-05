//! Что лежит за кнопкой «Скачать» на странице мода — без скачивания файла.
//!
//! Обычный запрос проходит ссылку по шагам и читает только заголовки: у BeamNG это
//! сразу файл с именем и точным размером, у STMods и ModsGaming — страница modsfire.
//! Её закрывает Cloudflare, поэтому читает её встроенный браузер, и только когда
//! адрес файла сменился: новый файл у автора — новый адрес на modsfire.

use std::sync::atomic::AtomicBool;

use url::Url;

use bmm_domain::library::FileSnapshot;
use bmm_domain::models::{Listing, OfferKind};

use crate::net::{FILE_PAGE_HOSTS, PoliteFetcher, name_from_url};
use crate::parsers::modsfire;

/// Под этим именем идут страницы файлообменника в вежливом загрузчике: их заглушка
/// «вы человек?» не должна закрывать сам сайт мода.
pub const FILE_HOST_SOURCE: &str = "modsfire";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileProbe {
    /// Что узнали; `None` — ссылки нет или она не прошла.
    pub file: Option<FileSnapshot>,
    /// Открывали страницу файлообменника (встроенным браузером, это дорого).
    pub opened_page: bool,
}

/// Ссылка «Скачать» на странице: первый бесплатный вариант.
pub fn offer_url(listing: &Listing) -> Option<&Url> {
    listing
        .offers
        .iter()
        .find(|offer| matches!(offer.kind, OfferKind::Direct | OfferKind::ExternalHost))
        .map(|offer| &offer.url)
}

fn is_file_page(url: &Url) -> bool {
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };
    FILE_PAGE_HOSTS.iter().any(|known| {
        host == *known
            || host
                .strip_suffix(known)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

/// Адрес файла без подписи доступа: у хранилищ она новая при каждом запросе.
fn without_query(url: &Url) -> String {
    let mut url = url.clone();
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

/// Пройти ссылку `offer`. `previous` — прошлый снимок: если адрес файла не сменился,
/// страницу файлообменника второй раз не открываем. `open_page` — можно ли сейчас её
/// открыть (на одну проверку их немного: браузер медленный).
pub fn probe(
    fetcher: &PoliteFetcher,
    offer: &Url,
    previous: Option<&FileSnapshot>,
    open_page: bool,
    cancel: &AtomicBool,
) -> FileProbe {
    let Ok(peek) = fetcher.peek(offer, cancel) else {
        return FileProbe::default();
    };
    if !peek.page {
        let exact = peek.size.is_some();
        return FileProbe {
            file: Some(FileSnapshot {
                url: without_query(&peek.final_url),
                name: peek.name.or_else(|| name_from_url(&peek.final_url)),
                size: peek.size,
                exact,
                page_key: None,
            }),
            opened_page: false,
        };
    }
    let url = peek.final_url.to_string();
    if let Some(previous) = previous
        && previous.url == url
        && previous.name.is_some()
    {
        // Тот же адрес — тот же файл: страницу не открываем.
        return FileProbe {
            file: Some(FileSnapshot {
                page_key: None,
                ..previous.clone()
            }),
            opened_page: false,
        };
    }
    let bare = FileSnapshot {
        url: url.clone(),
        name: name_from_url(&peek.final_url),
        size: None,
        exact: false,
        page_key: None,
    };
    if !is_file_page(&peek.final_url) || !open_page {
        return FileProbe {
            file: Some(bare),
            opened_page: false,
        };
    }
    let result = fetcher.get_cancellable(FILE_HOST_SOURCE, &peek.final_url, cancel);
    let hosted = result
        .response
        .filter(|response| response.status == 200)
        .and_then(|response| modsfire::parse(&response.text));
    FileProbe {
        file: Some(match hosted {
            Some(hosted) => FileSnapshot {
                url,
                name: Some(hosted.name),
                size: hosted.size_bytes,
                exact: false,
                page_key: None,
            },
            None => bare,
        }),
        opened_page: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Arc, Mutex};

    use crate::net::{FetchError, PageFetcher, Peek, PoliteOptions, RawResponse};

    struct Fake {
        peek: Mutex<Result<Peek, ()>>,
        page: String,
        pages: Mutex<Vec<String>>,
    }

    impl PageFetcher for Fake {
        fn get(&self, url: &Url) -> Result<RawResponse, FetchError> {
            self.pages.lock().unwrap().push(url.to_string());
            Ok(RawResponse {
                status: 200,
                text: self.page.clone(),
                final_url: url.clone(),
                retry_after: None,
                etag: None,
                last_modified: None,
            })
        }

        fn peek(&self, _url: &Url, _cancel: &AtomicBool) -> Result<Peek, FetchError> {
            self.peek
                .lock()
                .unwrap()
                .clone()
                .map_err(|_| FetchError::Network)
        }
    }

    fn fetcher(peek: Result<Peek, ()>, page: &str) -> (Arc<Fake>, PoliteFetcher) {
        let fake = Arc::new(Fake {
            peek: Mutex::new(peek),
            page: page.to_string(),
            pages: Mutex::new(Vec::new()),
        });
        let options = PoliteOptions {
            pause: std::time::Duration::ZERO,
            jitter: std::time::Duration::ZERO,
            ..PoliteOptions::default()
        };
        (fake.clone(), PoliteFetcher::new(fake, options))
    }

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    fn modsfire_page() -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/modsfire/file-real.html"),
        )
        .unwrap()
    }

    static NO: AtomicBool = AtomicBool::new(false);

    #[test]
    fn direct_file_gives_exact_size_and_name_without_downloading() {
        // BeamNG: ссылка ведёт сразу на файл в хранилище; подпись доступа в адресе
        // меняется при каждом запросе — её не запоминаем.
        let (fake, polite) = fetcher(
            Ok(Peek {
                final_url: url(
                    "https://store.example/mods/X/73505/acng-freeroam.zip?X-Amz-Signature=abc",
                ),
                page: false,
                name: None,
                size: Some(102_644),
            }),
            "",
        );
        let probe = probe(
            &polite,
            &url("https://www.beamng.com/resources/x.1/download?version=73505"),
            None,
            true,
            &NO,
        );
        let file = probe.file.unwrap();
        assert_eq!(file.name.as_deref(), Some("acng-freeroam.zip"));
        assert_eq!(file.size, Some(102_644));
        assert!(file.exact);
        assert_eq!(
            file.url,
            "https://store.example/mods/X/73505/acng-freeroam.zip"
        );
        assert!(!probe.opened_page);
        assert!(
            fake.pages.lock().unwrap().is_empty(),
            "страниц не открывали"
        );
    }

    #[test]
    fn new_file_host_address_opens_its_page_once() {
        let address = "https://modsfire.com/v62ZYDuU3xMxZzA";
        let (fake, polite) = fetcher(
            Ok(Peek {
                final_url: url(address),
                page: true,
                name: None,
                size: None,
            }),
            &modsfire_page(),
        );
        let first = probe(
            &polite,
            &url("https://stmods.org/download_file.php?update_id=1"),
            None,
            true,
            &NO,
        );
        assert!(first.opened_page);
        let file = first.file.unwrap();
        assert_eq!(file.name.as_deref(), Some("E34_matissator_UNZIP.7z"));
        assert!(!file.exact, "сайт пишет размер округлённым");
        assert_eq!(file.url, address);

        // Адрес тот же — страницу второй раз не открываем.
        let again = probe(
            &polite,
            &url("https://stmods.org/download_file.php?update_id=1"),
            Some(&FileSnapshot {
                page_key: Some("3.3".to_string()),
                ..file.clone()
            }),
            true,
            &NO,
        );
        assert!(!again.opened_page);
        assert_eq!(again.file.as_ref().unwrap().name, file.name);
        assert_eq!(
            again.file.unwrap().page_key,
            None,
            "подпись ставит проверка"
        );
        assert_eq!(fake.pages.lock().unwrap().len(), 1);
    }

    #[test]
    fn changed_file_host_address_is_read_again() {
        let (fake, polite) = fetcher(
            Ok(Peek {
                final_url: url("https://modsfire.com/NEWFILE"),
                page: true,
                name: None,
                size: None,
            }),
            &modsfire_page(),
        );
        let previous = FileSnapshot {
            url: "https://modsfire.com/OLDFILE".to_string(),
            name: Some("old.zip".to_string()),
            size: Some(1),
            exact: false,
            page_key: Some("1".to_string()),
        };
        let probe = probe(
            &polite,
            &url("https://stmods.org/d"),
            Some(&previous),
            true,
            &NO,
        );
        assert!(probe.opened_page);
        assert_eq!(
            probe.file.unwrap().name.as_deref(),
            Some("E34_matissator_UNZIP.7z")
        );
        assert_eq!(
            fake.pages.lock().unwrap().as_slice(),
            ["https://modsfire.com/NEWFILE"]
        );
    }

    #[test]
    fn page_is_not_opened_when_the_budget_is_spent() {
        let (fake, polite) = fetcher(
            Ok(Peek {
                final_url: url("https://modsfire.com/NEWFILE"),
                page: true,
                name: None,
                size: None,
            }),
            &modsfire_page(),
        );
        let probe = probe(&polite, &url("https://stmods.org/d"), None, false, &NO);
        assert!(!probe.opened_page);
        let file = probe.file.unwrap();
        assert_eq!(file.url, "https://modsfire.com/NEWFILE");
        assert_eq!(file.name, None, "имя узнаем в следующий раз");
        assert!(fake.pages.lock().unwrap().is_empty());
    }

    #[test]
    fn challenge_instead_of_the_file_page_leaves_the_name_unknown() {
        let (_, polite) = fetcher(
            Ok(Peek {
                final_url: url("https://modsfire.com/NEWFILE"),
                page: true,
                name: None,
                size: None,
            }),
            "<title>Just a moment...</title>",
        );
        let probe = probe(&polite, &url("https://stmods.org/d"), None, true, &NO);
        assert!(probe.opened_page);
        assert_eq!(probe.file.unwrap().name, None);
    }

    #[test]
    fn broken_link_says_nothing() {
        let (_, polite) = fetcher(Err(()), "");
        assert_eq!(
            probe(&polite, &url("https://stmods.org/d"), None, true, &NO),
            FileProbe::default()
        );
    }

    #[test]
    fn other_file_host_pages_are_not_opened() {
        // mediafire, Google Диск: имя берём из адреса, если оно там есть.
        let (fake, polite) = fetcher(
            Ok(Peek {
                final_url: url("https://www.mediafire.com/file/abc123/car_v2.zip/file"),
                page: true,
                name: None,
                size: None,
            }),
            "",
        );
        let probe = probe(&polite, &url("https://modland.net/x"), None, true, &NO);
        assert!(!probe.opened_page);
        assert!(fake.pages.lock().unwrap().is_empty());
        assert_eq!(
            probe.file.unwrap().url,
            "https://www.mediafire.com/file/abc123/car_v2.zip/file"
        );
    }

    #[test]
    fn paid_links_are_not_followed() {
        let mut listing = Listing::new("stmods", url("https://stmods.org/m/x/"), "x", "X");
        listing.offers = vec![
            bmm_domain::models::Offer {
                kind: OfferKind::PaidLink,
                host: "patreon.com".to_string(),
                url: url("https://patreon.com/x"),
                host_file_id: None,
                size_bytes: None,
            },
            bmm_domain::models::Offer {
                kind: OfferKind::ExternalHost,
                host: "modsfire".to_string(),
                url: url("https://modsfire.com/abc"),
                host_file_id: None,
                size_bytes: None,
            },
        ];
        assert_eq!(
            offer_url(&listing).unwrap().as_str(),
            "https://modsfire.com/abc"
        );
        listing.offers.clear();
        assert!(offer_url(&listing).is_none());
    }
}

/// Живая проверка на настоящих сайтах: `cargo test -p bmm-sources live_download_links -- --ignored`.
#[cfg(test)]
mod live {
    use std::sync::atomic::AtomicBool;

    use url::Url;

    use crate::net::{HttpPageFetcher, PageFetcher};

    #[test]
    #[ignore = "ходит на настоящие сайты"]
    fn live_download_links() {
        let client = HttpPageFetcher::new().unwrap();
        let cancel = AtomicBool::new(false);
        for link in [
            "https://www.beamng.com/resources/acng-tire-heat-pressure-wear.39760/download?version=73505",
            "https://stmods.org/download_file.php?update_id=170744&path=link1",
            "https://modsgaming.ru/load/0-0-1-160435-20",
        ] {
            let peek = client.peek(&Url::parse(link).unwrap(), &cancel);
            println!("{link}\n  -> {peek:?}");
            let peek = peek.expect("ссылка проходит");
            if link.contains("beamng") {
                assert!(!peek.page);
                assert_eq!(peek.name.as_deref(), Some("acng-freeroam.zip"));
                assert!(peek.size.is_some_and(|size| size > 0));
            } else {
                assert!(peek.page);
                assert_eq!(peek.final_url.host_str(), Some("modsfire.com"));
            }
        }
    }
}
