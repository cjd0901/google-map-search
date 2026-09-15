use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs::File,
    io::Write,
};

use chrono::Local;
use csv::WriterBuilder;
use tauri::{AppHandle, Manager};

use crate::{
    db,
    models::{Business, ExportResult},
    state::AppState,
    website,
};

pub fn export_csv(
    app: &AppHandle,
    state: &AppState,
    job_ids: Vec<String>,
) -> Result<ExportResult, String> {
    let job_ids = unique_job_ids(job_ids)?;
    let (businesses, job_labels) = load_export_data(state, &job_ids)?;
    if businesses.is_empty() {
        return Err("所选任务没有未导出的商家".to_string());
    }

    let exported_business_ids = businesses
        .iter()
        .map(|business| business.id)
        .collect::<Vec<_>>();
    let input_records = businesses.len();
    let businesses = deduplicate_businesses(businesses);
    let exported_records = businesses.len();
    let path = create_export_path(app, job_ids.len())?;
    write_csv(&path, businesses, &job_labels)?;
    {
        let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
        db::mark_businesses_exported(&connection, &exported_business_ids)?;
    }

    Ok(ExportResult {
        path: path.to_string_lossy().to_string(),
        source_jobs: job_ids.len(),
        input_records,
        exported_records,
        duplicates_removed: input_records.saturating_sub(exported_records),
    })
}

fn unique_job_ids(job_ids: Vec<String>) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let unique_ids: Vec<_> = job_ids
        .into_iter()
        .filter(|id| !id.trim().is_empty())
        .filter(|id| seen.insert(id.clone()))
        .collect();
    if unique_ids.is_empty() {
        return Err("请至少选择一个任务".to_string());
    }
    Ok(unique_ids)
}

fn load_export_data(
    state: &AppState,
    job_ids: &[String],
) -> Result<(Vec<Business>, HashMap<String, String>), String> {
    let connection = state.db.lock().map_err(|_| "数据库已锁定".to_string())?;
    let mut businesses = Vec::new();
    let mut labels = HashMap::new();
    for job_id in job_ids {
        let job =
            db::get_job(&connection, job_id)?.ok_or_else(|| format!("任务不存在：{job_id}"))?;
        labels.insert(
            job_id.clone(),
            format!("{} | {}", job.keyword, job.location),
        );
        businesses.extend(
            db::list_businesses(&connection, job_id)?
                .into_iter()
                .filter(|business| business.exported_at.is_none()),
        );
    }
    Ok((businesses, labels))
}

fn create_export_path(app: &AppHandle, job_count: usize) -> Result<std::path::PathBuf, String> {
    let directory = app
        .path()
        .download_dir()
        .or_else(|_| app.path().desktop_dir())
        .or_else(|_| app.path().app_data_dir())
        .map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join(format!(
        "businesses-{job_count}-tasks-{}.csv",
        Local::now().format("%Y%m%d-%H%M%S")
    )))
}

