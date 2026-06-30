use worker::{Result, SqlStorage};

use crate::migration::{self, Migration};

const MIGRATIONS: &[Migration] = &[
    Migration::sql(1, CREATE_COUNTERS_TABLE),
    Migration::sql(2, RENAME_MAILBOX_COUNTER),
];

const CREATE_COUNTERS_TABLE: &str = "CREATE TABLE IF NOT EXISTS counters (
    name TEXT PRIMARY KEY,
    value INTEGER NOT NULL DEFAULT 0
)";

const RENAME_MAILBOX_COUNTER: &str = "UPDATE counters
    SET name = 'total_mailboxes_created'
    WHERE name = 'total_mailboxes'
      AND NOT EXISTS (
        SELECT 1 FROM counters WHERE name = 'total_mailboxes_created'
      )";

pub(crate) fn init_schema(sql: &SqlStorage) -> Result<()> {
    migration::run(sql, MIGRATIONS)
}
