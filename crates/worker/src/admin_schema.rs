use worker::{Result, SqlStorage};

use crate::migration::{self, Migration};

const MIGRATIONS: &[Migration] = &[Migration::sql(1, CREATE_COUNTERS_TABLE)];

const CREATE_COUNTERS_TABLE: &str = "CREATE TABLE IF NOT EXISTS counters (
    name TEXT PRIMARY KEY,
    value INTEGER NOT NULL DEFAULT 0
)";

pub(crate) fn init_schema(sql: &SqlStorage) -> Result<()> {
    migration::run(sql, MIGRATIONS)
}
