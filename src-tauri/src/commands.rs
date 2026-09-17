use tauri::{AppHandle, State};
use tokio::sync::watch;
use uuid::Uuid;

use crate::{
    crawler, db, email_csv, export,
    models::{AppSnapshot, Business, EmailCsvResult, ExportResult, SearchJob, SearchRequest},
    state::AppState,
};

const LICENSE_DEVICE_ID_SETTING: &str = "license.device_id";

#[tauri::command]
pub fn get_or_create_device_id(
    state: State<'_, AppState>,
    legacy_device_id: Option<String>,
) -> Result<String, String> {
    let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
    if let Some(stored) = db::get_setting(&connection, LICENSE_DEVICE_ID_SETTING)? {
        if let Some(device_id) = normalize_device_id(&stored) {
            return Ok(device_id);
        }
    }

    let device_id = legacy_device_id
        .as_deref()
        .and_then(normalize_device_id)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    db::set_setting(&connection, LICENSE_DEVICE_ID_SETTING, &device_id)?;
    state
        .diagnostics
        .write("license.device_id", "设备授权标识已保存到本地数据库");
    Ok(device_id)
}

fn normalize_device_id(value: &str) -> Option<String> {
    let value = value.trim();
    if !(8..=128).contains(&value.len())
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:".contains(character))
    {
        return None;
    }
    Some(value.to_string())
}

#[tauri::command]
pub fn get_snapshot(
    state: State<'_, AppState>,
    job_id: Option<String>,
) -> Result<AppSnapshot, String> {
    let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
    let jobs = db::list_jobs(&connection)?;
    let selected_job_id = job_id.or_else(|| jobs.first().map(|job| job.id.clone()));
    let businesses = match selected_job_id {
        Some(id) => db::list_businesses(&connection, &id)?,
        None => Vec::new(),
    };
    Ok(AppSnapshot { jobs, businesses })
}

