use std::{path::PathBuf, process::Stdio, time::Instant};

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
    let started_at = Instant::now();
    state.diagnostics.write(
        "task.start",
        format!(
            "========== TASK START ========== job_id={job_id} keyword={:?} location={:?} max_results={} language={} headless={}",
            request.keyword,
            request.location,
            request.max_results,
            request.language,
            request.headless
        ),
    );
    let _browser_guard = state.browser_lock.lock().await;
    state
        .diagnostics
        .write("crawler", format!("job_id={job_id} 已取得浏览器锁"));
    if let Err(error) = set_job_status(&app, &state, &job_id, "running", "正在启动浏览器") {
        state.diagnostics.write(
            "crawler.error",
            format!("job_id={job_id} 更新任务状态失败：{error}"),
        );
        log_task_end(&state, &job_id, "failed", &error, started_at);
        let _ = app.emit("crawler-error", error);
        return;
    }

    let script = match crawler_script_path(&app) {
        Some(path) => command_compatible_path(&path),
        None => {
            state.diagnostics.write(
                "crawler.error",
                format!("job_id={job_id} 找不到 Google Maps 采集器脚本"),
            );
            finish_with_error(&app, &state, &job_id, "找不到 Google Maps 采集器脚本");
            log_task_end(
                &state,
                &job_id,
                "failed",
                "找不到 Google Maps 采集器脚本",
                started_at,
            );
            return;
        }
    };
    let request_json = match serde_json::to_string(&request) {
        Ok(value) => value,
        Err(error) => {
            state.diagnostics.write(
                "crawler.error",
                format!("job_id={job_id} 请求序列化失败：{error}"),
            );
            finish_with_error(&app, &state, &job_id, &error.to_string());
            log_task_end(&state, &job_id, "failed", &error.to_string(), started_at);
            return;
        }
    };

    let project_dir = script
        .parent()
        .and_then(|path| path.parent())
        .unwrap_or_else(|| script.parent().unwrap());
    let node_runtime = command_compatible_path(&node_runtime_path(&app));
    let project_dir = command_compatible_path(project_dir);
    state.diagnostics.write(
        "crawler.runtime",
        format!(
            "job_id={job_id} node={} node_exists={} script={} cwd={}",
            node_runtime.display(),
            node_runtime.exists(),
            script.display(),
            project_dir.display()
        ),
    );
    let mut command = Command::new(&node_runtime);
    command
        .arg(&script)
        .arg(request_json)
        .current_dir(&project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = match command.spawn() {
        Ok(child) => {
            state.diagnostics.write(
                "crawler.runtime",
                format!("job_id={job_id} Node 进程已启动 pid={:?}", child.id()),
            );
            child
        }
        Err(error) => {
            state.diagnostics.write(
                "crawler.error",
                format!("job_id={job_id} 无法启动内置采集运行时：{error}"),
            );
            finish_with_error(
                &app,
                &state,
                &job_id,
                &format!("无法启动内置采集运行时：{error}"),
            );
            log_task_end(&state, &job_id, "failed", &error.to_string(), started_at);
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
    let mut discovered_count = 0;
    let mut saved_businesses = 0;

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
                        Ok(CrawlerEvent::Diagnostic { message }) => {
                            state
                                .diagnostics
                                .write("crawler.diagnostic", format!("job_id={job_id} {message}"));
                        }
                        Ok(CrawlerEvent::Status { message }) => {
                            state.diagnostics.write("crawler.status", format!("job_id={job_id} {message}"));
                            let _ = set_job_status(&app, &state, &job_id, "running", &message);
                        }
                        Ok(CrawlerEvent::Progress { discovered, message }) => {
                            discovered_count = discovered_count.max(discovered);
                            state.diagnostics.write("crawler.progress", format!("job_id={job_id} discovered={discovered} {message}"));
                            if let Ok(connection) = state.db.lock() {
                                let _ = db::update_discovered(&connection, &job_id, discovered, &message);
                            }
                            emit_job(&app, &state, &job_id);
                        }
                        Ok(CrawlerEvent::Business { data }) => {
                            let saved = match state.db.lock() {
                                Ok(connection) => db::upsert_business(&connection, &job_id, &data),
                                Err(_) => Err("数据库已锁定".to_string()),
                            };
                            match saved {
                                Ok(business) => {
                                    saved_businesses += 1;
                                    state.diagnostics.write(
                                        "crawler.business",
                                        format!(
                                            "job_id={job_id} saved={saved_businesses} business_id={} name={:?} category={:?} website={:?}",
                                            business.id, business.name, business.category, business.website
                                        ),
                                    );
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
                                Err(error) => {
                                    state.diagnostics.write(
                                        "crawler.business.error",
                                        format!(
                                            "job_id={job_id} name={:?} 保存失败：{error}",
                                            data.name
                                        ),
                                    );
                                }
                            }
                        }
                        Ok(CrawlerEvent::Blocked { message }) => {
                            state.diagnostics.write("crawler.blocked", format!("job_id={job_id} {message}"));
                            let _ = set_job_status(&app, &state, &job_id, "blocked", &message);
                        }
                        Ok(CrawlerEvent::Error { message }) => {
                            state.diagnostics.write("crawler.error", format!("job_id={job_id} {message}"));
                            event_error = message.clone();
                            let _ = set_job_status(&app, &state, &job_id, "running", &message);
                        }
                        Ok(CrawlerEvent::Done { discovered }) => {
                            discovered_count = discovered_count.max(discovered);
                            state.diagnostics.write("crawler.done", format!("job_id={job_id} discovered={discovered}"));
                            if let Ok(connection) = state.db.lock() {
                                let _ = db::update_discovered(&connection, &job_id, discovered, "Google Maps 采集完成，正在整理官网结果");
                            }
                            emit_job(&app, &state, &job_id);
                        }
                        Err(error) => {
                            state.diagnostics.write(
                                "crawler.stdout",
                                format!("job_id={job_id} 无法解析输出：{error}; raw={line}"),
                            );
                        }
                    },
                    Ok(None) => break,
                    Err(error) => {
                        state.diagnostics.write(
                            "crawler.error",
                            format!("job_id={job_id} 读取 Node 输出失败：{error}"),
                        );
                        event_error = error.to_string();
                        break;
                    }
                }
            }
        }
    }

    let exit_status = child.wait().await.ok();
    state.diagnostics.write(
        "website",
        format!(
            "job_id={job_id} 等待 {} 个官网采集任务完成",
            website_tasks.len()
        ),
    );
    for task in website_tasks {
        let _ = task.await;
    }
    let stderr = stderr_task.await.unwrap_or_default();
    state.diagnostics.write(
        "crawler.exit",
        format!(
            "job_id={job_id} stopped_by_control={stopped_by_control} exit_status={exit_status:?}"
        ),
    );
    if !stderr.trim().is_empty() {
        state
            .diagnostics
            .write("crawler.stderr", format!("job_id={job_id}\n{stderr}"));
    }

    if stopped_by_control {
        log_task_end(
            &state,
            &job_id,
            "stopped",
            &format!("discovered={discovered_count} saved={saved_businesses}"),
            started_at,
        );
        emit_job(&app, &state, &job_id);
    } else if exit_status.map(|status| status.success()).unwrap_or(false) {
        let _ = set_job_status(&app, &state, &job_id, "completed", "任务已完成");
        log_task_end(
            &state,
            &job_id,
            "completed",
            &format!("discovered={discovered_count} saved={saved_businesses}"),
            started_at,
        );
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
        log_task_end(
            &state,
            &job_id,
            "failed",
            &format!("discovered={discovered_count} saved={saved_businesses} error={detail}"),
            started_at,
        );
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
    state.diagnostics.write(
        "website.result",
        format!(
            "job_id={job_id} business_id={} name={:?} status={} emails_found={} error={:?}",
            business.id,
            business.name,
            result.status,
            result.emails.len(),
            result.error
        ),
    );
    if result.status == "failed" {
        state.diagnostics.write(
            "website.error",
            format!(
                "job_id={job_id} business_id={} {}",
                business.id, result.error
            ),
        );
    }
    if let Ok(connection) = state.db.lock() {
        if let Ok(updated) = db::update_website(&connection, business.id, &result) {
            let failed = u32::from(result.status == "failed");
            let _ = db::increment_job(&connection, &job_id, 1, result.emails.len() as u32, failed);
            let _ = app.emit("business-upsert", updated);
        }
    }
    emit_job(&app, &state, &job_id);
}

