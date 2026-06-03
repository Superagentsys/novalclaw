use calamine::{open_workbook_auto, Data, Reader};
use std::path::Path;

pub struct ParsedSheet {
    pub name: String,
    pub rows: Vec<Vec<String>>,
}

pub fn parse_excel_file(path: &Path, max_rows_per_sheet: usize) -> anyhow::Result<Vec<ParsedSheet>> {
    let mut workbook = open_workbook_auto(path)?;
    let sheet_names = workbook.sheet_names().to_vec();
    let mut sheets = Vec::new();

    for name in sheet_names {
        let range = workbook
            .worksheet_range(&name)
            .map_err(|e| anyhow::anyhow!("sheet '{name}': {e}"))?;
        let mut rows = Vec::new();
        for (idx, row) in range.rows().enumerate() {
            if idx >= max_rows_per_sheet {
                break;
            }
            let cells: Vec<String> = row.iter().map(cell_to_string).collect();
            if cells.iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            rows.push(cells);
        }
        if !rows.is_empty() {
            sheets.push(ParsedSheet {
                name,
                rows,
            });
        }
    }

    if sheets.is_empty() {
        anyhow::bail!("Excel 文件中没有可导入的数据行");
    }
    Ok(sheets)
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Float(f) => {
            if f.fract().abs() < f64::EPSILON {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(f) => f.to_string(),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("#ERR({e:?})"),
    }
}

pub fn row_to_chunk_text(sheet: &str, row_index: usize, cells: &[String], headers: Option<&[String]>) -> String {
    let parts: Vec<String> = if let Some(headers) = headers {
        cells
            .iter()
            .enumerate()
            .map(|(i, value)| {
                let label = headers
                    .get(i)
                    .filter(|h| !h.trim().is_empty())
                    .map(|h| h.as_str())
                    .unwrap_or("列");
                format!("{label}: {value}")
            })
            .collect()
    } else {
        cells
            .iter()
            .enumerate()
            .map(|(i, value)| format!("列{}: {value}", i + 1))
            .collect()
    };
    format!(
        "[表:{sheet} 行:{}] {}",
        row_index + 1,
        parts.join(" | ")
    )
}

pub fn detect_headers(first_row: &[String]) -> Option<Vec<String>> {
    if first_row.is_empty() {
        return None;
    }
    let non_empty = first_row.iter().filter(|c| !c.trim().is_empty()).count();
    if non_empty == 0 {
        return None;
    }
    let numeric_like = first_row
        .iter()
        .filter(|c| !c.trim().is_empty())
        .filter(|c| c.parse::<f64>().is_ok())
        .count();
    if numeric_like * 2 >= non_empty {
        return None;
    }
    Some(
        first_row
            .iter()
            .map(|c| c.trim().to_string())
            .collect(),
    )
}
