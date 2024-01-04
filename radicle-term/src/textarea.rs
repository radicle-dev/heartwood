use crate::{cell::Cell, Constraint, Element, Line, Paint, Size};

/// Default text wrap width.
pub const DEFAULT_WRAP: usize = 80;
/// Soft tab replacement for '\t'.
pub const SOFT_TAB: &str = "  ";

/// Text area.
///
/// A block of text that can contain multiple lines.
#[derive(Debug)]
pub struct TextArea {
    body: Paint<String>,
    wrap: usize,
}

impl TextArea {
    /// Create a new text area.
    pub fn new(body: impl Into<Paint<String>>) -> Self {
        Self {
            body: body.into(),
            wrap: DEFAULT_WRAP,
        }
    }

    /// Set wrap width.
    pub fn wrap(mut self, cols: usize) -> Self {
        self.wrap = cols;
        self
    }

    /// Get the lines of text in this text area.
    pub fn lines(&self) -> impl Iterator<Item = String> {
        let mut lines: Vec<String> = Vec::new();
        let mut fenced = false;

        for line in self
            .body
            .content()
            .lines()
            // Replace tabs as their visual width cannot be calculated.
            .map(|l| l.replace('\t', SOFT_TAB))
        {
            // Fenced code block support.
            if line.starts_with("```") {
                fenced = !fenced;
            }
            // Code blocks are not wrapped, they are truncated.
            if fenced || line.starts_with('\t') || line.starts_with(' ') {
                lines.push(line.truncate(self.wrap, "…"));
                continue;
            }
            let mut current = String::new();

            for word in line.split_whitespace() {
                if current.width() + word.width() > self.wrap {
                    lines.push(current.trim_end().to_owned());
                    current = word.to_owned();
                } else {
                    current.push_str(word);
                }
                current.push(' ');
            }
            lines.push(current.trim_end().to_owned());
        }
        lines.into_iter()
    }

    /// Box the text area.
    pub fn boxed(self) -> Box<dyn Element> {
        Box::new(self)
    }
}

impl Element for TextArea {
    fn size(&self, _parent: Constraint) -> Size {
        let cols = self.lines().map(|l| l.width()).max().unwrap_or(0);
        let rows = self.lines().count();

        Size::new(cols, rows)
    }

    fn render(&self, _parent: Constraint) -> Vec<Line> {
        self.lines()
            .map(|l| Line::new(Paint::new(l).with_style(self.body.style)))
            .collect()
    }
}

/// Create a new text area.
pub fn textarea(content: impl Into<Paint<String>>) -> TextArea {
    TextArea::new(content)
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_wrapping() {
        let t = TextArea::new(
            "Radicle enables users to run their own nodes, \
            ensuring censorship-resistant code collaboration \
            and fostering a resilient network without reliance \
            on third-parties.",
        )
        .wrap(50);
        let wrapped = t.lines().collect::<Vec<_>>();

        assert_eq!(
            wrapped,
            vec![
                "Radicle enables users to run their own nodes,".to_owned(),
                "ensuring censorship-resistant code collaboration".to_owned(),
                "and fostering a resilient network without reliance".to_owned(),
                "on third-parties.".to_owned(),
            ]
        );
    }

    #[test]
    fn test_wrapping_paragraphs() {
        let t = TextArea::new(
            "Radicle enables users to run their own nodes, \
            ensuring censorship-resistant code collaboration \
            and fostering a resilient network without reliance \
            on third-parties.\n\n\
            All social artifacts are stored in git, and signed \
            using public-key cryptography. Radicle verifies \
            the authenticity and authorship of all data \
            automatically.",
        )
        .wrap(50);
        let wrapped = t.lines().collect::<Vec<_>>();

        assert_eq!(
            wrapped,
            vec![
                "Radicle enables users to run their own nodes,".to_owned(),
                "ensuring censorship-resistant code collaboration".to_owned(),
                "and fostering a resilient network without reliance".to_owned(),
                "on third-parties.".to_owned(),
                "".to_owned(),
                "All social artifacts are stored in git, and signed".to_owned(),
                "using public-key cryptography. Radicle verifies".to_owned(),
                "the authenticity and authorship of all data".to_owned(),
                "automatically.".to_owned(),
            ]
        );
    }

    #[test]
    fn test_wrapping_code_block() {
        let t = TextArea::new(
            "\
Here's an example:

  $ git push rad://z3gqcJUoA1n9HaHKufZs5FCSGazv5/z6MksFqXN3Yhqk8pTJdUGLwATkRfQvwZXPqR2qMEhbS9wzpT
  $ rad sync

Run the above and wait for your project to sync.\
        ",
        )
        .wrap(50);
        let wrapped = t.lines().collect::<Vec<_>>();

        assert_eq!(
            wrapped,
            vec![
                "Here's an example:".to_owned(),
                "".to_owned(),
                "  $ git push rad://z3gqcJUoA1n9HaHKufZs5FCSGazv5/…".to_owned(),
                "  $ rad sync".to_owned(),
                "".to_owned(),
                "Run the above and wait for your project to sync.".to_owned()
            ]
        );
    }

    #[test]
    fn test_wrapping_fenced_block() {
        let t = TextArea::new(
            "\
Here's an example:
```
$ git push rad://z3gqcJUoA1n9HaHKufZs5FCSGazv5/z6MksFqXN3Yhqk8pTJdUGLwATkRfQvwZXPqR2qMEhbS9wzpT
$ rad sync
```
Run the above and wait for your project to sync.\
        ",
        )
        .wrap(40);
        let wrapped = t.lines().collect::<Vec<_>>();

        assert_eq!(
            wrapped,
            vec![
                "Here's an example:".to_owned(),
                "```".to_owned(),
                "$ git push rad://z3gqcJUoA1n9HaHKufZs5F…".to_owned(),
                "$ rad sync".to_owned(),
                "```".to_owned(),
                "Run the above and wait for your project".to_owned(),
                "to sync.".to_owned()
            ]
        );
    }
}
