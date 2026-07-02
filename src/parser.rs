//! A pragmatic parser for a useful subset of the Fountain screenplay format.
//!
//! Fountain is a plain-text markup for screenplays (<https://fountain.io>).
//! This parser implements the constructs writers reach for daily:
//!
//! * Title page key/value pairs at the top of the file.
//! * Scene headings (`INT.`/`EXT.`… or forced with a leading `.`).
//! * Character cues, dialogue, and parentheticals.
//! * Transitions (`… TO:` or forced with a leading `>`).
//! * Centered text (`> text <`) and page breaks (`===`).
//! * Notes (`[[ ]]`) and boneyard (`/* */`) are stripped.

use crate::element::{Element, Screenplay};

/// Parse the full source text of a `.fountain` file into a [`Screenplay`].
pub fn parse(source: &str) -> Screenplay {
    let cleaned = strip_boneyard_and_notes(source);
    let lines: Vec<&str> = cleaned.lines().collect();

    let mut screenplay = Screenplay::default();
    let mut idx = parse_title_page(&lines, &mut screenplay);
    let n = lines.len();

    while idx < n {
        let raw = lines[idx];
        let line = raw.trim_end();
        let trimmed = line.trim();

        // Blank lines separate blocks; they carry no element of their own.
        if trimmed.is_empty() {
            idx += 1;
            continue;
        }

        // Page break: a line of three or more '=' characters.
        if trimmed.len() >= 3 && trimmed.chars().all(|c| c == '=') {
            screenplay.body.push(Element::PageBreak);
            idx += 1;
            continue;
        }

        // Centered text: '> ... <'.
        if trimmed.starts_with('>') && trimmed.ends_with('<') && trimmed.len() > 1 {
            let inner = trimmed[1..trimmed.len() - 1].trim().to_string();
            screenplay.body.push(Element::Centered(inner));
            idx += 1;
            continue;
        }

        // Forced transition ('>') or a natural transition ('... TO:').
        if let Some(rest) = trimmed.strip_prefix('>') {
            screenplay
                .body
                .push(Element::Transition(rest.trim().to_uppercase()));
            idx += 1;
            continue;
        }
        if is_transition(trimmed) {
            screenplay
                .body
                .push(Element::Transition(trimmed.to_string()));
            idx += 1;
            continue;
        }

        // Scene heading: forced with '.' or matching a known prefix.
        if let Some(rest) = forced_scene_heading(trimmed) {
            screenplay.body.push(Element::SceneHeading(rest));
            idx += 1;
            continue;
        }
        if is_scene_heading(trimmed) {
            screenplay
                .body
                .push(Element::SceneHeading(trimmed.to_string()));
            idx += 1;
            continue;
        }

        // Character cue: needs a preceding blank line and following non-blank
        // line, and must look like a cue (uppercase, or forced with '@').
        let prev_blank = idx == 0 || lines[idx - 1].trim().is_empty();
        let next_nonblank = idx + 1 < n && !lines[idx + 1].trim().is_empty();
        if prev_blank && next_nonblank {
            if let Some(name) = character_cue(trimmed) {
                screenplay.body.push(Element::Character(name));
                idx += 1;
                // Consume the dialogue block that follows.
                idx = parse_dialogue_block(&lines, idx, &mut screenplay);
                continue;
            }
        }

        // Anything else is action. Gather consecutive non-blank lines so a
        // multi-line paragraph stays a single element.
        let mut block = vec![strip_action_marker(line)];
        idx += 1;
        while idx < n && !lines[idx].trim().is_empty() {
            block.push(lines[idx].trim_end().to_string());
            idx += 1;
        }
        screenplay.body.push(Element::Action(block.join("\n")));
    }

    screenplay
}

/// Parse the dialogue, parentheticals following a character cue.
/// Returns the index of the first line after the dialogue block.
fn parse_dialogue_block(lines: &[&str], mut idx: usize, sp: &mut Screenplay) -> usize {
    while idx < lines.len() {
        let trimmed = lines[idx].trim();
        if trimmed.is_empty() {
            break;
        }
        if trimmed.starts_with('(') && trimmed.ends_with(')') {
            sp.body.push(Element::Parenthetical(trimmed.to_string()));
        } else {
            sp.body.push(Element::Dialogue(trimmed.to_string()));
        }
        idx += 1;
    }
    idx
}

