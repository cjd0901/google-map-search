use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::IpAddr,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use regex::Regex;
use reqwest::{header, Client, StatusCode};
use scraper::{Html, Selector};
use url::Url;

use crate::models::WebsiteResult;

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_PAGES: usize = 7;
const HOST_REQUEST_DELAY: Duration = Duration::from_secs(1);

static HOST_LAST_REQUEST_AT: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

const ASSET_EXTENSIONS: &[&str] = &[
    "avif", "bmp", "css", "gif", "ico", "jpeg", "jpg", "js", "json", "map", "png", "svg", "tif",
    "tiff", "webp", "woff", "woff2",
];
const BLOCKED_EMAIL_DOMAINS: &[&str] = &[
    "example.com",
    "example.org",
    "example.net",
    "sentry.io",
    "hugedomains.com",
    "sedo.com",
    "godaddy.com",
    "namecheap.com",
    "domain.com",
    "parkingcrew.net",
    "mystore.com",
    "yourdomain.com",
    "email.com",
    "myemail.com",
    "site.com",
    "maestra.io",
    "shopify.com",
    "wix.com",
    "wixsite.com",
    "squarespace.com",
    "klaviyo.com",
    "mailchimp.com",
    "zendesk.com",
    "intercom.io",
    "getresponse.com",
    "cloudflare.com",
    "google.com",
    "facebook.com",
    "instagram.com",
    "tiktok.com",
    "whatnot.com",
    "wolt.com",
    "doordash.com",
    "ubereats.com",
    "grubhub.com",
    "yelp.com",
    "tripadvisor.com",
    "ivof.com",
];
const PLACEHOLDER_EMAIL_LOCALS: &[&str] = &[
    "john.doe",
    "jane.doe",
    "johnsmith",
    "janesmith",
    "test",
    "test1",
    "example",
    "user",
    "username",
    "yourname",
    "jean.dupont",
    "firstname.lastname",
    "name.surname",
];

async fn throttle_host(url: &Url) {
    let host = normalized_host(url);
    let now = Instant::now();
    let schedule = HOST_LAST_REQUEST_AT.get_or_init(|| Mutex::new(HashMap::new()));
    let wait = if let Ok(mut last_requests) = schedule.lock() {
        let next = last_requests
            .get(&host)
            .copied()
            .map(|last| last + HOST_REQUEST_DELAY)
            .unwrap_or(now);
        let scheduled = next.max(now);
        last_requests.insert(host, scheduled);
        scheduled.saturating_duration_since(now)
    } else {
        Duration::ZERO
    };
    if !wait.is_zero() {
        tokio::time::sleep(wait).await;
    }
}

pub async fn crawl_website(raw_url: &str) -> WebsiteResult {
    crawl_website_with_options(raw_url, true).await
}

pub async fn crawl_website_without_facebook(raw_url: &str) -> WebsiteResult {
    crawl_website_with_options(raw_url, false).await
}

