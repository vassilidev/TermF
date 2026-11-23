//! A query's table, masked as the console shows it, in the usual formats: CSV, TSV, JSON, NDJSON, Markdown, SQL
//! `INSERT`s and Excel (`.xlsx`, a zip of SpreadsheetML stored without compression), written row by row.

use std::io::{Seek, Write};

use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::Kind;
use crate::sql::Table;

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Csv,
    Tsv,
    Json,
    Ndjson,
    Markdown,
    Sql,
    Xlsx,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Tsv => "tsv",
            Format::Json => "json",
            Format::Ndjson => "ndjson",
            Format::Markdown => "md",
            Format::Sql => "sql",
            Format::Xlsx => "xlsx",
        }
    }
}

/// The console shows a database's NULL as `NULL`.
fn null(cell: &str) -> bool {
    cell == "NULL"
}

fn object(columns: &[String], row: &[String]) -> Value {
    let mut map = Map::new();
    for (column, cell) in columns.iter().zip(row) {
        let value = if null(cell) {
            Value::Null
        } else {
            Value::String(cell.clone())
        };
        map.insert(column.clone(), value);
    }
    Value::Object(map)
}

/// A number a spreadsheet or a database reads as one, never a code with leading zeros or a long id.
fn number(cell: &str) -> bool {
    let digits = cell.strip_prefix('-').unwrap_or(cell);
    let (int, frac) = digits.split_once('.').unwrap_or((digits, "0"));
    !int.is_empty()
        && int.len() <= 15
        && (int == "0" || !int.starts_with('0'))
        && int.bytes().all(|b| b.is_ascii_digit())
        && !frac.is_empty()
        && frac.bytes().all(|b| b.is_ascii_digit())
}

/// An identifier as the database quotes it.
fn ident(kind: Kind, name: &str) -> String {
    match kind {
        Kind::Mysql | Kind::Mariadb | Kind::Clickhouse => format!("`{}`", name.replace('`', "``")),
        _ => format!("\"{}\"", name.replace('"', "\"\"")),
    }
}

/// Writes `table` as `format`; `name`: the table an `INSERT` fills.
pub fn write(
    table: &Table,
    format: Format,
    kind: Kind,
    name: &str,
    out: &mut (impl Write + Seek),
) -> Result<()> {
    let cols = &table.columns;
    match format {
        Format::Csv => {
            let cell = |c: &String| {
                if c.contains([',', '"', '\n', '\r']) {
                    format!("\"{}\"", c.replace('"', "\"\""))
                } else {
                    c.clone()
                }
            };
            for row in std::iter::once(cols).chain(&table.rows) {
                writeln!(
                    out,
                    "{}",
                    row.iter().map(cell).collect::<Vec<_>>().join(",")
                )?;
            }
        }
        Format::Tsv => {
            // As a spreadsheet pastes it: a cell's own tabs and line breaks as spaces.
            let cell = |c: &String| c.replace(['\t', '\r', '\n'], " ");
            for row in std::iter::once(cols).chain(&table.rows) {
                writeln!(
                    out,
                    "{}",
                    row.iter().map(cell).collect::<Vec<_>>().join("\t")
                )?;
            }
        }
        Format::Json => {
            write!(out, "[")?;
            for (i, row) in table.rows.iter().enumerate() {
                let sep = if i == 0 { "\n  " } else { ",\n  " };
                write!(out, "{sep}{}", object(cols, row))?;
            }
            writeln!(out, "{}]", if table.rows.is_empty() { "" } else { "\n" })?;
        }
        Format::Ndjson => {
            for row in &table.rows {
                writeln!(out, "{}", object(cols, row))?;
            }
        }
        Format::Markdown => {
            let cell = |c: &String| {
                c.replace('\\', "\\\\")
                    .replace('|', "\\|")
                    .replace("\r\n", "<br>")
                    .replace(['\n', '\r'], "<br>")
            };
            let line = |r: &[String]| {
                format!("| {} |", r.iter().map(cell).collect::<Vec<_>>().join(" | "))
            };
            writeln!(out, "{}", line(cols))?;
            writeln!(out, "|{}|", vec![" --- "; cols.len()].join("|"))?;
            for row in &table.rows {
                writeln!(out, "{}", line(row))?;
            }
        }
        Format::Sql => {
            let into = format!(
                "INSERT INTO {} ({}) VALUES",
                ident(kind, name),
                cols.iter()
                    .map(|c| ident(kind, c))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let backslash = matches!(kind, Kind::Mysql | Kind::Mariadb | Kind::Clickhouse);
            let literal = |c: &String| match c {
                c if null(c) => "NULL".to_owned(),
                c if number(c) => c.clone(),
                // Where a backslash escapes, it is doubled too.
                c if backslash => format!("'{}'", c.replace('\\', "\\\\").replace('\'', "''")),
                c => format!("'{}'", c.replace('\'', "''")),
            };
            for row in &table.rows {
                writeln!(
                    out,
                    "{into} ({});",
                    row.iter().map(literal).collect::<Vec<_>>().join(", ")
                )?;
            }
        }
        Format::Xlsx => xlsx(table, out)?,
    }
    Ok(())
}

/// A cell's text as XML takes it: escaped, and without the control characters XML 1.0 refuses.
fn xml(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `A`, `B`… `Z`, `AA`: a column's letters.
fn letters(mut n: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (n % 26) as u8);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).expect("ascii")
}

