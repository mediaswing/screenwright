//! Render a [`Screenplay`] to industry-standard fixed-width text.
//!
//! Margins follow the conventional US screenplay layout measured in
//! characters on a 12pt Courier / 8.5"x11" page (≈61 chars wide between the
//! 1.5" left and 1" right margins):
//!
//! ```text
//! Scene heading / action  flush left
//! Character cue           indented ~22 cols
//! Parenthetical           indented ~16 cols, wrapped to ~31 wide
//! Dialogue                indented ~10 cols, wrapped to ~35 wide
//! Transition              right-aligned within the page
//! ```

use crate::element::{Element, Screenplay};

const PAGE_WIDTH: usize = 61;
const ACTION_WIDTH: usize = 61;
const DIALOGUE_INDENT: usize = 10;
const DIALOGUE_WIDTH: usize = 35;
const PAREN_INDENT: usize = 16;
const PAREN_WIDTH: usize = 31;
const CHARACTER_INDENT: usize = 22;

/// Render an entire screenplay, title page included, to a printable string.
pub fn render(sp: &Screenplay) -> String {
    let mut out = String::new();

    if !sp.title_page.is_empty() {
        out.push_str(&render_title_page(sp));
        out.push_str("\n\n");
    }

    let mut prev_was_dialogue = false;
    for el in &sp.body {
        match el {
            Element::SceneHeading(t) => {
                blank_before(&mut out);
                out.push_str(&t.to_uppercase());
                out.push_str("\n\n");
                prev_was_dialogue = false;
            }
            Element::Action(t) => {
                blank_before(&mut out);
                for para_line in t.split('\n') {
                    out.push_str(&wrap(para_line, ACTION_WIDTH, 0));
                    out.push('\n');
                }
                out.push('\n');
                prev_was_dialogue = false;
            }
            Element::Character(name) => {
                if !prev_was_dialogue {
                    blank_before(&mut out);
                }
                out.push_str(&indent(&name.to_uppercase(), CHARACTER_INDENT));
                out.push('\n');
                prev_was_dialogue = true;
            }
            Element::Parenthetical(t) => {
                out.push_str(&wrap(t, PAREN_WIDTH, PAREN_INDENT));
                out.push('\n');
                prev_was_dialogue = true;
            }
            Element::Dialogue(t) => {
                out.push_str(&wrap(t, DIALOGUE_WIDTH, DIALOGUE_INDENT));
                out.push('\n');
                prev_was_dialogue = true;
            }
            Element::Transition(t) => {
                blank_before(&mut out);
                out.push_str(&right_align(&t.to_uppercase(), PAGE_WIDTH));
                out.push_str("\n\n");
                prev_was_dialogue = false;
            }
            Element::Centered(t) => {
                blank_before(&mut out);
                out.push_str(&center(t, PAGE_WIDTH));
                out.push_str("\n\n");
                prev_was_dialogue = false;
            }
            Element::PageBreak => {
                out.push_str("\n");
                out.push_str(&"=".repeat(PAGE_WIDTH));
                out.push_str("\n\n");
                prev_was_dialogue = false;
            }
        }
    }

    // Collapse any trailing blank lines to a single newline.
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// Render the title page, with the title centered roughly a third down.
fn render_title_page(sp: &Screenplay) -> String {
    let mut out = String::new();
    if let Some(title) = sp.meta("title") {
        out.push_str(&"\n".repeat(8));
        for line in title.split('\n') {
            out.push_str(&center(&line.to_uppercase(), PAGE_WIDTH));
            out.push('\n');
        }
        out.push('\n');
    }
    if let Some(credit) = sp.meta("credit") {
        out.push_str(&center(credit, PAGE_WIDTH));
        out.push('\n');
    }
    if let Some(author) = sp.meta("author").or_else(|| sp.meta("authors")) {
        out.push_str(&center(author, PAGE_WIDTH));
        out.push('\n');
    }
    // Remaining fields (Draft date, Contact, …) go bottom-left.
    out.push_str(&"\n".repeat(6));
    for (k, v) in &sp.title_page {
        let lower = k.to_lowercase();
        if matches!(lower.as_str(), "title" | "credit" | "author" | "authors") {
            continue;
        }
        out.push_str(&format!("{k}: {v}\n"));
    }
    out
}

/// Insert a blank separator line unless output already ends with one.
fn blank_before(out: &mut String) {
    if !out.is_empty() && !out.ends_with("\n\n") {
        if out.ends_with('\n') {
            out.push('\n');
        } else {
            out.push_str("\n\n");
        }
    }
}

/// Word-wrap `text` to `width` columns, prefixing every line with `indent`.
fn wrap(text: &str, width: usize, indent_cols: usize) -> String {
    let pad = " ".repeat(indent_cols);
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(format!("{pad}{current}"));
            current = word.to_string();
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(format!("{pad}{current}"));
    }
    lines.join("\n")
}

fn indent(text: &str, cols: usize) -> String {
    format!("{}{}", " ".repeat(cols), text)
}

fn right_align(text: &str, width: usize) -> String {
    let pad = width.saturating_sub(text.chars().count());
    format!("{}{}", " ".repeat(pad), text)
}

fn center(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.to_string();
    }
    let pad = (width - len) / 2;
    format!("{}{}", " ".repeat(pad), text)
}