async fn crawl_website_with_options(raw_url: &str, collect_facebook_links: bool) -> WebsiteResult {
    let mut result = WebsiteResult {
        status: "crawling".to_string(),
        ..WebsiteResult::default()
    };

    let start_url = match normalize_url(raw_url) {
        Ok(url) => url,
        Err(error) => return failed_result(error),
    };

    let client = match Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15))
        .user_agent("BusinessContactResearchTool/0.1 (+local desktop research)")
        .build()
    {
        Ok(client) => client,
        Err(error) => return failed_result(error.to_string()),
    };

    if let Err(error) = validate_public_url(&start_url).await {
        return failed_result(error);
    }

    let mut queue = VecDeque::from([(start_url.clone(), true)]);
    let mut robots_cache: HashMap<String, Option<String>> = HashMap::new();
    let mut visited = HashSet::new();
    let mut emails = HashSet::new();
    let mut sources = HashSet::new();
    let mut source_by_email = HashMap::new();
    let mut facebook_urls = Vec::new();
    let mut summary = String::new();

    while let Some((url, is_home)) = queue.pop_front() {
        if visited.len() >= MAX_PAGES {
            break;
        }
        let normalized = url.as_str().trim_end_matches('/').to_string();
        if !visited.insert(normalized) {
            continue;
        }
        let robots_key = normalized_host(&url);
        let robots = if let Some(cached) = robots_cache.get(&robots_key) {
            cached.clone()
        } else {
            let fetched = fetch_robots(&client, &url).await;
            robots_cache.insert(robots_key, fetched.clone());
            fetched
        };
        if !robots_allows(robots.as_deref(), url.path()) {
            continue;
        }

        match fetch_html(&client, url.clone()).await {
            Ok((final_url, html)) => {
                let page_emails = extract_emails(&html);
                if !page_emails.is_empty() {
                    let source_url = final_url.to_string();
                    sources.insert(source_url.clone());
                    for email in &page_emails {
                        source_by_email
                            .entry(email.clone())
                            .or_insert_with(|| source_url.clone());
                    }
                    emails.extend(page_emails);
                }
                if summary.is_empty() {
                    summary = extract_summary(&html);
                }
                let (contact_links, facebook_links) = discover_priority_links(&html, &final_url);
                if collect_facebook_links {
                    facebook_urls.extend(facebook_links.into_iter().map(|url| url.to_string()));
                }
                if is_home {
                    queue.extend(contact_links.into_iter().map(|link| (link, false)));
                }
            }
            Err(error) => {
                if visited.len() == 1 {
                    result.error = error;
                }
            }
        }
    }

    let mut email_list: Vec<_> = emails.into_iter().collect();
    email_list.sort();
    let mut source_list: Vec<_> = sources.into_iter().collect();
    source_list.sort();
    facebook_urls.sort();
    facebook_urls.dedup();

    result.summary = summary;
    result.emails = email_list;
    result.source_urls = source_list;
    result.source_by_email = source_by_email;
    result.facebook_urls = facebook_urls;
    result.status = if result.emails.is_empty() {
        if result.error.is_empty() {
            "no_email".to_string()
        } else {
            "failed".to_string()
        }
    } else {
        "completed".to_string()
    };
    result
}

fn failed_result(error: String) -> WebsiteResult {
    WebsiteResult {
        status: "failed".to_string(),
        error,
        ..WebsiteResult::default()
    }
}

fn normalize_url(raw_url: &str) -> Result<Url, String> {
    let value = raw_url.trim();
    if value.is_empty() {
        return Err("官网为空".to_string());
    }
    let lower_value = value.to_ascii_lowercase();
    let with_scheme = if lower_value.starts_with("http://") || lower_value.starts_with("https://") {
        value.to_string()
    } else {
        format!("https://{value}")
    };
    let url = Url::parse(&with_scheme).map_err(|_| "官网地址无效".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("只允许访问 HTTP/HTTPS 官网".to_string());
    }
    Ok(url)
}

async fn validate_public_url(url: &Url) -> Result<(), String> {
    let host = url.host_str().ok_or_else(|| "官网缺少主机名".to_string())?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("官网地址包含不允许的登录信息".to_string());
    }
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".local") {
        return Err("禁止访问本机或局域网地址".to_string());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| "官网域名解析失败".to_string())?;
    for address in addresses {
        if is_private_ip(address.ip()) {
            return Err("禁止访问本机、内网或保留地址".to_string());
        }
    }
    Ok(())
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_multicast()
                || ip.octets()[0] >= 240
                || ip.is_documentation()
                || ip.is_unspecified()
                || ip.octets()[0] == 0
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
                || ip.segments()[0] & 0xfe00 == 0xfc00
        }
    }
}

async fn fetch_html(client: &Client, mut url: Url) -> Result<(Url, String), String> {
    for _ in 0..=3 {
        validate_public_url(&url).await?;
        throttle_host(&url).await;
        let response = client
            .get(url.clone())
            .header(header::ACCEPT, "text/html,application/xhtml+xml")
            .send()
            .await
            .map_err(|error| format!("访问官网失败：{error}"))?;

        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| "官网返回了无效重定向".to_string())?;
            url = url
                .join(location)
                .map_err(|_| "官网重定向地址无效".to_string())?;
            continue;
        }
        if !response.status().is_success() {
            return Err(format!("官网返回 HTTP {}", response.status().as_u16()));
        }
        if let Some(content_type) = response.headers().get(header::CONTENT_TYPE) {
            let content_type = content_type
                .to_str()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !content_type.contains("text/html") && !content_type.contains("application/xhtml") {
                return Err("官网首页不是 HTML 页面".to_string());
            }
        }
        if response.content_length().unwrap_or(0) > MAX_RESPONSE_BYTES as u64 {
            return Err("官网页面超过 2 MB 限制".to_string());
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("读取官网失败：{error}"))?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err("官网页面超过 2 MB 限制".to_string());
        }
        return Ok((url, String::from_utf8_lossy(&bytes).to_string()));
    }
    Err("官网重定向次数过多".to_string())
}

