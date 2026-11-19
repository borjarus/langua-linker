//! Daily study reminders on the platform (plan Phase 15, ADR-20).
//!
//! `application::reminders` decides *when* (rolling window, no material → no reminder); this
//! module asks the OS for permission and keeps the scheduled one-shot notifications in line with
//! that plan through `tauri-plugin-notification`. Scheduling exists on Android and iOS only; on
//! desktop the plugin cannot schedule, so the permission reads `unsupported` (D-01 keeps desktop
//! reminders out of scope).
//!
//! Every call into the mobile plugin blocks until the native side answers, so callers run it on
//! a worker thread, never on the main (UI) thread.

use langualinker_application::repositories::UnitOfWork;
use langualinker_contracts::dto::PermissionState;
use tauri::{AppHandle, Runtime};

use crate::backend::Backend;

/// Notification ids owned by the reminders: `BASE..BASE + SPAN`.
pub const REMINDER_ID_BASE: i32 = 15_000;
const REMINDER_ID_SPAN: i32 = 100;
#[cfg_attr(not(mobile), allow(dead_code))]
/// Android channel of the reminders (users can mute it separately in the system settings).
pub const CHANNEL_ID: &str = "study-reminders";
#[cfg_attr(not(mobile), allow(dead_code))]
/// Where a tap on a reminder leads (validated again by the WebView's deep-link adapter).
pub const REMINDER_URL: &str = "langualinker://app/home";

/// Current notification permission as the settings screen shows it.
pub fn permission<R: Runtime>(app: &AppHandle<R>) -> PermissionState {
    platform::permission(app)
}

/// Shows the OS prompt when it may still be shown; returns the resulting state.
pub fn request_permission<R: Runtime>(app: &AppHandle<R>) -> PermissionState {
    platform::request_permission(app)
}

/// Replaces the scheduled reminders with the current plan. Errors are logged, never shown: a
/// missed reminder must not break the screen that triggered the sync.
pub fn sync<R: Runtime>(app: &AppHandle<R>, backend: &Backend) {
    let Ok(collection) = backend.collection() else {
        return;
    };
    let now = collection.clock.now();
    let plan = collection
        .db
        .read(move |tx| langualinker_application::reminders::plan(tx, now));
    match plan {
        Ok(plan) => platform::apply(app, &plan),
        Err(error) => tracing::warn!(code = error.code(), "reminder plan not computed"),
    }
}

/// [`sync`] on a worker thread.
pub fn sync_in_background<R: Runtime>(app: &AppHandle<R>) {
    use tauri::Manager;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(backend) = app.try_state::<Backend>() {
            sync(&app, &backend);
        }
    });
}

fn owned(id: i32) -> bool {
    (REMINDER_ID_BASE..REMINDER_ID_BASE + REMINDER_ID_SPAN).contains(&id)
}

#[cfg(mobile)]
mod platform {
    use std::sync::Mutex;

    use langualinker_application::reminders::{self, ReminderPlan};
    #[cfg(target_os = "android")]
    use langualinker_application::settings::UiLanguage;
    use langualinker_contracts::dto::PermissionState;
    use tauri::plugin::PermissionState as OsPermission;
    use tauri::{AppHandle, Manager, Runtime};
    // Channels exist only on Android; the iOS plugin does not export them.
    #[cfg(target_os = "android")]
    use tauri_plugin_notification::{Channel, Importance};
    use tauri_plugin_notification::{Notification, Schedule};

    use super::{CHANNEL_ID, REMINDER_ID_BASE, REMINDER_URL, owned};

    /// One sync at a time: two overlapping ones could cancel each other's notifications.
    static SYNC: Mutex<()> = Mutex::new(());

