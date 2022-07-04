use tuirealm::props::{AttrValue, Attribute};
use tuirealm::tui::layout::Rect;

use tuirealm::tui::layout::{Constraint, Direction, Layout};
use tuirealm::MockComponent;

pub fn v_stack(
    widgets: Vec<Box<dyn MockComponent>>,
    area: Rect,
) -> Vec<(Box<dyn MockComponent>, Rect)> {
    let constraints = widgets
        .iter()
        .map(|w| {
            Constraint::Length(
                w.query(Attribute::Height)
                    .unwrap_or(AttrValue::Size(0))
                    .unwrap_size(),
            )
        })
        .collect::<Vec<_>>();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    widgets.into_iter().zip(layout.into_iter()).collect()
}

pub fn h_stack(
    widgets: Vec<Box<dyn MockComponent>>,
    area: Rect,
) -> Vec<(Box<dyn MockComponent>, Rect)> {
    let constraints = widgets
        .iter()
        .map(|w| {
            Constraint::Length(
                w.query(Attribute::Width)
                    .unwrap_or(AttrValue::Size(0))
                    .unwrap_size(),
            )
        })
        .collect::<Vec<_>>();
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    widgets.into_iter().zip(layout.into_iter()).collect()
}
