use anyhow::{Result, anyhow};
use calamine::{Data, Reader, open_workbook_auto};
use cfb::CompoundFile;
use docx_rs::{
    DocumentChild, Docx, Paragraph, ParagraphChild, Run, RunChild, Table, TableCell,
    TableCellContent, read_docx,
};
use encoding_rs::{Encoding, WINDOWS_1252};
use epub::doc::EpubDoc;
use pdfium_render::prelude::*;
#[cfg(target_os = "macos")]
use std::ffi::OsStr;
use std::io::Read;
#[cfg(target_os = "macos")]
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
use uuid::Uuid;

pub fn load_any_file(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "doc" => load_doc(path),
        "docx" => load_docx(path),
        "pdf" => load_pdf(path),
        #[cfg(target_os = "macos")]
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "tif" | "tiff" | "webp" | "heic" => {
            load_image_with_macos_ocr(path)
        }
        "epub" => load_epub(path),
        "rtf" => load_rtf(path),
        "xlsx" | "xls" | "ods" => load_spreadsheet(path),
        "html" | "htm" => load_html(path),
        _ => {
            let bytes = std::fs::read(path)?;
            Ok(decode_text_bytes(&bytes))
        }
    }
}

fn decode_text_bytes(bytes: &[u8]) -> String {
    if let Some(text) = decode_text_bytes_with_bom(bytes) {
        return text;
    }
    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
        return text;
    }
    if let Some(text) = decode_utf16_without_bom(bytes) {
        return text;
    }
    let (decoded, _, _) = WINDOWS_1252.decode(bytes);
    decoded.into_owned()
}

fn decode_text_bytes_with_bom(bytes: &[u8]) -> Option<String> {
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => Some(String::from_utf8_lossy(rest).into_owned()),
        [0xFF, 0xFE, rest @ ..] => Some(decode_utf16_units(rest, true)),
        [0xFE, 0xFF, rest @ ..] => Some(decode_utf16_units(rest, false)),
        _ => None,
    }
}

fn decode_utf16_without_bom(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 4 || bytes.len() % 2 != 0 {
        return None;
    }
    let even_nuls = bytes.iter().step_by(2).filter(|&&byte| byte == 0).count();
    let odd_nuls = bytes.iter().skip(1).step_by(2).filter(|&&byte| byte == 0).count();
    let pair_count = bytes.len() / 2;
    if odd_nuls >= pair_count / 4 && even_nuls <= pair_count / 16 {
        return Some(decode_utf16_units(bytes, true));
    }
    if even_nuls >= pair_count / 4 && odd_nuls <= pair_count / 16 {
        return Some(decode_utf16_units(bytes, false));
    }
    None
}

fn decode_utf16_units(bytes: &[u8], little_endian: bool) -> String {
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&units)
}

fn load_doc(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path).map_err(|e| anyhow!("Errore DOC: {}", e))?;
    match CompoundFile::open(&file) {
        Ok(mut comp) => {
            let buffer = {
                let mut stream = comp
                    .open_stream("WordDocument")
                    .map_err(|_| anyhow!("Errore DOC: stream WordDocument mancante"))?;
                let mut buffer = Vec::new();
                stream
                    .read_to_end(&mut buffer)
                    .map_err(|e| anyhow!("Errore DOC: {}", e))?;
                buffer
            };

            let mut table_bytes = Vec::new();
            if let Ok(mut table_stream) = comp.open_stream("1Table") {
                if let Err(err) = table_stream.read_to_end(&mut table_bytes) {
                    return Err(anyhow!("Errore DOC: {}", err));
                }
            } else if let Ok(mut table_stream) = comp.open_stream("0Table")
                && let Err(err) = table_stream.read_to_end(&mut table_bytes)
            {
                return Err(anyhow!("Errore DOC: {}", err));
            }

            if !table_bytes.is_empty()
                && let Some(text) = extract_doc_text_piece_table(&buffer, &table_bytes)
            {
                return Ok(clean_doc_text(text));
            }

            let text_utf16 = extract_utf16_strings(&buffer);
            let text_ascii = extract_ascii_strings(&buffer);

            if text_utf16.len() > 100 {
                return Ok(clean_doc_text(text_utf16));
            }
            if !text_ascii.is_empty() {
                return Ok(clean_doc_text(text_ascii));
            }
            Ok(clean_doc_text(text_utf16))
        }
        Err(_) => {
            let bytes = std::fs::read(path).map_err(|e| anyhow!("Errore DOC: {}", e))?;
            if looks_like_rtf(&bytes) {
                return Ok(extract_rtf_text(&bytes));
            }
            if let Ok(text) = load_docx(path) {
                return Ok(clean_doc_text(text));
            }

            let text_utf16 = extract_utf16_strings(&bytes);
            if text_utf16.len() > 100 {
                return Ok(clean_doc_text(text_utf16));
            }
            let text_ascii = extract_ascii_strings(&bytes);
            if !text_ascii.is_empty() {
                return Ok(clean_doc_text(text_ascii));
            }
            if !text_utf16.is_empty() {
                return Ok(clean_doc_text(text_utf16));
            }
            Err(anyhow!("Errore DOC: formato non riconosciuto"))
        }
    }
}

fn load_docx(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let docx = read_docx(&bytes).map_err(|e| anyhow!("Errore DOCX: {}", e))?;
    Ok(extract_docx_text(&docx))
}

