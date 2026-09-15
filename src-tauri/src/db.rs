use std::{fs, path::Path};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    models::{Business, BusinessInput, SearchJob, SearchRequest, WebsiteResult},
    website,
};

pub fn open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS search_jobs (
               id TEXT PRIMARY KEY,
               keyword TEXT NOT NULL,
               location TEXT NOT NULL,
               max_results INTEGER NOT NULL,
               language TEXT NOT NULL,
               headless INTEGER NOT NULL DEFAULT 1,
               status TEXT NOT NULL,
               discovered INTEGER NOT NULL DEFAULT 0,
               enriched INTEGER NOT NULL DEFAULT 0,
               emails_found INTEGER NOT NULL DEFAULT 0,
               failed INTEGER NOT NULL DEFAULT 0,
               message TEXT NOT NULL DEFAULT '',
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS businesses (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               job_id TEXT NOT NULL,
               name TEXT NOT NULL DEFAULT '',
               category TEXT NOT NULL DEFAULT '',
               address TEXT NOT NULL DEFAULT '',
               phone TEXT NOT NULL DEFAULT '',
               website TEXT NOT NULL DEFAULT '',
               maps_url TEXT NOT NULL,
               rating REAL,
               review_count INTEGER,
               business_summary TEXT NOT NULL DEFAULT '',
               emails TEXT NOT NULL DEFAULT '[]',
               email_source_urls TEXT NOT NULL DEFAULT '[]',
               facebook_urls TEXT NOT NULL DEFAULT '[]',
               status TEXT NOT NULL DEFAULT 'pending',
                error TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                exported_at TEXT,
                FOREIGN KEY(job_id) REFERENCES search_jobs(id) ON DELETE CASCADE,
               UNIQUE(job_id, maps_url)
             );
             CREATE INDEX IF NOT EXISTS idx_businesses_job ON businesses(job_id);
             CREATE INDEX IF NOT EXISTS idx_businesses_website ON businesses(website);",
        )
        .map_err(|error| error.to_string())?;
    ensure_facebook_urls_column(&connection)?;
    ensure_exported_at_column(&connection)?;
    Ok(connection)
}

