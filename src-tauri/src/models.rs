use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub keyword: String,
    pub location: String,
    pub max_results: u32,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_headless")]
    pub headless: bool,
}

fn default_language() -> String {
    "zh-CN".to_string()
}

fn default_headless() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchJob {
    pub id: String,
    pub keyword: String,
    pub location: String,
    pub max_results: u32,
    pub language: String,
    pub headless: bool,
    pub status: String,
    pub discovered: u32,
    pub enriched: u32,
    pub emails_found: u32,
    pub failed: u32,
    pub business_count: u32,
    pub exported: u32,
    pub message: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusinessInput {
    pub name: String,
    pub category: String,
    pub address: String,
    pub phone: String,
    pub website: String,
    pub maps_url: String,
    pub rating: Option<f64>,
    pub review_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Business {
    pub id: i64,
    pub job_id: String,
    pub name: String,
    pub category: String,
    pub address: String,
    pub phone: String,
    pub website: String,
    pub maps_url: String,
    pub rating: Option<f64>,
    pub review_count: Option<u32>,
    pub business_summary: String,
    pub emails: Vec<String>,
    pub email_source_urls: Vec<String>,
    pub facebook_urls: Vec<String>,
    pub status: String,
    pub error: String,
    pub created_at: String,
    pub updated_at: String,
    pub exported_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CrawlerEvent {
    Diagnostic { message: String },
    Status { message: String },
    Progress { discovered: u32, message: String },
    Business { data: BusinessInput },
    Blocked { message: String },
    Error { message: String },
    Done { discovered: u32 },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub jobs: Vec<SearchJob>,
    pub businesses: Vec<Business>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub source_jobs: usize,
    pub input_records: usize,
    pub exported_records: usize,
    pub duplicates_removed: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailCsvResult {
    pub csv: String,
    pub processed_records: usize,
    pub records_with_email: usize,
    pub emails_found: usize,
    pub no_email: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Default)]
pub struct WebsiteResult {
    pub summary: String,
    pub emails: Vec<String>,
    pub source_urls: Vec<String>,
    pub source_by_email: HashMap<String, String>,
    pub facebook_urls: Vec<String>,
    pub status: String,
    pub error: String,
}