fn xlsx(table: &Table, out: &mut (impl Write + Seek)) -> Result<()> {
    use zip::write::SimpleFileOptions;
    // Excel takes 1 048 576 rows, the header included.
    if table.rows.len() >= 1 << 20 {
        bail!("Excel takes 1 048 575 rows at most");
    }
    let mut zip = zip::ZipWriter::new(out);
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let files: [(&str, &str); 4] = [
        (
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        (
            "xl/workbook.xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Result" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
        ),
    ];
    for (name, text) in files {
        zip.start_file(name, stored)?;
        zip.write_all(text.as_bytes())?;
    }
    zip.start_file("xl/worksheets/sheet1.xml", stored)?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
    )?;
    for (r, row) in std::iter::once(&table.columns)
        .chain(&table.rows)
        .enumerate()
    {
        write!(zip, "<row r=\"{}\">", r + 1)?;
        for (c, cell) in row.iter().enumerate() {
            let at = format!("{}{}", letters(c), r + 1);
            if r > 0 && null(cell) {
                continue;
            }
            if r > 0 && number(cell) {
                write!(zip, "<c r=\"{at}\"><v>{cell}</v></c>")?;
            } else {
                write!(
                    zip,
                    "<c r=\"{at}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                    xml(cell)
                )?;
            }
        }
        zip.write_all(b"</row>")?;
    }
    zip.write_all(b"</sheetData></worksheet>")?;
    zip.finish()?;
    Ok(())
}