fn log_task_end(state: &AppState, job_id: &str, status: &str, detail: &str, started_at: Instant) {
    state.diagnostics.write(
        "task.end",
        format!(
            "========== TASK END ========== job_id={job_id} status={status} elapsed_ms={} {detail}",
            started_at.elapsed().as_millis()
        ),
    );
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

fn command_compatible_path(path: &std::path::Path) -> PathBuf {
    #[cfg(windows)]
    {
        let value = path.to_string_lossy();
        if let Some(value) = value.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{value}"));
        }
        if let Some(value) = value.strip_prefix(r"\\?\") {
            return PathBuf::from(value);
        }
    }
    path.to_path_buf()
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::command_compatible_path;

    #[test]
    fn keeps_regular_paths_unchanged() {
        let path = Path::new(r"E:\迎风数据\runtime\google_maps.mjs");
        assert_eq!(command_compatible_path(path), path);
    }

    #[cfg(windows)]
    #[test]
    fn removes_windows_verbatim_disk_prefix() {
        let path = Path::new(r"\\?\E:\迎风数据\runtime\google_maps.mjs");
        assert_eq!(
            command_compatible_path(path),
            Path::new(r"E:\迎风数据\runtime\google_maps.mjs")
        );
    }

    #[cfg(windows)]
    #[test]
    fn converts_windows_verbatim_unc_prefix() {
        let path = Path::new(r"\\?\UNC\server\share\google_maps.mjs");
        assert_eq!(
            command_compatible_path(path),
            Path::new(r"\\server\share\google_maps.mjs")
        );
    }
}