/// Parse leading `Key: Value` title-page lines. Returns the body start index.
fn parse_title_page(lines: &[&str], sp: &mut Screenplay) -> usize {
    // A title page only exists if the very first line is a recognized title
    // key. Requiring a known key stops a scene-opening line that merely looks
    // like `Key: value` — e.g. `FADE IN:` or `Later: he runs.` — from being
    // swallowed into the title page and vanishing from the body.
    let first = lines.first().map(|l| l.trim()).unwrap_or("");
    if !is_title_page_start(first) {
        return 0;
    }

    let mut idx = 0;
    let mut current_key: Option<String> = None;
    let mut current_val = String::new();

    while idx < lines.len() {
        let line = lines[idx];
        let trimmed = line.trim();

        // A blank line terminates the title page.
        if trimmed.is_empty() {
            idx += 1;
            break;
        }

        if let Some(colon) = key_colon_pos(trimmed) {
            // Flush the previous key before starting a new one.
            if let Some(key) = current_key.take() {
                sp.title_page.push((key, current_val.trim().to_string()));
                current_val.clear();
            }
            let key = trimmed[..colon].trim().to_string();
            let val = trimmed[colon + 1..].trim().to_string();
            current_key = Some(key);
            current_val = val;
        } else if current_key.is_some() {
            // Indented continuation line for the current key.
            if !current_val.is_empty() {
                current_val.push('\n');
            }
            current_val.push_str(trimmed);
        }
        idx += 1;
    }

    if let Some(key) = current_key.take() {
        sp.title_page.push((key, current_val.trim().to_string()));
    }
    idx
}

/// Known scene-heading prefixes (case-insensitive).
const SCENE_PREFIXES: &[&str] = &[
    "INT.", "EXT.", "EST.", "INT./EXT.", "INT/EXT.", "I/E.", "INT ", "EXT ",
];

fn is_scene_heading(line: &str) -> bool {
    let upper = line.to_uppercase();
    SCENE_PREFIXES.iter().any(|p| upper.starts_with(p))
}

/// A line forced into a scene heading with a leading `.` (but not `..`,
/// which escapes to a literal action line beginning with a period).
fn forced_scene_heading(line: &str) -> Option<String> {
    if line.starts_with('.') && !line.starts_with("..") {
        Some(line[1..].trim().to_uppercase())
    } else {
        None
    }
}

fn is_transition(line: &str) -> bool {
    let upper = line.to_uppercase();
    upper == line && (upper.ends_with("TO:") || upper == "CUT TO BLACK." || upper == "FADE OUT.")
}

/// Decide whether a line is a character cue, returning the normalized name.
/// A cue is all-uppercase (ignoring a trailing extension like `(V.O.)`), or
/// forced with a leading `@`.
fn character_cue(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix('@') {
        return Some(rest.trim().to_string());
    }
    // Strip a trailing parenthetical extension before testing case.
    let core = match line.find('(') {
        Some(p) => line[..p].trim(),
        None => line,
    };
    if core.is_empty() {
        return None;
    }
    let has_alpha = core.chars().any(|c| c.is_alphabetic());
    let is_upper = core == core.to_uppercase();
    if has_alpha && is_upper {
        Some(line.to_string())
    } else {
        None
    }
}

/// A line beginning with `!` is forced action; strip the marker.
fn strip_action_marker(line: &str) -> String {
    line.strip_prefix('!').unwrap_or(line).to_string()
}

/// Recognized Fountain title-page keys (case-insensitive). Used only to decide
/// whether a file opens with a title page at all; once one is confirmed, any
/// custom `Key: value` line inside the block is still captured.
const TITLE_KEYS: &[&str] = &[
    "title", "credit", "author", "authors", "source", "notes", "draft date",
    "date", "contact", "copyright", "revision",
];

