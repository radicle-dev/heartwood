use tuirealm::tui::layout::{Constraint, Rect};

use super::common::list::ColumnWidth;

/// Calculates `Constraint::Percentage` for each fixed column width in `widths`,
/// taking into account the available width in `area` and the column spacing given by `spacing`.
pub fn column_widths(area: Rect, widths: &[ColumnWidth], spacing: u16) -> Vec<Constraint> {
    let total_spacing = spacing.saturating_mul(widths.len() as u16);
    let fixed_width = widths
        .iter()
        .fold(0u16, |total, &width| match width {
            ColumnWidth::Fixed(w) => total + w,
            ColumnWidth::Grow => total,
        })
        .saturating_add(total_spacing);

    let grow_count = widths.iter().fold(0u16, |count, &w| {
        if w == ColumnWidth::Grow {
            count + 1
        } else {
            count
        }
    });
    let grow_width = area
        .width
        .saturating_sub(fixed_width)
        .checked_div(grow_count)
        .unwrap_or(0);

    widths
        .iter()
        .map(|width| match width {
            ColumnWidth::Fixed(w) => {
                let p: f64 = *w as f64 / area.width as f64 * 100_f64;
                Constraint::Percentage(p.ceil() as u16)
            }
            ColumnWidth::Grow => {
                let p: f64 = grow_width as f64 / area.width as f64 * 100_f64;
                Constraint::Percentage(p.floor() as u16)
            }
        })
        .collect()
}