#[tauri::command]
pub fn list_businesses(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<Vec<Business>, String> {
    let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
    db::list_businesses(&connection, &job_id)
}

#[tauri::command]
pub fn update_business_emails(
    state: State<'_, AppState>,
    business_id: i64,
    emails: Vec<String>,
) -> Result<Business, String> {
    let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
    db::update_business_emails(&connection, business_id, emails)
}

#[tauri::command]
pub fn delete_business(state: State<'_, AppState>, business_id: i64) -> Result<(), String> {
    let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
    if db::delete_business(&connection, business_id)? {
        Ok(())
    } else {
        Err("商家记录不存在或已经被删除".to_string())
    }
}

#[tauri::command]
pub fn start_search(
    app: AppHandle,
    state: State<'_, AppState>,
    request: SearchRequest,
) -> Result<SearchJob, String> {
    let request = validate_request(request)?;
    ensure_no_active_job(state.inner())?;
    let job_id = Uuid::new_v4().to_string();
    state.diagnostics.write(
        "task.create",
        format!(
            "job_id={job_id} keyword={:?} location={:?} max_results={} language={} headless={}",
            request.keyword,
            request.location,
            request.max_results,
            request.language,
            request.headless
        ),
    );
    {
        let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
        db::insert_job(&connection, &job_id, &request)?;
    }
    launch_job(app, state.inner().clone(), job_id.clone(), request)?;
    let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
    db::get_job(&connection, &job_id)?.ok_or_else(|| "任务不存在".to_string())
}

#[tauri::command]
pub fn resume_search(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> Result<SearchJob, String> {
    ensure_no_active_job(state.inner())?;
    state
        .diagnostics
        .write("task.resume", format!("job_id={job_id}"));
    let request = {
        let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
        db::request_for_job(&connection, &job_id)?
    };
    launch_job(app, state.inner().clone(), job_id.clone(), request)?;
    let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
    db::get_job(&connection, &job_id)?.ok_or_else(|| "任务不存在".to_string())
}

#[tauri::command]
pub fn control_search(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    action: String,
) -> Result<(), String> {
    let action = ControlAction::try_from(action.as_str())?;
    state.diagnostics.write(
        "task.control",
        format!("job_id={job_id} action={}", action.as_str()),
    );
    let sender = state
        .controls
        .lock()
        .map_err(|_| "任务状态已锁定".to_string())?
        .remove(&job_id);
    if let Some(sender) = sender {
        let _ = sender.send(action.as_str().to_string());
    }
    {
        let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
        db::update_job_status(&connection, &job_id, action.status(), action.message())?;
    }
    crawler::emit_job(&app, state.inner(), &job_id);
    Ok(())
}

#[tauri::command]
pub fn delete_search(state: State<'_, AppState>, job_id: String) -> Result<(), String> {
    let sender = state
        .controls
        .lock()
        .map_err(|_| "任务状态已锁定".to_string())?
        .remove(&job_id);
    if let Some(sender) = sender {
        let _ = sender.send("cancel".to_string());
    }
    let deleted = {
        let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
        db::delete_job(&connection, &job_id)?
    };
    if deleted {
        Ok(())
    } else {
        Err("任务不存在或已经被删除".to_string())
    }
}

#[tauri::command]
pub fn export_csv(
    app: AppHandle,
    state: State<'_, AppState>,
    job_ids: Vec<String>,
) -> Result<ExportResult, String> {
    export::export_csv(&app, state.inner(), job_ids)
}

#[tauri::command]
pub async fn scrape_emails_csv(
    state: State<'_, AppState>,
    csv_content: String,
) -> Result<EmailCsvResult, String> {
    ensure_no_active_job(state.inner())?;
    email_csv::scrape_csv(csv_content, state.website_semaphore.clone()).await
}

#[tauri::command]
pub fn get_diagnostic_log_path(state: State<'_, AppState>) -> String {
    state.diagnostics.path().to_string_lossy().into_owned()
}

#[tauri::command]
pub fn write_diagnostic_log(state: State<'_, AppState>, source: String, message: String) {
    let source = source.chars().take(80).collect::<String>();
    let message = message.chars().take(20_000).collect::<String>();
    state.diagnostics.write(&source, message);
}

fn ensure_no_active_job(state: &AppState) -> Result<(), String> {
    let controls = state
        .controls
        .lock()
        .map_err(|_| "任务状态已锁定".to_string())?;
    if controls.is_empty() {
        Ok(())
    } else {
        Err("当前已有采集任务运行，请先暂停或取消该任务".to_string())
    }
}

fn launch_job(
    app: AppHandle,
    state: AppState,
    job_id: String,
    request: SearchRequest,
) -> Result<(), String> {
    let (sender, receiver) = watch::channel(String::new());
    state
        .controls
        .lock()
        .map_err(|_| "任务状态已锁定".to_string())?
        .insert(job_id.clone(), sender);
    {
        let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
        db::update_job_status(&connection, &job_id, "running", "正在准备采集器")?;
    }
    crawler::emit_job(&app, &state, &job_id);
    state.diagnostics.write(
        "command.launch_job",
        format!("job_id={job_id} 后台任务已提交"),
    );
    tauri::async_runtime::spawn(crawler::run_search(app, state, job_id, request, receiver));
    Ok(())
}

fn validate_request(mut request: SearchRequest) -> Result<SearchRequest, String> {
    request.keyword = request.keyword.trim().to_string();
    request.location = request.location.trim().to_string();
    request.language = request.language.trim().to_string();
    if request.keyword.is_empty() || request.location.is_empty() {
        return Err("关键词和地区不能为空".to_string());
    }
    if !(1..=200).contains(&request.max_results) {
        return Err("结果数量必须在 1 到 200 之间".to_string());
    }
    if request.language.is_empty() {
        request.language = "zh-CN".to_string();
    }
    Ok(request)
}

enum ControlAction {
    Pause,
    Cancel,
}

impl ControlAction {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Cancel => "cancel",
        }
    }

    fn status(&self) -> &'static str {
        match self {
            Self::Pause => "paused",
            Self::Cancel => "cancelled",
        }
    }

    fn message(&self) -> &'static str {
        match self {
            Self::Pause => "任务已暂停，可从现有结果继续",
            Self::Cancel => "任务已取消",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_device_id;

    #[test]
    fn accepts_existing_device_ids() {
        assert_eq!(
            normalize_device_id(" 6a8bc927-0872-4a5b-8ca4-9e226e41d53b "),
            Some("6a8bc927-0872-4a5b-8ca4-9e226e41d53b".to_string())
        );
        assert_eq!(
            normalize_device_id("device-12345678"),
            Some("device-12345678".to_string())
        );
    }

    #[test]
    fn rejects_unsafe_device_ids() {
        assert_eq!(normalize_device_id("short"), None);
        assert_eq!(normalize_device_id("device id with spaces"), None);
        assert_eq!(normalize_device_id("设备-12345678"), None);
    }
}

impl TryFrom<&str> for ControlAction {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pause" => Ok(Self::Pause),
            "cancel" => Ok(Self::Cancel),
            _ => Err("不支持的任务操作".to_string()),
        }
    }
}
