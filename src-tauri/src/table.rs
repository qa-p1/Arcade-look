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
    let enc = encoding_rs::Encoding::for_label(encoding.as_bytes()).unwrap_or(encoding_rs::UTF_8);

    let file = std::io::BufReader::with_capacity(256 * 1024, std::fs::File::open(path).or_str()?);
    // Fields are split on bytes, so other encodings become UTF-8 first: in
    // UTF-16 every character spans two bytes, and in Shift_JIS a delimiter
    // byte can be half of a character.
    let input: Box<dyn std::io::Read> = if enc == encoding_rs::UTF_8 {
        Box::new(file)
    } else {
        Box::new(Utf8Reader::new(file, enc))
    };
    // Skip a UTF-8 BOM so the first header cell is clean.
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .has_headers(false)
        .flexible(true)
        .quoting(true)
        .from_reader(input);

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
                    return Err(format!("Couldn't parse as CSV: {e}"));
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
                let f = if i == 0 && t.rows.is_empty() {
                    f.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(f)
                } else {
                    f
                };
                clip(String::from_utf8_lossy(f).into_owned())
            })
            .collect();
        t.columns = t.columns.max(row.len());
        t.rows.push(row);
    }
    Ok(t)
}

/// Decodes `inner` from `encoding` to UTF-8 while it is read (dropping a BOM).
struct Utf8Reader<R> {
    inner: R,
    decoder: encoding_rs::Decoder,
    raw: Vec<u8>,
    out: Vec<u8>,
    pos: usize,
    done: bool,
}

impl<R> Utf8Reader<R> {
    fn new(inner: R, encoding: &'static encoding_rs::Encoding) -> Self {
        Self {
            inner,
            decoder: encoding.new_decoder_with_bom_removal(),
            raw: vec![0; 64 * 1024],
            out: Vec::new(),
            pos: 0,
            done: false,
        }
    }
}