async fn fetch_robots(client: &Client, base_url: &Url) -> Option<String> {
    let mut robots_url = base_url.clone();
    robots_url.set_path("/robots.txt");
    robots_url.set_query(None);
    robots_url.set_fragment(None);
    if validate_public_url(&robots_url).await.is_err() {
        return None;
    }
    throttle_host(&robots_url).await;
    let response = client.get(robots_url).send().await.ok()?;
    if response.status() != StatusCode::OK || response.content_length().unwrap_or(0) > 256 * 1024 {
        return None;
    }
    response.text().await.ok()
}

fn robots_allows(content: Option<&str>, path: &str) -> bool {
    let Some(content) = content else { return true };
    let mut relevant_group = false;
    let mut best: Option<(usize, bool)> = None;
    for raw_line in content.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        if key == "user-agent" {
            relevant_group =
                value == "*" || value.eq_ignore_ascii_case("BusinessContactResearchTool");
            continue;
        }
        if relevant_group
            && (key == "allow" || key == "disallow")
            && !value.is_empty()
            && path.starts_with(value)
        {
            let allowed = key == "allow";
            if best
                .map(|(length, _)| value.len() >= length)
                .unwrap_or(true)
            {
                best = Some((value.len(), allowed));
            }
        }
    }
    best.map(|(_, allowed)| allowed).unwrap_or(true)
}

fn extract_emails(html: &str) -> HashSet<String> {
    static BLOCK_REGEX: OnceLock<Regex> = OnceLock::new();
    let block_regex = BLOCK_REGEX.get_or_init(|| {
        Regex::new(
            r"(?is)<(?:script|style|noscript|template|svg)\b[^>]*>.*?</(?:script|style|noscript|template|svg)\s*>",
        )
        .unwrap()
    });
    let sanitized_html = block_regex.replace_all(html, " ");
    let document = Html::parse_document(&sanitized_html);
    let body_selector = Selector::parse("body").unwrap();
    let mut searchable = document
        .select(&body_selector)
        .next()
        .map(|body| body.text().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| sanitized_html.to_string());

    let link_document = Html::parse_document(html);
    let link_selector = Selector::parse("a[href]").unwrap();
    for link in link_document.select(&link_selector) {
        if let Some(href) = link.value().attr("href") {
            if href
                .get(..7)
                .map(|value| value.eq_ignore_ascii_case("mailto:"))
                == Some(true)
            {
                let mailbox = &href[7..];
                searchable.push(' ');
                searchable.push_str(mailbox.split('?').next().unwrap_or_default());
            }
        }
    }

    let image_selector = Selector::parse("img").unwrap();
    for image in link_document.select(&image_selector) {
        for attribute in ["alt", "title", "aria-label"] {
            if let Some(value) = image.value().attr(attribute) {
                searchable.push(' ');
                searchable.push_str(value);
            }
        }
    }

    let cloudflare_selector = Selector::parse("[data-cfemail]").unwrap();
    for node in link_document.select(&cloudflare_selector) {
        if let Some(encoded) = node.value().attr("data-cfemail") {
            if let Some(decoded) = decode_cloudflare_email(encoded) {
                searchable.push(' ');
                searchable.push_str(&decoded);
            }
        }
    }

    collect_valid_emails(&searchable)
}

fn decode_cloudflare_email(encoded: &str) -> Option<String> {
    let encoded = encoded.trim();
    if encoded.len() < 4
        || !encoded.len().is_multiple_of(2)
        || !encoded.as_bytes().iter().all(u8::is_ascii_hexdigit)
    {
        return None;
    }
    let key = u8::from_str_radix(&encoded[..2], 16).ok()?;
    let mut bytes = Vec::with_capacity(encoded.len() / 2 - 1);
    for index in (2..encoded.len()).step_by(2) {
        let value = u8::from_str_radix(&encoded[index..index + 2], 16).ok()?;
        bytes.push(value ^ key);
    }
    String::from_utf8(bytes).ok()
}

