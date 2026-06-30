use worker::{Result, SqlStorage};

use crate::migration::{self, Migration};

const MIGRATIONS: &[Migration] = &[Migration::sql(1, CREATE_MESSAGES_TABLE)];

const CREATE_MESSAGES_TABLE: &str = "CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    from_addr TEXT NOT NULL,
    from_name TEXT NOT NULL,
    subject TEXT NOT NULL,
    preview TEXT NOT NULL,
    html TEXT NOT NULL,
    text TEXT NOT NULL,
    attachments TEXT NOT NULL,
    received_at INTEGER NOT NULL,
    read INTEGER NOT NULL DEFAULT 0
)";

pub(crate) fn init_schema(sql: &SqlStorage) -> Result<()> {
    migration::run(sql, MIGRATIONS)
}
