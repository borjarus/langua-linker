//! Navigation guard (ADR-21). The WebView only ever shows the app: a link to anything else
//! (licences, privacy policy) is cancelled, and an `https` one opens in the system browser.
//! Without it, Android loads the page inside the app's WebView in place of the app, and iOS
//! ignores `target="_blank"`.

use tauri::{Runtime, Url, plugin::TauriPlugin};
use tauri_plugin_opener::OpenerExt;

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
        _ => Navigation::Block,
    }
}

pub fn guard<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::new("links")
        .on_navigation(|webview, url| match classify(url) {
            Navigation::Allow => true,
            Navigation::OpenExternally => {
                if let Err(error) = webview.opener().open_url(url.as_str(), None::<&str>) {
                    tracing::warn!(target: "links", "external link not opened: {error}");
                }
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
    fn other_schemes_are_dropped() {
        for url in [
            "http://example.com/",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "intent://scan/#Intent;scheme=zxing;end",
            "mailto:someone@example.com",
            "data:text/html,hi",
        ] {
            assert_eq!(kind(url), Navigation::Block, "{url}");
        }
    }
}
