use std::marker::PhantomData;

use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::Props;
use tuirealm::tui::layout::{Constraint, Direction, Layout, Rect};
use tuirealm::{AttrValue, Attribute, Frame, MockComponent, State};

use crate::ui::layout;
use crate::ui::widget::{Widget, WidgetComponent};

use super::label::Label;
use super::list::{List, Table};

pub struct Browser<T> {
    list: Widget<Table>,
    phantom: PhantomData<T>,
}

impl<T: List> Browser<T> {
    pub fn new(list: Widget<Table>) -> Self {
        Self {
            list,
            phantom: PhantomData,
        }
    }
}

impl<T: List> WidgetComponent for Browser<T> {
    fn view(&mut self, _properties: &Props, frame: &mut Frame, area: Rect) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Min(1)].as_ref())
            .split(area);

        self.list.view(frame, layout[0]);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, cmd: Cmd) -> CmdResult {
        self.list.perform(cmd)
    }
}

pub struct PatchActivity {
    label: Widget<Label>,
}

impl PatchActivity {
    pub fn new(label: Widget<Label>) -> Self {
        Self { label }
    }
}

impl WidgetComponent for PatchActivity {
    fn view(&mut self, _properties: &Props, frame: &mut Frame, area: Rect) {
        let label_w = self
            .label
            .query(Attribute::Width)
            .unwrap_or(AttrValue::Size(1))
            .unwrap_size();
        let rect = layout::centered_label(label_w, area);

        self.label.view(frame, rect);
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
}

impl PatchFiles {
    pub fn new(label: Widget<Label>) -> Self {
        Self { label }
    }
}

impl WidgetComponent for PatchFiles {
    fn view(&mut self, _properties: &Props, frame: &mut Frame, area: Rect) {
        let label_w = self
            .label
            .query(Attribute::Width)
            .unwrap_or(AttrValue::Size(1))
            .unwrap_size();
        let rect = layout::centered_label(label_w, area);

        self.label.view(frame, rect);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}
