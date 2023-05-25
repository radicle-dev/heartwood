use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::Rect;
use tuirealm::{Frame, MockComponent, State};

use super::label::Label;

use crate::ui::layout;
use crate::ui::widget::{Widget, WidgetComponent};

/// A shortcut that consists of a label displaying the "hotkey", a label that displays
/// the action and a spacer between them.
#[derive(Clone)]
pub struct Shortcut {
    short: Widget<Label>,
    divider: Widget<Label>,
    long: Widget<Label>,
}

impl Shortcut {
    pub fn new(short: Widget<Label>, divider: Widget<Label>, long: Widget<Label>) -> Self {
        Self {
            short,
            divider,
            long,
        }
    }
}

impl WidgetComponent for Shortcut {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();

        if display {
            let labels: Vec<Box<dyn MockComponent>> = vec![
                self.short.clone().to_boxed(),
                self.divider.clone().to_boxed(),
                self.long.clone().to_boxed(),
            ];

            let layout = layout::h_stack(labels, area);
            for (mut shortcut, area) in layout {
                shortcut.view(frame, area);
            }
        }
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

/// A shortcut bar that displays multiple shortcuts and separates them with a
/// divider.
pub struct Shortcuts {
    shortcuts: Vec<Widget<Shortcut>>,
    divider: Widget<Label>,
}

impl Shortcuts {
    pub fn new(shortcuts: Vec<Widget<Shortcut>>, divider: Widget<Label>) -> Self {
        Self { shortcuts, divider }
    }
}

impl WidgetComponent for Shortcuts {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();

        if display {
            let mut widgets: Vec<Box<dyn MockComponent>> = vec![];
            let mut shortcuts = self.shortcuts.iter_mut().peekable();

            while let Some(shortcut) = shortcuts.next() {
                if shortcuts.peek().is_some() {
                    widgets.push(shortcut.clone().to_boxed());
                    widgets.push(self.divider.clone().to_boxed())
                } else {
                    widgets.push(shortcut.clone().to_boxed());
                }
            }

            let layout = layout::h_stack(widgets, area);
            for (mut widget, area) in layout {
                widget.view(frame, area);
            }
        }
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

pub struct ContextBar {
    context: Widget<Label>,
    id: Widget<Label>,
    author: Widget<Label>,
    title: Widget<Label>,
    comments: Widget<Label>,
}

impl ContextBar {
    pub fn new(
        context: Widget<Label>,
        id: Widget<Label>,
        author: Widget<Label>,
        title: Widget<Label>,
        comments: Widget<Label>,
    ) -> Self {
        Self {
            context,
            id,
            author,
            title,
            comments,
        }
    }
}

impl WidgetComponent for ContextBar {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();

        let context_w = self.context.query(Attribute::Width).unwrap().unwrap_size();
        let id_w = self.id.query(Attribute::Width).unwrap().unwrap_size();
        let author_w = self.author.query(Attribute::Width).unwrap().unwrap_size();
        let count_w = self.comments.query(Attribute::Width).unwrap().unwrap_size();

        if display {
            let layout = layout::h_stack(
                vec![
                    self.context.clone().to_boxed(),
                    self.id.clone().to_boxed(),
                    self.title
                        .clone()
                        .width(
                            area.width
                                .saturating_sub(context_w + id_w + author_w + count_w),
                        )
                        .to_boxed(),
                    self.author.clone().to_boxed(),
                    self.comments.clone().to_boxed(),
                ],
                area,
            );

            for (mut component, area) in layout {
                component.view(frame, area);
            }
        }
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}
