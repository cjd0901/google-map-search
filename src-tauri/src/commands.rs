use tauri::{AppHandle, State};
use tokio::sync::watch;
use uuid::Uuid;

use crate::{
    crawler, db, email_csv, export,
    models::{AppSnapshot, Business, EmailCsvResult, ExportResult, SearchJob, SearchRequest},
    state::AppState,
};

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
    let job = {
        let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
        db::insert_job(&connection, &job_id, &request)?
    };
    launch_job(app, state.inner().clone(), job_id, request)?;
    Ok(job)
}

#[tauri::command]
pub fn resume_search(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> Result<SearchJob, String> {
    ensure_no_active_job(state.inner())?;
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
