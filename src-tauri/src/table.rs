//! Tabular data: CSV/TSV (with delimiter sniffing) and spreadsheets via calamine.

use crate::util::{OrStr, Res};
use serde::Serialize;
use std::path::Path;

const MAX_ROWS: usize = 10_000;
const MAX_COLS: usize = 256;
const MAX_CELL: usize = 2000;

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TableData {
    pub sheets: Vec<String>,
    pub sheet: usize,
    pub rows: Vec<Vec<String>>,
    pub columns: usize,
    pub truncated_rows: bool,
    pub truncated_cols: bool,
    pub delimiter: Option<String>,
    pub encoding: Option<String>,
}

fn clip(mut s: String) -> String {
    if s.len() > MAX_CELL {
        let mut cut = MAX_CELL;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push('…');
    }
    s
}

pub fn read_csv(path: &Path, format: &str) -> Res<TableData> {
    let (head, _) = crate::text::read_capped(path, 64 * 1024)?;
    let (sample, encoding) = crate::text::decode(&head);
    let delim = match format {
        "tsv" => b'\t',
        "psv" => b'|',
        _ => sniff_delimiter(&sample),
    };
    let utf8 = encoding.starts_with("UTF-8");
    let enc = encoding_rs::Encoding::for_label(encoding.as_bytes()).unwrap_or(encoding_rs::UTF_8);

    let file = std::fs::File::open(path).or_str()?;
    // Skip a UTF-8 BOM so the first header cell is clean.
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .has_headers(false)
        .flexible(true)
        .quoting(true)
        .from_reader(std::io::BufReader::with_capacity(256 * 1024, file));

    let mut t = TableData {
        delimiter: Some(match delim {
            b'\t' => "tab".into(),
            d => (d as char).to_string(),
        }),
        encoding: Some(encoding.clone()),
        ..Default::default()
    };
    let mut rec = csv::ByteRecord::new();
    loop {
        if t.rows.len() >= MAX_ROWS {
            t.truncated_rows = rdr.read_byte_record(&mut rec).unwrap_or(false);
            break;
        }
        match rdr.read_byte_record(&mut rec) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => {
                if t.rows.is_empty() {
                    return Err(format!("Could not parse as CSV: {e}"));
                }
                break;
            }
        }
        if rec.len() > MAX_COLS {
            t.truncated_cols = true;
        }
        let row: Vec<String> = rec
            .iter()
            .take(MAX_COLS)
            .enumerate()
            .map(|(i, f)| {
                let f = if i == 0 && t.rows.is_empty() { f.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(f) } else { f };
                clip(if utf8 {
                    String::from_utf8_lossy(f).into_owned()
                } else {
                    enc.decode(f).0.into_owned()
                })
            })
            .collect();
        t.columns = t.columns.max(row.len());
        t.rows.push(row);
    }
    Ok(t)
}

/// Pick the delimiter that splits the first lines most consistently.
pub fn sniff_delimiter(sample: &str) -> u8 {
    let lines: Vec<&str> = sample.lines().filter(|l| !l.trim().is_empty()).take(20).collect();
    let mut best = (b',', 0usize);
    for d in [b',', b';', b'\t', b'|'] {
        let counts: Vec<usize> = lines.iter().map(|l| count_outside_quotes(l, d as char)).collect();
        let Some(&first) = counts.first() else { continue };
        if first == 0 {
            continue;
        }
        let consistent = counts.iter().filter(|&&c| c == first).count();
        let score = consistent * 1000 + first;
        if score > best.1 {
            best = (d, score);
        }
    }
    best.0
}

fn count_outside_quotes(line: &str, d: char) -> usize {
    let mut q = false;
    line.chars()
        .filter(|&c| {
            if c == '"' {
                q = !q;
            }
            !q && c == d
        })
        .count()
}

pub fn read_sheet(path: &Path, index: usize) -> Res<TableData> {
    use calamine::{open_workbook_auto, Data, Reader};
    let mut wb = open_workbook_auto(path).ctx("Could not open spreadsheet")?;
    let sheets = wb.sheet_names();
    if sheets.is_empty() {
        return Err("The workbook has no sheets".into());
    }
    let index = index.min(sheets.len() - 1);
    let range = wb.worksheet_range(&sheets[index]).ctx("Could not read sheet")?;
    let (h, w) = range.get_size();
    let mut t = TableData {
        sheets: sheets.clone(),
        sheet: index,
        truncated_rows: h > MAX_ROWS,
        truncated_cols: w > MAX_COLS,
        ..Default::default()
    };
    // Keep absolute positions: a sheet that starts at C5 still shows from A1.
    let (r0, c0) = range.start().map(|(r, c)| (r as usize, c as usize)).unwrap_or((0, 0));
    let lead_cols = c0.min(MAX_COLS);
    for _ in 0..r0.min(MAX_ROWS) {
        t.rows.push(Vec::new());
    }
    for row in range.rows().take(MAX_ROWS.saturating_sub(t.rows.len())) {
        let mut out: Vec<String> = vec![String::new(); lead_cols];
        for cell in row.iter().take(MAX_COLS - lead_cols) {
            out.push(clip(match cell {
                Data::Empty => String::new(),
                Data::Float(f) => fmt_float(*f),
                Data::DateTime(d) => {
                    let (y, mo, da, hh, mi, ss, _) = d.to_ymd_hms_milli();
                    if d.is_duration() {
                        format!("{:.0}h", d.as_f64() * 24.0)
                    } else if hh == 0 && mi == 0 && ss == 0 {
                        format!("{y:04}-{mo:02}-{da:02}")
                    } else if y <= 1900 && d.as_f64() < 1.0 {
                        format!("{hh:02}:{mi:02}:{ss:02}")
                    } else {
                        format!("{y:04}-{mo:02}-{da:02} {hh:02}:{mi:02}")
                    }
                }
                Data::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
                other => other.to_string(),
            }));
        }
        while out.last().is_some_and(|s| s.is_empty()) {
            out.pop();
        }
        t.columns = t.columns.max(out.len());
        t.rows.push(out);
    }
    Ok(t)
}

fn fmt_float(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        let s = format!("{:.10}", f);
        let s = s.trim_end_matches('0').trim_end_matches('.');
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs() {
        assert_eq!(sniff_delimiter("a;b;c\n1;2;3\n"), b';');
        assert_eq!(sniff_delimiter("a,b\n\"x,y\",2\n"), b',');
        assert_eq!(sniff_delimiter("a\tb\tc\n1\t2\t3"), b'\t');
    }

    #[test]
    fn reads_csv() {
        let p = std::env::temp_dir().join(format!("alook-{}.csv", std::process::id()));
        std::fs::write(&p, "\u{feff}name,age\n\"Doe, J\",42\nA,\n").unwrap();
        let t = read_csv(&p, "csv").unwrap();
        assert_eq!(t.rows[0], vec!["name", "age"]);
        assert_eq!(t.rows[1], vec!["Doe, J", "42"]);
        assert_eq!(t.columns, 2);
    }

    #[test]
    fn floats() {
        assert_eq!(fmt_float(3.0), "3");
        assert_eq!(fmt_float(0.1 + 0.2), "0.3");
        assert_eq!(fmt_float(-2.5), "-2.5");
    }
}