fn extract_docx_text(docx: &Docx) -> String {
    let mut out = String::new();
    for child in &docx.document.children {
        append_document_child_text(&mut out, child);
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

fn append_document_child_text(out: &mut String, child: &DocumentChild) {
    match child {
        DocumentChild::Paragraph(p) => {
            append_paragraph_text(out, p);
            out.push('\n');
        }
        DocumentChild::Table(t) => append_table_text(out, t),
        _ => {}
    }
}

fn append_paragraph_text(out: &mut String, paragraph: &Paragraph) {
    for child in &paragraph.children {
        append_paragraph_child_text(out, child);
    }
}

fn append_paragraph_child_text(out: &mut String, child: &ParagraphChild) {
    match child {
        ParagraphChild::Run(run) => append_run_text(out, run),
        ParagraphChild::Hyperlink(link) => {
            for child in &link.children {
                append_paragraph_child_text(out, child);
            }
        }
        _ => {}
    }
}

fn append_run_text(out: &mut String, run: &Run) {
    for child in &run.children {
        match child {
            RunChild::Text(t) => out.push_str(&t.text),
            RunChild::Tab(_) => out.push('\t'),
            _ => {}
        }
    }
}

fn append_table_text(out: &mut String, table: &Table) {
    for row in &table.rows {
        let docx_rs::TableChild::TableRow(row) = row;
        let mut first_cell = true;
        for cell in &row.cells {
            let docx_rs::TableRowChild::TableCell(cell) = cell;
            if !first_cell {
                out.push('\t');
            }
            first_cell = false;
            out.push_str(&extract_table_cell_text(cell));
        }
        out.push('\n');
    }
}

fn extract_table_cell_text(cell: &TableCell) -> String {
    let mut out = String::new();
    for content in &cell.children {
        match content {
            TableCellContent::Paragraph(p) => {
                append_paragraph_text(&mut out, p);
                out.push('\n');
            }
            TableCellContent::Table(t) => append_table_text(&mut out, t),
            _ => {}
        }
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

fn load_pdf(path: &Path) -> Result<String> {
    log_pdf_debug(&format!("start path={}", path.display()));
    let text = extract_pdf_text_with_fallback(path)?;
    #[cfg(target_os = "macos")]
    log_pdf_debug(&format!(
        "raw_extraction length={} meaningful={}",
        text.len(),
        is_probably_meaningful_pdf_text(&text)
    ));
    #[cfg(not(target_os = "macos"))]
    log_pdf_debug(&format!("raw_extraction length={}", text.len()));
    #[cfg(target_os = "macos")]
    if should_attempt_macos_pdf_ocr(&text) {
        log_pdf_debug(&format!(
            "macos_ocr.selected reason={}",
            macos_pdf_ocr_reason(&text)
        ));
        let ocr_text = extract_pdf_text_macos_ocr(path)?;
        if ocr_text.trim().is_empty() {
            log_pdf_debug("macos_ocr.completed length=0");
            return Ok(String::new());
        }
        log_pdf_debug(&format!("macos_ocr.completed length={}", ocr_text.len()));
        let repaired_text = repair_pdf_text_encoding(&ocr_text);
        return Ok(normalize_pdf_paragraphs(&repaired_text));
    }

    #[cfg(not(target_os = "macos"))]
    if text.trim().is_empty() {
        log_pdf_debug("raw_extraction.empty");
        return Ok(String::new());
    }
    #[cfg(target_os = "macos")]
    log_pdf_debug("macos_ocr.skipped using_raw_pdf_text=true");
    let repaired_text = repair_pdf_text_encoding(&text);
    Ok(normalize_pdf_paragraphs(&repaired_text))
}

fn extract_pdf_text_with_fallback(path: &Path) -> Result<String> {
    let extract_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_extract::extract_text(path)
    }));
    match extract_result {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(err)) => extract_pdf_text_pdfium(path)
            .map_err(|pdfium_err| anyhow!("Errore PDF: {} / fallback pdfium: {}", err, pdfium_err)),
        Err(_) => extract_pdf_text_pdfium(path).map_err(|pdfium_err| {
            anyhow!("Errore PDF: crash parser / fallback pdfium: {}", pdfium_err)
        }),
    }
}

fn extract_pdf_text_pdfium(path: &Path) -> Result<String> {
    let bindings = bind_pdfium_library()?;
    let pdfium = Pdfium::new(bindings);
    let document = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|err| anyhow!("pdfium load failed: {err}"))?;
    let mut out = String::new();
    for page in document.pages().iter() {
        let page_text = page
            .text()
            .map_err(|err| anyhow!("pdfium page text failed: {err}"))?;
        let text = page_text.all();
        if !text.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&text);
        }
    }
    Ok(out)
}

fn repair_pdf_text_encoding(text: &str) -> String {
    let Some(repaired) = try_repair_pdf_utf8_mojibake(text) else {
        return text.to_string();
    };
    if should_use_repaired_pdf_text(text, &repaired) {
        repaired
    } else {
        text.to_string()
    }
}

fn try_repair_pdf_utf8_mojibake(text: &str) -> Option<String> {
    let (encoded, _, had_errors) = WINDOWS_1252.encode(text);
    if had_errors {
        return None;
    }
    String::from_utf8(encoded.into_owned()).ok()
}

fn should_use_repaired_pdf_text(current: &str, repaired: &str) -> bool {
    if current == repaired {
        return false;
    }

    let current_mojibake = mojibake_latin1_score(current) + mojibake_cp1252_symbol_score(current);
    if current_mojibake == 0 {
        return false;
    }

    let repaired_mojibake =
        mojibake_latin1_score(repaired) + mojibake_cp1252_symbol_score(repaired);
    if repaired_mojibake >= current_mojibake {
        return false;
    }

    let current_replacements = current.chars().filter(|&ch| ch == '\u{FFFD}').count();
    let repaired_replacements = repaired.chars().filter(|&ch| ch == '\u{FFFD}').count();
    if repaired_replacements > current_replacements {
        return false;
    }

    let current_western = western_european_char_score(current);
    let repaired_western = western_european_char_score(repaired);

    repaired_western > current_western || repaired_mojibake + 2 <= current_mojibake
}

#[cfg(target_os = "macos")]
fn should_attempt_macos_pdf_ocr(text: &str) -> bool {
    text.trim().is_empty() || !is_probably_meaningful_pdf_text(text)
}

#[cfg(target_os = "macos")]
fn macos_pdf_ocr_reason(text: &str) -> &'static str {
    if text.trim().is_empty() {
        "raw_text_empty"
    } else {
        "raw_text_not_meaningful"
    }
}

