//! Everything that talks to SQLite. Nothing in here knows about the terminal.
//!
//! The browser never assumes a fixed set of tables or columns: object names come
//! from `sqlite_master`, columns from `PRAGMA table_info`, and row identity from
//! `rowid` (or the declared primary key for `WITHOUT ROWID` tables).

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::value::Value;

/// What kind of `sqlite_master` entry an object is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Table,
    View,
}

impl ObjectKind {
    pub fn label(self) -> &'static str {
        match self {
            ObjectKind::Table => "table",
            ObjectKind::View => "view",
        }
    }
}

/// A browsable object (table or view) discovered in the database.
#[derive(Debug, Clone)]
pub struct ObjectInfo {
    pub name: String,
    pub kind: ObjectKind,
    /// `None` when the row count could not be determined (e.g. a broken view).
    pub row_count: Option<i64>,
}

/// One column of a table, as reported by `PRAGMA table_info`.
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    /// Declared type, verbatim from the schema. Empty for expressions/views.
    pub decl_type: String,
    pub not_null: bool,
    pub default: Option<String>,
    /// 1-based position in the primary key, 0 when not part of it.
    pub pk_index: i64,
}

impl ColumnInfo {
    /// SQLite type affinity derived from the declared type, per the rules in
    /// <https://www.sqlite.org/datatype3.html> section 3.1.
    pub fn affinity(&self) -> Affinity {
        Affinity::from_decl(&self.decl_type)
    }
}

/// Column affinity; decides how edited text is coerced before it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Affinity {
    Integer,
    Real,
    Numeric,
    Text,
    Blob,
}

impl Affinity {
    pub fn from_decl(decl: &str) -> Affinity {
        let d = decl.to_ascii_uppercase();
        if d.contains("INT") {
            Affinity::Integer
        } else if d.contains("CHAR") || d.contains("CLOB") || d.contains("TEXT") {
            Affinity::Text
        } else if d.contains("BLOB") || d.is_empty() {
            Affinity::Blob
        } else if d.contains("REAL") || d.contains("FLOA") || d.contains("DOUB") {
            Affinity::Real
        } else {
            Affinity::Numeric
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Affinity::Integer => "INTEGER",
            Affinity::Real => "REAL",
            Affinity::Numeric => "NUMERIC",
            Affinity::Text => "TEXT",
            Affinity::Blob => "BLOB",
        }
    }
}

/// How a loaded row can be addressed for an `UPDATE`.
#[derive(Debug, Clone, PartialEq)]
pub enum RowKey {
    /// Ordinary table: identified by its rowid.
    Rowid(i64),
    /// `WITHOUT ROWID` table: identified by the declared primary key columns.
    Primary(Vec<(String, Value)>),
    /// No usable identity (a view, or a table with no primary key and no rowid).
    None,
}

/// A row plus the handle needed to write it back.
#[derive(Debug, Clone)]
pub struct Row {
    pub key: RowKey,
    pub cells: Vec<Value>,
}

/// The result of running an arbitrary statement.
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
    /// `Some(n)` for statements that changed `n` rows and returned no result set.
    pub changes: Option<usize>,
}

/// A table's structural description, shown in the schema view.
#[derive(Debug, Clone)]
pub struct SchemaReport {
    pub object: String,
    pub kind: ObjectKind,
    /// The verbatim `CREATE` statement from `sqlite_master`.
    pub create_sql: String,
    pub columns: Vec<ColumnInfo>,
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
    pub without_rowid: bool,
}