fn ensure_facebook_urls_column(connection: &Connection) -> Result<(), String> {
    let exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('businesses') WHERE name = 'facebook_urls'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if exists == 0 {
        connection
            .execute(
                "ALTER TABLE businesses ADD COLUMN facebook_urls TEXT NOT NULL DEFAULT '[]'",
                [],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn ensure_exported_at_column(connection: &Connection) -> Result<(), String> {
    let exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('businesses') WHERE name = 'exported_at'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if exists == 0 {
        connection
            .execute("ALTER TABLE businesses ADD COLUMN exported_at TEXT", [])
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn insert_job(
    connection: &Connection,
    id: &str,
    request: &SearchRequest,
) -> Result<SearchJob, String> {
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT INTO search_jobs
             (id, keyword, location, max_results, language, headless, status, message, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued', '任务已创建', ?7, ?7)",
            params![
                id,
                request.keyword,
                request.location,
                request.max_results,
                request.language,
                request.headless,
                now
            ],
        )
        .map_err(|error| error.to_string())?;
    get_job(connection, id)?.ok_or_else(|| "创建任务失败".to_string())
}

pub fn get_job(connection: &Connection, id: &str) -> Result<Option<SearchJob>, String> {
    connection
        .query_row(
            "SELECT id, keyword, location, max_results, language, headless, status,
                    discovered, enriched, emails_found, failed, message, created_at, updated_at,
                    (SELECT COUNT(*) FROM businesses WHERE businesses.job_id = search_jobs.id),
                    (SELECT COUNT(*) FROM businesses WHERE businesses.job_id = search_jobs.id
                     AND businesses.exported_at IS NOT NULL)
             FROM search_jobs WHERE id = ?1",
            [id],
            job_from_row,
        )
        .optional()
        .map_err(|error| error.to_string())
}

pub fn list_jobs(connection: &Connection) -> Result<Vec<SearchJob>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, keyword, location, max_results, language, headless, status,
                    discovered, enriched, emails_found, failed, message, created_at, updated_at,
                    (SELECT COUNT(*) FROM businesses WHERE businesses.job_id = search_jobs.id),
                    (SELECT COUNT(*) FROM businesses WHERE businesses.job_id = search_jobs.id
                     AND businesses.exported_at IS NOT NULL)
             FROM search_jobs ORDER BY created_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], job_from_row)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchJob> {
    Ok(SearchJob {
        id: row.get(0)?,
        keyword: row.get(1)?,
        location: row.get(2)?,
        max_results: row.get(3)?,
        language: row.get(4)?,
        headless: row.get::<_, i64>(5)? != 0,
        status: row.get(6)?,
        discovered: row.get(7)?,
        enriched: row.get(8)?,
        emails_found: row.get(9)?,
        failed: row.get(10)?,
        message: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        business_count: row.get(14)?,
        exported: row.get(15)?,
    })
}

pub fn update_job_status(
    connection: &Connection,
    id: &str,
    status: &str,
    message: &str,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE search_jobs SET status = ?2, message = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, status, message, Utc::now().to_rfc3339()],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn update_discovered(
    connection: &Connection,
    id: &str,
    discovered: u32,
    message: &str,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE search_jobs SET discovered = MAX(discovered, ?2), message = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, discovered, message, Utc::now().to_rfc3339()],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn increment_job(
    connection: &Connection,
    id: &str,
    enriched: u32,
    emails: u32,
    failed: u32,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE search_jobs
             SET enriched = enriched + ?2, emails_found = emails_found + ?3,
                 failed = failed + ?4, updated_at = ?5 WHERE id = ?1",
            params![id, enriched, emails, failed, Utc::now().to_rfc3339()],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn upsert_business(
    connection: &Connection,
    job_id: &str,
    business: &BusinessInput,
) -> Result<Business, String> {
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT INTO businesses
             (job_id, name, category, address, phone, website, maps_url, rating, review_count, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', ?10, ?10)
             ON CONFLICT(job_id, maps_url) DO UPDATE SET
               name = excluded.name, category = excluded.category, address = excluded.address,
               phone = excluded.phone, website = excluded.website, rating = excluded.rating,
               review_count = excluded.review_count, updated_at = excluded.updated_at",
            params![
                job_id,
                business.name,
                business.category,
                business.address,
                business.phone,
                business.website,
                business.maps_url,
                business.rating,
                business.review_count,
                now
            ],
        )
        .map_err(|error| error.to_string())?;
    let id: i64 = connection
        .query_row(
            "SELECT id FROM businesses WHERE job_id = ?1 AND maps_url = ?2",
            params![job_id, business.maps_url],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    get_business(connection, id)?.ok_or_else(|| "保存商家失败".to_string())
}

pub fn update_website(
    connection: &Connection,
    id: i64,
    result: &WebsiteResult,
) -> Result<Business, String> {
    let emails = serde_json::to_string(&result.emails).map_err(|error| error.to_string())?;
    let sources = serde_json::to_string(&result.source_urls).map_err(|error| error.to_string())?;
    let facebook_urls =
        serde_json::to_string(&result.facebook_urls).map_err(|error| error.to_string())?;
    connection
        .execute(
            "UPDATE businesses SET business_summary = ?2, emails = ?3, email_source_urls = ?4,
             facebook_urls = ?5, status = ?6, error = ?7, updated_at = ?8 WHERE id = ?1",
            params![
                id,
                result.summary,
                emails,
                sources,
                facebook_urls,
                result.status,
                result.error,
                Utc::now().to_rfc3339()
            ],
        )
        .map_err(|error| error.to_string())?;
    get_business(connection, id)?.ok_or_else(|| "更新商家失败".to_string())
}

pub fn get_business(connection: &Connection, id: i64) -> Result<Option<Business>, String> {
    connection
        .query_row(
            "SELECT id, job_id, name, category, address, phone, website, maps_url, rating,
                    review_count, business_summary, emails, email_source_urls, facebook_urls,
                    status, error, created_at, updated_at, exported_at
             FROM businesses WHERE id = ?1",
            [id],
            business_from_row,
        )
        .optional()
        .map_err(|error| error.to_string())
}

pub fn list_businesses(connection: &Connection, job_id: &str) -> Result<Vec<Business>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, job_id, name, category, address, phone, website, maps_url, rating,
                    review_count, business_summary, emails, email_source_urls, facebook_urls,
                    status, error, created_at, updated_at, exported_at
              FROM businesses WHERE job_id = ?1 ORDER BY id DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([job_id], business_from_row)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn update_business_emails(
    connection: &Connection,
    id: i64,
    values: Vec<String>,
) -> Result<Business, String> {
    let has_value = values.iter().any(|value| !value.trim().is_empty());
    let emails = website::filter_email_list(values);
    if has_value && emails.is_empty() {
        return Err("请输入有效的邮箱地址".to_string());
    }
    let emails_json = serde_json::to_string(&emails).map_err(|error| error.to_string())?;
    let affected = connection
        .execute(
            "UPDATE businesses SET emails = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, emails_json, Utc::now().to_rfc3339()],
        )
        .map_err(|error| error.to_string())?;
    if affected == 0 {
        return Err("商家记录不存在或已经被删除".to_string());
    }
    get_business(connection, id)?.ok_or_else(|| "更新邮箱失败".to_string())
}

pub fn delete_business(connection: &Connection, id: i64) -> Result<bool, String> {
    let affected = connection
        .execute("DELETE FROM businesses WHERE id = ?1", [id])
        .map_err(|error| error.to_string())?;
    Ok(affected > 0)
}

pub fn mark_businesses_exported(connection: &Connection, ids: &[i64]) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    let placeholders = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "UPDATE businesses SET exported_at = ?1, updated_at = ?2 WHERE id IN ({})",
        placeholders
    );
    let now = Utc::now().to_rfc3339();
    let mut values: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(ids.len() + 2);
    values.push(&now);
    values.push(&now);
    for id in ids {
        values.push(id);
    }
    connection
        .execute(&sql, rusqlite::params_from_iter(values))
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn business_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Business> {
    let emails_json: String = row.get(11)?;
    let sources_json: String = row.get(12)?;
    let facebook_urls_json: String = row.get(13)?;
    Ok(Business {
        id: row.get(0)?,
        job_id: row.get(1)?,
        name: row.get(2)?,
        category: row.get(3)?,
        address: row.get(4)?,
        phone: row.get(5)?,
        website: row.get(6)?,
        maps_url: row.get(7)?,
        rating: row.get(8)?,
        review_count: row.get(9)?,
        business_summary: row.get(10)?,
        emails: website::filter_email_list(serde_json::from_str(&emails_json).unwrap_or_default()),
        email_source_urls: serde_json::from_str(&sources_json).unwrap_or_default(),
        facebook_urls: serde_json::from_str(&facebook_urls_json).unwrap_or_default(),
        status: row.get(14)?,
        error: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        exported_at: row.get(18)?,
    })
}

pub fn request_for_job(connection: &Connection, id: &str) -> Result<SearchRequest, String> {
    let job = get_job(connection, id)?.ok_or_else(|| "任务不存在".to_string())?;
    Ok(SearchRequest {
        keyword: job.keyword,
        location: job.location,
        max_results: job.max_results,
        language: job.language,
        headless: job.headless,
    })
}

pub fn delete_job(connection: &Connection, id: &str) -> Result<bool, String> {
    let affected = connection
        .execute("DELETE FROM search_jobs WHERE id = ?1", [id])
        .map_err(|error| error.to_string())?;
    Ok(affected > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_selected_businesses_as_exported() {
        let connection = Connection::open_in_memory().expect("open database");
        connection
            .execute_batch(
                "CREATE TABLE businesses (
                   id INTEGER PRIMARY KEY,
                   exported_at TEXT,
                   updated_at TEXT NOT NULL
                 );
                 INSERT INTO businesses (id, updated_at) VALUES (1, 'old'), (2, 'old');",
            )
            .expect("create fixture");

        mark_businesses_exported(&connection, &[1]).expect("mark businesses");

        let exported: Option<String> = connection
            .query_row(
                "SELECT exported_at FROM businesses WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .expect("read marked business");
        let untouched: Option<String> = connection
            .query_row(
                "SELECT exported_at FROM businesses WHERE id = 2",
                [],
                |row| row.get(0),
            )
            .expect("read untouched business");

        assert!(exported.is_some());
        assert!(untouched.is_none());
    }
}