#[cfg(any(target_os = "macos", test))]
fn is_probably_meaningful_pdf_text(text: &str) -> bool {
    let mut visible = 0usize;
    let mut controls = 0usize;
    let mut alnum = 0usize;

    for ch in text.chars() {
        if ch.is_whitespace() {
            continue;
        }
        if ch.is_control() {
            controls += 1;
            continue;
        }
        visible += 1;
        if ch.is_alphanumeric() {
            alnum += 1;
        }
    }

    if visible == 0 {
        return false;
    }
    if controls > visible / 8 {
        return false;
    }

    let alnum_ratio = alnum as f32 / visible as f32;
    alnum >= 24 || (visible >= 12 && alnum_ratio >= 0.55)
}

#[cfg(target_os = "macos")]
fn extract_pdf_text_macos_ocr(path: &Path) -> Result<String> {
    let image_dir = render_pdf_pages_for_macos_ocr(path)?;
    let mut image_paths = collect_macos_ocr_image_paths(&image_dir)?;
    let result = extract_text_from_image_paths_macos_ocr(&image_paths);

    if let Err(err) = std::fs::remove_dir_all(&image_dir) {
        println!(
            "ERROR: rimozione immagini OCR macOS fallita: {} ({})",
            image_dir.display(),
            err
        );
    }
    image_paths.clear();

    result
}

#[cfg(target_os = "macos")]
fn load_image_with_macos_ocr(path: &Path) -> Result<String> {
    let image_paths = vec![path.to_path_buf()];
    let text = extract_text_from_image_paths_macos_ocr(&image_paths)?;
    let normalized = normalize_pdf_paragraphs(&text);
    if normalized.trim().is_empty() {
        return Err(anyhow!("OCR immagine macOS non ha trovato testo"));
    }
    Ok(normalized)
}

#[cfg(target_os = "macos")]
fn extract_text_from_image_paths_macos_ocr(image_paths: &[PathBuf]) -> Result<String> {
    let script_path = write_macos_pdf_ocr_script()?;
    let swift = macos_swift_command()
        .ok_or_else(|| anyhow!("OCR PDF macOS non disponibile: interpreter Swift non trovato"))?;

    let mut command = Command::new(&swift);
    if swift.file_name() == Some(OsStr::new("xcrun")) {
        command.arg("swift");
    }
    command.arg(&script_path);
    for image_path in image_paths {
        command.arg(image_path);
    }
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| anyhow!("OCR PDF macOS fallito: {}", err))?;

    if let Err(err) = std::fs::remove_file(&script_path) {
        println!(
            "ERROR: rimozione script OCR macOS fallita: {} ({})",
            script_path.display(),
            err
        );
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    for line in stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        log_pdf_debug(&format!("macos_helper {line}"));
    }

    if !output.status.success() {
        let detail = if stderr.trim().is_empty() {
            format!("uscita {:?}", output.status.code())
        } else {
            stderr.trim().to_string()
        };
        return Err(anyhow!("OCR PDF macOS fallito: {}", detail));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(target_os = "macos")]
fn render_pdf_pages_for_macos_ocr(path: &Path) -> Result<PathBuf> {
    let bindings = bind_pdfium_library()?;
    let pdfium = Pdfium::new(bindings);
    let document = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|err| anyhow!("pdfium render OCR load failed: {err}"))?;
    let image_dir =
        std::env::temp_dir().join(format!("sonarpad_minimal_pdfium_ocr_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&image_dir)
        .map_err(|err| anyhow!("creazione cartella immagini OCR macOS fallita: {}", err))?;

    for (page_index, page) in document.pages().iter().enumerate() {
        let bitmap = page
            .render_with_config(
                &PdfRenderConfig::new()
                    .set_target_width(2200)
                    .render_form_data(true)
                    .use_lcd_text_rendering(true)
                    .use_print_quality(true),
            )
            .map_err(|err| anyhow!("pdfium render OCR pagina {} fallito: {err}", page_index + 1))?;
        let image = bitmap.as_image();
        let image_path = image_dir.join(format!("page_{:04}.png", page_index + 1));
        image.save(&image_path).map_err(|err| {
            anyhow!(
                "salvataggio immagine OCR macOS pagina {} fallito: {}",
                page_index + 1,
                err
            )
        })?;
    }

    Ok(image_dir)
}

#[cfg(target_os = "macos")]
fn collect_macos_ocr_image_paths(image_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut image_paths = std::fs::read_dir(image_dir)
        .map_err(|err| anyhow!("lettura cartella immagini OCR macOS fallita: {}", err))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|err| anyhow!("enumerazione immagini OCR macOS fallita: {}", err))?;
    image_paths.sort();
    Ok(image_paths)
}

#[cfg(target_os = "macos")]
fn macos_swift_command() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("/usr/bin/swift"),
        PathBuf::from("/usr/bin/xcrun"),
    ];
    candidates.into_iter().find(|candidate| candidate.exists())
}

#[cfg(target_os = "macos")]
fn write_macos_pdf_ocr_script() -> Result<PathBuf> {
    let script_path =
        std::env::temp_dir().join(format!("sonarpad_minimal_pdf_ocr_{}.swift", Uuid::new_v4()));
    let mut file = std::fs::File::create(&script_path)
        .map_err(|err| anyhow!("creazione script OCR macOS fallita: {}", err))?;
    file.write_all(MACOS_PDF_OCR_SWIFT.as_bytes())
        .map_err(|err| anyhow!("scrittura script OCR macOS fallita: {}", err))?;
    Ok(script_path)
}

#[cfg(target_os = "macos")]
const MACOS_PDF_OCR_SWIFT: &str = r#"import AppKit
import Foundation
import PDFKit
import Vision

func logInfo(_ message: String) {
    fputs("PDF macOS: \(message)\n", stderr)
}

func appendPageText(_ text: String, pageNumber: Int, to output: inout String) {
    let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return }
    if !output.isEmpty {
        output.append("\n\n")
    }
    output.append("Pagina \(pageNumber)\n")
    output.append(trimmed)
}