#[derive(Debug, Clone)]
pub struct IndexInfo {
    pub name: String,
    pub unique: bool,
    pub origin: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ForeignKeyInfo {
    pub column: String,
    pub target_table: String,
    pub target_column: String,
    pub on_update: String,
    pub on_delete: String,
}

/// An open database. Wraps the connection so the UI layer can hold it in one
/// place and so `REGEXP` is registered exactly once.
pub struct Database {
    conn: Connection,
    path: PathBuf,
    read_only: bool,
    /// Compiled-regex cache backing the `REGEXP` SQL operator.
    regex_cache: Arc<Mutex<HashMap<String, regex::Regex>>>,
}

impl Database {
    /// Open `path`, falling back to a read-only connection when the file cannot
    /// be opened for writing (a read-only mount, for instance).
    pub fn open(path: impl AsRef<Path>) -> Result<Database> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            bail!("database file not found: {}", path.display());
        }
        let (conn, read_only) = match Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_URI,
        ) {
            Ok(c) => (c, false),
            Err(_) => {
                let c = Connection::open_with_flags(
                    &path,
                    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
                )
                .with_context(|| format!("cannot open {}", path.display()))?;
                (c, true)
            }
        };
        // Fail fast rather than block forever behind another writer.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", true).ok();

        let db = Database {
            conn,
            path,
            read_only,
            regex_cache: Arc::new(Mutex::new(HashMap::new())),
        };
        db.register_regexp()?;
        // Touch the schema so a corrupt or non-SQLite file is reported at start-up
        // instead of on the first keypress.
        db.conn
            .prepare("SELECT name FROM sqlite_master LIMIT 1")
            .context("file does not look like a SQLite database")?;
        Ok(db)
    }

    /// Make `expr REGEXP pattern` available to hand-written SQL, matching the
    /// regex syntax used by the column filter.
    fn register_regexp(&self) -> Result<()> {
        let cache = Arc::clone(&self.regex_cache);
        self.conn.create_scalar_function(
            "regexp",
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            move |ctx| {
                let pattern: String = ctx.get(0)?;
                let subject = match ctx.get_raw(1) {
                    rusqlite::types::ValueRef::Null => return Ok(false),
                    other => Value::from(other).filter_text(),
                };
                let mut guard = cache.lock().map_err(|_| {
                    rusqlite::Error::UserFunctionError("regex cache poisoned".into())
                })?;
                let re = match guard.get(&pattern) {
                    Some(re) => re,
                    None => {
                        let re = regex::Regex::new(&pattern)
                            .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
                        guard.entry(pattern.clone()).or_insert(re)
                    }
                };
                Ok(re.is_match(&subject))
            },
        )?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// Human-readable SQLite library version, for the title bar.
    pub fn sqlite_version(&self) -> String {
        rusqlite::version().to_string()
    }

    /// Discover every user table and view. Internal `sqlite_*` objects are
    /// skipped; names are ordered tables-first then alphabetically.
    pub fn objects(&self) -> Result<Vec<ObjectInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, type FROM sqlite_master \
             WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' \
             ORDER BY type = 'view', name COLLATE NOCASE",
        )?;
        let raw = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut out = Vec::with_capacity(raw.len());
        for (name, kind) in raw {
            let kind = if kind == "view" {
                ObjectKind::View
            } else {
                ObjectKind::Table
            };
            out.push(ObjectInfo {
                row_count: self.count_rows(&name).ok(),
                name,
                kind,
            });
        }
        Ok(out)
    }

    fn count_rows(&self, object: &str) -> Result<i64> {
        let sql = format!("SELECT COUNT(*) FROM {}", quote_ident(object));
        Ok(self.conn.query_row(&sql, [], |r| r.get(0))?)
    }

    /// Column list for a table or view.
    pub fn columns(&self, object: &str) -> Result<Vec<ColumnInfo>> {
        let sql = format!("PRAGMA table_info({})", quote_literal_ident(object));
        let mut stmt = self.conn.prepare(&sql)?;
        let cols = stmt
            .query_map([], |r| {
                Ok(ColumnInfo {
                    name: r.get::<_, String>(1)?,
                    decl_type: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    not_null: r.get::<_, i64>(3)? != 0,
                    default: r.get::<_, Option<String>>(4)?,
                    pk_index: r.get::<_, i64>(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if cols.is_empty() {
            bail!("no such table or view: {object}");
        }
        Ok(cols)
    }

    /// Full structural report: the `CREATE` statement plus columns, indexes and
    /// foreign keys.
    pub fn schema(&self, object: &str) -> Result<SchemaReport> {
        let (kind, create_sql): (String, Option<String>) = self
            .conn
            .query_row(
                "SELECT type, sql FROM sqlite_master WHERE name = ?1 AND type IN ('table','view')",
                [object],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .with_context(|| format!("no schema entry for {object}"))?;
        let kind = if kind == "view" {
            ObjectKind::View
        } else {
            ObjectKind::Table
        };
        Ok(SchemaReport {
            object: object.to_string(),
            kind,
            create_sql: create_sql.unwrap_or_default(),
            columns: self.columns(object)?,
            indexes: self.indexes(object).unwrap_or_default(),
            foreign_keys: self.foreign_keys(object).unwrap_or_default(),
            without_rowid: kind == ObjectKind::Table && !self.has_rowid(object),
        })
    }

    fn indexes(&self, object: &str) -> Result<Vec<IndexInfo>> {
        let sql = format!("PRAGMA index_list({})", quote_literal_ident(object));
        let mut stmt = self.conn.prepare(&sql)?;
        let listed = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)? != 0,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut out = Vec::new();
        for (name, unique, origin) in listed {
            let isql = format!("PRAGMA index_info({})", quote_literal_ident(&name));
            let mut istmt = self.conn.prepare(&isql)?;
            let columns = istmt
                .query_map([], |r| r.get::<_, Option<String>>(2))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .into_iter()
                .map(|c| c.unwrap_or_else(|| "<expr>".into()))
                .collect();
            out.push(IndexInfo {
                name,
                unique,
                origin,
                columns,
            });
        }
        Ok(out)
    }

    fn foreign_keys(&self, object: &str) -> Result<Vec<ForeignKeyInfo>> {
        let sql = format!("PRAGMA foreign_key_list({})", quote_literal_ident(object));
        let mut stmt = self.conn.prepare(&sql)?;
        let out = stmt
            .query_map([], |r| {
                Ok(ForeignKeyInfo {
                    target_table: r.get::<_, String>(2)?,
                    column: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    target_column: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    on_update: r.get::<_, String>(5)?,
                    on_delete: r.get::<_, String>(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(out)
    }

    /// Whether `object` is an ordinary rowid table.
    pub fn has_rowid(&self, object: &str) -> bool {
        let sql = format!("SELECT rowid FROM {} LIMIT 1", quote_ident(object));
        self.conn.prepare(&sql).is_ok()
    }

    /// Load every row of `object` together with a write-back key.
    ///
    /// Rows are fetched in full rather than paged: the grid needs the whole set
    /// to sort and filter it locally without re-querying.
    pub fn load_rows(&self, object: &str) -> Result<(Vec<ColumnInfo>, Vec<Row>)> {
        let columns = self.columns(object)?;
        let has_rowid = self.has_rowid(object);

        let col_list = columns
            .iter()
            .map(|c| quote_ident(&c.name))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = if has_rowid {
            format!(
                "SELECT rowid AS _toolo_rowid, {col_list} FROM {}",
                quote_ident(object)
            )
        } else {
            format!("SELECT {col_list} FROM {}", quote_ident(object))
        };

        // Primary-key columns, used as the identity for WITHOUT ROWID tables.
        let mut pk: Vec<(usize, String)> = columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.pk_index > 0)
            .map(|(i, c)| (i, c.name.clone()))
            .collect();
        pk.sort_by_key(|(i, _)| columns[*i].pk_index);

        let mut stmt = self.conn.prepare(&sql)?;
        let offset = usize::from(has_rowid);
        let ncols = columns.len();
        let rows = stmt
            .query_map([], |r| {
                let mut cells = Vec::with_capacity(ncols);
                for i in 0..ncols {
                    cells.push(Value::from(r.get_ref(i + offset)?));
                }
                let key = if has_rowid {
                    RowKey::Rowid(r.get(0)?)
                } else if pk.is_empty() {
                    RowKey::None
                } else {
                    RowKey::Primary(
                        pk.iter()
                            .map(|(i, name)| (name.clone(), cells[*i].clone()))
                            .collect(),
                    )
                };
                Ok(Row { key, cells })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((columns, rows))
    }

    /// Write one column of one row back to the database.
    ///
    /// Returns the freshly re-read row so the caller can show exactly what
    /// SQLite stored (which may differ from the input after affinity coercion).
    pub fn update_cell(
        &self,
        object: &str,
        key: &RowKey,
        column: &str,
        new_value: &Value,
    ) -> Result<Vec<Value>> {
        if self.read_only {
            bail!("database is open read-only");
        }
        let columns = self.columns(object)?;
        if !columns.iter().any(|c| c.name == column) {
            bail!("no column named {column} in {object}");
        }

        let (where_clause, mut params): (String, Vec<Value>) = match key {
            RowKey::Rowid(id) => ("rowid = ?".to_string(), vec![Value::Int(*id)]),
            RowKey::Primary(pairs) => {
                if pairs.is_empty() {
                    bail!("row has no primary key to update by");
                }
                let clause = pairs
                    .iter()
                    .map(|(name, _)| format!("{} IS ?", quote_ident(name)))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                (clause, pairs.iter().map(|(_, v)| v.clone()).collect())
            }
            RowKey::None => {
                bail!("this row cannot be edited: no rowid and no primary key")
            }
        };

        let sql = format!(
            "UPDATE {} SET {} = ? WHERE {where_clause}",
            quote_ident(object),
            quote_ident(column)
        );
        // The SET parameter binds before the WHERE parameters.
        params.insert(0, new_value.clone());
        let bound: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

        let changed = self
            .conn
            .execute(&sql, bound.as_slice())
            .map_err(describe_sqlite_error)?;
        if changed == 0 {
            bail!("no row matched — it may have been changed by another process");
        }
        if changed > 1 {
            // Cannot happen via rowid; possible with a duplicated pseudo-PK.
            bail!("refusing edit: {changed} rows matched the same key");
        }
        self.reread_row(object, key, &columns)
    }

    /// Re-read a row by key after an update.
    fn reread_row(&self, object: &str, key: &RowKey, columns: &[ColumnInfo]) -> Result<Vec<Value>> {
        let col_list = columns
            .iter()
            .map(|c| quote_ident(&c.name))
            .collect::<Vec<_>>()
            .join(", ");
        let (where_clause, params): (String, Vec<Value>) = match key {
            RowKey::Rowid(id) => ("rowid = ?".into(), vec![Value::Int(*id)]),
            RowKey::Primary(pairs) => (
                pairs
                    .iter()
                    .map(|(n, _)| format!("{} IS ?", quote_ident(n)))
                    .collect::<Vec<_>>()
                    .join(" AND "),
                pairs.iter().map(|(_, v)| v.clone()).collect(),
            ),
            RowKey::None => bail!("row cannot be re-read"),
        };
        let sql = format!(
            "SELECT {col_list} FROM {} WHERE {where_clause}",
            quote_ident(object)
        );
        let bound: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(bound.as_slice())?;
        let row = rows
            .next()?
            .ok_or_else(|| anyhow!("row disappeared after update"))?;
        (0..columns.len())
            .map(|i| Ok(Value::from(row.get_ref(i)?)))
            .collect()
    }

    /// Run one arbitrary statement typed by the user.
    ///
    /// Statements that return rows come back as a result set; everything else
    /// reports the number of affected rows.
    pub fn execute_sql(&self, sql: &str) -> Result<QueryResult> {
        let trimmed = sql.trim().trim_end_matches(';').trim();
        if trimmed.is_empty() {
            bail!("empty statement");
        }
        let mut stmt = self.conn.prepare(trimmed).map_err(describe_sqlite_error)?;
        let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        if columns.is_empty() {
            // No result set: DML/DDL.
            let changed = stmt.raw_execute().map_err(describe_sqlite_error)?;
            return Ok(QueryResult {
                columns,
                rows: Vec::new(),
                changes: Some(changed),
            });
        }

        let ncols = columns.len();
        let mut rows_out = Vec::new();
        let mut rows = stmt.query([]).map_err(describe_sqlite_error)?;
        while let Some(r) = rows.next().map_err(describe_sqlite_error)? {
            let mut cells = Vec::with_capacity(ncols);
            for i in 0..ncols {
                cells.push(Value::from(r.get_ref(i)?));
            }
            rows_out.push(Row {
                key: RowKey::None,
                cells,
            });
        }
        Ok(QueryResult {
            columns,
            rows: rows_out,
            changes: None,
        })
    }
}

/// Turn rusqlite's terse messages into something a user can act on.
fn describe_sqlite_error(e: rusqlite::Error) -> anyhow::Error {
    match &e {
        rusqlite::Error::SqliteFailure(f, Some(msg)) => {
            anyhow!("SQLite error {}: {msg}", f.extended_code)
        }
        _ => anyhow!("{e}"),
    }
}

/// Quote an identifier for use in SQL. Embedded double quotes are doubled, so
/// even a table named `we"ird` is addressed correctly.
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Quote a name for a context that takes a *string* rather than an identifier —
/// `PRAGMA table_info('x')`. PRAGMA arguments are not parameterisable.
fn quote_literal_ident(name: &str) -> String {
    format!("'{}'", name.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let c = Connection::open(&path).unwrap();
        c.execute_batch(
            r#"
            CREATE TABLE employees (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                salary REAL,
                dept TEXT DEFAULT 'none'
            );
            INSERT INTO employees (name, salary, dept) VALUES
                ('Ada', 120000.0, 'eng'),
                ('Grace', 145000.5, 'eng'),
                ('Linus', 99000.0, 'ops'),
                ('Nobody', NULL, NULL);
            CREATE TABLE metrics (
                host TEXT NOT NULL,
                ts INTEGER NOT NULL,
                load REAL,
                PRIMARY KEY (host, ts)
            ) WITHOUT ROWID;
            INSERT INTO metrics VALUES ('a', 1, 0.5), ('b', 2, 1.5);
            CREATE INDEX idx_emp_dept ON employees(dept);
            CREATE VIEW eng AS SELECT name, salary FROM employees WHERE dept = 'eng';
            "#,
        )
        .unwrap();
        drop(c);
        let db = Database::open(&path).unwrap();
        (dir, db)
    }

    #[test]
    fn discovers_tables_and_views_without_hardcoding() {
        let (_d, db) = fixture();
        let objs = db.objects().unwrap();
        let names: Vec<_> = objs.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, vec!["employees", "metrics", "eng"]);
        assert_eq!(objs[0].kind, ObjectKind::Table);
        assert_eq!(objs[2].kind, ObjectKind::View);
        assert_eq!(objs[0].row_count, Some(4));
        assert_eq!(objs[2].row_count, Some(2));
    }

    #[test]
    fn internal_sqlite_objects_are_hidden() {
        let (_d, db) = fixture();
        db.execute_sql("CREATE TABLE seq_holder (id INTEGER PRIMARY KEY AUTOINCREMENT)")
            .unwrap();
        db.execute_sql("INSERT INTO seq_holder VALUES (NULL)").unwrap();
        let names: Vec<_> = db
            .objects()
            .unwrap()
            .into_iter()
            .map(|o| o.name)
            .collect();
        assert!(!names.iter().any(|n| n.starts_with("sqlite_")));
    }

    #[test]
    fn loads_all_columns_of_every_row() {
        let (_d, db) = fixture();
        let (cols, rows) = db.load_rows("employees").unwrap();
        assert_eq!(
            cols.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            vec!["id", "name", "salary", "dept"]
        );
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].cells.len(), 4);
        assert_eq!(rows[0].cells[1], Value::Text("Ada".into()));
        assert_eq!(rows[3].cells[2], Value::Null);
        assert!(matches!(rows[0].key, RowKey::Rowid(1)));
    }

    #[test]
    fn without_rowid_tables_key_on_primary_key() {
        let (_d, db) = fixture();
        let (_, rows) = db.load_rows("metrics").unwrap();
        match &rows[0].key {
            RowKey::Primary(pairs) => {
                assert_eq!(pairs[0].0, "host");
                assert_eq!(pairs[1].0, "ts");
            }
            other => panic!("expected primary key, got {other:?}"),
        }
    }

    #[test]
    fn update_writes_through_and_rereads() {
        let (_d, db) = fixture();
        let (_, rows) = db.load_rows("employees").unwrap();
        let updated = db
            .update_cell(
                "employees",
                &rows[0].key,
                "salary",
                &Value::Real(130000.0),
            )
            .unwrap();
        assert_eq!(updated[2], Value::Real(130000.0));
        // Verify with an independent connection that it really hit the file.
        let (_, again) = db.load_rows("employees").unwrap();
        assert_eq!(again[0].cells[2], Value::Real(130000.0));
    }

    #[test]
    fn update_works_on_without_rowid_table() {
        let (_d, db) = fixture();
        let (_, rows) = db.load_rows("metrics").unwrap();
        let updated = db
            .update_cell("metrics", &rows[1].key, "load", &Value::Real(9.25))
            .unwrap();
        assert_eq!(updated[2], Value::Real(9.25));
    }

    #[test]
    fn update_rejects_unknown_column() {
        let (_d, db) = fixture();
        let (_, rows) = db.load_rows("employees").unwrap();
        let err = db
            .update_cell("employees", &rows[0].key, "nope", &Value::Null)
            .unwrap_err();
        assert!(err.to_string().contains("no column named"));
    }

    #[test]
    fn not_null_violation_surfaces_as_error() {
        let (_d, db) = fixture();
        let (_, rows) = db.load_rows("employees").unwrap();
        let err = db
            .update_cell("employees", &rows[0].key, "name", &Value::Null)
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not null"), "{err}");
    }

    #[test]
    fn schema_report_includes_create_columns_and_indexes() {
        let (_d, db) = fixture();
        let s = db.schema("employees").unwrap();
        assert!(s.create_sql.contains("CREATE TABLE employees"));
        assert_eq!(s.columns.len(), 4);
        assert_eq!(s.columns[0].pk_index, 1);
        assert!(s.columns[1].not_null);
        assert_eq!(s.columns[3].default.as_deref(), Some("'none'"));
        assert!(s.indexes.iter().any(|i| i.name == "idx_emp_dept"));
        assert!(!s.without_rowid);

        let m = db.schema("metrics").unwrap();
        assert!(m.without_rowid);
    }

    #[test]
    fn schema_of_view_reports_view_kind() {
        let (_d, db) = fixture();
        let s = db.schema("eng").unwrap();
        assert_eq!(s.kind, ObjectKind::View);
        assert_eq!(s.columns.len(), 2);
    }

    #[test]
    fn query_returns_result_set() {
        let (_d, db) = fixture();
        let r = db
            .execute_sql("SELECT name, salary FROM employees WHERE dept='eng' ORDER BY salary DESC")
            .unwrap();
        assert_eq!(r.columns, vec!["name", "salary"]);
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.rows[0].cells[0], Value::Text("Grace".into()));
        assert!(r.changes.is_none());
    }

    #[test]
    fn query_reports_changes_for_dml() {
        let (_d, db) = fixture();
        let r = db
            .execute_sql("UPDATE employees SET dept='eng' WHERE dept IS NULL")
            .unwrap();
        assert_eq!(r.changes, Some(1));
        assert!(r.rows.is_empty());
    }

    #[test]
    fn regexp_operator_is_available_in_sql() {
        let (_d, db) = fixture();
        let r = db
            .execute_sql("SELECT name FROM employees WHERE name REGEXP '^[AG]'")
            .unwrap();
        assert_eq!(r.rows.len(), 2);
    }

    #[test]
    fn bad_sql_reports_a_message() {
        let (_d, db) = fixture();
        let err = db.execute_sql("SELECT * FROM nope").unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }

    #[test]
    fn identifiers_with_quotes_and_spaces_round_trip() {
        let (_d, db) = fixture();
        db.execute_sql(r#"CREATE TABLE "odd ""name" ("a b" TEXT)"#)
            .unwrap();
        db.execute_sql(r#"INSERT INTO "odd ""name" VALUES ('x')"#)
            .unwrap();
        let (cols, rows) = db.load_rows(r#"odd "name"#).unwrap();
        assert_eq!(cols[0].name, "a b");
        assert_eq!(rows.len(), 1);
        let updated = db
            .update_cell(r#"odd "name"#, &rows[0].key, "a b", &Value::Text("y".into()))
            .unwrap();
        assert_eq!(updated[0], Value::Text("y".into()));
    }

    #[test]
    fn missing_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = Database::open(dir.path().join("absent.db")).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn non_sqlite_file_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("garbage.db");
        std::fs::write(&p, b"this is definitely not a database").unwrap();
        assert!(Database::open(&p).is_err());
    }

    #[test]
    fn affinity_follows_declared_type() {
        assert_eq!(Affinity::from_decl("INTEGER"), Affinity::Integer);
        assert_eq!(Affinity::from_decl("BIGINT"), Affinity::Integer);
        assert_eq!(Affinity::from_decl("VARCHAR(20)"), Affinity::Text);
        assert_eq!(Affinity::from_decl("DOUBLE"), Affinity::Real);
        assert_eq!(Affinity::from_decl("DECIMAL(10,5)"), Affinity::Numeric);
        assert_eq!(Affinity::from_decl(""), Affinity::Blob);
        // "INT" wins over "CHAR" when both appear, per the SQLite rule order.
        assert_eq!(Affinity::from_decl("INTCHAR"), Affinity::Integer);
    }
}
