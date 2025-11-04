use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, NaiveDateTime, Utc};
use clickhouse::Client;
use once_cell::sync::OnceCell;
use reqwest::Client as HttpClient;
use serde::Deserialize;

use std::collections::BTreeSet;

const TABLE_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS dataitem_tags
(
    dataitem_id String,
    content_type String,
    created_at   DateTime64(3, 'UTC'),
    tag_key      String,
    tag_value    String
)
ENGINE = ReplacingMergeTree(created_at)
ORDER BY (tag_key, tag_value, dataitem_id);
"#;

static CLIENT: OnceCell<Client> = OnceCell::new();
static HTTP_CLIENT: OnceCell<HttpClient> = OnceCell::new();

#[derive(Debug, Clone)]
struct ClickhouseConfig {
    url: String,
    database: String,
    user: Option<String>,
    password: Option<String>,
}

impl ClickhouseConfig {
    fn load() -> Result<Self> {
        let url = std::env::var("CLICKHOUSE_URL").context("CLICKHOUSE_URL env var not set")?;
        let database = std::env::var("CLICKHOUSE_DATABASE").unwrap();
        let user = std::env::var("CLICKHOUSE_USER").ok().filter(|v| !v.is_empty());
        let password = std::env::var("CLICKHOUSE_PASSWORD").ok().filter(|v| !v.is_empty());

        Ok(Self { url, database, user, password })
    }
}

fn client() -> Result<&'static Client> {
    CLIENT.get_or_try_init(|| {
        let cfg = ClickhouseConfig::load()?;
        let mut builder = Client::default().with_url(cfg.url).with_database(cfg.database);
        if let Some(user) = cfg.user {
            builder = builder.with_user(user);
        }
        if let Some(password) = cfg.password {
            builder = builder.with_password(password);
        }
        Ok(builder)
    })
}

fn http_client() -> Result<&'static HttpClient> {
    HTTP_CLIENT.get_or_try_init(|| HttpClient::builder().build().map_err(|err| anyhow!(err)))
}

async fn ensure_schema() -> Result<()> {
    let client = client()?;
    client.query(TABLE_DDL).execute().await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct JsonRow {
    dataitem_id: String,
    content_type: String,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct JsonResponse {
    data: Vec<JsonRow>,
}

#[derive(Debug, Clone)]
pub struct DataitemRecord {
    pub dataitem_id: String,
    pub content_type: String,
    pub created_at: DateTime<Utc>,
}

fn normalize_tags(tags: &[(String, String)]) -> Vec<(String, String)> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for (key, value) in tags {
        let key_trimmed = key.trim();
        let value_trimmed = value.trim();
        if key_trimmed.is_empty() || value_trimmed.is_empty() {
            continue;
        }
        if key_trimmed.len() > 1024 || value_trimmed.len() > 1024 {
            continue;
        }
        let key_owned = key_trimmed.to_string();
        let value_owned = value_trimmed.to_string();
        if seen.insert((key_owned.clone(), value_owned.clone())) {
            normalized.push((key_owned, value_owned));
        }
    }
    normalized
}

pub async fn index_dataitem(
    dataitem_id: &str,
    content_type: &str,
    tags: &[(String, String)],
) -> Result<()> {
    if tags.is_empty() {
        return Ok(());
    }

    ensure_schema().await?;
    let client = client()?;
    let created_at = Utc::now();
    let normalized = normalize_tags(tags);

    if normalized.is_empty() {
        return Ok(());
    }

    for (tag_key, tag_value) in normalized.iter() {
        client
            .query(
                "INSERT INTO dataitem_tags \
                 (dataitem_id, content_type, created_at, tag_key, tag_value) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(dataitem_id)
            .bind(content_type)
            .bind(created_at)
            .bind(tag_key)
            .bind(tag_value)
            .execute()
            .await
            .with_context(|| {
                format!("failed to insert tag ({tag_key}, {tag_value}) for dataitem {dataitem_id}")
            })?;
    }
    Ok(())
}

pub async fn query_dataitems_by_tags(filters: &[(String, String)]) -> Result<Vec<DataitemRecord>> {
    if filters.is_empty() {
        return Ok(Vec::new());
    }

    ensure_schema().await?;

    let normalized_filters =
        normalize_tags(&filters.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>());
    if normalized_filters.is_empty() {
        return Ok(Vec::new());
    }

    let expected = normalized_filters.len();
    let tuple_sql = normalized_filters
        .iter()
        .map(|(k, v)| format!("('{}','{}')", escape_single(k), escape_single(v)))
        .collect::<Vec<_>>()
        .join(", ");
    // took this sql query string formatting route because the clickhouse lib was acting oddly with
    // the Row binding
    let sql = format!(
        "SELECT dataitem_id,
                any(content_type) AS content_type,
                max(created_at) AS created_at
         FROM dataitem_tags
         WHERE (tag_key, tag_value) IN ({tuple_sql})
         GROUP BY dataitem_id
         HAVING countDistinct(tag_key) = {expected}
         ORDER BY created_at DESC"
    );

    let cfg = ClickhouseConfig::load()?;
    let client = http_client()?;
    let mut request = client
        .post(format!("{}/?database={}", cfg.url, cfg.database))
        .body(format!("{sql} FORMAT JSON"))
        .header("content-type", "text/plain");

    if let Some(user) = cfg.user {
        request = request.basic_auth(user, cfg.password);
    }

    let response = request.send().await.context("clickhouse HTTP query failed")?;
    let status = response.status();
    let body = response.text().await.context("failed to read clickhouse response body")?;

    if !status.is_success() {
        return Err(anyhow!("clickhouse http query failed with status {status}"));
    }

    let parsed: JsonResponse =
        serde_json::from_str(&body).context("failed to parse clickhouse json")?;

    let mut out = Vec::with_capacity(parsed.data.len());
    for row in parsed.data {
        let created_at = parse_clickhouse_datetime(&row.created_at)?;
        out.push(DataitemRecord {
            dataitem_id: row.dataitem_id,
            content_type: row.content_type,
            created_at,
        });
    }

    Ok(out)
}

fn escape_single(input: &str) -> String {
    input.replace('\'', "''")
}

fn parse_clickhouse_datetime(value: &str) -> Result<DateTime<Utc>> {
    const FORMATS: [&str; 2] = ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S"];
    for fmt in FORMATS {
        if let Ok(naive) = NaiveDateTime::parse_from_str(value, fmt) {
            return Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
    }
    Err(anyhow!("unsupported datetime format returned by clickhouse: {value}"))
}
