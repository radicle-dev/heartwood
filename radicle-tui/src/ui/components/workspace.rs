use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::{Constraint, Direction, Layout, Rect};
use tuirealm::{Frame, MockComponent, State};

use crate::ui::components::label::Label;
use crate::ui::widget::{Widget, WidgetComponent};

use super::container::Tabs;

/// Workspace header that displays all labels horizontally aligned and separated
/// by a divider. Highlights the label defined by the current tab index.
#[derive(Clone)]
struct Header {
    tabs: Widget<Tabs>,
    info: Widget<Label>,
}

impl Header {
    pub fn new(tabs: Widget<Tabs>, info: Widget<Label>) -> Self {
        Self { tabs, info }
    }
}

impl WidgetComponent for Header {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();
        let info_width = self
            .info
            .query(Attribute::Width)
            .unwrap_or(AttrValue::Size(1))
            .unwrap_size();
        let tabs_width = area.width.saturating_sub(info_width);

        if display {
            let layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Length(tabs_width),
                        Constraint::Length(info_width),
                    ]
                    .as_ref(),
                )
                .split(area);

            self.tabs.view(frame, layout[0]);
            self.info.view(frame, layout[1]);
        }
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, cmd: Cmd) -> CmdResult {
        self.tabs.perform(cmd)
    }
}

/// A container with a tab header. Displays the component selected by the index
/// held in the header state.
pub struct Workspaces {
    header: Widget<Header>,
    children: Vec<Box<dyn MockComponent>>,
}

impl Workspaces {
    pub fn new(
        tabs: Widget<Tabs>,
        info: Widget<Label>,
        children: Vec<Box<dyn MockComponent>>,
    ) -> Self {
        Self {
            header: Widget::new(Header::new(tabs, info)),
            children,
        }
    }
}

impl WidgetComponent for Workspaces {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();
        let header_height = self
            .header
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(1))
            .unwrap_size();
        let selected = self.header.tabs.state().unwrap_one().unwrap_u16();

        if display {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Length(header_height),
                        Constraint::Length(1),
                        Constraint::Length(0),
                    ]
                    .as_ref(),
                )
                .split(area);

            self.header.view(frame, layout[0]);

            if let Some(child) = self.children.get_mut(selected as usize) {
                child.view(frame, layout[2]);
            }
        }
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, cmd: Cmd) -> CmdResult {
        CmdResult::Batch(
            [
                self.children
                    .iter_mut()
                    .map(|child| child.perform(cmd))
                    .collect(),
                vec![self.header.perform(cmd)],
            ]
            .concat(),
        )
    }
}
