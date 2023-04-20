use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::Props;
use tuirealm::tui::layout::Rect;
use tuirealm::{AttrValue, Attribute, Frame, MockComponent, State};

use crate::ui::layout;
use crate::ui::widget::{Widget, WidgetComponent};

use super::common::context::{ContextBar, Shortcuts};
use super::common::label::Label;

pub struct Activity {
    label: Widget<Label>,
    context: Widget<ContextBar>,
    shortcuts: Widget<Shortcuts>,
}

impl Activity {
    pub fn new(
        label: Widget<Label>,
        context: Widget<ContextBar>,
        shortcuts: Widget<Shortcuts>,
    ) -> Self {
        Self {
            label,
            context,
            shortcuts,
        }
    }
}

impl WidgetComponent for Activity {
    fn view(&mut self, _properties: &Props, frame: &mut Frame, area: Rect) {
        let label_w = self
            .label
            .query(Attribute::Width)
            .unwrap_or(AttrValue::Size(1))
            .unwrap_size();
        let context_h = self
            .context
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let shortcuts_h = self
            .shortcuts
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let layout = layout::root_component_with_context(area, context_h, shortcuts_h);

        self.label
            .view(frame, layout::centered_label(label_w, layout[0]));
        self.context.view(frame, layout[1]);
        self.shortcuts.view(frame, layout[2]);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

pub struct Files {
    label: Widget<Label>,
    context: Widget<ContextBar>,
    shortcuts: Widget<Shortcuts>,
}

impl Files {
    pub fn new(
        label: Widget<Label>,
        context: Widget<ContextBar>,
        shortcuts: Widget<Shortcuts>,
    ) -> Self {
        Self {
            label,
            context,
            shortcuts,
        }
    }
}

impl WidgetComponent for Files {
    fn view(&mut self, _properties: &Props, frame: &mut Frame, area: Rect) {
        let label_w = self
            .label
            .query(Attribute::Width)
            .unwrap_or(AttrValue::Size(1))
            .unwrap_size();
        let context_h = self
            .context
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let shortcuts_h = self
            .shortcuts
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let layout = layout::root_component_with_context(area, context_h, shortcuts_h);

        self.label
            .view(frame, layout::centered_label(label_w, layout[0]));
        self.context.view(frame, layout[1]);
        self.shortcuts.view(frame, layout[2]);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}
