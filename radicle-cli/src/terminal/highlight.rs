use std::{collections::HashMap, path::Path};

use radicle_term as term;
use tree_sitter_highlight as ts;

/// Highlight groups enabled.
const HIGHLIGHTS: &[&str] = &[
    "attribute",
    "constant",
    "constant.builtin",
    "comment",
    "constructor",
    "function.builtin",
    "function",
    "integer_literal",
    "float.literal",
    "keyword",
    "label",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "text.literal",
    "text.title",
];

/// Syntax highlighter based on `tree-sitter`.
#[derive(Default)]
pub struct Highlighter {
    configs: HashMap<&'static str, ts::HighlightConfiguration>,
}

/// Syntax theme.
pub struct Theme {
    color: fn(&'static str) -> Option<term::Color>,
}

impl Default for Theme {
    fn default() -> Self {
        let color = if term::Paint::truecolor() {
            term::colors::rgb::theme
        } else {
            term::colors::fixed::theme
        };
        Self { color }
    }
}

impl Theme {
    /// Get the named color.
    pub fn color(&self, color: &'static str) -> term::Color {
        if let Some(c) = (self.color)(color) {
            c
        } else {
            term::Color::Unset
        }
    }

    /// Return the color of a syntax group.
    pub fn highlight(&self, group: &'static str) -> Option<term::Color> {
        let color = match group {
            "keyword" => self.color("red"),
            "comment" => self.color("grey"),
            "constant" => self.color("orange"),
            "number" => self.color("blue"),
            "string" => self.color("teal"),
            "string.special" => self.color("green"),
            "function" => self.color("purple"),
            "operator" => self.color("blue"),
            // Eg. `true` and `false` in rust.
            "constant.builtin" => self.color("blue"),
            "type.builtin" => self.color("teal"),
            "punctuation.bracket" | "punctuation.delimiter" => term::Color::default(),
            // Eg. the '#' in Markdown titles.
            "punctuation.special" => self.color("dim"),
            // Eg. Markdown code blocks.
            "text.literal" => self.color("blue"),
            "text.title" => self.color("orange"),
            "variable.builtin" => term::Color::default(),
            "property" => self.color("blue"),
            // Eg. `#[derive(Debug)]` in rust
            "attribute" => self.color("blue"),
            "label" => self.color("green"),
            // `Option`
            "type" => self.color("grey.light"),
            "variable.parameter" => term::Color::default(),
            "constructor" => self.color("orange"),

            _ => return None,
        };
        Some(color)
    }
}

/// Syntax highlighted file builder.
#[derive(Default)]
struct Builder {
    /// Output lines.
    lines: Vec<term::Line>,
    /// Current output line.
    line: Vec<term::Label>,
    /// Current label.
    label: Vec<u8>,
    /// Current stack of styles.
    styles: Vec<term::Style>,
}

impl Builder {
    /// Run the builder to completion.
    fn run(
        mut self,
        highlights: impl Iterator<Item = Result<ts::HighlightEvent, ts::Error>>,
        code: &[u8],
        theme: &Theme,
    ) -> Result<Vec<term::Line>, ts::Error> {
        for event in highlights {
            match event? {
                ts::HighlightEvent::Source { start, end } => {
                    for (i, byte) in code.iter().enumerate().skip(start).take(end - start) {
                        if *byte == b'\n' {
                            self.advance();
                            // Start on new line.
                            self.lines.push(term::Line::from(self.line.clone()));
                            self.line.clear();
                        } else if i == code.len() - 1 {
                            // File has no `\n` at the end.
                            self.label.push(*byte);
                            self.advance();
                            self.lines.push(term::Line::from(self.line.clone()));
                        } else {
                            // Add to existing label.
                            self.label.push(*byte);
                        }
                    }
                }
                ts::HighlightEvent::HighlightStart(h) => {
                    let name = HIGHLIGHTS[h.0];
                    let style =
                        term::Style::default().fg(theme.highlight(name).unwrap_or_default());

                    self.advance();
                    self.styles.push(style);
                }
                ts::HighlightEvent::HighlightEnd => {
                    self.advance();
                    self.styles.pop();
                }
            }
        }
        Ok(self.lines)
    }

    /// Advance the state by pushing the current label onto the current line,
    /// using the current styling.
    fn advance(&mut self) {
        if !self.label.is_empty() {
            // Take the top-level style when there are more than one.
            let style = self.styles.first().cloned().unwrap_or_default();
            self.line
                .push(term::Label::new(String::from_utf8_lossy(&self.label).as_ref()).style(style));
            self.label.clear();
        }
    }
}

impl Highlighter {
    /// Highlight a source code file.
    pub fn highlight(&mut self, path: &Path, code: &[u8]) -> Result<Vec<term::Line>, ts::Error> {
        let theme = Theme::default();
        let mut highlighter = ts::Highlighter::new();
        let Some(config) = self.detect(path, code) else {
            let Ok(code) = std::str::from_utf8(code) else {
                return Err(ts::Error::Unknown);
            };
            return Ok(code.lines().map(term::Line::new).collect());
        };
        config.configure(HIGHLIGHTS);

        let highlights = highlighter.highlight(config, code, None, |_| {
            // Language injection callback.
            None
        })?;

        Builder::default().run(highlights, code, &theme)
    }

