use langualinker_contracts::{AppInfoDto, API_VERSION};
use tauri::Emitter;

#[tauri::command]
fn get_app_info() -> AppInfoDto {
    AppInfoDto {
        api_version: API_VERSION.to_owned(),
        app_name: "LanguaLinker".to_owned(),
        mode: "development".to_owned(),
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            app.emit("app-ready", "ready")?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_app_info])
        .run(tauri::generate_context!())
        .expect("error while running LanguaLinker");
}