fn collect_valid_emails(value: &str) -> HashSet<String> {
    static ASSET_URL_REGEX: OnceLock<Regex> = OnceLock::new();
    static EMAIL_REGEX: OnceLock<Regex> = OnceLock::new();
    static AT_PATTERN: OnceLock<Regex> = OnceLock::new();
    static DOT_PATTERN: OnceLock<Regex> = OnceLock::new();
    static ROT13_HINT: OnceLock<Regex> = OnceLock::new();
    let asset_url_regex = ASSET_URL_REGEX.get_or_init(|| {
        Regex::new(
            r#"(?i)(?://|https?://)[^\s\"'<>]*?\.(?:avif|bmp|css|gif|ico|jpe?g|js|json|map|png|svg|tiff?|webp|woff2?)"#,
        )
        .unwrap()
    });
    let value = asset_url_regex.replace_all(value, " ");
    let email_regex = EMAIL_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)[a-z0-9][a-z0-9.!#$%&'*+/=?^_`{|}~-]{0,63}@[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)+",
        )
        .unwrap()
    });
    let at_pattern = AT_PATTERN
        .get_or_init(|| Regex::new(r"(?i)\s*(?:\[at\]|\(at\)|\{at\}|\s+at\s+)\s*").unwrap());
    let dot_pattern = DOT_PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\s*(?:\[dot\]|\(dot\)|\{dot\}|\s+dot\s+|\s*·\s*)\s*").unwrap()
    });
    let rot13_hint = ROT13_HINT.get_or_init(|| {
        Regex::new(r#"(?i)(?:@[^\s<>"']+\.pbz\b|\b(?:vasb|pbagnpg|fnyrf|fgnss)@)"#).unwrap()
    });
    let deobfuscated = deobfuscate_email_text(&value, at_pattern, dot_pattern);
    let mut candidates = vec![deobfuscated.clone()];
    if rot13_hint.is_match(&deobfuscated) {
        candidates.push(rot13(&deobfuscated));
    }

    candidates
        .into_iter()
        .flat_map(|candidate| {
            email_regex
                .find_iter(&candidate)
                .map(|item| {
                    item.as_str()
                        .trim_matches(|character: char| {
                            " .;,，。:：()[]{}<>\"'".contains(character)
                        })
                        .to_ascii_lowercase()
                })
                .collect::<Vec<_>>()
        })
        .filter(|email| is_valid_public_email(email))
        .collect()
}

fn is_valid_public_email(email: &str) -> bool {
    if email.len() > 254 || email.contains("..") || email.contains(['/', '\\']) {
        return false;
    }
    let Some((local, domain)) = email.rsplit_once('@') else {
        return false;
    };
    if local.is_empty() || local.len() > 64 || domain.is_empty() || !domain.contains('.') {
        return false;
    }
    let top_level_domain = domain.rsplit('.').next().unwrap_or_default();
    if ASSET_EXTENSIONS.contains(&top_level_domain)
        || top_level_domain.len() < 2
        || !top_level_domain
            .chars()
            .any(|character| character.is_ascii_alphabetic())
    {
        return false;
    }
    if BLOCKED_EMAIL_DOMAINS.contains(&domain) || PLACEHOLDER_EMAIL_LOCALS.contains(&local) {
        return false;
    }
    static PLACEHOLDER_DOMAIN_REGEX: OnceLock<Regex> = OnceLock::new();
    static ASSET_LOCAL_REGEX: OnceLock<Regex> = OnceLock::new();
    if (local == "email" || local == "mail")
        && PLACEHOLDER_DOMAIN_REGEX
            .get_or_init(|| {
                Regex::new(r"(?i)(?:my|fake|test|sample|your|no)?(?:email|mail)").unwrap()
            })
            .is_match(domain)
    {
        return false;
    }
    if ASSET_LOCAL_REGEX
        .get_or_init(|| Regex::new(r"(?i)(?:@2x|\d+x|placeholder|image|photo|asset)[._-]").unwrap())
        .is_match(local)
    {
        return false;
    }
    true
}

fn deobfuscate_email_text(value: &str, at_pattern: &Regex, dot_pattern: &Regex) -> String {
    let deobfuscated = at_pattern.replace_all(value, "@");
    dot_pattern.replace_all(&deobfuscated, ".").to_string()
}