func textQualityScore(_ text: String) -> Int {
    let lower = " \(text.lowercased()) "
    let markers = [
        " di ", " del ", " della ", " dei ", " il ", " la ", " e ", " per ",
        " con ", " che ", " ai ", " articolo ", " decreto ", " iva ", " handicap "
    ]
    let markerScore = markers.reduce(0) { partial, marker in
        partial + lower.components(separatedBy: marker).count - 1
    }
    let letters = text.unicodeScalars.filter { CharacterSet.letters.contains($0) }.count
    let digits = text.unicodeScalars.filter { CharacterSet.decimalDigits.contains($0) }.count
    let punctuation = text.filter { "§|[]{}<>".contains($0) }.count
    return markerScore * 40 + letters - digits * 2 - punctuation * 4
}

func cleanedText(_ text: String?) -> String {
    guard let text else { return "" }
    return text
        .replacingOccurrences(of: "\u{00a0}", with: " ")
        .trimmingCharacters(in: .whitespacesAndNewlines)
}

func recognizeText(from imageUrl: URL) throws -> String {
    guard let image = NSImage(contentsOf: imageUrl) else {
        throw NSError(domain: "SonarpadOCR", code: 2, userInfo: [NSLocalizedDescriptionKey: "unable to load image"])
    }
    guard let tiff = image.tiffRepresentation,
          let bitmap = NSBitmapImageRep(data: tiff),
          let cgImage = bitmap.cgImage else {
        throw NSError(domain: "SonarpadOCR", code: 3, userInfo: [NSLocalizedDescriptionKey: "unable to build cgimage"])
    }

    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    request.recognitionLanguages = ["it-IT"]
    request.usesLanguageCorrection = true

    let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
    try handler.perform([request])

    let observations = (request.results ?? []).compactMap { observation -> (CGRect, String)? in
        guard let candidate = observation.topCandidates(1).first else { return nil }
        let text = cleanedText(candidate.string)
        guard !text.isEmpty else { return nil }
        return (observation.boundingBox, text)
    }

    let sorted = observations.sorted { lhs, rhs in
        if abs(lhs.0.midY - rhs.0.midY) > 0.015 {
            return lhs.0.midY > rhs.0.midY
        }
        return lhs.0.minX < rhs.0.minX
    }

    var lines: [[(CGRect, String)]] = []
    for observation in sorted {
        if let lastIndex = lines.indices.last {
            let lastMidY = lines[lastIndex][0].0.midY
            if abs(lastMidY - observation.0.midY) <= 0.015 {
                lines[lastIndex].append(observation)
                continue
            }
        }
        lines.append([observation])
    }

    let mergedLines = lines.map { line -> String in
        let sortedLine = line.sorted { lhs, rhs in lhs.0.minX < rhs.0.minX }
        return sortedLine.map(\.1).joined(separator: " ")
    }

    return mergedLines.joined(separator: "\n")
}

guard CommandLine.arguments.count >= 2 else {
    fputs("missing image paths\n", stderr)
    exit(2)
}

var output = ""
for (index, imagePath) in CommandLine.arguments.dropFirst().enumerated() {
    autoreleasepool {
        do {
            let recognized = try recognizeText(from: URL(fileURLWithPath: imagePath))
            let score = textQualityScore(recognized)
            logInfo("page=\(index + 1) source=pdfium_vision length=\(recognized.count) score=\(score)")
            appendPageText(recognized, pageNumber: index + 1, to: &output)
        } catch {
            logInfo("page=\(index + 1) source=pdfium_vision_error length=0 score=0 detail=\(error.localizedDescription)")
        }
    }
}

FileHandle.standardOutput.write(output.data(using: .utf8) ?? Data())
"#;

fn bind_pdfium_library() -> Result<Box<dyn PdfiumLibraryBindings>> {
    for candidate in pdfium_library_candidates() {
        if !candidate.exists() {
            continue;
        }
        if let Ok(bindings) = Pdfium::bind_to_library(&candidate) {
            return Ok(bindings);
        }
    }

    Pdfium::bind_to_system_library().map_err(|err| anyhow!("pdfium bind failed: {err}"))
}

fn pdfium_library_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        candidates.push(exe_dir.join(pdfium_library_file_name()));

        #[cfg(target_os = "macos")]
        if let Some(contents_dir) = exe_dir.parent() {
            candidates.push(
                contents_dir
                    .join("Frameworks")
                    .join(pdfium_library_file_name()),
            );
            candidates.push(contents_dir.join("MacOS").join(pdfium_library_file_name()));
        }
    }

    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(pdfium_library_file_name()));
    }

    candidates.push(Pdfium::pdfium_platform_library_name_at_path("."));
    dedupe_paths(candidates)
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        if !unique.iter().any(|seen| seen == &path) {
            unique.push(path);
        }
    }
    unique
}

#[cfg(target_os = "windows")]
fn pdfium_library_file_name() -> &'static str {
    "pdfium.dll"
}

#[cfg(target_os = "macos")]
fn pdfium_library_file_name() -> &'static str {
    "libpdfium.dylib"
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn pdfium_library_file_name() -> &'static str {
    "libpdfium.so"
}

fn normalize_pdf_paragraphs(text: &str) -> String {
    let mut out = String::new();
    let mut current = String::new();
    let avg_len = average_pdf_line_len(text);
    let mut last_line = String::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            flush_pdf_paragraph(&mut out, &mut current);
            last_line.clear();
            continue;
        }
        if is_pdf_page_marker(line) {
            flush_pdf_paragraph(&mut out, &mut current);
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(line);
            last_line.clear();
            continue;
        }
        if current.is_empty() {
            current.push_str(line);
            last_line.clear();
            last_line.push_str(line);
            continue;
        }
        if looks_like_list_item(line) {
            flush_pdf_paragraph(&mut out, &mut current);
            current.push_str(line);
            last_line.clear();
            last_line.push_str(line);
            continue;
        }
        if should_break_pdf_paragraph(&last_line, line, avg_len) {
            flush_pdf_paragraph(&mut out, &mut current);
            current.push_str(line);
            last_line.clear();
            last_line.push_str(line);
            continue;
        }
        if last_line.ends_with('-') {
            last_line.pop();
            current.pop();
            current.push_str(line);
        } else {
            if !current.ends_with(' ') {
                current.push(' ');
            }
            current.push_str(line);
        }
        last_line.clear();
        last_line.push_str(line);
    }
    flush_pdf_paragraph(&mut out, &mut current);
    out
}

