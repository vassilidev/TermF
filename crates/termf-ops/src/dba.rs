//! What the SQL console's DBA (the `dba` role, a real Claude Code session) is told: the database's schema, read
//! by TermF in a read-only session, the console's rules, how to answer, and the data it may reach (`ai_access`:
//! none, or rows through `ops_run` and the user's approval, personal data masked for `redacted`).

use crate::sql::SchemaTable;
use crate::{AiAccess, Connection, Kind};

/// What the user asks.
pub enum Ask<'a> {
    /// A query from words.
    Write(&'a str),
    /// A query in plain words.
    Explain(&'a str),
    /// A faster query, from the database's plan.
    Optimize { query: &'a str, plan: &'a str },
}

/// The schema sent at most: a large database is listed in part, and says so.
const SCHEMA_CHARS: usize = 60_000;

fn engine(kind: Kind) -> &'static str {
    match kind {
        Kind::Postgres => "PostgreSQL",
        Kind::Mysql => "MySQL",
        Kind::Mariadb => "MariaDB",
        Kind::Sqlite => "SQLite",
        Kind::Clickhouse => "ClickHouse",
        _ => "SQL",
    }
}

/// The tables, one line each, then their indexes.
pub fn schema_text(schema: &[SchemaTable]) -> String {
    let mut out = String::new();
    for (i, t) in schema.iter().enumerate() {
        let name = if t.schema.is_empty() {
            t.name.clone()
        } else {
            format!("{}.{}", t.schema, t.name)
        };
        let columns: Vec<String> = t
            .columns
            .iter()
            .map(|c| {
                let mut s = format!("{} {}", c.name, c.kind);
                if !c.key.is_empty() {
                    s.push(' ');
                    s.push_str(&c.key);
                }
                if !c.nullable {
                    s.push_str(" not null");
                }
                s
            })
            .collect();
        let mut block = format!("{name}({})\n", columns.join(", "));
        for index in &t.indexes {
            let unique = if index.unique { "unique " } else { "" };
            block.push_str(&format!(
                "  {unique}index {}: {}\n",
                index.name, index.columns
            ));
        }
        if out.len() + block.len() > SCHEMA_CHARS {
            out.push_str(&format!(
                "… and {} more tables, not listed\n",
                schema.len() - i
            ));
            break;
        }
        out.push_str(&block);
    }
    out
}

fn request(ask: &Ask) -> String {
    match ask {
        Ask::Write(words) => format!("Request: {words}"),
        Ask::Explain(query) => format!(
            "Explain this query in plain words, step by step, in five short lines at most, then give it back tidied in a ```sql block:\n```sql\n{query}\n```"
        ),
        Ask::Optimize { query, plan } => format!(
            "This query is slow. The database's plan for it:\n```\n{plan}\n```\nGive a faster read-only query in a ```sql block, then in four short lines at most why it is faster, and the index that would help if any (a suggestion only: the user creates it elsewhere).\n```sql\n{query}\n```"
        ),
    }
}

/// The first message of a DBA conversation on `conn`; `lang`: the interface's (`fr`, `en`).
pub fn first(
    conn: &Connection,
    schema: &[SchemaTable],
    data: bool,
    ask: &Ask,
    lang: &str,
) -> String {
    let language = if lang == "fr" { "French" } else { "English" };
    let reach = match (data, conn.ai_access) {
        (true, AiAccess::Full) => format!(
            "You may run read-only queries on `{}` with the ops_run tool (recipe sql-readonly): each run waits for the user's approval in TermF, and secrets come back masked.",
            conn.name
        ),
        (true, _) => format!(
            "You may run read-only queries on `{}` with the ops_run tool (recipe sql-readonly): each run waits for the user's approval in TermF, and personal data comes back masked.",
            conn.name
        ),
        (false, _) => {
            "You see no rows of this database, only its schema: never guess its data.".into()
        }
    };
    format!(
        "You are the DBA of TermF's SQL console, for the {engine} database `{name}` ({env}). Help the user write queries they run and save.\n\n\
         The console runs one statement that only reads (SELECT, WITH … SELECT, EXPLAIN, SHOW, DESCRIBE) in a read-only session, adds a LIMIT when there is none, and refuses anything that writes.\n\
         Answer in {language}: first exactly one query in a ```sql block, written for {engine}, then what it does in three short lines at most. Use only the tables and columns of the schema below, and say what is missing when the request needs more. A request that writes gets the read-only query that checks it first, and a word that the write is for the user to run elsewhere.\n\
         {reach}\n\
         Read the repository's files only when the request is about them.\n\n\
         Schema of `{name}`, read by TermF ({count} tables):\n{schema}\n{request}",
        engine = engine(conn.kind),
        name = conn.name,
        env = conn.env.as_str(),
        count = schema.len(),
        schema = schema_text(schema),
        request = request(ask),
    )
}

/// A later message of the same conversation: the schema is already in it.
pub fn next(ask: &Ask) -> String {
    request(ask)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::{Column, Index};

    #[test]
    fn the_dba_gets_the_schema_and_what_it_may_reach() {
        let conn: Connection = toml::from_str(
            "name = \"shop\"\nkind = \"postgres\"\nenv = \"prod\"\nhost = \"db\"\nuser = \"ro\"\ndatabase = \"shop\"\n",
        )
        .unwrap();
        let schema = vec![SchemaTable {
            schema: String::new(),
            name: "orders".into(),
            columns: vec![
                Column {
                    name: "id".into(),
                    kind: "bigint".into(),
                    nullable: false,
                    key: "pk".into(),
                },
                Column {
                    name: "note".into(),
                    kind: "text".into(),
                    nullable: true,
                    key: String::new(),
                },
            ],
            indexes: vec![Index {
                name: "orders_pkey".into(),
                columns: "id".into(),
                unique: true,
            }],
        }];
        let text = first(
            &conn,
            &schema,
            false,
            &Ask::Write("les 10 dernières commandes"),
            "fr",
        );
        assert!(
            text.contains("orders(id bigint pk not null, note text)"),
            "{text}"
        );
        assert!(text.contains("  unique index orders_pkey: id"));
        assert!(text.contains("PostgreSQL database `shop` (prod)"));
        assert!(text.contains("Answer in French"));
        assert!(text.contains("no rows of this database"));
        assert!(text.ends_with("Request: les 10 dernières commandes"));
        let reach = first(&conn, &schema, true, &Ask::Write("x"), "en");
        assert!(reach.contains("ops_run") && reach.contains("personal data comes back masked"));
        let opt = next(&Ask::Optimize {
            query: "SELECT 1",
            plan: "Seq Scan on orders",
        });
        assert!(opt.contains("Seq Scan on orders") && opt.contains("```sql\nSELECT 1\n```"));
        assert!(!opt.contains("Schema"));
        let many: Vec<SchemaTable> = (0..5000)
            .map(|i| SchemaTable {
                name: format!("table_{i}"),
                ..schema[0].clone()
            })
            .collect();
        let listed = schema_text(&many);
        assert!(listed.len() <= SCHEMA_CHARS + 100 && listed.contains("more tables, not listed"));
    }
}
