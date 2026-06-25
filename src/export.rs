//! Export a screenplay to PDF and DOCX.
//!
//! * **PDF** reuses the monospace layout from [`crate::format`], typeset in
//!   12pt Courier so the printed page matches the in-app preview exactly, and
//!   paginated at the conventional ~54 lines per US-Letter page.
//! * **DOCX** is generated from the parsed [`Element`]s directly, producing an
//!   *editable* Word document with real paragraph indents (Courier New 12pt),
//!   so a writer can keep working in Word or Final Draft-compatible tooling.

use std::io;
use std::path::Path;

use docx_rs::{AlignmentType, Docx, Paragraph, Run, RunFonts};
use printpdf::{
    BuiltinFont, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt, TextItem,
};

use crate::element::{Element, Screenplay};

// US Letter, in millimeters.
const PAGE_W_MM: f32 = 215.9;
const PAGE_H_MM: f32 = 279.4;
// 1.5" left margin, 1" top margin (right/bottom implied by the 61-col layout).
const LEFT_MM: f32 = 38.1;
const TOP_MM: f32 = PAGE_H_MM - 25.4;
const FONT_PT: f32 = 12.0;
const LINES_PER_PAGE: usize = 54;

/// 1 inch expressed in twips (1/1440"), the unit DOCX indents use.
const TWIPS_PER_INCH: i32 = 1440;
const COURIER: &str = "Courier New";

// ---------------------------------------------------------------------------
// PDF
// ---------------------------------------------------------------------------

/// Render the formatted (monospace) screenplay text to PDF bytes.
///
/// `formatted` is the output of [`crate::format::render`]; lines made entirely
/// of `=` are treated as forced page breaks.
pub fn to_pdf(title: &str, formatted: &str) -> Vec<u8> {
    let mut doc = PdfDocument::new(title);
    let mut pages = Vec::new();

    for page_lines in paginate(formatted) {
        let mut ops = vec![
            Op::SaveGraphicsState,
            Op::StartTextSection,
            Op::SetFont {
                font: PdfFontHandle::Builtin(BuiltinFont::Courier),
                size: Pt(FONT_PT),
            },
            Op::SetLineHeight { lh: Pt(FONT_PT) },
            Op::SetTextCursor {
                pos: Point::new(Mm(LEFT_MM), Mm(TOP_MM)),
            },
        ];
        for line in page_lines {
            if !line.is_empty() {
                ops.push(Op::ShowText {
                    items: vec![TextItem::Text(line.to_string())],
                });
            }
            // Advance to the next baseline regardless of whether the line had
            // any text, so blank lines preserve vertical spacing.
            ops.push(Op::AddLineBreak);
        }
        ops.push(Op::EndTextSection);
        ops.push(Op::RestoreGraphicsState);

        pages.push(PdfPage::new(Mm(PAGE_W_MM), Mm(PAGE_H_MM), ops));
    }

    doc.with_pages(pages).save(&PdfSaveOptions::default(), &mut Vec::new())
}

