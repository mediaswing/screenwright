//! Compute production-useful statistics over a parsed screenplay.

use crate::element::{Element, Screenplay};
use std::collections::BTreeMap;

/// Aggregated metrics for a screenplay.
pub struct Stats {
    pub scenes: usize,
    pub action_words: usize,
    pub dialogue_words: usize,
    /// Dialogue-line count per character, normalized to the bare name.
    pub lines_per_character: BTreeMap<String, usize>,
    /// Estimated page count (≈55 typeset lines per page).
    pub estimated_pages: usize,
}

const LINES_PER_PAGE: usize = 55;

/// Walk the body once and accumulate [`Stats`].
pub fn compute(sp: &Screenplay, rendered: &str) -> Stats {
    let mut scenes = 0;
    let mut action_words = 0;
    let mut dialogue_words = 0;
    let mut lines_per_character: BTreeMap<String, usize> = BTreeMap::new();
    let mut current_speaker: Option<String> = None;

    for el in &sp.body {
        match el {
            Element::SceneHeading(_) => scenes += 1,
            Element::Action(t) => action_words += word_count(t),
            Element::Character(name) => {
                current_speaker = Some(normalize_character(name));
            }
            Element::Dialogue(t) => {
                dialogue_words += word_count(t);
                if let Some(name) = &current_speaker {
                    *lines_per_character.entry(name.clone()).or_insert(0) += 1;
                }
            }
            _ => {}
        }
    }

    let rendered_lines = rendered.lines().count();
    let estimated_pages = rendered_lines.div_ceil(LINES_PER_PAGE).max(1);

    Stats {
        scenes,
        action_words,
        dialogue_words,
        lines_per_character,
        estimated_pages,
    }
}

/// Strip a trailing extension like `(V.O.)` and uppercase for grouping.
fn normalize_character(name: &str) -> String {
    let core = match name.find('(') {
        Some(p) => name[..p].trim(),
        None => name.trim(),
    };
    core.to_uppercase()
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

impl Stats {
    /// Format the statistics as a human-readable report.
    pub fn report(&self, title: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!("Screenplay statistics — {title}\n"));
        out.push_str(&"-".repeat(40));
        out.push('\n');
        out.push_str(&format!("Estimated pages : {}\n", self.estimated_pages));
        out.push_str(&format!("Scenes          : {}\n", self.scenes));
        out.push_str(&format!("Action words    : {}\n", self.action_words));
        out.push_str(&format!("Dialogue words  : {}\n", self.dialogue_words));

        if !self.lines_per_character.is_empty() {
            out.push_str("\nDialogue lines by character:\n");
            // Sort by line count descending, then name.
            let mut rows: Vec<(&String, &usize)> = self.lines_per_character.iter().collect();
            rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let widest = rows.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
            for (name, count) in rows {
                out.push_str(&format!("  {name:<widest$}  {count}\n"));
            }
        }
        out
    }
}
