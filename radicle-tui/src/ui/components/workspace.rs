use std::marker::PhantomData;

use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::Props;
use tuirealm::tui::layout::Rect;
use tuirealm::{AttrValue, Attribute, Frame, MockComponent, State};

use crate::ui::layout;
use crate::ui::widget::{Widget, WidgetComponent};

use super::container::LabeledContainer;
use super::context::{ContextBar, Shortcuts};
use super::label::Label;
use super::list::{List, Table};

pub struct Browser<T> {
    list: Widget<Table>,
    shortcuts: Widget<Shortcuts>,
    phantom: PhantomData<T>,
}

impl<T: List> Browser<T> {
    pub fn new(list: Widget<Table>, shortcuts: Widget<Shortcuts>) -> Self {
        Self {
            list,
            shortcuts,
            phantom: PhantomData,
        }
    }
}

impl<T: List> WidgetComponent for Browser<T> {
    fn view(&mut self, _properties: &Props, frame: &mut Frame, area: Rect) {
        let shortcuts_h = self
            .shortcuts
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let layout = layout::root_component(area, shortcuts_h);

        self.list.view(frame, layout[0]);
        self.shortcuts.view(frame, layout[1]);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, cmd: Cmd) -> CmdResult {
        self.list.perform(cmd)
    }
}

pub struct Dashboard {
    about: Widget<LabeledContainer>,
    shortcuts: Widget<Shortcuts>,
}
impl Dashboard {
    pub fn new(about: Widget<LabeledContainer>, shortcuts: Widget<Shortcuts>) -> Self {
        Self { about, shortcuts }
    }
}

impl WidgetComponent for Dashboard {
    fn view(&mut self, _properties: &Props, frame: &mut Frame, area: Rect) {
        let shortcuts_h = self
            .shortcuts
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let layout = layout::root_component(area, shortcuts_h);

        self.about.view(frame, layout[0]);
        self.shortcuts.view(frame, layout[1]);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

pub struct IssueBrowser {
    label: Widget<Label>,
    shortcuts: Widget<Shortcuts>,
}

impl IssueBrowser {
    pub fn new(label: Widget<Label>, shortcuts: Widget<Shortcuts>) -> Self {
        Self { label, shortcuts }
    }
}

impl WidgetComponent for IssueBrowser {
    fn view(&mut self, _properties: &Props, frame: &mut Frame, area: Rect) {
        let label_w = self
            .label
            .query(Attribute::Width)
            .unwrap_or(AttrValue::Size(1))
            .unwrap_size();
        let shortcuts_h = self
            .shortcuts
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let layout = layout::root_component(area, shortcuts_h);

        self.label
            .view(frame, layout::centered_label(label_w, layout[0]));
        self.shortcuts.view(frame, layout[1])
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

pub struct PatchActivity {
    label: Widget<Label>,
    context: Widget<ContextBar>,
    shortcuts: Widget<Shortcuts>,
}

impl PatchActivity {
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

impl WidgetComponent for PatchActivity {
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

pub struct PatchFiles {
    label: Widget<Label>,
    context: Widget<ContextBar>,
    shortcuts: Widget<Shortcuts>,
}

impl PatchFiles {
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

impl WidgetComponent for PatchFiles {
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
