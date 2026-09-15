use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use rusqlite::Connection;
use tokio::sync::{watch, Mutex as AsyncMutex, Semaphore};

use crate::diagnostics::DiagnosticLog;

#[derive(Clone)]
pub struct AppState {
    pub(crate) db: Arc<Mutex<Connection>>,
    pub(crate) controls: Arc<Mutex<HashMap<String, watch::Sender<String>>>>,
    pub(crate) browser_lock: Arc<AsyncMutex<()>>,
    pub(crate) website_semaphore: Arc<Semaphore>,
    pub(crate) diagnostics: DiagnosticLog,
}

impl AppState {
    pub fn new(connection: Connection, diagnostics: DiagnosticLog) -> Self {
        Self {
            db: Arc::new(Mutex::new(connection)),
            controls: Arc::new(Mutex::new(HashMap::new())),
            browser_lock: Arc::new(AsyncMutex::new(())),
            website_semaphore: Arc::new(Semaphore::new(4)),
            diagnostics,
        }
    }
}
