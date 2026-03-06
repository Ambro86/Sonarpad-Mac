use anyhow::{Result, anyhow};
use calamine::{Data, Reader, open_workbook_auto};
use docx_rs::{DocumentChild, Paragraph, ParagraphChild, RunChild, TableCellContent, read_docx};
use epub::doc::EpubDoc;
use std::path::Path;

pub fn load_any_file(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "docx" => load_docx(path),
        "pdf" => load_pdf(path),
        "epub" => load_epub(path),
        "xlsx" | "xls" | "ods" => load_spreadsheet(path),
        "html" | "htm" => load_html(path),
        _ => {
            let bytes = std::fs::read(path)?;
            Ok(String::from_utf8_lossy(&bytes).to_string())
        }
    }
}

fn load_docx(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let docx = read_docx(&bytes).map_err(|e| anyhow!("Errore DOCX: {}", e))?;
    let mut out = String::new();
    for child in &docx.document.children {
        append_document_child_text(&mut out, child);
    }
    Ok(out)
}

fn append_document_child_text(out: &mut String, child: &DocumentChild) {
    match child {
        DocumentChild::Paragraph(p) => {
            append_paragraph_text(out, p);
            out.push('\n');
        }
        DocumentChild::Table(t) => {
            for row in &t.rows {
                let docx_rs::TableChild::TableRow(row) = row;
                for cell in &row.cells {
                    let docx_rs::TableRowChild::TableCell(cell) = cell;
                    for content in &cell.children {
                        if let TableCellContent::Paragraph(p) = content {
                            append_paragraph_text(out, p);
                            out.push(' ');
                        }
                    }
                    out.push('\t');
                }
                out.push('\n');
            }
        }
        _ => {}
    }
}

fn append_paragraph_text(out: &mut String, p: &Paragraph) {
    for child in &p.children {
        if let ParagraphChild::Run(run) = child {
            for run_child in &run.children {
                if let RunChild::Text(t) = run_child {
                    out.push_str(&t.text);
                }
            }
        }
    }
}

fn load_pdf(path: &Path) -> Result<String> {
    pdf_extract::extract_text(path).map_err(|e| anyhow!("Errore PDF: {}", e))
}

fn load_epub(path: &Path) -> Result<String> {
    let mut doc = EpubDoc::new(path).map_err(|e| anyhow!("Errore EPUB: {}", e))?;
    let mut out = String::new();

    if let Some(title_items) = doc.mdata("title") {
        out.push_str(&format!("TITOLO: {:?}\n\n", title_items));
    }

    let ids: Vec<String> = doc.resources.keys().cloned().collect();
    for id in ids {
        if let Some((content, mime)) = doc.get_resource(&id)
            && (mime.contains("xhtml") || mime.contains("html"))
        {
            let html = String::from_utf8_lossy(&content);
            if let Ok(text) = html2text::from_read(html.as_bytes(), 80) {
                out.push_str(&text);
                out.push_str("\n\n");
            }
        }
    }
    Ok(out)
}

fn load_spreadsheet(path: &Path) -> Result<String> {
    let mut workbook = open_workbook_auto(path).map_err(|e| anyhow!("Errore Excel: {}", e))?;
    let mut out = String::new();
    for sheet in workbook.sheet_names().to_vec() {
        out.push_str(&format!("--- Foglio: {} ---\n", sheet));
        if let Ok(range) = workbook.worksheet_range(&sheet) {
            for row in range.rows() {
                for cell in row {
                    match cell {
                        Data::Empty => out.push_str(""),
                        Data::String(s) => out.push_str(s),
                        Data::Float(f) => out.push_str(&f.to_string()),
                        Data::Int(i) => out.push_str(&i.to_string()),
                        Data::Bool(b) => out.push_str(&b.to_string()),
                        _ => {}
                    }
                    out.push('\t');
                }
                out.push('\n');
            }
        }
        out.push('\n');
    }
    Ok(out)
}

fn load_html(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let html = String::from_utf8_lossy(&bytes);
    html2text::from_read(html.as_bytes(), 80).map_err(|e| anyhow!("Errore HTML: {}", e))
}