/// The table as text, for the clipboard (`xlsx` is a file only).
pub fn text(table: &Table, format: Format, kind: Kind, name: &str) -> Result<String> {
    if format == Format::Xlsx {
        bail!("Excel is a file: export it");
    }
    let mut out = std::io::Cursor::new(Vec::new());
    write(table, format, kind, name, &mut out)?;
    Ok(String::from_utf8(out.into_inner())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Table {
        Table {
            columns: vec!["id".into(), "note".into(), "zip".into()],
            rows: vec![
                vec!["1".into(), "it's | a, \"b\"\nc".into(), "007".into()],
                vec!["2.50".into(), "NULL".into(), "a\\b".into()],
            ],
        }
    }

    fn out(format: Format, kind: Kind) -> String {
        text(&sample(), format, kind, "orders").unwrap()
    }

    #[test]
    fn each_format_keeps_cells_whole() {
        assert_eq!(
            out(Format::Csv, Kind::Postgres),
            "id,note,zip\n1,\"it's | a, \"\"b\"\"\nc\",007\n2.50,NULL,a\\b\n"
        );
        assert_eq!(
            out(Format::Tsv, Kind::Postgres),
            "id\tnote\tzip\n1\tit's | a, \"b\" c\t007\n2.50\tNULL\ta\\b\n"
        );
        let json: Value = serde_json::from_str(&out(Format::Json, Kind::Postgres)).unwrap();
        assert_eq!(json[0]["note"], "it's | a, \"b\"\nc");
        assert_eq!(json[1]["note"], Value::Null);
        assert_eq!(json[0]["zip"], "007", "a code stays text");
        let nd = out(Format::Ndjson, Kind::Postgres);
        assert_eq!(nd.lines().count(), 2);
        assert_eq!(
            serde_json::from_str::<Value>(nd.lines().nth(1).unwrap()).unwrap()["id"],
            "2.50"
        );
        assert_eq!(
            out(Format::Markdown, Kind::Postgres),
            "| id | note | zip |\n| --- | --- | --- |\n| 1 | it's \\| a, \"b\"<br>c | 007 |\n| 2.50 | NULL | a\\\\b |\n"
        );
        assert_eq!(
            out(Format::Sql, Kind::Postgres).lines().next().unwrap(),
            "INSERT INTO \"orders\" (\"id\", \"note\", \"zip\") VALUES (1, 'it''s | a, \"b\"\nc', '007');".lines().next().unwrap()
        );
        let pg = out(Format::Sql, Kind::Postgres);
        assert!(pg.ends_with("VALUES (2.50, NULL, 'a\\b');\n"), "{pg}");
        let my = out(Format::Sql, Kind::Mysql);
        assert!(
            my.starts_with("INSERT INTO `orders` (`id`, `note`, `zip`)"),
            "{my}"
        );
        assert!(my.ends_with("VALUES (2.50, NULL, 'a\\\\b');\n"), "{my}");
        assert!(text(&sample(), Format::Xlsx, Kind::Postgres, "t").is_err());
        let empty = Table {
            columns: vec!["a".into()],
            rows: vec![],
        };
        assert_eq!(
            text(&empty, Format::Json, Kind::Postgres, "t").unwrap(),
            "[]\n"
        );
    }

    #[test]
    fn an_xlsx_is_a_zip_a_spreadsheet_opens() {
        let mut file = std::io::Cursor::new(Vec::new());
        write(&sample(), Format::Xlsx, Kind::Postgres, "t", &mut file).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(file.into_inner())).unwrap();
        let names: Vec<String> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
        for part in [
            "[Content_Types].xml",
            "_rels/.rels",
            "xl/workbook.xml",
            "xl/_rels/workbook.xml.rels",
            "xl/worksheets/sheet1.xml",
        ] {
            assert!(names.iter().any(|n| n == part), "{part} in {names:?}");
        }
        let mut sheet = String::new();
        std::io::Read::read_to_string(
            &mut zip.by_name("xl/worksheets/sheet1.xml").unwrap(),
            &mut sheet,
        )
        .unwrap();
        assert!(sheet.contains("<c r=\"A2\"><v>1</v></c>"), "{sheet}");
        assert!(
            sheet.contains(
                "<c r=\"C2\" t=\"inlineStr\"><is><t xml:space=\"preserve\">007</t></is></c>"
            ),
            "{sheet}"
        );
        assert!(sheet.contains("it&apos;s".replace("&apos;", "'").as_str()));
        assert!(
            !sheet.contains("r=\"B3\""),
            "NULL is an empty cell: {sheet}"
        );
        assert_eq!(letters(0), "A");
        assert_eq!(letters(25), "Z");
        assert_eq!(letters(26), "AA");
        assert_eq!(letters(27 * 26), "AAA");
    }
}
