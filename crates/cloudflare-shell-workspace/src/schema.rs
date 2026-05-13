//! SQL schema for the Workspace table. Columns + constraints match
//! @cloudflare/shell's filesystem.ts so a Workspace populated by the JS
//! package is readable by this port and vice versa.

pub const DEFAULT_NAMESPACE: &str = "default";

/// CREATE TABLE IF NOT EXISTS for the workspace table. The table name
/// is parameterised on the namespace (`cf_workspace_<ns>`).
pub fn create_table_sql(table: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {table} (\
            path            TEXT PRIMARY KEY, \
            parent_path     TEXT NOT NULL, \
            name            TEXT NOT NULL, \
            type            TEXT NOT NULL CHECK(type IN ('file','directory','symlink')), \
            mime_type       TEXT NOT NULL DEFAULT 'text/plain', \
            size            INTEGER NOT NULL DEFAULT 0, \
            storage_backend TEXT NOT NULL DEFAULT 'inline' \
                            CHECK(storage_backend IN ('inline','r2')), \
            r2_key          TEXT, \
            target          TEXT, \
            content_encoding TEXT NOT NULL DEFAULT 'utf8', \
            content         TEXT, \
            created_at      INTEGER NOT NULL DEFAULT (unixepoch()), \
            modified_at     INTEGER NOT NULL DEFAULT (unixepoch()) \
        )"
    )
}

/// CREATE INDEX IF NOT EXISTS on parent_path so read_dir scales.
pub fn create_index_sql(index: &str, table: &str) -> String {
    format!("CREATE INDEX IF NOT EXISTS {index} ON {table} (parent_path)")
}
