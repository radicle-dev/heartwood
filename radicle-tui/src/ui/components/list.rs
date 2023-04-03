use radicle::Profile;
use tuirealm::command::{Cmd, CmdResult, Direction};
use tuirealm::props::{
    AttrValue, Attribute, Color, PropPayload, PropValue, Props, Style, TextModifiers, TextSpan,
};
use tuirealm::tui::layout::{Constraint, Rect};
use tuirealm::tui::widgets::{Cell, Row, TableState};
use tuirealm::{Frame, MockComponent, State, StateValue};

use crate::ui::components::label::Label;
use crate::ui::layout;
use crate::ui::theme::Theme;
use crate::ui::widget::{Widget, WidgetComponent};

pub trait List {
    fn row(&self, theme: &Theme, profile: &Profile) -> Vec<TextSpan>;
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

pub struct Table {
    state: TableState,
}

impl Default for Table {
    fn default() -> Self {
        let mut state = TableState::default();
        state.select(Some(0));
        Self { state }
    }
}

impl Table {
    fn select_previous(&mut self) {
        let index = match self.state.selected() {
            Some(selected) if selected == 0 => 0,
            Some(selected) => selected.saturating_sub(1),
            None => 0,
        };
        self.state.select(Some(index));
    }

    fn select_next(&mut self, len: usize) {
        let index = match self.state.selected() {
            Some(selected) if selected >= len.saturating_sub(1) => len.saturating_sub(1),
            Some(selected) => selected.saturating_add(1),
            None => 0,
        };
        self.state.select(Some(index));
    }

    fn header<'a>(spans: Vec<PropValue>) -> Row<'a> {
        Row::new(
            spans
                .iter()
                .map(|span| {
                    Cell::from(span.clone().unwrap_text_span().content)
                        .style(Style::default().add_modifier(TextModifiers::BOLD))
                })
                .collect::<Vec<_>>(),
        )
    }

    fn rows<'a>(spans: Vec<Vec<TextSpan>>) -> Vec<Row<'a>> {
        spans
            .iter()
            .map(|spans| {
                let cells = spans.iter().map(|span| {
                    let style = Style::default().fg(span.fg);
                    Cell::from(span.content.clone()).style(style)
                });
                Row::new(cells).height(1)
            })
            .collect::<Vec<Row>>()
    }

    fn widths(widths: Vec<PropValue>) -> Vec<Constraint> {
        widths
            .iter()
            .map(|prop| Constraint::Percentage(prop.clone().unwrap_u16()))
            .collect()
    }
}

impl WidgetComponent for Table {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        let content = properties
            .get_or(Attribute::Content, AttrValue::Table(vec![]))
            .unwrap_table();
        let background = properties
            .get_or(Attribute::Background, AttrValue::Color(Color::Reset))
            .unwrap_color();
        let highlight = properties
            .get_or(Attribute::HighlightedColor, AttrValue::Color(Color::Reset))
            .unwrap_color();
        let header = properties
            .get_or(
                Attribute::Custom("header"),
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

        let header = Self::header(header);
        let rows = Self::rows(content);
        let widths = Self::widths(widths);

        let table = tuirealm::tui::widgets::Table::new(rows)
            .highlight_style(Style::default().bg(highlight))
            .style(Style::default().bg(background))
            .column_spacing(3u16)
            .header(header)
            .widths(&widths);

        frame.render_stateful_widget(table, area, &mut self.state);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, properties: &Props, cmd: Cmd) -> CmdResult {
        let content = properties
            .get_or(Attribute::Content, AttrValue::Table(vec![]))
            .unwrap_table();

        match cmd {
            Cmd::Move(Direction::Up) => {
                self.select_previous();
                if let Some(selected) = self.state.selected() {
                    CmdResult::Changed(State::One(StateValue::Usize(selected)))
                } else {
                    CmdResult::None
                }
            }
            Cmd::Move(Direction::Down) => {
                self.select_next(content.len());
                if let Some(selected) = self.state.selected() {
                    CmdResult::Changed(State::One(StateValue::Usize(selected)))
                } else {
                    CmdResult::None
                }
            }
            Cmd::Submit => {
                if let Some(selected) = self.state.selected() {
                    CmdResult::Submit(State::One(StateValue::Usize(selected)))
                } else {
                    CmdResult::None
                }
            }
            _ => CmdResult::None,
        }
    }
}
