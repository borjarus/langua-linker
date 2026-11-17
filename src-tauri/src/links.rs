//! Navigation guard (ADR-21). The WebView only ever shows the app: a link to anything else
//! (licences, privacy policy) is cancelled, and an `https` one opens in the system browser.
//! A `mailto:` link opens the mail app only for the association report address (Play
//! AI-generated content policy); any other recipient is dropped.
//! Without it, Android loads the page inside the app's WebView in place of the app, and iOS
//! ignores `target="_blank"`.

use tauri::{Runtime, Url, plugin::TauriPlugin};
use tauri_plugin_opener::OpenerExt;

/// Recipient of association reports; the same constant as `REPORT_EMAIL` in
/// `frontend/src/services/report.ts` (checked by `frontend/tests/report.test.ts`).
pub const REPORT_EMAIL: &str = "borjarus@gmail.com";

/// What happens to a navigation request.
#[derive(Debug, PartialEq, Eq)]
pub enum Navigation {
    /// The app itself (bundled assets, IPC, the dev server).
    Allow,
    /// An external https page: open it outside the app.
    OpenExternally,
    /// Anything else (http, file, intent, javascript…): dropped.
    Block,
}

pub fn classify(url: &Url) -> Navigation {
    let local = url
        .host_str()
        .is_some_and(|host| host == "localhost" || host.ends_with(".localhost"));
    match url.scheme() {
        "tauri" | "asset" | "ipc" | "about" => Navigation::Allow,
        "http" | "https" if local => Navigation::Allow,
        "https" if url.host_str().is_some() => Navigation::OpenExternally,
        // Only the report address, and only one recipient (no `to=`/`cc=`/`bcc=` extras).
        "mailto"
            if url.path().eq_ignore_ascii_case(REPORT_EMAIL)
                && url
                    .query_pairs()
                    .all(|(key, _)| key == "subject" || key == "body") =>
        {
            Navigation::OpenExternally
        }
        _ => Navigation::Block,
    }
}

pub fn guard<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::new("links")
        .on_navigation(|webview, url| match classify(url) {
            Navigation::Allow => true,
            Navigation::OpenExternally => {
                // Not from this callback's thread: on Android it is the main thread inside
                // `shouldOverrideUrlLoading`, and the opener's Kotlin plugin call waits for that
                // same thread — the app froze for good on the first link (device run 2026-11-17).
                let webview = webview.clone();
                let url = url.to_string();
                std::thread::spawn(move || {
                    if let Err(error) = webview.opener().open_url(url, None::<&str>) {
                        tracing::warn!(target: "links", "external link not opened: {error}");
                    }
                });
                false
            }
            Navigation::Block => {
                tracing::warn!(target: "links", "navigation blocked (scheme {})", url.scheme());
                false
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(url: &str) -> Navigation {
        classify(&Url::parse(url).unwrap())
    }

    #[test]
    fn the_app_itself_stays_in_the_webview() {
        for url in [
            "tauri://localhost/index.html#/settings",
            "http://tauri.localhost/",
            "https://tauri.localhost/",
            "http://localhost:5173/",
            "ipc://localhost/get_dashboard",
            "http://ipc.localhost/get_dashboard",
            "asset://localhost/x",
            "about:blank",
        ] {
            assert_eq!(kind(url), Navigation::Allow, "{url}");
        }
    }

    #[test]
    fn https_pages_open_outside_the_app() {
        assert_eq!(
            kind("https://sqlite.org/copyright.html"),
            Navigation::OpenExternally
        );
        assert_eq!(
            kind("https://localhost.example.com/"),
            Navigation::OpenExternally
        );
    }

    #[test]
    fn association_reports_open_the_mail_app() {
        assert_eq!(
            kind(
                "mailto:borjarus@gmail.com?subject=LanguaLinker%3A%20report&body=Reason%3A%20wrong%0AFront%3A%20%C5%BC"
            ),
            Navigation::OpenExternally
        );
        assert_eq!(
            kind("mailto:borjarus@gmail.com"),
            Navigation::OpenExternally
        );
    }

    #[test]
    fn other_schemes_are_dropped() {
        for url in [
            "http://example.com/",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "intent://scan/#Intent;scheme=zxing;end",
            "mailto:someone@example.com",
            "mailto:borjarus@gmail.com,someone@example.com",
            "mailto:borjarus@gmail.com?cc=someone@example.com",
            "mailto:borjarus@gmail.com?bcc=someone@example.com&subject=x",
            "data:text/html,hi",
        ] {
            assert_eq!(kind(url), Navigation::Block, "{url}");
        }
    }
}
