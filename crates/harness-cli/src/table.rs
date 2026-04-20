use crate::style;

pub fn print(headers: &[&str], rows: &[Vec<String>]) {
    if headers.is_empty() {
        return;
    }
    let cols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().take(cols).enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let dim = style::dim();
    let mut header_line = String::new();
    for (i, h) in headers.iter().enumerate() {
        write_cell(&mut header_line, h, widths[i], i + 1 < cols);
    }
    println!("{dim}{header_line}{dim:#}");
    for row in rows {
        let mut line = String::new();
        for (i, width) in widths.iter().enumerate() {
            let cell = row.get(i).map(String::as_str).unwrap_or("");
            write_cell(&mut line, cell, *width, i + 1 < cols);
        }
        println!("{line}");
    }
}

fn write_cell(out: &mut String, cell: &str, width: usize, more: bool) {
    out.push_str(cell);
    let pad = width.saturating_sub(cell.chars().count());
    out.push_str(&" ".repeat(pad));
    if more {
        out.push_str("  ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_headers_is_noop() {
        print(&[], &[]);
    }

    #[test]
    fn missing_cells_pad_to_width() {
        let mut out = String::new();
        write_cell(&mut out, "abc", 6, true);
        assert_eq!(out, "abc     ");
    }

    #[test]
    fn last_cell_has_no_trailing_separator() {
        let mut out = String::new();
        write_cell(&mut out, "abc", 6, false);
        assert_eq!(out, "abc   ");
    }
}
