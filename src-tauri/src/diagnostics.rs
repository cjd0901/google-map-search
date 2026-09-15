use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::Utc;

#[derive(Clone)]
pub struct DiagnosticLog {
    path: Arc<PathBuf>,
    file: Arc<Mutex<File>>,
}

impl DiagnosticLog {
    pub fn new(data_dir: &Path) -> Result<Self, String> {
        let log_dir = data_dir.join("diagnostics");
        fs::create_dir_all(&log_dir).map_err(|error| error.to_string())?;
        let filename = format!("yingfeng-data-{}.log", Utc::now().format("%Y%m%d-%H%M%S"));
        let path = log_dir.join(filename);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            path: Arc::new(path),
            file: Arc::new(Mutex::new(file)),
        })
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn write(&self, source: &str, message: impl AsRef<str>) {
        let Ok(mut file) = self.file.lock() else {
            return;
        };
        let timestamp = Utc::now().to_rfc3339();
        let message = message.as_ref().replace('\0', "�");
        let mut lines = message.lines();
        if let Some(first) = lines.next() {
            let _ = writeln!(file, "[{timestamp}] [{source}] {first}");
            for line in lines {
                let _ = writeln!(file, "[{timestamp}] [{source}] | {line}");
            }
        } else {
            let _ = writeln!(file, "[{timestamp}] [{source}]");
        }
        let _ = file.flush();
    }
}
