use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{AttrValue, Attribute, BorderSides, BorderType, Color, Props, Style};
use tuirealm::tui::layout::{Constraint, Direction, Layout, Rect};
use tuirealm::tui::widgets::{Block, Cell, Row, TableState};
use tuirealm::{Frame, MockComponent, State, StateValue};

use crate::ui::layout;
use crate::ui::theme::Theme;
use crate::ui::widget::{utils, Widget, WidgetComponent};

use super::container::Header;
use super::label::Label;

/// A generic item that can be displayed in a table with [`W`] columns.
pub trait TableItem<const W: usize> {
    /// Should return fields as table cells.
    fn row(&self, theme: &Theme) -> [Cell; W];
}

/// Grow behavior of a table column.
///
/// [`tui::widgets::Table`] does only support percental column widths.
/// A [`ColumnWidth`] is used to specify the grow behaviour of a table column
/// and a percental column width is calculated based on that.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ColumnWidth {
    /// A fixed-size column.
    Fixed(u16),
    /// A growable column.
    Grow,
}

/// A component that displays a labeled property.
#[derive(Clone)]
pub struct Property {
    label: Widget<Label>,
    divider: Widget<Label>,
    property: Widget<Label>,
}

impl Property {
    pub fn new(label: Widget<Label>, divider: Widget<Label>, property: Widget<Label>) -> Self {
        Self {
            label,
            divider,
            property,
        }
    }
}

impl WidgetComponent for Property {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();

        if display {
            let labels: Vec<Box<dyn MockComponent>> = vec![
                self.label.clone().to_boxed(),
                self.divider.clone().to_boxed(),
                self.property.clone().to_boxed(),
            ];

            let layout = layout::h_stack(labels, area);
            for (mut label, area) in layout {
                label.view(frame, area);
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

/// A component that can display lists of labeled properties
#[derive(Default)]
pub struct PropertyList {
    properties: Vec<Widget<Property>>,
}

impl PropertyList {
    pub fn new(properties: Vec<Widget<Property>>) -> Self {
        Self { properties }
    }
}

impl WidgetComponent for PropertyList {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();

        if display {
            let properties = self
                .properties
                .iter()
                .map(|property| property.clone().to_boxed() as Box<dyn MockComponent>)
                .collect();

            let layout = layout::v_stack(properties, area);
            for (mut property, area) in layout {
                property.view(frame, area);
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

/// A table component that can display a list of [`TableItem`]s hold by a [`TableModel`].
pub struct Table<V, const W: usize>
where
    V: TableItem<W> + Clone,
{
    /// Items hold by this model.
    items: Vec<V>,
    /// The table header.
    header: [Widget<Label>; W],
    /// Grow behavior of table columns.
    widths: [ColumnWidth; W],
    state: TableState,
    theme: Theme,
}

impl<V, const W: usize> Table<V, W>
where
    V: TableItem<W> + Clone,
{
    pub fn new(
        items: &[V],
        header: [Widget<Label>; W],
        widths: [ColumnWidth; W],
        theme: Theme,
    ) -> Self {
        let mut state = TableState::default();
        state.select(Some(0));

        Self {
            items: items.to_vec(),
            header,
            widths,
            state,
            theme,
        }
    }

    fn select_previous(&mut self) -> Option<usize> {
        let old_index = self.state.selected();
        let new_index = match old_index {
            Some(selected) if selected == 0 => Some(0),
            Some(selected) => Some(selected.saturating_sub(1)),
            None => Some(0),
        };

        if old_index != new_index {
            self.state.select(new_index);
            self.state.selected()
        } else {
            None
        }
    }

    fn select_next(&mut self, len: usize) -> Option<usize> {
        let old_index = self.state.selected();
        let new_index = match old_index {
            Some(selected) if selected >= len.saturating_sub(1) => Some(len.saturating_sub(1)),
            Some(selected) => Some(selected.saturating_add(1)),
            None => Some(0),
        };

        if old_index != new_index {
            self.state.select(new_index);
            self.state.selected()
        } else {
            None
        }
    }
}

impl<V, const W: usize> WidgetComponent for Table<V, W>
where
    V: TableItem<W> + Clone,
{
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let highlight = properties
            .get_or(Attribute::HighlightedColor, AttrValue::Color(Color::Reset))
            .unwrap_color();

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(3), Constraint::Min(1)])
            .split(area);

        let widths = utils::column_widths(area, &self.widths, self.theme.tables.spacing);
        let rows: Vec<Row<'_>> = self
            .items
            .iter()
            .map(|item| Row::new(item.row(&self.theme)))
            .collect();

        let table = tuirealm::tui::widgets::Table::new(rows)
            .block(
                Block::default()
                    .borders(BorderSides::BOTTOM | BorderSides::LEFT | BorderSides::RIGHT)
                    .border_style(Style::default().fg(Color::Rgb(48, 48, 48)))
                    .border_type(BorderType::Rounded),
            )
            .highlight_style(Style::default().bg(highlight))
            .column_spacing(self.theme.tables.spacing)
            .widths(&widths);

        let mut header = Widget::new(Header::new(
            self.header.clone(),
            self.widths,
            self.theme.clone(),
        ));
        header.view(frame, layout[0]);
        frame.render_stateful_widget(table, layout[1], &mut self.state);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, cmd: Cmd) -> CmdResult {
        use tuirealm::command::Direction;
        match cmd {
            Cmd::Move(Direction::Up) => match self.select_previous() {
                Some(selected) => CmdResult::Changed(State::One(StateValue::Usize(selected))),
                None => CmdResult::None,
            },
            Cmd::Move(Direction::Down) => match self.select_next(self.items.len()) {
                Some(selected) => CmdResult::Changed(State::One(StateValue::Usize(selected))),
                None => CmdResult::None,
            },
            Cmd::Submit => match self.state.selected() {
                Some(selected) => CmdResult::Submit(State::One(StateValue::Usize(selected))),
                None => CmdResult::None,
            },
            _ => CmdResult::None,
        }
    }
}
