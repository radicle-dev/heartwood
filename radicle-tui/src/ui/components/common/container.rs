use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{
    AttrValue, Attribute, BorderSides, BorderType, Color, PropPayload, PropValue, Props, Style,
};
use tuirealm::tui::layout::{Constraint, Direction, Layout, Rect};
use tuirealm::tui::text::{Span, Spans};
use tuirealm::tui::widgets::{Block, Cell, Row};
use tuirealm::{Frame, MockComponent, State, StateValue};

use crate::ui::components::common::label::Label;
use crate::ui::ext::HeaderBlock;
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
#[derive(Default)]
pub struct Header;

impl Header {
    fn content<'a>(spans: Vec<PropValue>) -> Row<'a> {
        Row::new(
            spans
                .iter()
                .map(|span| Cell::from(span.clone().unwrap_text_span().content))
                .collect::<Vec<_>>(),
        )
    }

    fn widths(widths: Vec<PropValue>) -> Vec<Constraint> {
        widths
            .iter()
            .map(|prop| Constraint::Percentage(prop.clone().unwrap_u16()))
            .collect()
    }
}

impl WidgetComponent for Header {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();
        let content = properties
            .get_or(
                Attribute::Content,
                AttrValue::Payload(PropPayload::Vec(vec![])),
            )
            .unwrap_payload()
            .unwrap_vec();
        let widths = properties
            .get_or(
                Attribute::Custom("widths"),
                AttrValue::Payload(PropPayload::Vec(vec![])),
            )
            .unwrap_payload()
            .unwrap_vec();

        if display {
            let block = HeaderBlock::default()
                .borders(BorderSides::all())
                .border_style(Style::default().fg(Color::Rgb(48, 48, 48)))
                .border_type(BorderType::Rounded);
            frame.render_widget(block, area);

            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Min(1)])
                .vertical_margin(1)
                .horizontal_margin(1)
                .split(area);

            let header = Self::content(content);
            let widths = Self::widths(widths);

            let table = tuirealm::tui::widgets::Table::new(vec![])
                .column_spacing(3u16)
                .header(header)
                .widths(&widths);
            frame.render_widget(table, layout[0]);
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
    pub fn new(header: Widget<Header>, component: Box<dyn MockComponent>) -> Self {
        Self { header, component }
    }
}

impl WidgetComponent for LabeledContainer {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();
        let header_height = self
            .header
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(3))
            .unwrap_size();

        if display {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(header_height), Constraint::Length(0)].as_ref())
                .split(area);

            // Make some space on the left
            let inner_layout = Layout::default()
                .direction(Direction::Horizontal)
                .horizontal_margin(1)
                .constraints(vec![Constraint::Length(1), Constraint::Min(0)].as_ref())
                .split(layout[1]);
            // reverse draw order: child needs to be drawn first?
            self.component.view(frame, inner_layout[1]);

            let block = Block::default()
                .borders(BorderSides::BOTTOM | BorderSides::LEFT | BorderSides::RIGHT)
                .border_style(Style::default().fg(Color::Rgb(48, 48, 48)))
                .border_type(BorderType::Rounded);
            frame.render_widget(block, layout[1]);

            self.header.view(frame, layout[0]);
        }
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, cmd: Cmd) -> CmdResult {
        self.component.perform(cmd)
    }
}