/// Split formatted text into pages: at most [`LINES_PER_PAGE`] lines each, with
/// an early break whenever a `===` rule (page break) line is encountered.
fn paginate(formatted: &str) -> Vec<Vec<&str>> {
    let mut pages = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for line in formatted.lines() {
        let trimmed = line.trim();
        let is_rule = trimmed.len() >= 3 && trimmed.chars().all(|c| c == '=');
        if is_rule {
            pages.push(std::mem::take(&mut current));
            continue;
        }
        current.push(line);
        if current.len() >= LINES_PER_PAGE {
            pages.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() || pages.is_empty() {
        pages.push(current);
    }
    pages
}

// ---------------------------------------------------------------------------
// DOCX
// ---------------------------------------------------------------------------

/// Build an editable Word document from the parsed screenplay and write it to
/// `path`.
pub fn write_docx(sp: &Screenplay, path: &Path) -> io::Result<()> {
    let mut docx = Docx::new();

    // Title page: centered title block, then a page break before the body.
    if !sp.title_page.is_empty() {
        docx = docx.add_paragraph(blank());
        docx = docx.add_paragraph(blank());
        docx = docx.add_paragraph(blank());
        if let Some(title) = sp.meta("title") {
            for line in title.split('\n') {
                docx = docx.add_paragraph(centered(&line.to_uppercase(), true));
            }
        }
        docx = docx.add_paragraph(blank());
        if let Some(credit) = sp.meta("credit") {
            docx = docx.add_paragraph(centered(credit, false));
        }
        if let Some(author) = sp.meta("author").or_else(|| sp.meta("authors")) {
            docx = docx.add_paragraph(centered(author, false));
        }
        // Remaining fields, bottom-left-ish.
        for (k, v) in &sp.title_page {
            let lower = k.to_lowercase();
            if matches!(lower.as_str(), "title" | "credit" | "author" | "authors") {
                continue;
            }
            docx = docx.add_paragraph(left(&format!("{k}: {v}"), 0));
        }
        // Start the body on a fresh page.
        docx = docx.add_paragraph(Paragraph::new().page_break_before(true));
    }

    let mut prev_was_dialogue = false;
    for el in &sp.body {
        let para = match el {
            Element::SceneHeading(t) => {
                spacer(&mut docx, prev_was_dialogue);
                prev_was_dialogue = false;
                bold_left(&t.to_uppercase(), 0)
            }
            Element::Action(t) => {
                spacer(&mut docx, prev_was_dialogue);
                prev_was_dialogue = false;
                left(&t.replace('\n', " "), 0)
            }
            Element::Character(name) => {
                spacer(&mut docx, prev_was_dialogue);
                prev_was_dialogue = true;
                left(&name.to_uppercase(), inches(2.2))
            }
            Element::Parenthetical(t) => {
                prev_was_dialogue = true;
                left(t, inches(1.6))
            }
            Element::Dialogue(t) => {
                prev_was_dialogue = true;
                left(t, inches(1.0))
            }
            Element::Transition(t) => {
                spacer(&mut docx, prev_was_dialogue);
                prev_was_dialogue = false;
                aligned(&t.to_uppercase(), AlignmentType::Right)
            }
            Element::Centered(t) => {
                spacer(&mut docx, prev_was_dialogue);
                prev_was_dialogue = false;
                aligned(t, AlignmentType::Center)
            }
            Element::PageBreak => {
                prev_was_dialogue = false;
                Paragraph::new().page_break_before(true)
            }
        };
        docx = docx.add_paragraph(para);
    }

    let file = std::fs::File::create(path)?;
    docx.build()
        .pack(file)
        .map_err(|e| io::Error::other(e.to_string()))?;
    Ok(())
}

/// Add an empty separator paragraph before block-level elements (but not
/// between a character cue and the dialogue that immediately follows it).
fn spacer(docx: &mut Docx, prev_was_dialogue: bool) {
    if !prev_was_dialogue {
        // `Docx::add_paragraph` consumes self; swap through a temporary.
        let taken = std::mem::take(docx);
        *docx = taken.add_paragraph(blank());
    }
}

fn inches(n: f32) -> i32 {
    (n * TWIPS_PER_INCH as f32) as i32
}

/// A Courier New 12pt run.
fn run(text: &str) -> Run {
    Run::new()
        .fonts(RunFonts::new().ascii(COURIER).hi_ansi(COURIER))
        .size(24) // half-points → 12pt
        .add_text(text)
}

fn blank() -> Paragraph {
    Paragraph::new().add_run(run(""))
}

fn left(text: &str, indent_twips: i32) -> Paragraph {
    let p = Paragraph::new().add_run(run(text));
    if indent_twips > 0 {
        p.indent(Some(indent_twips), None, None, None)
    } else {
        p
    }
}

fn bold_left(text: &str, indent_twips: i32) -> Paragraph {
    let p = Paragraph::new().add_run(run(text).bold());
    if indent_twips > 0 {
        p.indent(Some(indent_twips), None, None, None)
    } else {
        p
    }
}

fn aligned(text: &str, align: AlignmentType) -> Paragraph {
    Paragraph::new().add_run(run(text)).align(align)
}

fn centered(text: &str, bold: bool) -> Paragraph {
    let r = if bold { run(text).bold() } else { run(text) };
    Paragraph::new().add_run(r).align(AlignmentType::Center)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{format, parser};

    const SAMPLE: &str = "Title: Test Reel\nAuthor: Tester\n\n\
        FADE IN:\n\nINT. ROOM - DAY\n\nA test scene.\n\n\
        MARY\n(softly)\nHello.\n\n===\n\nEXT. STREET - DAY\n\nMore action.\n\nCUT TO:\n";

    #[test]
    fn pdf_has_valid_header_and_multiple_pages() {
        let sp = parser::parse(SAMPLE);
        let formatted = format::render(&sp);
        let bytes = to_pdf("Test Reel", &formatted);
        assert!(bytes.starts_with(b"%PDF-"), "missing PDF magic header");
        assert!(bytes.ends_with(b"%%EOF") || bytes.len() > 800, "truncated PDF");
        // Two `paginate` pages (split on the `===` rule) should be emitted.
        assert_eq!(paginate(&formatted).len(), 2);
    }

    #[test]
    fn paginate_splits_on_rule() {
        let text = "a\nb\n===\nc\n";
        let pages = paginate(text);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0], vec!["a", "b"]);
        assert_eq!(pages[1], vec!["c"]);
    }

    #[test]
    fn docx_is_a_valid_zip_with_document_xml() {
        let sp = parser::parse(SAMPLE);
        let dir = std::env::temp_dir();
        let path = dir.join("screenwright_export_test.docx");
        write_docx(&sp, &path).expect("write_docx failed");
        let bytes = std::fs::read(&path).unwrap();
        // DOCX is an OOXML zip: it must start with the PK zip signature.
        assert_eq!(&bytes[..2], b"PK", "not a zip / docx");
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        zip.by_name("word/document.xml")
            .expect("missing word/document.xml");
        let _ = std::fs::remove_file(&path);
    }
}