fn rot13(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            'a'..='m' | 'A'..='M' => char::from_u32(character as u32 + 13).unwrap_or(character),
            'n'..='z' | 'N'..='Z' => char::from_u32(character as u32 - 13).unwrap_or(character),
            _ => character,
        })
        .collect()
}

pub fn filter_email_list(values: Vec<String>) -> Vec<String> {
    let mut emails = HashSet::new();
    for value in values {
        emails.extend(collect_valid_emails(&value));
    }
    let mut emails: Vec<_> = emails.into_iter().collect();
    emails.sort();
    emails
}

fn extract_summary(html: &str) -> String {
    let document = Html::parse_document(html);
    let meta_selector =
        Selector::parse("meta[name='description'], meta[property='og:description']").unwrap();
    for node in document.select(&meta_selector) {
        if let Some(value) = node.value().attr("content") {
            let cleaned = clean_text(value);
            if cleaned.len() >= 20 {
                return truncate(&cleaned, 360);
            }
        }
    }
    let content_selector = Selector::parse("h1, main p, article p, body p").unwrap();
    for node in document.select(&content_selector) {
        let cleaned = clean_text(&node.text().collect::<Vec<_>>().join(" "));
        if cleaned.len() >= 40 {
            return truncate(&cleaned, 360);
        }
    }
    String::new()
}

fn discover_priority_links(html: &str, base_url: &Url) -> (Vec<Url>, Vec<Url>) {
    let document = Html::parse_document(html);
    let selector = Selector::parse("a[href]").unwrap();
    let button_selector =
        Selector::parse("button, [role='button'], input[type='button'], input[type='submit']")
            .unwrap();
    static BUTTON_URL_REGEX: OnceLock<Regex> = OnceLock::new();
    let button_url_regex = BUTTON_URL_REGEX.get_or_init(|| {
        Regex::new(
            r#"(?i)(?:location(?:\.href)?|window\.open|(?:data-)?(?:href|url)|formaction)\s*(?:=|\(|:)\s*[\"']([^\"']+)[\"']"#,
        )
        .unwrap()
    });
    let direct_contact_keywords = ["contact", "impressum", "kontakt", "联系"];
    let secondary_keywords = ["about", "legal", "team", "company", "关于", "团队"];
    let base_host = normalized_host(base_url);
    let mut contact_links = Vec::new();
    let mut secondary_links = Vec::new();
    let mut facebook_links = Vec::new();
    let mut seen_contacts = HashSet::new();
    let mut seen_facebook = HashSet::new();
    let mut actions = Vec::new();
    for node in document.select(&selector) {
        actions.push((
            node.value().attr("href").unwrap_or_default().to_string(),
            clean_text(&node.text().collect::<Vec<_>>().join(" ")),
        ));
    }
    for node in document.select(&button_selector) {
        let value = node.value();
        let href = value
            .attr("formaction")
            .or_else(|| value.attr("data-href"))
            .or_else(|| value.attr("data-url"))
            .or_else(|| value.attr("href"))
            .or_else(|| {
                value
                    .attr("onclick")
                    .and_then(|onclick| button_url_regex.captures(onclick))
                    .and_then(|captures| captures.get(1))
                    .map(|capture| capture.as_str())
            })
            .unwrap_or_default();
        let label = value
            .attr("value")
            .map(str::to_string)
            .unwrap_or_else(|| clean_text(&node.text().collect::<Vec<_>>().join(" ")));
        actions.push((href.to_string(), label));
    }
    for (href, label) in actions {
        let text = label.to_ascii_lowercase();
        let target = format!("{} {}", href.to_ascii_lowercase(), text);
        if let Ok(mut link) = base_url.join(&href) {
            link.set_fragment(None);
            if !matches!(link.scheme(), "http" | "https") {
                continue;
            }
            if is_facebook_url(&link) {
                if seen_facebook.insert(link.to_string()) {
                    facebook_links.push(link);
                }
            } else if normalized_host(&link) == base_host {
                if direct_contact_keywords
                    .iter()
                    .any(|keyword| target.contains(keyword))
                    && seen_contacts.insert(link.to_string())
                {
                    contact_links.push(link);
                } else if secondary_keywords
                    .iter()
                    .any(|keyword| target.contains(keyword))
                    && seen_contacts.insert(link.to_string())
                {
                    secondary_links.push(link);
                }
            }
        }
    }
    let base = base_url.clone();
    for path in [
        "/contact-us",
        "/contact",
        "/contacto",
        "/kontakt",
        "/impressum",
        "/联系我们",
    ] {
        let mut link = base.clone();
        link.set_path(path);
        link.set_query(None);
        link.set_fragment(None);
        if seen_contacts.insert(link.to_string()) {
            contact_links.push(link);
        }
    }
    contact_links.extend(secondary_links);
    (
        contact_links.into_iter().take(MAX_PAGES - 1).collect(),
        facebook_links.into_iter().take(2).collect(),
    )
}