fn write_csv(
    path: &std::path::Path,
    businesses: Vec<MergedBusiness>,
    job_labels: &HashMap<String, String>,
) -> Result<(), String> {
    let mut file = File::create(path).map_err(|error| error.to_string())?;
    file.write_all(&[0xEF, 0xBB, 0xBF])
        .map_err(|error| error.to_string())?;
    let mut writer = WriterBuilder::new().from_writer(file);
    writer
        .write_record([
            "来源任务",
            "名称",
            "主营分类",
            "电话",
            "地址",
            "官网",
            "Facebook 链接",
            "邮箱",
            "业务摘要",
            "评分",
            "评价数",
            "Google Maps 链接",
            "邮箱来源",
            "状态",
            "错误",
        ])
        .map_err(|error| error.to_string())?;

    for item in businesses {
        let source_jobs = item
            .source_job_ids
            .iter()
            .filter_map(|id| job_labels.get(id))
            .cloned()
            .collect::<Vec<_>>()
            .join("; ");
        let business = item.business;
        writer
            .write_record([
                source_jobs,
                business.name,
                business.category,
                business.phone,
                business.address,
                business.website,
                business.facebook_urls.join("; "),
                business.emails.join("; "),
                business.business_summary,
                business
                    .rating
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                business
                    .review_count
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                business.maps_url,
                business.email_source_urls.join("; "),
                business.status,
                business.error,
            ])
            .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

#[derive(Debug)]
struct MergedBusiness {
    business: Business,
    source_job_ids: BTreeSet<String>,
}

fn deduplicate_businesses(businesses: Vec<Business>) -> Vec<MergedBusiness> {
    let mut merged: Vec<MergedBusiness> = Vec::new();
    let mut index_by_key = HashMap::new();
    for business in businesses {
        let key = business_identity(&business);
        if let Some(index) = index_by_key.get(&key).copied() {
            merge_business(&mut merged[index], business);
        } else {
            let mut source_job_ids = BTreeSet::new();
            source_job_ids.insert(business.job_id.clone());
            index_by_key.insert(key, merged.len());
            merged.push(MergedBusiness {
                business,
                source_job_ids,
            });
        }
    }
    merged
}

fn business_identity(business: &Business) -> String {
    if let Some(maps_id) = maps_business_id(&business.maps_url) {
        return format!("maps:{maps_id}");
    }
    if let Ok(url) = url::Url::parse(&business.maps_url) {
        if let Some((_, value)) = url
            .query_pairs()
            .find(|(key, _)| key == "cid" || key == "ftid")
        {
            return format!("maps-query:{}", value.to_ascii_lowercase());
        }
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        let path = url.path().trim_end_matches('/').to_ascii_lowercase();
        if !host.is_empty() && !path.is_empty() {
            return format!("maps-url:{host}{path}");
        }
    }

    let name = normalize_identity_text(&business.name);
    let address = normalize_identity_text(&business.address);
    if !name.is_empty() && !address.is_empty() {
        return format!("name-address:{name}:{address}");
    }
    let phone: String = business
        .phone
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect();
    if !name.is_empty() && phone.len() >= 7 {
        return format!("name-phone:{name}:{phone}");
    }
    if let Ok(url) = url::Url::parse(&business.website) {
        let domain = url
            .host_str()
            .unwrap_or_default()
            .trim_start_matches("www.")
            .to_ascii_lowercase();
        if !domain.is_empty() && !name.is_empty() {
            return format!("name-domain:{name}:{domain}");
        }
    }
    format!("record:{}:{}", business.job_id, business.id)
}

fn maps_business_id(url: &str) -> Option<String> {
    let start = url.find("!1s")? + 3;
    let tail = &url[start..];
    let end = tail.find('!').unwrap_or(tail.len());
    let value = tail[..end].trim();
    (!value.is_empty()).then(|| value.to_ascii_lowercase())
}

fn normalize_identity_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn merge_business(target: &mut MergedBusiness, source: Business) {
    target.source_job_ids.insert(source.job_id.clone());
    fill_if_empty(&mut target.business.name, &source.name);
    fill_if_empty(&mut target.business.category, &source.category);
    fill_if_empty(&mut target.business.address, &source.address);
    fill_if_empty(&mut target.business.phone, &source.phone);
    fill_if_empty(&mut target.business.website, &source.website);
    fill_if_empty(&mut target.business.maps_url, &source.maps_url);
    if target.business.rating.is_none() {
        target.business.rating = source.rating;
    }
    if target.business.review_count.is_none() {
        target.business.review_count = source.review_count;
    }
    if source.business_summary.chars().count() > target.business.business_summary.chars().count() {
        target.business.business_summary = source.business_summary;
    }
    let mut emails = target.business.emails.clone();
    emails.extend(source.emails);
    target.business.emails = website::filter_email_list(emails);
    let mut source_urls: BTreeSet<_> = target.business.email_source_urls.iter().cloned().collect();
    source_urls.extend(source.email_source_urls);
    target.business.email_source_urls = source_urls.into_iter().collect();
    let mut facebook_urls: BTreeSet<_> = target.business.facebook_urls.iter().cloned().collect();
    facebook_urls.extend(source.facebook_urls);
    target.business.facebook_urls = facebook_urls.into_iter().collect();
    if target.business.status != "completed" && source.status == "completed" {
        target.business.status = source.status;
        target.business.error.clear();
    }
    if source.updated_at > target.business.updated_at {
        target.business.updated_at = source.updated_at;
    }
}

fn fill_if_empty(target: &mut String, source: &str) {
    if target.trim().is_empty() && !source.trim().is_empty() {
        *target = source.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn business(
        id: i64,
        job_id: &str,
        maps_id: &str,
        name: &str,
        address: &str,
        email: &str,
    ) -> Business {
        Business {
            id,
            job_id: job_id.to_string(),
            name: name.to_string(),
            category: "咖啡店".to_string(),
            address: address.to_string(),
            phone: "+86 21 1234 5678".to_string(),
            website: "https://example.org".to_string(),
            maps_url: format!("https://www.google.com/maps/place/shop/data=!4m7!1s{maps_id}!8m2"),
            rating: Some(4.5),
            review_count: Some(10),
            business_summary: String::new(),
            emails: if email.is_empty() {
                Vec::new()
            } else {
                vec![email.to_string()]
            },
            email_source_urls: if email.is_empty() {
                Vec::new()
            } else {
                vec!["https://example.org/contact".to_string()]
            },
            facebook_urls: Vec::new(),
            status: "completed".to_string(),
            error: String::new(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            exported_at: None,
        }
    }

    #[test]
    fn merges_the_same_maps_business_across_jobs() {
        let first = business(
            1,
            "job-a",
            "0xabc:0x123",
            "示例咖啡",
            "上海市静安区 1 号",
            "hello@demo.org",
        );
        let second = business(
            2,
            "job-b",
            "0xabc:0x123",
            "示例咖啡店",
            "上海市静安区1号",
            "sales@demo.org",
        );

        let merged = deduplicate_businesses(vec![first, second]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source_job_ids.len(), 2);
        assert_eq!(
            merged[0].business.emails,
            vec!["hello@demo.org", "sales@demo.org"]
        );
    }

    #[test]
    fn keeps_different_maps_businesses_separate() {
        let first = business(1, "job-a", "0xaaa:0x111", "分店", "地址 A", "");
        let second = business(2, "job-b", "0xbbb:0x222", "分店", "地址 B", "");

        assert_eq!(deduplicate_businesses(vec![first, second]).len(), 2);
    }

    #[test]
    fn falls_back_to_normalized_name_and_address() {
        let mut first = business(1, "job-a", "unused-a", "ABC Dental", "12 Main St.", "");
        let mut second = business(2, "job-b", "unused-b", "abc dental", "12 Main St", "");
        first.maps_url.clear();
        second.maps_url.clear();

        assert_eq!(deduplicate_businesses(vec![first, second]).len(), 1);
    }
}