#[cfg(any(target_os = "macos", windows))]
fn log_pdf_debug(message: &str) {
    crate::append_podcast_log(&format!("PDF: {message}"));
}

#[cfg(not(any(target_os = "macos", windows)))]
fn log_pdf_debug(_message: &str) {}

fn western_european_char_score(text: &str) -> usize {
    text.chars()
        .filter(|ch| {
            matches!(
                ch,
                'à' | 'è'
                    | 'ì'
                    | 'ò'
                    | 'ù'
                    | 'À'
                    | 'È'
                    | 'Ì'
                    | 'Ò'
                    | 'Ù'
                    | 'á'
                    | 'é'
                    | 'í'
                    | 'ó'
                    | 'ú'
                    | 'Á'
                    | 'É'
                    | 'Í'
                    | 'Ó'
                    | 'Ú'
                    | 'â'
                    | 'ê'
                    | 'î'
                    | 'ô'
                    | 'û'
                    | 'Â'
                    | 'Ê'
                    | 'Î'
                    | 'Ô'
                    | 'Û'
                    | 'ã'
                    | 'õ'
                    | 'Ã'
                    | 'Õ'
                    | 'ç'
                    | 'Ç'
                    | 'ñ'
                    | 'Ñ'
            )
        })
        .count()
}

fn mojibake_latin1_score(text: &str) -> usize {
    text.chars()
        .filter(|ch| {
            matches!(
                ch,
                'Â' | 'Ã'
                    | 'Ä'
                    | 'Å'
                    | 'Æ'
                    | 'Ç'
                    | 'Ð'
                    | 'Ñ'
                    | 'Ò'
                    | 'Ó'
                    | 'Ô'
                    | 'Õ'
                    | 'Ö'
                    | '×'
                    | 'Ø'
                    | 'Ù'
                    | 'Ú'
                    | 'Û'
                    | 'Ü'
                    | 'Ý'
                    | 'Þ'
                    | 'ß'
                    | 'à'
                    | 'á'
                    | 'â'
                    | 'ã'
                    | 'ä'
                    | 'å'
                    | 'æ'
                    | 'ç'
                    | 'è'
                    | 'é'
                    | 'ê'
                    | 'ë'
                    | 'ì'
                    | 'í'
                    | 'î'
                    | 'ï'
                    | 'ð'
                    | 'ñ'
                    | 'ò'
                    | 'ó'
                    | 'ô'
                    | 'õ'
                    | 'ö'
                    | 'ø'
                    | 'ù'
                    | 'ú'
                    | 'û'
                    | 'ü'
                    | 'ý'
                    | 'þ'
                    | 'ÿ'
            )
        })
        .count()
}

fn mojibake_cp1252_symbol_score(text: &str) -> usize {
    text.chars()
        .filter(|ch| {
            matches!(
                ch,
                '‚' | 'ƒ'
                    | '„'
                    | '…'
                    | '†'
                    | '‡'
                    | 'ˆ'
                    | '‰'
                    | 'Š'
                    | '‹'
                    | 'Œ'
                    | 'Ž'
                    | '‘'
                    | '’'
                    | '“'
                    | '”'
                    | '•'
                    | '–'
                    | '—'
                    | '˜'
                    | '™'
                    | 'š'
                    | '›'
                    | 'œ'
                    | 'ž'
                    | 'Ÿ'
            )
        })
        .count()
}

fn flush_pdf_paragraph(out: &mut String, current: &mut String) {
    if current.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(current.trim());
    current.clear();
}

fn is_pdf_page_marker(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("Pagina ") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit())
}

fn should_break_pdf_paragraph(prev: &str, next: &str, avg_len: usize) -> bool {
    if prev.is_empty() || avg_len == 0 {
        return false;
    }
    let ends_sentence = prev.ends_with('.') || prev.ends_with('!') || prev.ends_with('?');
    let starts_new = next
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false);
    if prev.len() < (avg_len * 8 / 10) && ends_sentence {
        return true;
    }
    ends_sentence && starts_new
}

fn average_pdf_line_len(text: &str) -> usize {
    let mut total = 0usize;
    let mut count = 0usize;
    for raw_line in text.lines().take(2000) {
        let line = raw_line.trim();
        if line.is_empty() || looks_like_list_item(line) {
            continue;
        }
        total += line.len();
        count += 1;
    }
    total.checked_div(count).unwrap_or(0)
}

fn looks_like_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        return true;
    }
    let chars = trimmed.chars();
    let mut digits = 0usize;
    for c in chars {
        if c.is_ascii_digit() {
            digits += 1;
        } else if c == '.' && digits > 0 {
            return true;
        } else {
            break;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::is_probably_meaningful_pdf_text;

    #[test]
    fn pdf_text_heuristic_accepts_normal_text() {
        let text = "Modulo compilato da Mario Rossi.\nCodice fiscale RSSMRA80A01H501U.\nFirma del richiedente.";
        assert!(is_probably_meaningful_pdf_text(text));
    }

    #[test]
    fn pdf_text_heuristic_rejects_scan_garbage() {
        let text = "x\u{0003}?\u{0010}&?\u{0007}??\\\\???\n.?\\?\\\0?\u{000B}\t?\nendstream";
        assert!(!is_probably_meaningful_pdf_text(text));
    }
}

fn looks_like_rtf(bytes: &[u8]) -> bool {
    let mut start = 0usize;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        start = 3;
    }
    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    bytes
        .get(start..start + 5)
        .map(|s| s == b"{\\rtf")
        .unwrap_or(false)
}

