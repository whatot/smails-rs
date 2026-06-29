use std::collections::HashSet;

use serde::Deserialize;
use worker::{Result, SqlStorage};

const CURRENT_SCHEMA_VERSION: i64 = 1;
const MESSAGES_TABLE: &str = "messages";
const LEGACY_MESSAGES_TABLE: &str = "messages_legacy_v0";
const NEXT_MESSAGES_TABLE: &str = "messages_v1";

const CREATE_MESSAGES: &str = "CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    from_addr TEXT NOT NULL,
    from_name TEXT NOT NULL,
    subject TEXT NOT NULL,
    preview TEXT NOT NULL,
    raw TEXT NOT NULL,
    received_at INTEGER NOT NULL,
    read INTEGER NOT NULL DEFAULT 0
)";

#[derive(Deserialize)]
struct SchemaVersion {
    version: i64,
}

#[derive(Deserialize)]
struct ColumnInfo {
    name: String,
}

pub(crate) fn init_schema(sql: &SqlStorage) -> Result<()> {
    sql.exec(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        )",
        None,
    )?;

    if current_version(sql)? < CURRENT_SCHEMA_VERSION {
        migrate_v1(sql)?;
        mark_applied(sql, CURRENT_SCHEMA_VERSION)?;
    } else if table_columns(sql, MESSAGES_TABLE)?.is_empty() {
        sql.exec(CREATE_MESSAGES, None)?;
    }

    Ok(())
}

fn current_version(sql: &SqlStorage) -> Result<i64> {
    let rows = sql
        .exec(
            "SELECT version FROM schema_migrations ORDER BY version DESC LIMIT 1",
            None,
        )?
        .to_array::<SchemaVersion>()?;
    Ok(rows.first().map(|row| row.version).unwrap_or_default())
}

fn migrate_v1(sql: &SqlStorage) -> Result<()> {
    let columns = table_columns(sql, MESSAGES_TABLE)?;
    if columns.is_empty() {
        let legacy_columns = table_columns(sql, LEGACY_MESSAGES_TABLE)?;
        if !legacy_columns.is_empty() {
            rebuild_messages(sql, LEGACY_MESSAGES_TABLE, &legacy_columns, false)?;
            return Ok(());
        }
        sql.exec(CREATE_MESSAGES, None)?;
        return Ok(());
    }

    rebuild_messages(sql, MESSAGES_TABLE, &columns, true)
}

fn rebuild_messages(
    sql: &SqlStorage,
    source_table: &str,
    columns: &HashSet<String>,
    rename_current: bool,
) -> Result<()> {
    sql.exec(
        format!("DROP TABLE IF EXISTS {NEXT_MESSAGES_TABLE}").as_str(),
        None,
    )?;
    sql.exec(
        CREATE_MESSAGES
            .replace(MESSAGES_TABLE, NEXT_MESSAGES_TABLE)
            .as_str(),
        None,
    )?;
    sql.exec(copy_messages_sql(columns, source_table).as_str(), None)?;
    if rename_current {
        sql.exec(
            format!("DROP TABLE IF EXISTS {LEGACY_MESSAGES_TABLE}").as_str(),
            None,
        )?;
        sql.exec(
            format!("ALTER TABLE {MESSAGES_TABLE} RENAME TO {LEGACY_MESSAGES_TABLE}").as_str(),
            None,
        )?;
    }
    sql.exec(
        format!("ALTER TABLE {NEXT_MESSAGES_TABLE} RENAME TO {MESSAGES_TABLE}").as_str(),
        None,
    )?;
    sql.exec(
        format!("DROP TABLE IF EXISTS {LEGACY_MESSAGES_TABLE}").as_str(),
        None,
    )?;
    Ok(())
}

fn mark_applied(sql: &SqlStorage, version: i64) -> Result<()> {
    sql.exec(
        "INSERT OR REPLACE INTO schema_migrations (version, applied_at)
         VALUES (?, unixepoch('now'))",
        vec![version.into()],
    )?;
    Ok(())
}

fn table_columns(sql: &SqlStorage, table: &str) -> Result<HashSet<String>> {
    let columns = sql
        .exec(format!("PRAGMA table_info({table})").as_str(), None)?
        .to_array::<ColumnInfo>()?;
    Ok(columns.into_iter().map(|column| column.name).collect())
}

fn copy_messages_sql(columns: &HashSet<String>, source_table: &str) -> String {
    format!(
        "INSERT INTO {NEXT_MESSAGES_TABLE} (id, from_addr, from_name, subject, preview, raw, received_at, read)
         SELECT {}, {}, {}, {}, {}, {}, {}, {} FROM {source_table}",
        expr(columns, "id", "lower(hex(randomblob(16)))"),
        expr(columns, "from_addr", "''"),
        expr(columns, "from_name", "''"),
        expr(columns, "subject", "'(no subject)'"),
        expr(columns, "preview", "''"),
        raw_expr(columns),
        expr(columns, "received_at", "0"),
        expr(columns, "read", "0"),
    )
}

fn expr(columns: &HashSet<String>, column: &str, default: &str) -> String {
    if columns.contains(column) {
        format!("COALESCE({column}, {default})")
    } else {
        default.to_owned()
    }
}

fn raw_expr(columns: &HashSet<String>) -> String {
    if columns.contains("raw") {
        "COALESCE(raw, '')".to_owned()
    } else if columns.contains("body") {
        "COALESCE(body, '')".to_owned()
    } else {
        "''".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_sql_preserves_old_body_column_as_raw() {
        let columns = ["id", "body", "received_at"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let sql = copy_messages_sql(&columns, "messages_legacy_v0");

        assert!(sql.contains("COALESCE(body, '')"));
        assert!(sql.contains("COALESCE(received_at, 0)"));
        assert!(sql.contains("SELECT COALESCE(id, lower(hex(randomblob(16))))"));
        assert!(sql.contains("FROM messages_legacy_v0"));
    }
}
