use tui_realm_stdlib::Phantom;

use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{AttrValue, Attribute, Color, Props, Style};
use tuirealm::tui::layout::{Constraint, Direction, Layout, Rect};
use tuirealm::tui::text::{Span, Spans};
use tuirealm::tui::widgets::Block;
use tuirealm::{Frame, MockComponent, State, StateValue};

use crate::ui::components::label::Label;
use crate::ui::layout;
use crate::ui::state::TabState;
use crate::ui::widget::{Widget, WidgetComponent};

/// Some user events need to be handled globally (e.g. user presses key `q` to quit
/// the application). This component can be used in conjunction with SubEventClause
/// to handle those events.
#[derive(Default)]
pub struct GlobalListener {}

impl WidgetComponent for GlobalListener {
    fn view(&mut self, _properties: &Props, _frame: &mut Frame, _area: Rect) {}

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

/// Some user events need to be handled globally (e.g. user presses key `q` to quit
/// the application). This component can be used in conjunction with SubEventClause
/// to handle those events.
#[derive(Default, MockComponent)]
pub struct GlobalPhantom {
    component: Phantom,
}

/// A tab header that displays all labels horizontally aligned and separated
/// by a divider. Highlights the label defined by the current tab index.
#[derive(Clone)]
pub struct Tabs {
    tabs: Vec<Widget<Label>>,
    divider: Widget<Label>,
    state: TabState,
}

impl Tabs {
    pub fn new(tabs: Vec<Widget<Label>>, divider: Widget<Label>) -> Self {
        let count = &tabs.len();
        Self {
            tabs,
            divider,
            state: TabState {
                selected: 0,
                len: *count as u16,
            },
        }
    }
}

impl WidgetComponent for Tabs {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let selected = self.state().unwrap_one().unwrap_u16();
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();
        let foreground = properties
            .get_or(Attribute::Foreground, AttrValue::Color(Color::Reset))
            .unwrap_color();
        let highlight = properties
            .get_or(Attribute::HighlightedColor, AttrValue::Color(Color::Reset))
            .unwrap_color();

        if display {
            let spans = self
                .tabs
                .iter()
                .map(|tab| Spans::from(vec![Span::from(tab)]))
                .collect::<Vec<_>>();

            let tabs = tuirealm::tui::widgets::Tabs::new(spans)
                .style(Style::default().fg(foreground))
                .highlight_style(Style::default().fg(highlight))
                .divider(Span::from(&self.divider))
                .select(selected as usize);

            frame.render_widget(tabs, area);
        }
    }

    fn state(&self) -> State {
        State::One(StateValue::U16(self.state.selected))
    }

    fn perform(&mut self, _properties: &Props, cmd: Cmd) -> CmdResult {
        use tuirealm::command::Direction;

        match cmd {
            Cmd::Move(Direction::Right) => {
                let prev = self.state.selected;
                self.state.incr_tab_index(true);
                if prev != self.state.selected {
                    CmdResult::Changed(self.state())
                } else {
                    CmdResult::None
                }
            }
            _ => CmdResult::None,
        }
    }
}

/// A labeled container header.
#[derive(Clone)]
struct Header {
    content: Widget<Label>,
}

impl Header {
    pub fn new(content: Widget<Label>) -> Self {
        Self { content }
    }
}

impl WidgetComponent for Header {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();
        let spacer = Widget::new(Label::default())
            .content(AttrValue::String(String::default()))
            .to_boxed();

        if display {
            let labels: Vec<Box<dyn MockComponent>> = vec![self.content.clone().to_boxed(), spacer];

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

pub struct LabeledContainer {
    header: Widget<Header>,
    component: Box<dyn MockComponent>,
}

impl LabeledContainer {
    pub fn new(content: Widget<Label>, component: Box<dyn MockComponent>) -> Self {
        Self {
            header: Widget::new(Header::new(content)),
            component,
        }
    }
}

impl WidgetComponent for LabeledContainer {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();
        let background = properties
            .get_or(Attribute::Background, AttrValue::Color(Color::Reset))
            .unwrap_color();
        let header_height = self
            .header
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(1))
            .unwrap_size();

        if display {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(header_height), Constraint::Length(0)].as_ref())
                .split(area);

            self.header.view(frame, layout[0]);

            // Make some space on the left
            let inner_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Length(1), Constraint::Min(0)].as_ref())
                .split(layout[1]);
            // reverse draw order: child needs to be drawn first?
            self.component.view(frame, inner_layout[1]);

            let block = Block::default().style(Style::default().bg(background));
            frame.render_widget(block, layout[1]);
        }
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, cmd: Cmd) -> CmdResult {
        self.component.perform(cmd)
    }
}