    /// Detect language.
    fn detect(&mut self, path: &Path, _code: &[u8]) -> Option<&mut ts::HighlightConfiguration> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => self.config("rust"),
            Some("ts" | "js") => self.config("typescript"),
            Some("json") => self.config("json"),
            Some("sh" | "bash") => self.config("shell"),
            Some("md" | "markdown") => self.config("markdown"),
            Some("go") => self.config("go"),
            Some("c") => self.config("c"),
            Some("py") => self.config("python"),
            Some("rb") => self.config("ruby"),
            Some("tsx") => self.config("tsx"),
            Some("html") | Some("htm") | Some("xml") => self.config("html"),
            Some("css") => self.config("css"),
            Some("toml") => self.config("toml"),
            _ => None,
        }
    }

    /// Get a language configuration.
    fn config(&mut self, language: &'static str) -> Option<&mut ts::HighlightConfiguration> {
        match language {
            "rust" => Some(self.configs.entry(language).or_insert_with(|| {
                ts::HighlightConfiguration::new(
                    tree_sitter_rust::language(),
                    tree_sitter_rust::HIGHLIGHT_QUERY,
                    tree_sitter_rust::INJECTIONS_QUERY,
                    "",
                )
                .expect("Highlighter::config: highlight configuration must be valid")
            })),
            "json" => Some(self.configs.entry(language).or_insert_with(|| {
                ts::HighlightConfiguration::new(
                    tree_sitter_json::language(),
                    tree_sitter_json::HIGHLIGHT_QUERY,
                    "",
                    "",
                )
                .expect("Highlighter::config: highlight configuration must be valid")
            })),
            "typescript" => Some(self.configs.entry(language).or_insert_with(|| {
                ts::HighlightConfiguration::new(
                    tree_sitter_typescript::language_typescript(),
                    tree_sitter_typescript::HIGHLIGHT_QUERY,
                    "",
                    tree_sitter_typescript::LOCALS_QUERY,
                )
                .expect("Highlighter::config: highlight configuration must be valid")
            })),
            "markdown" => Some(self.configs.entry(language).or_insert_with(|| {
                ts::HighlightConfiguration::new(
                    tree_sitter_md::language(),
                    tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
                    tree_sitter_md::INJECTION_QUERY_BLOCK,
                    "",
                )
                .expect("Highlighter::config: highlight configuration must be valid")
            })),
            "css" => Some(self.configs.entry(language).or_insert_with(|| {
                ts::HighlightConfiguration::new(
                    tree_sitter_css::language(),
                    tree_sitter_css::HIGHLIGHTS_QUERY,
                    "",
                    "",
                )
                .expect("Highlighter::config: highlight configuration must be valid")
            })),
            "go" => Some(self.configs.entry(language).or_insert_with(|| {
                ts::HighlightConfiguration::new(
                    tree_sitter_go::language(),
                    tree_sitter_go::HIGHLIGHT_QUERY,
                    "",
                    "",
                )
                .expect("Highlighter::config: highlight configuration must be valid")
            })),
            "shell" => Some(self.configs.entry(language).or_insert_with(|| {
                ts::HighlightConfiguration::new(
                    tree_sitter_bash::language(),
                    tree_sitter_bash::HIGHLIGHT_QUERY,
                    "",
                    "",
                )
                .expect("Highlighter::config: highlight configuration must be valid")
            })),
            "c" => Some(self.configs.entry(language).or_insert_with(|| {
                ts::HighlightConfiguration::new(
                    tree_sitter_c::language(),
                    tree_sitter_c::HIGHLIGHT_QUERY,
                    "",
                    "",
                )
                .expect("Highlighter::config: highlight configuration must be valid")
            })),
            "python" => Some(self.configs.entry(language).or_insert_with(|| {
                ts::HighlightConfiguration::new(
                    tree_sitter_python::language(),
                    tree_sitter_python::HIGHLIGHT_QUERY,
                    "",
                    "",
                )
                .expect("Highlighter::config: highlight configuration must be valid")
            })),
            #[cfg(feature = "tree-sitter-ruby")]
            "ruby" => Some(self.configs.entry(language).or_insert_with(|| {
                ts::HighlightConfiguration::new(
                    tree_sitter_ruby::language(),
                    tree_sitter_ruby::HIGHLIGHT_QUERY,
                    "",
                    tree_sitter_ruby::LOCALS_QUERY,
                )
                .expect("Highlighter::config: highlight configuration must be valid")
            })),
            "tsx" => Some(self.configs.entry(language).or_insert_with(|| {
                ts::HighlightConfiguration::new(
                    tree_sitter_typescript::language_tsx(),
                    tree_sitter_typescript::HIGHLIGHT_QUERY,
                    "",
                    tree_sitter_typescript::LOCALS_QUERY,
                )
                .expect("Highlighter::config: highlight configuration must be valid")
            })),
            "html" => Some(self.configs.entry(language).or_insert_with(|| {
                ts::HighlightConfiguration::new(
                    tree_sitter_html::language(),
                    tree_sitter_html::HIGHLIGHTS_QUERY,
                    tree_sitter_html::INJECTIONS_QUERY,
                    "",
                )
                .expect("Highlighter::config: highlight configuration must be valid")
            })),
            "toml" => Some(self.configs.entry(language).or_insert_with(|| {
                ts::HighlightConfiguration::new(
                    tree_sitter_toml::language(),
                    tree_sitter_toml::HIGHLIGHT_QUERY,
                    "",
                    "",
                )
                .expect("Highlighter::config: highlight configuration must be valid")
            })),
            _ => None,
        }
    }
}
