mod commands;
mod crawler;
mod db;
mod email_csv;
mod export;
mod models;
mod state;
mod website;

use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
            let connection = db::open(&data_dir.join("businesses.sqlite3"))?;
            connection.execute(
                "UPDATE search_jobs SET status = 'interrupted', message = '应用上次退出时任务仍在运行，可点击继续', updated_at = ?1
                 WHERE status IN ('queued', 'running', 'blocked')",
                [chrono::Utc::now().to_rfc3339()],
            )?;
            app.manage(AppState::new(connection));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::list_businesses,
            commands::update_business_emails,
            commands::delete_business,
            commands::start_search,
            commands::resume_search,
            commands::control_search,
            commands::delete_search,
            commands::export_csv,
            commands::scrape_emails_csv,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
