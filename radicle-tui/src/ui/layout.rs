use tuirealm::props::{AttrValue, Attribute};
use tuirealm::tui::layout::{Constraint, Direction, Layout, Rect};
use tuirealm::MockComponent;

pub struct IssuePreview {
    pub left: Rect,
    pub right: Rect,
    pub shortcuts: Rect,
}

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

pub fn default_page(area: Rect) -> Vec<Rect> {
    let nav_h = 3u16;
    let margin_h = 1u16;
    let content_h = area.height.saturating_sub(nav_h.saturating_add(margin_h));

    Layout::default()
        .direction(Direction::Vertical)
        .horizontal_margin(margin_h)
        .constraints([Constraint::Length(nav_h), Constraint::Length(content_h)].as_ref())
        .split(area)
}

pub fn headerless_page(area: Rect) -> Vec<Rect> {
    let margin_h = 1u16;
    let content_h = area.height.saturating_sub(margin_h);

    Layout::default()
        .direction(Direction::Vertical)
        .horizontal_margin(margin_h)
        .constraints([Constraint::Length(content_h)].as_ref())
        .split(area)
}

pub fn root_component(area: Rect, shortcuts_h: u16) -> Vec<Rect> {
    let content_h = area.height.saturating_sub(shortcuts_h);

    Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(content_h),
                Constraint::Length(shortcuts_h),
            ]
            .as_ref(),
        )
        .split(area)
}

pub fn root_component_with_context(area: Rect, context_h: u16, shortcuts_h: u16) -> Vec<Rect> {
    let content_h = area
        .height
        .saturating_sub(shortcuts_h.saturating_add(context_h));

    Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(content_h),
                Constraint::Length(context_h),
                Constraint::Length(shortcuts_h),
            ]
            .as_ref(),
        )
        .split(area)
}

pub fn centered_label(label_w: u16, area: Rect) -> Rect {
    let label_h = 1u16;
    let spacer_w = area.width.saturating_sub(label_w).saturating_div(2);
    let spacer_h = area.height.saturating_sub(label_h).saturating_div(2);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(spacer_h),
                Constraint::Length(label_h),
                Constraint::Length(spacer_h),
            ]
            .as_ref(),
        )
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Length(spacer_w),
                Constraint::Length(label_w),
                Constraint::Length(spacer_w),
            ]
            .as_ref(),
        )
        .split(layout[1])[1]
}

pub fn issue_preview(area: Rect, shortcuts_h: u16) -> IssuePreview {
    let content_h = area.height.saturating_sub(shortcuts_h);

    let root = Layout::default()
        .direction(Direction::Vertical)
        .horizontal_margin(1)
        .constraints(
            [
                Constraint::Length(content_h),
                Constraint::Length(shortcuts_h),
            ]
            .as_ref(),
        )
        .split(area);

    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(root[0]);

    IssuePreview {
        left: split[0],
        right: split[1],
        shortcuts: root[1],
    }
}
