use std::{collections::HashMap, sync::Arc};

use csv::{ReaderBuilder, WriterBuilder};
use tokio::sync::Semaphore;
use url::Url;

use crate::{models::EmailCsvResult, website};

const REQUIRED_COLUMNS: [&str; 10] = [
    "来源任务",
    "名称",
    "官网",
    "邮箱",
    "邮箱来源",
    "状态",
    "错误",
    "Google Maps 链接",
    "主营分类",
    "地址",
];

const GENERIC_MAILBOX_SCORE: &[(&str, i32)] = &[
    ("info", 8),
    ("contact", 8),
    ("hello", 7),
    ("support", 7),
    ("service", 6),
    ("customerservice", 6),
    ("customer.service", 6),
    ("sales", 4),
    ("office", 4),
    ("enquiries", 4),
    ("inquiries", 4),
    ("orders", 3),
];
const FREE_EMAIL_DOMAINS: &[&str] = &[
    "gmail.com",
    "googlemail.com",
    "hotmail.com",
    "outlook.com",
    "live.com",
    "icloud.com",
    "me.com",
    "yahoo.com",
    "yahoo.ca",
    "yahoo.com.au",
    "proton.me",
    "protonmail.com",
    "aol.com",
    "comcast.net",
    "cox.net",
];

fn normalized_host(raw_url: &str) -> String {
    let value = raw_url.trim();
    let value = if value.starts_with("http://") || value.starts_with("https://") {
        value.to_string()
    } else {
        format!("https://{value}")
    };
    Url::parse(&value)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_string()
}

fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect()
}

