use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{
    AttrValue, Attribute, BorderSides, BorderType, Color, Props, Style, TextModifiers,
};
use tuirealm::tui::layout::{Constraint, Direction, Layout, Rect};
use tuirealm::tui::widgets::{Block, Cell, Row};
use tuirealm::{Frame, MockComponent, State, StateValue};

use crate::ui::ext::HeaderBlock;
use crate::ui::layout;
use crate::ui::state::TabState;
use crate::ui::theme::Theme;
use crate::ui::widget::{utils, Widget, WidgetComponent};

use super::label::Label;
use super::list::ColumnWidth;

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
    line: Widget<Label>,
    state: TabState,
}

impl Tabs {
    pub fn new(tabs: Vec<Widget<Label>>, line: Widget<Label>) -> Self {
        let count = &tabs.len();
        Self {
            tabs,
            line,
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

        if display {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ])
                .split(area);

            // Render tabs on first row, highlighting the selected tab.
            let mut tabs = vec![];
            for (index, tab) in self.tabs.iter().enumerate() {
                let mut tab = tab.clone().to_boxed();
                if index == selected as usize {
                    tab.attr(
                        Attribute::TextProps,
                        AttrValue::TextModifiers(TextModifiers::REVERSED),
                    );
                }
                tabs.push(tab.clone().to_boxed() as Box<dyn MockComponent>);
            }
            tabs.push(Widget::new(Label::default()).to_boxed());

            let tab_layout = layout::h_stack(tabs, layout[1]);
            for (mut tab, area) in tab_layout {
                tab.view(frame, area);
            }

            // Repeat and render line on second row.
            let overlines = vec![self.line.clone(); area.width as usize];
            let overlines = overlines
                .iter()
                .map(|l| l.clone().to_boxed() as Box<dyn MockComponent>)
                .collect();
            let line_layout = layout::h_stack(overlines, layout[2]);
            for (mut line, area) in line_layout {
                line.view(frame, area);
            }
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
pub struct Header<const W: usize> {
    header: [Widget<Label>; W],
    widths: [ColumnWidth; W],
    theme: Theme,
}

impl<const W: usize> Header<W> {
    pub fn new(header: [Widget<Label>; W], widths: [ColumnWidth; W], theme: Theme) -> Self {
        Self {
            header,
            widths,
            theme,
        }
    }
}

impl<const W: usize> WidgetComponent for Header<W> {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();
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

            let widths = utils::column_widths(area, &self.widths, self.theme.tables.spacing);
            let header: [Cell; W] = self
                .header
                .iter()
                .map(|label| {
                    let cell: Cell = label.into();
                    cell.style(Style::default().fg(self.theme.colors.default_fg))
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            let header: Row<'_> = Row::new(header);

            let table = tuirealm::tui::widgets::Table::new(vec![])
                .column_spacing(self.theme.tables.spacing)
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

pub struct Container {
    component: Box<dyn MockComponent>,
}

impl Container {
    pub fn new(component: Box<dyn MockComponent>) -> Self {
        Self { component }
    }
}

impl WidgetComponent for Container {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();

        if display {
            // Make some space on the left
            let layout = Layout::default()
                .direction(Direction::Horizontal)
                .horizontal_margin(1)
                .vertical_margin(1)
                .constraints(vec![Constraint::Length(1), Constraint::Min(0)].as_ref())
                .split(area);
            // reverse draw order: child needs to be drawn first?
            self.component.view(frame, layout[1]);

            let block = Block::default()
                .borders(BorderSides::ALL)
                .border_style(Style::default().fg(Color::Rgb(48, 48, 48)))
                .border_type(BorderType::Rounded);
            frame.render_widget(block, area);
        }
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, cmd: Cmd) -> CmdResult {
        self.component.perform(cmd)
    }
}

pub struct LabeledContainer {
    header: Widget<Header<1>>,
    component: Box<dyn MockComponent>,
}

impl LabeledContainer {
    pub fn new(header: Widget<Header<1>>, component: Box<dyn MockComponent>) -> Self {
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
