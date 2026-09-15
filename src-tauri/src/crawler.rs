use std::{path::PathBuf, process::Stdio};

use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
    sync::watch,
};

use crate::{
    db,
    models::{Business, CrawlerEvent, SearchRequest, WebsiteResult},
    state::AppState,
    website,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub async fn run_search(
    app: AppHandle,
    state: AppState,
    job_id: String,
    request: SearchRequest,
    mut control: watch::Receiver<String>,
) {
    let _browser_guard = state.browser_lock.lock().await;
    if let Err(error) = set_job_status(&app, &state, &job_id, "running", "正在启动浏览器") {
        let _ = app.emit("crawler-error", error);
        return;
    }

    let script = match crawler_script_path(&app) {
        Some(path) => path,
        None => {
            finish_with_error(&app, &state, &job_id, "找不到 Google Maps 采集器脚本");
            return;
        }
    };
    let request_json = match serde_json::to_string(&request) {
        Ok(value) => value,
        Err(error) => {
            finish_with_error(&app, &state, &job_id, &error.to_string());
            return;
        }
    };

    let project_dir = script
        .parent()
        .and_then(|path| path.parent())
        .unwrap_or_else(|| script.parent().unwrap());
    let mut command = Command::new(node_runtime_path(&app));
    command
        .arg(&script)
        .arg(request_json)
        .current_dir(project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            finish_with_error(
                &app,
                &state,
                &job_id,
                &format!("无法启动内置采集运行时：{error}"),
            );
            return;
        }
    };

    let stdout = child.stdout.take().expect("crawler stdout");
    let mut stderr = child.stderr.take().expect("crawler stderr");
    let stderr_task = tokio::spawn(async move {
        let mut value = String::new();
        let _ = stderr.read_to_string(&mut value).await;
        value
    });
    let mut lines = BufReader::new(stdout).lines();
    let mut website_tasks = Vec::new();
    let mut stopped_by_control = false;
    let mut event_error = String::new();

    loop {
        tokio::select! {
            changed = control.changed() => {
                if changed.is_ok() {
                    stopped_by_control = true;
                    let _ = child.kill().await;
                    break;
                }
            }
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => match serde_json::from_str::<CrawlerEvent>(&line) {
                        Ok(CrawlerEvent::Status { message }) => {
                            let _ = set_job_status(&app, &state, &job_id, "running", &message);
                        }
                        Ok(CrawlerEvent::Progress { discovered, message }) => {
                            if let Ok(connection) = state.db.lock() {
                                let _ = db::update_discovered(&connection, &job_id, discovered, &message);
                            }
                            emit_job(&app, &state, &job_id);
                        }
                        Ok(CrawlerEvent::Business { data }) => {
                            let saved = state.db.lock().ok().and_then(|connection| {
                                db::upsert_business(&connection, &job_id, &data).ok()
                            });
                            if let Some(business) = saved {
                                let _ = app.emit("business-upsert", &business);
                                emit_job(&app, &state, &job_id);
                                if business.status == "pending" {
                                    let app_clone = app.clone();
                                    let state_clone = state.clone();
                                    let job_clone = job_id.clone();
                                    website_tasks.push(tokio::spawn(async move {
                                        enrich_business(app_clone, state_clone, job_clone, business).await;
                                    }));
                                }
                            }
                        }
                        Ok(CrawlerEvent::Blocked { message }) => {
                            let _ = set_job_status(&app, &state, &job_id, "blocked", &message);
                        }
                        Ok(CrawlerEvent::Error { message }) => {
                            event_error = message.clone();
                            let _ = set_job_status(&app, &state, &job_id, "running", &message);
                        }
                        Ok(CrawlerEvent::Done { discovered }) => {
                            if let Ok(connection) = state.db.lock() {
                                let _ = db::update_discovered(&connection, &job_id, discovered, "Google Maps 采集完成，正在整理官网结果");
                            }
                            emit_job(&app, &state, &job_id);
                        }
                        Err(_) => {
                            // Playwright may write non-protocol diagnostics; keep them out of the UI.
                        }
                    },
                    Ok(None) => break,
                    Err(error) => {
                        event_error = error.to_string();
                        break;
                    }
                }
            }
        }
    }

    let exit_status = child.wait().await.ok();
    for task in website_tasks {
        let _ = task.await;
    }
    let stderr = stderr_task.await.unwrap_or_default();

    if stopped_by_control {
        emit_job(&app, &state, &job_id);
    } else if exit_status.map(|status| status.success()).unwrap_or(false) {
        let _ = set_job_status(&app, &state, &job_id, "completed", "任务已完成");
    } else {
        let detail = if !event_error.is_empty() {
            event_error
        } else if !stderr.trim().is_empty() {
            stderr
                .lines()
                .last()
                .unwrap_or("采集器异常退出")
                .to_string()
        } else {
            "采集器异常退出".to_string()
        };
        finish_with_error(&app, &state, &job_id, &detail);
    }
    if let Ok(mut controls) = state.controls.lock() {
        controls.remove(&job_id);
    }
}

async fn enrich_business(app: AppHandle, state: AppState, job_id: String, business: Business) {
    let _permit = state.website_semaphore.acquire().await;
    let result = if business.website.trim().is_empty() {
        WebsiteResult {
            status: "no_website".to_string(),
            ..WebsiteResult::default()
        }
    } else {
        website::crawl_website(&business.website).await
    };
    if let Ok(connection) = state.db.lock() {
        if let Ok(updated) = db::update_website(&connection, business.id, &result) {
            let failed = u32::from(result.status == "failed");
            let _ = db::increment_job(&connection, &job_id, 1, result.emails.len() as u32, failed);
            let _ = app.emit("business-upsert", updated);
        }
    }
    emit_job(&app, &state, &job_id);
}

fn crawler_script_path(app: &AppHandle) -> Option<PathBuf> {
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("crawler")
        .join("google_maps.mjs");
    if development.exists() {
        return Some(development);
    }
    let resources = app.path().resource_dir().ok()?;
    [
        resources.join("runtime").join("google_maps.mjs"),
        resources
            .join("_up_")
            .join("runtime")
            .join("google_maps.mjs"),
        resources.join("crawler").join("google_maps.mjs"),
        resources
            .join("_up_")
            .join("crawler")
            .join("google_maps.mjs"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn node_runtime_path(app: &AppHandle) -> PathBuf {
    let binary_name = if cfg!(windows) { "node.exe" } else { "node" };
    if let Ok(resources) = app.path().resource_dir() {
        for path in [
            resources.join("runtime").join(binary_name),
            resources.join("_up_").join("runtime").join(binary_name),
        ] {
            if path.exists() {
                return path;
            }
        }
    }
    PathBuf::from("node")
}

fn set_job_status(
    app: &AppHandle,
    state: &AppState,
    job_id: &str,
    status: &str,
    message: &str,
) -> Result<(), String> {
    let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
    db::update_job_status(&connection, job_id, status, message)?;
    drop(connection);
    emit_job(app, state, job_id);
    Ok(())
}

fn finish_with_error(app: &AppHandle, state: &AppState, job_id: &str, message: &str) {
    let _ = set_job_status(app, state, job_id, "failed", message);
}

pub fn emit_job(app: &AppHandle, state: &AppState, job_id: &str) {
    if let Ok(connection) = state.db.lock() {
        if let Ok(Some(job)) = db::get_job(&connection, job_id) {
            let _ = app.emit("job-progress", job);
        }
    }
}
