pub mod container;
pub mod context;
pub mod label;
pub mod list;

use tuirealm::props::{AttrValue, Attribute};
use tuirealm::MockComponent;

use container::{GlobalListener, Header, LabeledContainer, Tabs};
use context::{Shortcut, Shortcuts};
use label::Label;
use list::{Property, PropertyList};

use self::list::{ColumnWidth, TableModel};

use super::Widget;

use crate::ui::theme::Theme;

pub fn global_listener() -> Widget<GlobalListener> {
    Widget::new(GlobalListener::default())
}

pub fn label(content: &str) -> Widget<Label> {
    // TODO: Remove when size constraints are implemented
    let width = content.chars().count() as u16;

    Widget::new(Label::default())
        .content(AttrValue::String(content.to_string()))
        .height(1)
        .width(width)
}

pub fn reversable_label(content: &str) -> Widget<Label> {
    let content = &format!(" {content} ");

    label(content)
}

pub fn container_header(theme: &Theme, label: Widget<Label>) -> Widget<Header<(), 1>> {
    let model = TableModel::new([label], [ColumnWidth::Fixed(100)]);

    Widget::new(Header::new(model, theme.clone()))
}

pub fn labeled_container(
    theme: &Theme,
    title: &str,
    component: Box<dyn MockComponent>,
) -> Widget<LabeledContainer> {
    let header = container_header(
        theme,
        label(&format!(" {title} ")).foreground(theme.colors.default_fg),
    );
    let container = LabeledContainer::new(header, component);

    Widget::new(container)
}

pub fn shortcut(theme: &Theme, short: &str, long: &str) -> Widget<Shortcut> {
    let short = label(short).foreground(theme.colors.shortcut_short_fg);
    let divider = label(&theme.icons.whitespace.to_string());
    let long = label(long).foreground(theme.colors.shortcut_long_fg);

    // TODO: Remove when size constraints are implemented
    let short_w = short.query(Attribute::Width).unwrap().unwrap_size();
    let divider_w = divider.query(Attribute::Width).unwrap().unwrap_size();
    let long_w = long.query(Attribute::Width).unwrap().unwrap_size();
    let width = short_w.saturating_add(divider_w).saturating_add(long_w);

    let shortcut = Shortcut::new(short, divider, long);

    Widget::new(shortcut).height(1).width(width)
}

pub fn shortcuts(theme: &Theme, shortcuts: Vec<Widget<Shortcut>>) -> Widget<Shortcuts> {
    let divider = label(&format!(" {} ", theme.icons.shortcutbar_divider))
        .foreground(theme.colors.shortcutbar_divider_fg);
    let shortcut_bar = Shortcuts::new(shortcuts, divider);

    Widget::new(shortcut_bar).height(1)
}

pub fn property(theme: &Theme, name: &str, value: &str) -> Widget<Property> {
    let name = label(name).foreground(theme.colors.property_name_fg);
    let divider = label(&format!(" {} ", theme.icons.property_divider));
    let value = label(value).foreground(theme.colors.default_fg);

    // TODO: Remove when size constraints are implemented
    let name_w = name.query(Attribute::Width).unwrap().unwrap_size();
    let divider_w = divider.query(Attribute::Width).unwrap().unwrap_size();
    let value_w = value.query(Attribute::Width).unwrap().unwrap_size();
    let width = name_w.saturating_add(divider_w).saturating_add(value_w);

    let property = Property::new(name, divider, value);

    Widget::new(property).height(1).width(width)
}

pub fn property_list(_theme: &Theme, properties: Vec<Widget<Property>>) -> Widget<PropertyList> {
    let property_list = PropertyList::new(properties);

    Widget::new(property_list)
}

pub fn tabs(theme: &Theme, tabs: Vec<Widget<Label>>) -> Widget<Tabs> {
    let line =
        label(&theme.icons.tab_overline.to_string()).foreground(theme.colors.tabs_highlighted_fg);
    let tabs = Tabs::new(tabs, line);

    Widget::new(tabs).height(2)
}
