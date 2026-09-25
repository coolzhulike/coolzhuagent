//! Database Tools Plugin — SQL parsing, migration management, ER diagrams.
//! C6: Provides SQL utilities for agent-driven database operations.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlStatement {
    Select {
        table: String,
        columns: Vec<String>,
        condition: Option<String>,
    },
    Insert {
        table: String,
        columns: Vec<String>,
        values: Vec<String>,
    },
    Update {
        table: String,
        sets: Vec<(String, String)>,
        condition: Option<String>,
    },
    Delete {
        table: String,
        condition: Option<String>,
    },
    CreateTable {
        name: String,
        columns: Vec<ColumnDef>,
    },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    pub name: String,
    pub col_type: String,
    pub nullable: bool,
    pub primary_key: bool,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub execution_ms: u64,
}

#[derive(Debug, Clone)]
pub struct Migration {
    pub version: u32,
    pub name: String,
    pub up_sql: String,
    pub down_sql: String,
    pub applied_at_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct MigrationManager {
    migrations: Vec<Migration>,
    current_version: u32,
}

impl MigrationManager {
    pub fn new() -> Self {
        Self {
            migrations: vec![],
            current_version: 0,
        }
    }

    pub fn add(&mut self, name: &str, up: &str, down: &str) {
        self.current_version += 1;
        self.migrations.push(Migration {
            version: self.current_version,
            name: name.into(),
            up_sql: up.into(),
            down_sql: down.into(),
            applied_at_ms: None,
        });
    }

    pub fn pending(&self) -> Vec<&Migration> {
        self.migrations
            .iter()
            .filter(|m| m.applied_at_ms.is_none())
            .collect()
    }

    pub fn apply_next(&mut self) -> Option<&Migration> {
        if let Some(m) = self
            .migrations
            .iter_mut()
            .find(|m| m.applied_at_ms.is_none())
        {
            m.applied_at_ms = Some(now_ms());
            Some(&*m)
        } else {
            None
        }
    }

    pub fn rollback_last(&mut self) -> Option<&Migration> {
        if let Some(m) = self
            .migrations
            .iter_mut()
            .rev()
            .find(|m| m.applied_at_ms.is_some())
        {
            m.applied_at_ms = None;
            Some(&*m)
        } else {
            None
        }
    }
}

pub fn parse_sql(sql: &str) -> SqlStatement {
    let upper = sql.trim().to_uppercase();
    if upper.starts_with("SELECT") {
        let rest = &sql[6..];
        let from_pos = rest.to_uppercase().find("FROM").unwrap_or(rest.len());
        let cols: Vec<String> = rest[..from_pos]
            .split(',')
            .map(|s| s.trim().into())
            .collect();
        let after = rest[from_pos + 4..].trim();
        let (table, cond) = if let Some(w) = after.to_uppercase().find("WHERE") {
            (after[..w].trim().into(), Some(after[w + 5..].trim().into()))
        } else {
            (after.trim().into(), None)
        };
        SqlStatement::Select {
            table,
            columns: cols,
            condition: cond,
        }
    } else if upper.starts_with("INSERT") {
        SqlStatement::Insert {
            table: "".into(),
            columns: vec![],
            values: vec![],
        }
    } else if upper.starts_with("CREATE") {
        SqlStatement::CreateTable {
            name: "".into(),
            columns: vec![],
        }
    } else {
        SqlStatement::Unknown
    }
}

/// Generate Mermaid ER diagram text from table definitions.
pub fn er_diagram(
    tables: &[(&str, &[(&str, &str)])],
    relationships: &[(&str, &str, &str)],
) -> String {
    let mut out = String::from("erDiagram\n");
    for (name, cols) in tables {
        out.push_str(&format!("    {name} {{\n"));
        for (col, tp) in *cols {
            out.push_str(&format!("        {tp} {col}\n"));
        }
        out.push_str("    }\n");
    }
    for (from, to, label) in relationships {
        out.push_str(&format!("    {from} ||--o{{ {to} : \"{label}\"\n"));
    }
    out
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_select_with_where() {
        let s = parse_sql("SELECT id, name FROM users WHERE active = 1");
        match s {
            SqlStatement::Select {
                table, condition, ..
            } => {
                assert_eq!(table, "users");
                assert_eq!(condition, Some("active = 1".into()));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn migration_apply_rollback() {
        let mut m = MigrationManager::new();
        m.add("init", "CREATE TABLE t (id INT)", "DROP TABLE t");
        assert!(m.apply_next().is_some());
        assert!(m.rollback_last().is_some());
    }

    #[test]
    fn er_diagram_output() {
        let d = er_diagram(
            &[("users", &[("id", "INT")])],
            &[("users", "posts", "writes")],
        );
        assert!(d.contains("erDiagram"));
        assert!(d.contains("writes"));
    }
}