struct DocPiece {
    offset: usize,
    cp_len: usize,
    compressed: bool,
}

fn extract_doc_text_piece_table(word: &[u8], table: &[u8]) -> Option<String> {
    let pieces = find_piece_table(table)?;
    let mut out = String::new();
    for piece in pieces {
        if piece.cp_len == 0 {
            continue;
        }
        if piece.compressed {
            let end = piece.offset.saturating_add(piece.cp_len);
            if end > word.len() {
                continue;
            }
            let (decoded, _, _) = WINDOWS_1252.decode(&word[piece.offset..end]);
            out.push_str(&decoded);
        } else {
            let byte_len = piece.cp_len.saturating_mul(2);
            let end = piece.offset.saturating_add(byte_len);
            if end > word.len() {
                continue;
            }
            let mut utf16 = Vec::with_capacity(byte_len / 2);
            for chunk in word[piece.offset..end].chunks_exact(2) {
                utf16.push(u16::from_le_bytes([chunk[0], chunk[1]]));
            }
            out.push_str(&String::from_utf16_lossy(&utf16));
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out.replace('\r', "\n"))
}

fn find_piece_table(table: &[u8]) -> Option<Vec<DocPiece>> {
    let mut best: Option<Vec<DocPiece>> = None;
    let mut i = 0usize;
    while i + 5 <= table.len() {
        if table[i] != 0x02 {
            i += 1;
            continue;
        }
        let lcb = table
            .get(i + 1..i + 5)
            .and_then(|slice| slice.try_into().ok())
            .map(u32::from_le_bytes)? as usize;
        let start = i + 5;
        let end = start.saturating_add(lcb);
        if lcb < 4 || end > table.len() {
            i += 1;
            continue;
        }
        if let Some(pieces) = parse_plc_pcd(&table[start..end])
            && best
                .as_ref()
                .map(|b| pieces.len() > b.len())
                .unwrap_or(true)
        {
            best = Some(pieces);
        }
        i += 1;
    }
    best
}

fn parse_plc_pcd(data: &[u8]) -> Option<Vec<DocPiece>> {
    if data.len() < 4 {
        return None;
    }
    let remaining = data.len().saturating_sub(4);
    if !remaining.is_multiple_of(12) {
        return None;
    }
    let piece_count = remaining / 12;
    if piece_count == 0 {
        return None;
    }
    let cp_count = piece_count + 1;
    let mut cps = Vec::with_capacity(cp_count);
    for idx in 0..cp_count {
        let value = data
            .get(idx * 4..idx * 4 + 4)
            .and_then(|slice| slice.try_into().ok())
            .map(u32::from_le_bytes)?;
        cps.push(value);
    }
    if cps.windows(2).any(|w| w[1] < w[0]) {
        return None;
    }
    let mut pieces = Vec::with_capacity(piece_count);
    let pcd_start = cp_count * 4;
    for idx in 0..piece_count {
        let off = pcd_start + idx * 8;
        if off + 8 > data.len() {
            return None;
        }
        let fc_raw = data
            .get(off + 2..off + 6)
            .and_then(|slice| slice.try_into().ok())
            .map(u32::from_le_bytes)?;
        let compressed = (fc_raw & 1) == 1;
        let fc = fc_raw & 0xFFFFFFFE;
        let offset = if compressed {
            (fc as usize) / 2
        } else {
            fc as usize
        };
        pieces.push(DocPiece {
            offset,
            cp_len: (cps[idx + 1].saturating_sub(cps[idx])) as usize,
            compressed,
        });
    }
    Some(pieces)
}

fn clean_doc_text(text: String) -> String {
    let mut cleaned = String::new();
    for line in text.lines() {
        let trimmed = line.trim_matches(|c: char| c.is_whitespace() || c.is_control());
        if trimmed.is_empty() || is_likely_garbage(trimmed) || trimmed.contains("11252") {
            continue;
        }
        cleaned.push_str(line);
        cleaned.push('\n');
    }
    cleaned
}

fn extract_utf16_strings(buffer: &[u8]) -> String {
    let mut text = String::new();
    let mut current_seq = Vec::new();
    for chunk in buffer.chunks_exact(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        if (unit >= 32 && unit != 0xFFFF) || unit == 10 || unit == 13 || unit == 9 {
            current_seq.push(unit);
            if current_seq.len() > 10000 {
                let s = String::from_utf16_lossy(&current_seq);
                if !is_likely_garbage(&s) {
                    text.push_str(&s);
                    text.push('\n');
                }
                current_seq.clear();
            }
        } else {
            if current_seq.len() > 5 {
                let s = String::from_utf16_lossy(&current_seq);
                if !is_likely_garbage(&s) {
                    text.push_str(&s);
                    text.push('\n');
                }
            }
            current_seq.clear();
        }
    }
    if current_seq.len() > 5 {
        let s = String::from_utf16_lossy(&current_seq);
        if !is_likely_garbage(&s) {
            text.push_str(&s);
        }
    }
    text
}

fn extract_ascii_strings(buffer: &[u8]) -> String {
    let mut text = String::new();
    let mut current_seq = Vec::new();
    for &byte in buffer {
        if (32..=126).contains(&byte) || byte == 10 || byte == 13 || byte == 9 {
            current_seq.push(byte);
            if current_seq.len() > 10000 {
                if let Ok(s) = String::from_utf8(current_seq.clone())
                    && !is_likely_garbage(&s)
                {
                    text.push_str(&s);
                    text.push('\n');
                }
                current_seq.clear();
            }
        } else {
            if current_seq.len() > 5
                && let Ok(s) = String::from_utf8(current_seq.clone())
                && !is_likely_garbage(&s)
            {
                text.push_str(&s);
                text.push('\n');
            }
            current_seq.clear();
        }
    }
    text
}

fn is_likely_garbage(s: &str) -> bool {
    let trimmed = s.trim_matches(|c: char| c.is_whitespace() || c.is_control());
    if s.contains("1125211")
        || s.contains("11252")
        || s.contains("Arial;")
        || s.contains("Times New Roman;")
        || s.contains("Courier New;")
    {
        return true;
    }
    if trimmed.starts_with('*') && trimmed.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) {
        return true;
    }
    if s.contains('|') && trimmed.chars().take(5).all(|c| c.is_ascii_digit()) {
        return true;
    }
    if s.contains("'01") || s.contains("'02") || s.contains("'03") {
        return true;
    }
    let letter_count = s.chars().filter(|c| c.is_alphabetic()).count();
    let digit_count = s.chars().filter(|c| c.is_ascii_digit()).count();
    let symbol_count = s
        .chars()
        .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
        .count();
    if letter_count == 0 {
        return true;
    }
    if (digit_count + symbol_count) * 2 > letter_count {
        return true;
    }
    let mut max_digit_run = 0usize;
    let mut current_digit_run = 0usize;
    for c in s.chars() {
        if c.is_ascii_digit() {
            current_digit_run += 1;
        } else {
            max_digit_run = max_digit_run.max(current_digit_run);
            current_digit_run = 0;
        }
    }
    max_digit_run = max_digit_run.max(current_digit_run);
    max_digit_run > 4
}