impl<R: std::io::Read> std::io::Read for Utf8Reader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        while self.pos == self.out.len() {
            if self.done {
                return Ok(0);
            }
            let n = self.inner.read(&mut self.raw)?;
            let last = n == 0;
            let room = self.decoder.max_utf8_buffer_length(n).unwrap_or(n * 3 + 16);
            self.out.resize(room, 0);
            let (_, _, written, _) =
                self.decoder
                    .decode_to_utf8(&self.raw[..n], &mut self.out, last);
            self.out.truncate(written);
            self.pos = 0;
            self.done = last;
        }
        let n = buf.len().min(self.out.len() - self.pos);
        buf[..n].copy_from_slice(&self.out[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Pick the delimiter that splits the first lines most consistently.
pub fn sniff_delimiter(sample: &str) -> u8 {
    let lines: Vec<&str> = sample
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(20)
        .collect();
    let mut best = (b',', 0usize);
    for d in *b",;\t|" {
        let counts: Vec<usize> = lines
            .iter()
            .map(|l| count_outside_quotes(l, d as char))
            .collect();
        let Some(&first) = counts.first() else {
            continue;
        };
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
    use calamine::{open_workbook_auto, Reader, Sheets};
    let mut wb = open_workbook_auto(path).ctx("Couldn't open spreadsheet")?;
    let sheets = wb.sheet_names();
    if sheets.is_empty() {
        return Err("The workbook has no sheets".into());
    }
    let index = index.min(sheets.len() - 1);
    let mut t = TableData {
        sheets: sheets.clone(),
        sheet: index,
        ..Default::default()
    };
    let name = &sheets[index];
    // XLSX and XLSB stream their cells, so reading stops at the preview's
    // last row instead of loading the whole sheet first.
    let mut grid = Grid::default();
    match &mut wb {
        Sheets::Xlsx(x) => {
            let mut cells = x.worksheet_cells_reader(name).ctx("Couldn't read sheet")?;
            while let Some(cell) = cells.next_cell().ctx("Couldn't read sheet")? {
                let ((row, col), value) = (cell.get_position(), cell.get_value().clone());
                if !grid.put(row as usize, col as usize, &value.into()) {
                    break;
                }
            }
        }
        Sheets::Xlsb(x) => {
            let mut cells = x.worksheet_cells_reader(name).ctx("Couldn't read sheet")?;
            while let Some(cell) = cells.next_cell().ctx("Couldn't read sheet")? {
                let ((row, col), value) = (cell.get_position(), cell.get_value().clone());
                if !grid.put(row as usize, col as usize, &value.into()) {
                    break;
                }
            }
        }
        _ => {
            let range = wb.worksheet_range(name).ctx("Couldn't read sheet")?;
            // Positions stay absolute: a sheet that starts at C5 still shows from A1.
            for (row, col, value) in range.used_cells() {
                let (r0, c0) = range.start().unwrap_or((0, 0));
                if !grid.put(r0 as usize + row, c0 as usize + col, value) {
                    break;
                }
            }
        }
    }
    t.truncated_rows = grid.more_rows;
    t.truncated_cols = grid.more_cols;
    t.rows = grid.rows;
    for row in &mut t.rows {
        while row.last().is_some_and(|s| s.is_empty()) {
            row.pop();
        }
    }
    t.columns = t.rows.iter().map(Vec::len).max().unwrap_or(0);
    Ok(t)
}

/// The previewed corner of a sheet, filled cell by cell in row order.
#[derive(Default)]
struct Grid {
    rows: Vec<Vec<String>>,
    more_rows: bool,
    more_cols: bool,
}

impl Grid {
    /// Places one cell. `false` once a cell lies below the preview: rows
    /// arrive in order, so nothing after it can be shown either.
    fn put(&mut self, row: usize, col: usize, value: &calamine::Data) -> bool {
        if row >= MAX_ROWS {
            self.more_rows = true;
            return false;
        }
        if col >= MAX_COLS {
            self.more_cols = true;
            return true;
        }
        let text = cell_text(value);
        if text.is_empty() {
            return true;
        }
        if self.rows.len() <= row {
            self.rows.resize_with(row + 1, Vec::new);
        }
        let cells = &mut self.rows[row];
        if cells.len() <= col {
            cells.resize(col + 1, String::new());
        }
        cells[col] = clip(text);
        true
    }
}

fn cell_text(cell: &calamine::Data) -> String {
    use calamine::Data;
    match cell {
        Data::Empty => String::new(),
        Data::Float(f) => fmt_float(*f),
        Data::DateTime(d) => {
            if d.is_duration() {
                return fmt_duration(d.as_f64());
            }
            let (y, mo, da, hh, mi, ss, _) = d.to_ymd_hms_milli();
            // Time-only values sit on the epoch day; midnight is still a time.
            if y <= 1900 && d.as_f64() < 1.0 {
                format!("{hh:02}:{mi:02}:{ss:02}")
            } else if hh == 0 && mi == 0 && ss == 0 {
                format!("{y:04}-{mo:02}-{da:02}")
            } else {
                format!("{y:04}-{mo:02}-{da:02} {hh:02}:{mi:02}")
            }
        }
        Data::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        other => other.to_string(),
    }
}

/// A duration in days as `[h]:mm:ss`, like the spreadsheet shows it.
fn fmt_duration(days: f64) -> String {
    let seconds = (days * 86_400.0).round();
    let sign = if seconds < 0.0 { "-" } else { "" };
    let total = seconds.abs() as u64;
    format!(
        "{sign}{}:{:02}:{:02}",
        total / 3600,
        total / 60 % 60,
        total % 60
    )
}

/// A number as a spreadsheet's General format shows it: at most 15
/// significant digits, and very small or large values in E notation rather
/// than rounded to 0 or spelled out.
fn fmt_float(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        return format!("{}", f as i64);
    }
    let rounded: f64 = format!("{f:.14e}").parse().unwrap_or(f);
    if rounded != 0.0 && (rounded.abs() < 1e-5 || rounded.abs() >= 1e15) {
        format!("{rounded:E}")
    } else {
        format!("{rounded}")
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
        assert_eq!(fmt_float(1e-12), "1E-12");
        assert_eq!(fmt_float(42.5), "42.5");
        assert_eq!(fmt_float(0.000123), "0.000123");
        assert_eq!(fmt_float(1.5e20), "1.5E20");
    }

    #[test]
    fn durations_and_midnight() {
        assert_eq!(fmt_duration(5.0 / 1440.0), "0:05:00");
        assert_eq!(fmt_duration(90.0 / 1440.0), "1:30:00");
        assert_eq!(fmt_duration(2.0 / 24.0), "2:00:00");
        assert_eq!(fmt_duration(30.0), "720:00:00");
        let midnight = calamine::Data::DateTime(calamine::ExcelDateTime::new(
            0.0,
            calamine::ExcelDateTimeType::DateTime,
            false,
        ));
        assert_eq!(cell_text(&midnight), "00:00:00");
    }

    #[test]
    fn utf16_tables_decode_before_splitting() {
        let p = std::env::temp_dir().join(format!("alook-u16-{}.tsv", std::process::id()));
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "name\tcity\r\nMaya\tParis\r\n".encode_utf16() {
            bytes.extend(unit.to_le_bytes());
        }
        std::fs::write(&p, bytes).unwrap();
        let t = read_csv(&p, "tsv").unwrap();
        assert_eq!(t.rows, vec![vec!["name", "city"], vec!["Maya", "Paris"]]);
        let _ = std::fs::remove_file(p);
    }
}
