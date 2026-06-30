use serde::Deserialize;
use worker::{Result, SqlStorage};

pub(crate) struct Migration {
    version: i64,
    up: MigrationStep,
}

enum MigrationStep {
    Sql(&'static str),
    Rust(fn(&SqlStorage) -> Result<()>),
}

impl Migration {
    pub(crate) const fn sql(version: i64, sql: &'static str) -> Self {
        Self {
            version,
            up: MigrationStep::Sql(sql),
        }
    }

    #[allow(dead_code)]
    pub(crate) const fn rust(version: i64, up: fn(&SqlStorage) -> Result<()>) -> Self {
        Self {
            version,
            up: MigrationStep::Rust(up),
        }
    }
}

#[derive(Deserialize)]
struct SchemaVersion {
    version: i64,
}

pub(crate) fn run(sql: &SqlStorage, migrations: &[Migration]) -> Result<()> {
    ensure_table(sql)?;
    let version = current_version(sql)?;
    for migration in migrations {
        if migration.version <= version {
            continue;
        }
        match migration.up {
            MigrationStep::Sql(statement) => {
                sql.exec(statement, None)?;
            }
            MigrationStep::Rust(up) => up(sql)?,
        }
        mark_applied(sql, migration.version)?;
    }
    Ok(())
}

fn ensure_table(sql: &SqlStorage) -> Result<()> {
    sql.exec(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        )",
        None,
    )
    .map(|_| ())
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

fn mark_applied(sql: &SqlStorage, version: i64) -> Result<()> {
    sql.exec(
        "INSERT OR REPLACE INTO schema_migrations (version, applied_at)
         VALUES (?, unixepoch('now'))",
        vec![version.into()],
    )
    .map(|_| ())
}
