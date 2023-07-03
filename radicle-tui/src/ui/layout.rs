use tuirealm::props::{AttrValue, Attribute};
use tuirealm::tui::layout::{Constraint, Direction, Layout, Rect};
use tuirealm::MockComponent;

pub struct AppHeader {
    pub nav: Rect,
    pub info: Rect,
    pub line: Rect,
}

pub struct IssuePreview {
    pub header: Rect,
    pub list: Rect,
    pub details: Rect,
    pub discussion: Rect,
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

pub fn app_header(area: Rect, info_w: u16) -> AppHeader {
    let nav_w = area.width.saturating_sub(info_w);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(nav_w), Constraint::Length(info_w)].as_ref())
        .split(layout[1]);

    AppHeader {
        nav: top[0],
        info: top[1],
        line: layout[2],
    }
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
    let header_h = 3u16;
    let content_h = area
        .height
        .saturating_sub(header_h)
        .saturating_sub(shortcuts_h);

    let root = Layout::default()
        .direction(Direction::Vertical)
        .horizontal_margin(1)
        .constraints(
            [
                Constraint::Length(header_h),
                Constraint::Length(content_h),
                Constraint::Length(shortcuts_h),
            ]
            .as_ref(),
        )
        .split(area);

    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(root[1]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(0)].as_ref())
        .split(split[1]);

    IssuePreview {
        header: root[0],
        list: split[0],
        details: right[0],
        discussion: right[1],
        shortcuts: root[2],
    }
}
