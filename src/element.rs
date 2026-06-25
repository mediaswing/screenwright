//! Screenplay element model.
//!
//! A parsed screenplay is a title page (key/value metadata) plus an ordered
//! list of body elements. Each element corresponds to a recognized Fountain
//! construct.

/// A single structural unit of a screenplay body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Element {
    /// A slug line, e.g. `INT. KITCHEN - DAY`.
    SceneHeading(String),
    /// Descriptive action / narrative text.
    Action(String),
    /// A speaking character cue, e.g. `MARY (V.O.)`.
    Character(String),
    /// A `(beat)`-style direction nested under a character.
    Parenthetical(String),
    /// A line of spoken dialogue.
    Dialogue(String),
    /// A transition, e.g. `CUT TO:`.
    Transition(String),
    /// Centered text (`> text <`).
    Centered(String),
    /// A page break (`===`).
    PageBreak,
}

/// A complete parsed screenplay.
#[derive(Debug, Default, Clone)]
pub struct Screenplay {
    /// Title-page fields in source order (e.g. `Title`, `Author`).
    pub title_page: Vec<(String, String)>,
    /// Body elements in document order.
    pub body: Vec<Element>,
}

impl Screenplay {
    /// Look up a title-page field case-insensitively.
    pub fn meta(&self, key: &str) -> Option<&str> {
        self.title_page
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
}
