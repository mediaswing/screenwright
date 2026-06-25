# screenwright

> **Note:** This project is in an experimental phase and will not be in active
> development.

A desktop screenplay writing app, written in Rust. You write in the
[Fountain](https://fountain.io) plain-text format on the left and see a live,
industry-standard formatted preview on the right, with page and scene
statistics updating as you type. Finished scripts export to **PDF** and
**editable Word (.docx)**.

## Why Fountain?

Fountain lets you write a screenplay using natural conventions — `INT. ROOM -
DAY` is a scene heading, an UPPERCASE line is a character cue, and so on.
screenwright parses that and does the formatting for you, so the source stays
diff-friendly and version-controllable while the app handles layout.

## Features

- Live two-pane editor: Fountain source ↔ formatted preview
- Statistics panel: page estimate, scene count, word counts, lines per character
- Native open/save of `.fountain` files, with unsaved-change prompts
- **Export to PDF** — 12pt Courier, US-Letter, paginated (matches the preview)
- **Export to DOCX** — an *editable* Word document with real paragraph indents
- Export to plain text
- **AI writing prompts** — generate a story prompt with your own Claude or
  ChatGPT account (optional; nothing is sent anywhere unless you ask)
- Keyboard shortcuts: ⌘N new, ⌘O open, ⌘S save

## AI writing prompts (bring your own account)

Click **Writing prompt…** in the toolbar to open the assistant. It can call
either **Claude (Anthropic)** or **ChatGPT (OpenAI)** using *your* API key —
the app has no built-in account and makes no network calls unless you press
**Generate**.

- Pick a provider, optionally tweak the model, and give an optional topic.
- The key is read from the `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` environment
  variable, or you can paste one into the (masked) field. Keys are kept in
  memory only and never written to disk.
- The request runs on a background thread, so the UI never freezes.
- Insert the result into your screenplay as a Fountain note, or copy it.

Set a key before launching, e.g.:

```sh
export ANTHROPIC_API_KEY=sk-ant-...
# or
export OPENAI_API_KEY=sk-...
cargo run --release
```

## Accessibility

screenwright is built to be usable without a mouse and with a screen reader:

- **Full keyboard operation** — every control (buttons, the Export menu, the
  Statistics checkbox, the editor) is reachable with `Tab` / `Shift+Tab` and
  activatable with `Enter` / `Space`. The editor deliberately does **not** lock
  `Tab`, so focus is never trapped in the text field.
- **Keyboard-scrollable panes** — the formatted preview and statistics are
  focusable, read-only regions: Tab into them and use the arrow keys,
  `PageUp`/`PageDown` and `Home`/`End` to scroll and read. Their text can also
  be selected and copied.
- **Screen reader support** — the UI ships with [AccessKit](https://accesskit.dev)
  (enabled by default via eframe), bridging to VoiceOver (macOS), Narrator
  (Windows) and Orca (Linux). The editor is associated with its visible label,
  so it is announced as "Screenplay source"; the formatted preview and
  statistics are exposed as named text regions.
- **Clear labelling** — controls use descriptive text rather than icon-only
  glyphs, tooltips add extra description, and the status bar speaks in words
  ("Unsaved changes", "3 pages (estimated)") instead of bare symbols.

## Build & run

```sh
cargo run --release      # launches the desktop app
cargo build --release    # binary at target/release/screenwright
```

The screenplay engine (`parser`, `format`, `stats`) is plain `std` Rust; the
desktop layer uses `eframe`/`egui` (UI), `rfd` (native file dialogs),
`printpdf` (PDF) and `docx-rs` (Word).

## Supported Fountain constructs

| Construct        | Syntax                                             |
|------------------|----------------------------------------------------|
| Title page       | `Title:`, `Author:`, … key/value lines at the top |
| Scene heading    | `INT.`/`EXT.`/`EST.`… or forced with leading `.`  |
| Action           | any plain paragraph (force with `!`)              |
| Character cue    | UPPERCASE line, or forced with `@`                |
| Parenthetical    | `(beat)` under a cue                              |
| Dialogue         | text following a cue                              |
| Transition       | `CUT TO:` etc., or forced with `>`               |
| Centered text    | `> text <`                                        |
| Page break       | `===`                                             |
| Notes / boneyard | `[[ ... ]]` and `/* ... */` (stripped on render)  |

## Project layout

- `src/element.rs` — the screenplay data model
- `src/parser.rs`  — Fountain parser (with unit tests)
- `src/format.rs`  — fixed-width industry-standard renderer
- `src/stats.rs`   — statistics and reporting
- `src/export.rs`  — PDF and DOCX exporters (with unit tests)
- `src/ai.rs`      — optional Claude/ChatGPT writing-prompt client (with tests)
- `src/gui.rs`     — the egui/eframe desktop UI
- `src/main.rs`    — app entry point / window setup

## Tests

```sh
cargo test
```

## Disclaimer

screenwright includes an optional feature that calls a third-party AI provider
(Claude or ChatGPT) with *your* own API key. **You are solely responsible for
the screenplays you write and for any AI-generated prompt output you choose to
use.** AI output may be inaccurate, derivative, or unsuitable for your purpose;
review it before relying on it, and ensure your use complies with the chosen
provider's terms of service. screenwright does not claim any rights over your
content and is provided "as is", without warranty of any kind.

## License

screenwright is released under the [MIT License](LICENSE). This license applies
to screenwright's own source code only.

### Third-party licenses

screenwright depends on a number of open-source crates (see `Cargo.toml`), each
under its own license. When distributing a built binary, bundle a notice listing
those dependencies and their licenses. The simplest way to generate one:

```sh
cargo install cargo-about
cargo about init                              # one-time: creates about.toml + template
cargo about generate about.hbs > THIRD_PARTY_LICENSES.html
```

Include the generated `THIRD_PARTY_LICENSES.html` (or a plain-text equivalent)
alongside each release. Alternatively, `cargo install cargo-license && cargo
license` prints a quick summary of dependency licenses.