/// Whether `line` is a title-page key line whose key is a recognized title key,
/// i.e. a plausible start of a title page block.
fn is_title_page_start(line: &str) -> bool {
    match key_colon_pos(line) {
        Some(colon) => TITLE_KEYS.contains(&line[..colon].trim().to_lowercase().as_str()),
        None => false,
    }
}

/// Position of the title-page `:` separator, if this looks like `Key: ...`.
fn key_colon_pos(line: &str) -> Option<usize> {
    let colon = line.find(':')?;
    let key = &line[..colon];
    if key.is_empty() || key.contains(' ') && key.split_whitespace().count() > 3 {
        return None;
    }
    // Keys are short identifiers; reject obvious sentences.
    if key.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '_') {
        Some(colon)
    } else {
        None
    }
}

/// Remove `/* boneyard */` blocks and `[[ note ]]` spans from the source.
///
/// Scans by `char` (not raw bytes) so multi-byte UTF-8 — accented names, curly
/// quotes, em dashes — passes through intact.
fn strip_boneyard_and_notes(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            // Skip to the closing '*/' (or end of input if unterminated).
            chars.next();
            while let Some(c) = chars.next() {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
        } else if c == '[' && chars.peek() == Some(&'[') {
            // Skip to the closing ']]' (or end of input if unterminated).
            chars.next();
            while let Some(c) = chars.next() {
                if c == ']' && chars.peek() == Some(&']') {
                    chars.next();
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scene_and_dialogue() {
        let src = "INT. HOUSE - DAY\n\nMary stands.\n\nMARY\n(softly)\nHello there.\n";
        let sp = parse(src);
        assert_eq!(
            sp.body,
            vec![
                Element::SceneHeading("INT. HOUSE - DAY".into()),
                Element::Action("Mary stands.".into()),
                Element::Character("MARY".into()),
                Element::Parenthetical("(softly)".into()),
                Element::Dialogue("Hello there.".into()),
            ]
        );
    }

    #[test]
    fn parses_title_page() {
        let src = "Title: Big Fish\nAuthor: John August\n\nFADE IN:\n";
        let sp = parse(src);
        assert_eq!(sp.meta("title"), Some("Big Fish"));
        assert_eq!(sp.meta("author"), Some("John August"));
    }

    #[test]
    fn transition_opening_is_not_a_title_page() {
        // `FADE IN:` looks like `Key: value` but must stay in the body rather
        // than being consumed as a title-page field.
        let src = "FADE IN:\n\nINT. HOUSE - DAY\n\nAction.\n";
        let sp = parse(src);
        assert!(sp.title_page.is_empty(), "FADE IN: was swallowed as a title page");
        assert_eq!(sp.body[0], Element::Action("FADE IN:".into()));
        assert_eq!(sp.body[1], Element::SceneHeading("INT. HOUSE - DAY".into()));
    }

    #[test]
    fn forced_heading_and_transition() {
        let src = ".A QUIET ROOM\n\n> FADE OUT <\n\nCUT TO:\n";
        let sp = parse(src);
        assert_eq!(sp.body[0], Element::SceneHeading("A QUIET ROOM".into()));
        assert_eq!(sp.body[1], Element::Centered("FADE OUT".into()));
        assert_eq!(sp.body[2], Element::Transition("CUT TO:".into()));
    }

    #[test]
    fn preserves_non_ascii_text() {
        let src = "Café — “oui” [[cut]] naïve.\n";
        let sp = parse(src);
        match &sp.body[0] {
            // The `[[cut]]` note is removed; the surrounding text (including the
            // multi-byte é, —, “ ” and ï) is preserved byte-for-byte.
            Element::Action(t) => assert_eq!(t, "Café — “oui”  naïve."),
            other => panic!("expected action, got {other:?}"),
        }
    }

    #[test]
    fn strips_notes_and_boneyard() {
        let src = "Action here [[fix this]] continues.\n\n/* hidden\nblock */\n";
        let sp = parse(src);
        assert_eq!(sp.body.len(), 1);
        match &sp.body[0] {
            Element::Action(t) => assert!(!t.contains("fix this") && !t.contains("hidden")),
            other => panic!("expected action, got {other:?}"),
        }
    }
}