fn extract_rtf_text(bytes: &[u8]) -> String {
    fn is_skip_destination(keyword: &str) -> bool {
        matches!(
            keyword,
            "fonttbl"
                | "colortbl"
                | "stylesheet"
                | "info"
                | "pict"
                | "object"
                | "filetbl"
                | "datastore"
                | "themedata"
                | "header"
                | "headerl"
                | "headerr"
                | "headerf"
                | "footer"
                | "footerl"
                | "footerr"
                | "footerf"
                | "generator"
                | "xmlopen"
                | "xmlattrname"
                | "xmlattrvalue"
        )
    }

    fn hex_val(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }

    fn emit_char(out: &mut String, skip_output: &mut usize, in_skip: bool, ch: char) {
        if *skip_output > 0 {
            *skip_output -= 1;
            return;
        }
        if in_skip {
            return;
        }
        match ch {
            '\r' | '\0' => {}
            '\n' => out.push('\n'),
            _ => out.push(ch),
        }
    }

    fn emit_str(out: &mut String, skip_output: &mut usize, in_skip: bool, s: &str) {
        for ch in s.chars() {
            emit_char(out, skip_output, in_skip, ch);
        }
    }

    fn encoding_from_codepage(codepage: i32) -> Option<&'static Encoding> {
        let label = if codepage == 65001 {
            "utf-8".to_string()
        } else {
            format!("windows-{}", codepage)
        };
        Encoding::for_label(label.as_bytes())
    }

    let mut out = String::new();
    let mut i = 0usize;
    let mut group_stack = vec![false];
    let mut uc_skip = 1usize;
    let mut skip_output = 0usize;
    let mut encoding: &'static Encoding = WINDOWS_1252;

    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                group_stack.push(*group_stack.last().unwrap_or(&false));
                i += 1;
            }
            b'}' => {
                if group_stack.len() > 1 {
                    group_stack.pop();
                }
                i += 1;
            }
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                match bytes[i] {
                    b'\\' | b'{' | b'}' => {
                        emit_char(
                            &mut out,
                            &mut skip_output,
                            *group_stack.last().unwrap_or(&false),
                            bytes[i] as char,
                        );
                        i += 1;
                    }
                    b'~' => {
                        emit_char(
                            &mut out,
                            &mut skip_output,
                            *group_stack.last().unwrap_or(&false),
                            ' ',
                        );
                        i += 1;
                    }
                    b'-' | b'_' => {
                        emit_char(
                            &mut out,
                            &mut skip_output,
                            *group_stack.last().unwrap_or(&false),
                            '-',
                        );
                        i += 1;
                    }
                    b'*' => {
                        if let Some(last) = group_stack.last_mut() {
                            *last = true;
                        }
                        i += 1;
                    }
                    b'\'' if i + 2 < bytes.len() => {
                        let h1 = bytes[i + 1];
                        let h2 = bytes[i + 2];
                        if let (Some(n1), Some(n2)) = (hex_val(h1), hex_val(h2)) {
                            let byte = (n1 << 4) | n2;
                            let buf = [byte];
                            let (decoded, _, _) = encoding.decode(&buf);
                            emit_str(
                                &mut out,
                                &mut skip_output,
                                *group_stack.last().unwrap_or(&false),
                                &decoded,
                            );
                            i += 3;
                        } else {
                            i += 1;
                        }
                    }
                    b'\'' => {
                        i += 1;
                    }
                    b if b.is_ascii_alphabetic() => {
                        let start = i;
                        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        let keyword = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                        let mut sign = 1i32;
                        if i < bytes.len() && bytes[i] == b'-' {
                            sign = -1;
                            i += 1;
                        }
                        let mut value = 0i32;
                        let mut has_digit = false;
                        while i < bytes.len() && bytes[i].is_ascii_digit() {
                            has_digit = true;
                            value = value * 10 + (bytes[i] - b'0') as i32;
                            i += 1;
                        }
                        let num = if has_digit { Some(value * sign) } else { None };
                        if i < bytes.len() && bytes[i] == b' ' {
                            i += 1;
                        }
                        match keyword {
                            "par" | "line" => emit_char(
                                &mut out,
                                &mut skip_output,
                                *group_stack.last().unwrap_or(&false),
                                '\n',
                            ),
                            "tab" => emit_char(
                                &mut out,
                                &mut skip_output,
                                *group_stack.last().unwrap_or(&false),
                                '\t',
                            ),
                            "emdash" => emit_str(
                                &mut out,
                                &mut skip_output,
                                *group_stack.last().unwrap_or(&false),
                                "--",
                            ),
                            "endash" => emit_char(
                                &mut out,
                                &mut skip_output,
                                *group_stack.last().unwrap_or(&false),
                                '-',
                            ),
                            "bullet" => emit_char(
                                &mut out,
                                &mut skip_output,
                                *group_stack.last().unwrap_or(&false),
                                '*',
                            ),
                            "u" => {
                                if let Some(n) = num {
                                    let mut code = n;
                                    if code < 0 {
                                        code += 65536;
                                    }
                                    if let Some(ch) = char::from_u32(code as u32) {
                                        emit_char(
                                            &mut out,
                                            &mut skip_output,
                                            *group_stack.last().unwrap_or(&false),
                                            ch,
                                        );
                                    }
                                    skip_output = uc_skip;
                                }
                            }
                            "uc" => {
                                if let Some(n) = num
                                    && n >= 0
                                {
                                    uc_skip = n as usize;
                                }
                            }
                            "ansicpg" => {
                                if let Some(n) = num
                                    && let Some(enc) = encoding_from_codepage(n)
                                {
                                    encoding = enc;
                                }
                            }
                            _ => {
                                if is_skip_destination(keyword)
                                    && let Some(last) = group_stack.last_mut()
                                {
                                    *last = true;
                                }
                            }
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            b'\r' | b'\n' => {
                i += 1;
            }
            b => {
                if b >= 0x80 {
                    let buf = [b];
                    let (decoded, _, _) = encoding.decode(&buf);
                    emit_str(
                        &mut out,
                        &mut skip_output,
                        *group_stack.last().unwrap_or(&false),
                        &decoded,
                    );
                } else {
                    emit_char(
                        &mut out,
                        &mut skip_output,
                        *group_stack.last().unwrap_or(&false),
                        b as char,
                    );
                }
                i += 1;
            }
        }
    }
    out
}