fn is_facebook_url(url: &Url) -> bool {
    let host = normalized_host(url);
    host == "facebook.com" || host.ends_with(".facebook.com") || host == "fb.com"
}

fn normalized_host(url: &Url) -> String {
    url.host_str()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .trim_start_matches("www.")
        .to_string()
}

fn clean_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut output: String = value.chars().take(max_chars).collect();
    output.push('…');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_and_normalizes_emails() {
        let emails = extract_emails("Mail INFO@Demo.ORG or sales [at] shop [dot] com");
        assert!(emails.contains("info@demo.org"));
        assert!(emails.contains("sales@shop.com"));
    }

    #[test]
    fn ignores_shopify_asset_urls_and_keeps_contact_email() {
        let html = r#"
            <html><body>
              <img src="//cdn.shopify.com/files/product_235x235@2x.png">
              <style>.hero { background: url('photo_1280x@2x.jpg'); }</style>
              <script>const image = '//cdn.shopify.com/image@2x.webp';</script>
              <a href="mailto:brooklynpetsupply11209@gmail.com">联系我们</a>
            </body></html>
        "#;
        let emails = extract_emails(html);
        assert_eq!(
            emails,
            HashSet::from(["brooklynpetsupply11209@gmail.com".to_string()])
        );
    }

    #[test]
    fn handles_rot13_and_filters_placeholder_addresses() {
        let emails = extract_emails("vasb@qrzb.bet and test@example.com");
        assert!(emails.contains("info@demo.org"));
        assert!(!emails.contains("test@example.com"));
    }

    #[test]
    fn decodes_cloudflare_protected_emails() {
        let html = r#"
            <a href="/cdn-cgi/l/email-protection" class="__cf_email__"
               data-cfemail="5a292a2833343d323336361a2a3f2e3574393537743b2f">
              [email&nbsp;protected]
            </a>
        "#;
        assert_eq!(
            extract_emails(html),
            HashSet::from(["springhill@peto.com.au".to_string()])
        );
    }

    #[test]
    fn cleans_previously_saved_concatenated_values() {
        let old_value = "//cdn.shopify.com/product_235x235@2x.jpg//cdn.shopify.com/file_1280x@2x.pngbrooklynpetsupply11209@gmail.com";
        assert_eq!(
            filter_email_list(vec![old_value.to_string()]),
            vec!["brooklynpetsupply11209@gmail.com".to_string()]
        );
    }

    #[test]
    fn applies_longest_robots_rule() {
        let robots = "User-agent: *\nDisallow: /private\nAllow: /private/contact";
        assert!(!robots_allows(Some(robots), "/private/list"));
        assert!(robots_allows(Some(robots), "/private/contact"));
    }

    #[test]
    fn prioritizes_contact_pages_and_collects_facebook_links() {
        let html = r#"
            <a href="/about">About</a>
            <a href="https://www.facebook.com/example">Facebook</a>
            <a href="/contact-us">Contact us</a>
        "#;
        let base = Url::parse("https://example.org/").unwrap();
        let (contacts, facebook) = discover_priority_links(html, &base);
        assert_eq!(contacts[0].path(), "/contact-us");
        assert!(facebook.iter().any(is_facebook_url));
    }

    #[test]
    fn rejects_private_addresses() {
        assert!(is_private_ip("127.0.0.1".parse().unwrap()));
        assert!(is_private_ip("192.168.1.2".parse().unwrap()));
        assert!(!is_private_ip("8.8.8.8".parse().unwrap()));
    }
}
