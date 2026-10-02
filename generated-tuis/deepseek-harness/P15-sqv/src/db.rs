//! Thin wrapper around `rusqlite` providing the operations the TUI needs.

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection};

use crate::model::{CellValue, ColumnInfo, DataRow, TableInfo};

/// Quote an identifier for safe interpolation into SQL.
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Result of executing an arbitrary SQL statement.
#[derive(Clone, Debug)]
pub enum QueryOutcome {
    Rows {
        columns: Vec<String>,
        rows: Vec<Vec<CellValue>>,
    },
    Affected {
        n: usize,
    },
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self { conn })
    }

    /// Discover every user table (excluding SQLite internal tables).
    pub fn list_tables(&self) -> rusqlite::Result<Vec<TableInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, sql FROM sqlite_master \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?;
        let rows = stmt.query_map(params![], |row| {
            let name: String = row.get(0)?;
            let sql: Option<String> = row.get(1)?;
            let has_rowid = sql
                .as_deref()
                .map(|s| !s.to_ascii_uppercase().contains("WITHOUT ROWID"))
                .unwrap_or(true);
            Ok(TableInfo { name, sql, has_rowid })
        })?;
        rows.collect()
    }

    /// Column metadata for a table.
    pub fn columns(&self, table: &str) -> rusqlite::Result<Vec<ColumnInfo>> {
        let sql = format!("PRAGMA table_info({})", quote_ident(table));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![], |row| {
            let notnull: i64 = row.get(3)?;
            Ok(ColumnInfo {
                cid: row.get::<_, i64>(0)? as usize,
                name: row.get(1)?,
                declared_type: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                notnull: notnull != 0,
                pk: row.get(5)?,
                default: row.get::<_, Option<String>>(4)?,
            })
        })?;
        rows.collect()
    }

    /// Load all rows of a table. For ordinary tables a hidden `rowid` column
    /// is fetched first so updates can target a stable key; if that fails
    /// (e.g. `WITHOUT ROWID`) we fall back to a plain `SELECT *`.
    pub fn load_rows(&self, table: &str, ncols: usize, has_rowid: bool) -> rusqlite::Result<Vec<DataRow>> {
        if has_rowid {
            match self.load_rows_with_rowid(table, ncols) {
                Ok(rows) => return Ok(rows),
                Err(_) => { /* fall through to the plain query */ }
            }
        }
        self.load_rows_plain(table, ncols)
    }

    fn load_rows_with_rowid(&self, table: &str, ncols: usize) -> rusqlite::Result<Vec<DataRow>> {
        let sql = format!("SELECT rowid, * FROM {}", quote_ident(table));
        let mut stmt = self.conn.prepare(&sql)?;
        let mut out = Vec::new();
        let mut rows = stmt.query(params![])?;
        while let Some(row) = rows.next()? {
            let rowid: i64 = row.get(0)?;
            let mut cells = Vec::with_capacity(ncols);
            for i in 0..ncols {
                let v: SqlValue = row.get(i + 1)?;
                cells.push(CellValue::from_sql(v));
            }
            out.push(DataRow {
                rowid: Some(rowid),
                cells,
            });
        }
        Ok(out)
    }

    fn load_rows_plain(&self, table: &str, ncols: usize) -> rusqlite::Result<Vec<DataRow>> {
        let sql = format!("SELECT * FROM {}", quote_ident(table));
        let mut stmt = self.conn.prepare(&sql)?;
        let mut out = Vec::new();
        let mut rows = stmt.query(params![])?;
        while let Some(row) = rows.next()? {
            let mut cells = Vec::with_capacity(ncols);
            for i in 0..ncols {
                let v: SqlValue = row.get(i)?;
                cells.push(CellValue::from_sql(v));
            }
            out.push(DataRow { rowid: None, cells });
        }
        Ok(out)
    }

    /// Update one cell. The caller guarantees the row is targetable (has a
    /// rowid or a primary key); see `App::row_is_editable`.
    pub fn update_cell(
        &self,
        table: &str,
        columns: &[ColumnInfo],
        pk_indices: &[usize],
        row: &DataRow,
        col: usize,
        new_value: &CellValue,
    ) -> rusqlite::Result<usize> {
        let mut params: Vec<SqlValue> = vec![new_value.to_sql()];
        let where_clause: String;
        let mut next = 2usize;

        if let Some(rid) = row.rowid {
            where_clause = "rowid = ?2".to_string();
            params.push(SqlValue::Integer(rid));
        } else {
            let mut clauses = Vec::new();
            for &cidx in pk_indices {
                clauses.push(format!("{} = ?{}", quote_ident(&columns[cidx].name), next));
                params.push(row.cells[cidx].to_sql());
                next += 1;
            }
            where_clause = clauses.join(" AND ");
        }

        let sql = format!(
            "UPDATE {} SET {} = ?1 WHERE {}",
            quote_ident(table),
            quote_ident(&columns[col].name),
            where_clause
        );
        self.conn.execute(&sql, params_from_iter(params))
    }

    /// Execute an arbitrary SQL statement and return either rows or an
    /// affected-row count.
    pub fn run_query(&self, sql: &str) -> rusqlite::Result<QueryOutcome> {
        let mut stmt = self.conn.prepare(sql)?;
        let ncols = stmt.column_count();
        if ncols > 0 {
            let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
            let mut rows: Vec<Vec<CellValue>> = Vec::new();
            const MAX_RESULT_ROWS: usize = 50_000;
            let mut q = stmt.query(params![])?;
            while let Some(row) = q.next()? {
                if rows.len() >= MAX_RESULT_ROWS {
                    break;
                }
                let mut cells = Vec::with_capacity(ncols);
                for i in 0..ncols {
                    let v: SqlValue = row.get(i)?;
                    cells.push(CellValue::from_sql(v));
                }
                rows.push(cells);
            }
            Ok(QueryOutcome::Rows { columns, rows })
        } else {
            let n = stmt.execute(params![])?;
            Ok(QueryOutcome::Affected { n })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CellValue;

    fn setup() -> Database {
        let db = Database::open(":memory:").unwrap();
        db.run_query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER, score REAL)").unwrap();
        db.run_query("INSERT INTO users (name, age, score) VALUES ('alice', 30, 9.5)").unwrap();
        db.run_query("INSERT INTO users (name, age, score) VALUES ('bob', 25, 8.0)").unwrap();
        db.run_query("INSERT INTO users (name, age, score) VALUES ('carol', 40, 7.25)").unwrap();
        db
    }

    #[test]
    fn discovers_tables_and_columns() {
        let db = setup();
        let tables = db.list_tables().unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "users");
        assert!(tables[0].has_rowid);
        let cols = db.columns("users").unwrap();
        assert_eq!(cols.len(), 4);
        assert_eq!(cols[0].name, "id");
        assert_eq!(cols[0].pk, 1);
    }

    #[test]
    fn loads_rows_and_updates_cell() {
        let db = setup();
        let cols = db.columns("users").unwrap();
        let rows = db.load_rows("users", cols.len(), true).unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].rowid.is_some());
        let n = db.update_cell("users", &cols, &[0], &rows[0], 2, &CellValue::Integer(31)).unwrap();
        assert_eq!(n, 1);
        let rows2 = db.load_rows("users", cols.len(), true).unwrap();
        assert_eq!(rows2[0].cells[2], CellValue::Integer(31));
    }

    #[test]
    fn runs_select_and_affected() {
        let db = setup();
        match db.run_query("SELECT name, age FROM users ORDER BY age DESC").unwrap() {
            QueryOutcome::Rows { columns, rows } => {
                assert_eq!(columns, vec!["name".to_string(), "age".to_string()]);
                assert_eq!(rows.len(), 3);
                assert_eq!(rows[0][0], CellValue::Text("carol".to_string()));
            }
            other => panic!("expected rows, got {:?}", other),
        }
        match db.run_query("UPDATE users SET age = age + 1").unwrap() {
            QueryOutcome::Affected { n } => assert_eq!(n, 3),
            other => panic!("expected affected, got {:?}", other),
        }
    }

    #[test]
    fn handles_without_rowid() {
        let db = Database::open(":memory:").unwrap();
        db.run_query("CREATE TABLE wr (k TEXT PRIMARY KEY, v INTEGER) WITHOUT ROWID").unwrap();
        db.run_query("INSERT INTO wr (k, v) VALUES ('a', 1), ('b', 2)").unwrap();
        let tables = db.list_tables().unwrap();
        assert!(!tables[0].has_rowid);
        let cols = db.columns("wr").unwrap();
        let rows = db.load_rows("wr", cols.len(), false).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].rowid.is_none());
        let n = db.update_cell("wr", &cols, &[0], &rows[0], 1, &CellValue::Integer(99)).unwrap();
        assert_eq!(n, 1);
        let rows2 = db.load_rows("wr", cols.len(), false).unwrap();
        assert_eq!(rows2[0].cells[1], CellValue::Integer(99));
    }
}
