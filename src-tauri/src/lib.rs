mod commands;
mod crawler;
mod db;
mod diagnostics;
mod email_csv;
mod export;
mod models;
mod state;
mod website;

use tauri::Manager;

use diagnostics::DiagnosticLog;
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
            let diagnostics = DiagnosticLog::new(&data_dir)?;
            diagnostics.write(
                "app",
                format!(
                    "迎风数据启动；version={} platform={} arch={} data_dir={}",
                    app.package_info().version,
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                    data_dir.display()
                ),
            );
            let connection = db::open(&data_dir.join("businesses.sqlite3")).map_err(|error| {
                diagnostics.write("database.error", &error);
                error
            })?;
            connection.execute(
                "UPDATE search_jobs SET status = 'interrupted', message = '应用上次退出时任务仍在运行，可点击继续', updated_at = ?1
                 WHERE status IN ('queued', 'running', 'blocked')",
                [chrono::Utc::now().to_rfc3339()],
            )?;
            diagnostics.write("app", "数据库初始化完成");
            app.manage(AppState::new(connection, diagnostics));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_or_create_device_id,
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
            commands::get_diagnostic_log_path,
            commands::write_diagnostic_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
