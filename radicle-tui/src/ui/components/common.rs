pub mod container;
pub mod context;
pub mod label;
pub mod list;

use std::marker::PhantomData;

use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::Props;
use tuirealm::tui::layout::Rect;
use tuirealm::{AttrValue, Attribute, Frame, MockComponent, State};

use crate::ui::layout;
use crate::ui::widget::{Widget, WidgetComponent};

use context::Shortcuts;
use list::{List, Table};

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
