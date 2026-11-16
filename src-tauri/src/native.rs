//! Platform adapters of Phase 15 that are not part of `AppApi` (ADR-20): text to speech and the
//! deep link a reminder opened the app with. On Android they run in the Kotlin `NativePlugin`
//! (`gen/android/.../NativePlugin.kt`); elsewhere the WebView's own Web Speech API is used and
//! there is no launch URL, so the commands answer "no engine" / `null`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{Runtime, plugin::TauriPlugin};

use crate::commands::parse_input;
use langualinker_contracts::{ErrorCode, ErrorDto};

/// Longest text sent to the engine (a card side, never a document).
const MAX_SPEECH_CHARS: usize = 1_000;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechStatus {
    /// A native engine exists; `false` means "use the WebView engine if it has one".
    pub engine: bool,
    /// The engine can speak this language.
    pub voice: bool,
    /// A voice for the language is installed on the device (works offline).
    pub offline: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StatusInput {
    lang: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpeakInput {
    text: String,
    lang: String,
    #[serde(default = "normal_rate")]
    rate: f64,
}

fn normal_rate() -> f64 {
    1.0
}

#[derive(Debug, Default, Deserialize)]
struct LaunchUrl {
    url: Option<String>,
}

fn language_tag(lang: &str) -> Result<(), ErrorDto> {
    let valid = (2..=16).contains(&lang.len())
        && lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if valid {
        Ok(())
    } else {
        Err(ErrorDto::new(ErrorCode::ValidationFailed).with_detail("field", "lang"))
    }
}

/// Registers the Kotlin plugin on Android; a no-op plugin elsewhere.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::new("native")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                let handle =
                    _api.register_android_plugin("com.mila.langualinker", "NativePlugin")?;
                _app.manage(platform::Handle(handle));
            }
            Ok(())
        })
        .build()
}

#[tauri::command]
pub async fn speech_status<R: Runtime>(
    app: tauri::AppHandle<R>,
    input: Value,
) -> Result<SpeechStatus, ErrorDto> {
    let input: StatusInput = parse_input(input)?;
    language_tag(&input.lang)?;
    worker(move || platform::speech_status(&app, &input.lang)).await
}

#[tauri::command]
pub async fn speak<R: Runtime>(app: tauri::AppHandle<R>, input: Value) -> Result<(), ErrorDto> {
    let mut input: SpeakInput = parse_input(input)?;
    language_tag(&input.lang)?;
    if !(0.5..=1.5).contains(&input.rate) {
        return Err(ErrorDto::new(ErrorCode::ValidationFailed).with_detail("field", "rate"));
    }
    input.text = input.text.chars().take(MAX_SPEECH_CHARS).collect();
    worker(move || platform::speak(&app, &input)).await
}

#[tauri::command]
pub async fn stop_speech<R: Runtime>(app: tauri::AppHandle<R>) -> Result<(), ErrorDto> {
    worker(move || platform::stop(&app)).await
}

/// The `langualinker://` URL of the reminder that opened or resumed the app, at most once.
#[tauri::command]
pub async fn take_launch_url<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Option<String>, ErrorDto> {
    worker(move || platform::take_launch_url(&app)).await
}

/// Mobile plugin calls block until the native side answers: never on the main thread.
async fn worker<T: Send + 'static>(
    run: impl FnOnce() -> Result<T, ErrorDto> + Send + 'static,
) -> Result<T, ErrorDto> {
    tauri::async_runtime::spawn_blocking(run)
        .await
        .map_err(|_| ErrorDto::new(ErrorCode::Internal))?
}

#[cfg(target_os = "android")]
mod platform {
    use langualinker_contracts::{ErrorCode, ErrorDto};
    use tauri::plugin::PluginHandle;
    use tauri::{AppHandle, Manager, Runtime};

    use super::{LaunchUrl, SpeakInput, SpeechStatus};

    pub struct Handle<R: Runtime>(pub PluginHandle<R>);

    fn call<R: Runtime, T: serde::de::DeserializeOwned>(
        app: &AppHandle<R>,
        command: &str,
        payload: impl serde::Serialize,
    ) -> Result<T, ErrorDto> {
        let handle = app
            .try_state::<Handle<R>>()
            .ok_or_else(|| ErrorDto::new(ErrorCode::NotImplemented))?;
        handle
            .0
            .run_mobile_plugin(command, payload)
            .map_err(|error| {
                tracing::warn!(%error, command, "native plugin call failed");
                ErrorDto::new(ErrorCode::NotImplemented)
            })
    }

    pub fn speech_status<R: Runtime>(
        app: &AppHandle<R>,
        lang: &str,
    ) -> Result<SpeechStatus, ErrorDto> {
        Ok(call(app, "speechStatus", serde_json::json!({ "lang": lang })).unwrap_or_default())
    }

    pub fn speak<R: Runtime>(app: &AppHandle<R>, input: &SpeakInput) -> Result<(), ErrorDto> {
        call::<R, serde_json::Value>(app, "speak", input).map(|_| ())
    }

    pub fn stop<R: Runtime>(app: &AppHandle<R>) -> Result<(), ErrorDto> {
        call::<R, serde_json::Value>(app, "stopSpeech", serde_json::json!({})).map(|_| ())
    }

    pub fn take_launch_url<R: Runtime>(app: &AppHandle<R>) -> Result<Option<String>, ErrorDto> {
        let launch: LaunchUrl =
            call(app, "takeLaunchUrl", serde_json::json!({})).unwrap_or_default();
        Ok(launch.url)
    }
}

#[cfg(not(target_os = "android"))]
mod platform {
    use langualinker_contracts::ErrorDto;
    use tauri::{AppHandle, Runtime};

    use super::{SpeakInput, SpeechStatus};

    pub fn speech_status<R: Runtime>(
        _app: &AppHandle<R>,
        _lang: &str,
    ) -> Result<SpeechStatus, ErrorDto> {
        Ok(SpeechStatus::default())
    }

    pub fn speak<R: Runtime>(_app: &AppHandle<R>, input: &SpeakInput) -> Result<(), ErrorDto> {
        let _ = (&input.text, &input.lang, input.rate);
        Err(ErrorDto::new(
            langualinker_contracts::ErrorCode::NotImplemented,
        ))
    }

    pub fn stop<R: Runtime>(_app: &AppHandle<R>) -> Result<(), ErrorDto> {
        Ok(())
    }

    pub fn take_launch_url<R: Runtime>(_app: &AppHandle<R>) -> Result<Option<String>, ErrorDto> {
        let _ = super::LaunchUrl::default().url;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_tags_are_short_bcp47_like_codes() {
        assert!(language_tag("de-DE").is_ok());
        assert!(language_tag("en").is_ok());
        assert!(language_tag("").is_err());
        assert!(language_tag("de;DROP").is_err());
        assert!(language_tag("x".repeat(17).as_str()).is_err());
    }
}