fn choose_email(
    emails: &[String],
    website: &str,
    name: &str,
    address: &str,
    category: &str,
) -> Option<String> {
    if emails.is_empty() {
        return None;
    }
    let site_host = normalized_host(website);
    let context = format!("{name} {address} {category}").to_ascii_lowercase();
    let mut site_tokens = Vec::new();
    for token in
        format!("{site_host} {context}").split(|character: char| !character.is_ascii_alphanumeric())
    {
        if token.len() >= 4 {
            site_tokens.push(token.to_string());
        }
    }
    let site_compact = compact(&site_host);
    let candidates: Vec<&String> = emails
        .iter()
        .filter(|email| {
            let Some((_, domain)) = email.split_once('@') else {
                return false;
            };
            let domain_compact = compact(domain);
            FREE_EMAIL_DOMAINS.contains(&domain)
                || domain == site_host
                || (!site_compact.is_empty()
                    && (domain_compact.contains(&site_compact)
                        || site_compact.contains(&domain_compact)))
                || site_tokens
                    .iter()
                    .any(|token| domain_compact.contains(token))
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates
        .into_iter()
        .max_by_key(|email| {
            let (local, domain) = email.split_once('@').unwrap_or_default();
            let mut score = GENERIC_MAILBOX_SCORE
                .iter()
                .find(|(mailbox, _)| *mailbox == local)
                .map(|(_, score)| *score)
                .unwrap_or(0);
            if domain == site_host || domain.trim_start_matches("www.") == site_host {
                score += 4;
            }
            if local
                .split(|character: char| !character.is_ascii_alphanumeric())
                .any(|token| token.len() >= 4 && context.contains(token))
            {
                score += 2;
            }
            if [
                "privacy",
                "dpo",
                "legal",
                "newsletter",
                "marketing",
                "career",
                "job",
            ]
            .iter()
            .any(|word| local.contains(word))
            {
                score -= 4;
            }
            (score, email.as_str())
        })
        .cloned()
}

pub async fn scrape_csv(
    csv_content: String,
    website_semaphore: Arc<Semaphore>,
) -> Result<EmailCsvResult, String> {
    if csv_content.len() > 10 * 1024 * 1024 {
        return Err("CSV 文件超过 10 MB 限制".to_string());
    }
    let csv_content = csv_content.trim_start_matches('\u{feff}');
    let mut reader = ReaderBuilder::new()
        .flexible(true)
        .from_reader(csv_content.as_bytes());
    let headers = reader
        .headers()
        .map_err(|error| format!("无法读取 CSV 表头：{error}"))?
        .clone();
    let header_names: Vec<String> = headers.iter().map(str::to_string).collect();
    for required in REQUIRED_COLUMNS {
        if !header_names.iter().any(|value| value.trim() == required) {
            return Err(format!("文件不是工具导出的 CSV，缺少“{required}”列"));
        }
    }

    let column = |name: &str| {
        header_names
            .iter()
            .position(|value| value.trim() == name)
            .ok_or_else(|| format!("CSV 缺少“{name}”列"))
    };
    let website_column = column("官网")?;
    let email_column = column("邮箱")?;
    let source_column = column("邮箱来源")?;
    let status_column = column("状态")?;
    let error_column = column("错误")?;
    let name_column = column("名称")?;
    let address_column = column("地址")?;
    let category_column = column("主营分类")?;
    let summary_column = header_names
        .iter()
        .position(|value| value.trim() == "业务摘要");

    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| format!("CSV 数据格式错误：{error}"))?;
        let mut row: Vec<String> = record.iter().map(str::to_string).collect();
        row.resize(header_names.len(), String::new());
        rows.push(row);
    }

    let mut groups: HashMap<String, (String, Vec<usize>)> = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        let raw_email = row
            .get(email_column)
            .map(String::as_str)
            .unwrap_or_default();
        let raw_website = row
            .get(website_column)
            .map(String::as_str)
            .unwrap_or_default()
            .trim();
        if raw_email.trim().is_empty() && !raw_website.is_empty() {
            let key = raw_website.to_ascii_lowercase();
            groups
                .entry(key)
                .or_insert_with(|| (raw_website.to_string(), Vec::new()))
                .1
                .push(index);
        }
    }

    let mut crawls = tokio::task::JoinSet::new();
    for (key, (raw_url, _)) in &groups {
        let key = key.clone();
        let raw_url = raw_url.clone();
        let semaphore = website_semaphore.clone();
        crawls.spawn(async move {
            let permit = semaphore
                .acquire_owned()
                .await
                .map_err(|_| "官网采集资源不可用".to_string())?;
            let result = website::crawl_website_without_facebook(&raw_url).await;
            drop(permit);
            Ok::<_, String>((key, result))
        });
    }

    let mut results = HashMap::new();
    while let Some(result) = crawls.join_next().await {
        let (key, crawl_result) =
            result.map_err(|error| format!("邮箱抓取任务异常：{error}"))??;
        results.insert(key, crawl_result);
    }

    let processed_records: usize = groups.values().map(|(_, rows)| rows.len()).sum();
    let mut records_with_email = 0;
    let mut emails_found = 0;
    let mut no_email = 0;
    let mut failed = 0;
    for (key, (_, indices)) in groups {
        let Some(result) = results.remove(&key) else {
            continue;
        };
        for index in indices {
            let row = &mut rows[index];
            let selected_email = choose_email(
                &result.emails,
                row.get(website_column)
                    .map(String::as_str)
                    .unwrap_or_default(),
                row.get(name_column).map(String::as_str).unwrap_or_default(),
                row.get(address_column)
                    .map(String::as_str)
                    .unwrap_or_default(),
                row.get(category_column)
                    .map(String::as_str)
                    .unwrap_or_default(),
            );
            if let Some(summary_column) = summary_column {
                if !result.summary.is_empty() {
                    row[summary_column] = result.summary.clone();
                }
            }
            if let Some(email) = selected_email {
                row[email_column] = email.clone();
                row[source_column] = result
                    .source_by_email
                    .get(&email)
                    .cloned()
                    .or_else(|| result.source_urls.first().cloned())
                    .unwrap_or_default();
                row[status_column] = "completed".to_string();
                row[error_column].clear();
                records_with_email += 1;
                emails_found += 1;
            } else {
                row[email_column].clear();
                row[source_column].clear();
                row[status_column] = if result.status == "failed" {
                    "failed"
                } else {
                    "no_email"
                }
                .to_string();
                row[error_column] = if result.status == "failed" {
                    result.error.clone()
                } else {
                    "已检查官网公开页面，未发现匹配的公开邮箱".to_string()
                };
                if result.status == "failed" {
                    failed += 1;
                } else {
                    no_email += 1;
                }
            }
        }
    }

    let mut output = vec![0xEF, 0xBB, 0xBF];
    {
        let mut writer = WriterBuilder::new().from_writer(&mut output);
        writer
            .write_record(&headers)
            .map_err(|error| format!("写出 CSV 表头失败：{error}"))?;
        for row in &rows {
            writer
                .write_record(row)
                .map_err(|error| format!("写出 CSV 数据失败：{error}"))?;
        }
        writer
            .flush()
            .map_err(|error| format!("写出 CSV 失败：{error}"))?;
    }
    let csv = String::from_utf8(output).map_err(|_| "生成的 CSV 编码异常".to_string())?;

    Ok(EmailCsvResult {
        csv,
        processed_records,
        records_with_email,
        emails_found,
        no_email,
        failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_non_exported_csv() {
        let result = scrape_csv(
            "name,website\nshop,https://example.org".to_string(),
            Arc::new(Semaphore::new(2)),
        )
        .await;
        assert!(result.unwrap_err().contains("文件不是工具导出的 CSV"));
    }

    #[tokio::test]
    async fn preserves_rows_that_already_have_email() {
        let csv = "来源任务,名称,主营分类,电话,地址,官网,邮箱,业务摘要,评分,评价数,Google Maps 链接,邮箱来源,状态,错误\n任务,商家,分类,,,,info@example.org,,,,,,completed,";
        let result = scrape_csv(csv.to_string(), Arc::new(Semaphore::new(2)))
            .await
            .unwrap();
        assert_eq!(result.processed_records, 0);
        assert_eq!(result.records_with_email, 0);
        assert!(result.csv.contains("info@example.org"));
    }
}