fn load_rtf(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(extract_rtf_text(&bytes))
}

fn load_epub(path: &Path) -> Result<String> {
    let mut doc = EpubDoc::new(path).map_err(|e| anyhow!("Errore EPUB: {}", e))?;
    let mut full_text = String::new();

    if let Some(title_item) = doc.mdata("title") {
        full_text.push_str(&title_item.value);
        full_text.push_str("\n\n");
    }

    let spine = doc.spine.clone();
    for item in spine {
        if let Some((content, mime)) = doc.get_resource(&item.idref)
            && (mime.contains("xhtml") || mime.contains("html") || mime.contains("xml"))
        {
            let text = String::from_utf8(content.clone())
                .unwrap_or_else(|_| String::from_utf8_lossy(&content).to_string());
            let cleaned = html_to_text(&text);
            for line in cleaned.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty()
                    || is_epub_metadata_noise_line(trimmed)
                    || (trimmed.starts_with("part") && trimmed.len() <= 12)
                {
                    continue;
                }
                full_text.push_str(trimmed);
                full_text.push('\n');
            }
            full_text.push('\n');
        }
    }

    if full_text.trim().is_empty() {
        return Err(anyhow!("Errore EPUB: nessun testo rilevato"));
    }

    Ok(full_text)
}

fn load_spreadsheet(path: &Path) -> Result<String> {
    let mut workbook = open_workbook_auto(path).map_err(|e| anyhow!("Errore Excel: {}", e))?;
    let mut out = String::new();
    if let Some(Ok(range)) = workbook.worksheet_range_at(0) {
        for row in range.rows() {
            let mut first = true;
            for cell in row {
                if !first {
                    out.push('\t');
                }
                first = false;
                match cell {
                    Data::Empty => {}
                    Data::String(s) => out.push_str(s),
                    Data::Float(f) => out.push_str(&f.to_string()),
                    Data::Int(i) => out.push_str(&i.to_string()),
                    Data::Bool(b) => out.push_str(&b.to_string()),
                    Data::Error(e) => out.push_str(&format!("{:?}", e)),
                    Data::DateTime(f) => out.push_str(&f.to_string()),
                    Data::DateTimeIso(s) | Data::DurationIso(s) => out.push_str(s),
                }
            }
            out.push('\n');
        }
    } else {
        return Err(anyhow!("Errore Excel: nessun foglio disponibile"));
    }
    Ok(out)
}

fn load_html(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let html = decode_text_bytes(&bytes);
    Ok(html_to_text(&html))
}

fn is_epub_metadata_noise_line(line: &str) -> bool {
    let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
    normalized.eq_ignore_ascii_case("epub r1.0")
        || normalized.eq_ignore_ascii_case("epub base r2.1")
}

fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    let mut tag = String::new();
    let mut last_newline = false;
    let mut skip_stack: Vec<String> = Vec::new();
    let mut in_comment = false;

    for ch in html.chars() {
        if in_comment {
            tag.push(ch);
            if tag.ends_with("-->") {
                in_comment = false;
                tag.clear();
            }
            continue;
        }

        if inside {
            if ch == '>' {
                inside = false;
                let tag_trimmed = tag.trim();
                if tag_trimmed.starts_with("!--") {
                    if !tag_trimmed.ends_with("--") {
                        in_comment = true;
                    }
                    tag.clear();
                    continue;
                }

                let tag_name = tag_trimmed
                    .trim()
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let is_closing = tag_trimmed.starts_with('/');

                if matches!(tag_name.as_str(), "head" | "style" | "script" | "title") {
                    if is_closing {
                        if let Some(pos) = skip_stack.iter().rposition(|t| t == &tag_name) {
                            skip_stack.truncate(pos);
                        }
                    } else {
                        skip_stack.push(tag_name.clone());
                    }
                    tag.clear();
                    continue;
                }
                if matches!(
                    tag_name.as_str(),
                    "br" | "p"
                        | "div"
                        | "li"
                        | "tr"
                        | "hr"
                        | "ul"
                        | "ol"
                        | "table"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                ) && skip_stack.is_empty()
                    && !last_newline
                    && !out.is_empty()
                {
                    out.push('\n');
                    last_newline = true;
                }
                tag.clear();
            } else {
                tag.push(ch);
            }
            continue;
        }
        if ch == '<' {
            inside = true;
            continue;
        }
        if !skip_stack.is_empty() {
            continue;
        }
        out.push(ch);
        last_newline = ch == '\n';
    }

    out.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}