    fn plugin<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::State<'_, Notification<R>>> {
        app.try_state::<Notification<R>>()
    }

    fn map(state: OsPermission) -> PermissionState {
        match state {
            OsPermission::Granted => PermissionState::Granted,
            OsPermission::Denied => PermissionState::Denied,
            OsPermission::Prompt | OsPermission::PromptWithRationale => PermissionState::Prompt,
        }
    }

    pub fn permission<R: Runtime>(app: &AppHandle<R>) -> PermissionState {
        plugin(app)
            .and_then(|plugin| plugin.permission_state().ok())
            .map_or(PermissionState::Unsupported, map)
    }

    pub fn request_permission<R: Runtime>(app: &AppHandle<R>) -> PermissionState {
        let Some(plugin) = plugin(app) else {
            return PermissionState::Unsupported;
        };
        match plugin.permission_state() {
            Ok(OsPermission::Granted) => return PermissionState::Granted,
            Ok(OsPermission::Denied) => return PermissionState::Denied,
            _ => {}
        }
        match plugin.request_permission() {
            Ok(state) => map(state),
            Err(error) => {
                tracing::warn!(%error, "notification permission request failed");
                PermissionState::Prompt
            }
        }
    }

    pub fn apply<R: Runtime>(app: &AppHandle<R>, plan: &ReminderPlan) {
        let Some(plugin) = plugin(app) else {
            return;
        };
        let _guard = SYNC.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // Our earlier reminders go first, also when the permission was revoked meanwhile.
        match plugin.pending() {
            Ok(pending) => {
                let ids: Vec<i32> = pending
                    .iter()
                    .map(|pending| pending.id())
                    .filter(|id| owned(*id))
                    .collect();
                if !ids.is_empty()
                    && let Err(error) = plugin.cancel(ids)
                {
                    tracing::warn!(%error, "old reminders not cancelled");
                }
            }
            Err(error) => tracing::warn!(%error, "pending notifications not listed"),
        }
        if !matches!(plugin.permission_state(), Ok(OsPermission::Granted)) {
            return;
        }
        #[cfg(target_os = "android")]
        {
            let name = match plan.language {
                UiLanguage::Pl => "Przypomnienia o nauce",
                UiLanguage::En => "Study reminders",
            };
            let channel = Channel::builder(CHANNEL_ID, name)
                .importance(Importance::Default)
                .build();
            if let Err(error) = plugin.create_channel(channel) {
                tracing::warn!(%error, "reminder channel not created");
            }
        }
        let mut scheduled = 0;
        for (index, slot) in plan.slots.iter().enumerate() {
            let Ok(date) =
                time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(slot.at.0) * 1_000_000)
            else {
                continue;
            };
            let (title, body) = reminders::text(plan.language, slot);
            let result = plugin
                .builder()
                .id(REMINDER_ID_BASE + index as i32)
                .channel_id(CHANNEL_ID)
                .title(title)
                .body(body)
                .auto_cancel()
                .extra("url", REMINDER_URL)
                .schedule(Schedule::At {
                    date,
                    repeating: false,
                    // Inexact but delivered in Doze: no SCHEDULE_EXACT_ALARM needed (step 5).
                    allow_while_idle: true,
                })
                .show();
            match result {
                Ok(()) => scheduled += 1,
                Err(error) => tracing::warn!(%error, "reminder not scheduled"),
            }
        }
        tracing::info!(scheduled, "reminders synced");
    }
}

#[cfg(not(mobile))]
mod platform {
    use langualinker_application::reminders::ReminderPlan;
    use langualinker_contracts::dto::PermissionState;
    use tauri::{AppHandle, Runtime};

    pub fn permission<R: Runtime>(_app: &AppHandle<R>) -> PermissionState {
        PermissionState::Unsupported
    }

    pub fn request_permission<R: Runtime>(_app: &AppHandle<R>) -> PermissionState {
        PermissionState::Unsupported
    }

    pub fn apply<R: Runtime>(_app: &AppHandle<R>, plan: &ReminderPlan) {
        let _ = (plan, super::owned(0));
    }
}
