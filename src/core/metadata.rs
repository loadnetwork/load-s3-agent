use anyhow::{Context, Result, anyhow};
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, NaiveDateTime, Utc};
use clickhouse::Client;
use once_cell::sync::OnceCell;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};

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

pub const DEFAULT_PAGE_SIZE: usize = 25;
pub const MAX_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone)]
pub struct TagQueryCursor {
    pub created_at: DateTime<Utc>,
    pub dataitem_id: String,
}

#[derive(Debug, Clone)]
pub struct TagQueryPagination {
    pub first: usize,
    pub after: Option<TagQueryCursor>,
}

#[derive(Debug, Clone)]
pub struct TagQueryPage {
    pub items: Vec<DataitemRecord>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
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

pub async fn query_dataitems_by_tags(
    filters: &[(String, String)],
    pagination: &TagQueryPagination,
) -> Result<TagQueryPage> {
    if filters.is_empty() {
        return Ok(TagQueryPage { items: Vec::new(), has_more: false, next_cursor: None });
    }

    ensure_schema().await?;

    let normalized_filters =
        normalize_tags(&filters.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>());
    if normalized_filters.is_empty() {
        return Ok(TagQueryPage { items: Vec::new(), has_more: false, next_cursor: None });
    }

    let limit = pagination.first.clamp(1, MAX_PAGE_SIZE);
    let fetch_limit = limit + 1;

    let expected = normalized_filters.len();
    let tuple_sql = normalized_filters
        .iter()
        .map(|(k, v)| format!("('{}','{}')", escape_single(k), escape_single(v)))
        .collect::<Vec<_>>()
        .join(", ");

    let created_at_condition = pagination.after.as_ref().map(|cursor| {
        let created_at_expr = format!(
            "toDateTime64('{}', 3, 'UTC')",
            cursor.created_at.format("%Y-%m-%d %H:%M:%S%.3f")
        );
        let escaped_id = escape_single(&cursor.dataitem_id);
        format!(
            "(created_at < {expr}) OR (created_at = {expr} AND dataitem_id < '{id}')",
            expr = created_at_expr,
            id = escaped_id,
        )
    });

    let base_query = format!(
        "SELECT dataitem_id,
                any(content_type) AS content_type,
                max(created_at) AS created_at
         FROM dataitem_tags
         WHERE (tag_key, tag_value) IN ({tuple_sql})
         GROUP BY dataitem_id
         HAVING countDistinct(tag_key) = {expected}"
    );

    let mut sql = format!(
        "SELECT dataitem_id, content_type, created_at
         FROM ({base_query}) AS aggregated"
    );

    if let Some(condition) = created_at_condition {
        sql.push_str(" WHERE ");
        sql.push_str(&condition);
    }

    sql.push_str(" ORDER BY created_at DESC, dataitem_id DESC");
    sql.push_str(&format!(" LIMIT {fetch_limit}"));

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

    let has_more = out.len() > limit;
    if has_more {
        out.truncate(limit);
    }

    let next_cursor = if has_more {
        out.last()
            .map(|record| encode_tag_query_cursor(record))
            .transpose()?
    } else {
        None
    };

    Ok(TagQueryPage { items: out, has_more, next_cursor })
}

#[derive(Serialize, Deserialize)]
struct CursorPayload {
    created_at: String,
    dataitem_id: String,
}

pub fn decode_tag_query_cursor(encoded: &str) -> Result<TagQueryCursor> {
    let raw = general_purpose::STANDARD_NO_PAD
        .decode(encoded)
        .context("invalid pagination cursor encoding")?;
    let payload: CursorPayload =
        serde_json::from_slice(&raw).context("invalid pagination cursor payload")?;
    let created_at = DateTime::parse_from_rfc3339(&payload.created_at)
        .context("invalid pagination cursor timestamp")?
        .with_timezone(&Utc);
    Ok(TagQueryCursor { created_at, dataitem_id: payload.dataitem_id })
}

fn encode_tag_query_cursor(record: &DataitemRecord) -> Result<String> {
    let payload = CursorPayload {
        created_at: record.created_at.to_rfc3339(),
        dataitem_id: record.dataitem_id.clone(),
    };
    let raw = serde_json::to_vec(&payload).context("failed to encode pagination cursor")?;
    Ok(general_purpose::STANDARD_NO_PAD.encode(raw))
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
