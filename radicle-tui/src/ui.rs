pub mod components;
pub mod layout;
pub mod theme;
pub mod widget;

use tuirealm::props::Attribute;
use tuirealm::{MockComponent, StateValue};

use components::{
    ContainerHeader, GlobalListener, Label, LabeledContainer, Property, PropertyList, Shortcut,
    ShortcutBar,
};
use widget::Widget;

pub fn global_listener() -> Widget<GlobalListener> {
    Widget::new(GlobalListener::default())
}

pub fn label(content: &str) -> Widget<Label> {
    // TODO: Remove when size constraints are implemented
    let width = content.chars().count() as u16;
    let label = Label::new(StateValue::String(content.to_owned()));

    Widget::new(label).height(1).width(width)
}

pub fn labeled_container(
    theme: &theme::Theme,
    title: &str,
    component: Box<dyn MockComponent>,
) -> Widget<LabeledContainer> {
    let title = label(&format!(" {} ", title))
        .foreground(theme.colors.default_fg)
        .background(theme.colors.labeled_container_bg);
    let spacer = label("");
    let header = Widget::new(ContainerHeader::new(title, spacer));
    let container = LabeledContainer::new(header, component);

    Widget::new(container).background(theme.colors.labeled_container_bg)
}

pub fn shortcut(theme: &theme::Theme, short: &str, long: &str) -> Widget<Shortcut> {
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

pub fn shortcut_bar(theme: &theme::Theme, shortcuts: Vec<Widget<Shortcut>>) -> Widget<ShortcutBar> {
    let divider = label(&format!(" {} ", theme.icons.shortcutbar_divider))
        .foreground(theme.colors.shortcutbar_divider_fg);
    let shortcut_bar = ShortcutBar::new(shortcuts, divider);

    Widget::new(shortcut_bar).height(1)
}

pub fn property(theme: &theme::Theme, name: &str, value: &str) -> Widget<Property> {
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

pub fn property_list(
    _theme: &theme::Theme,
    properties: Vec<Widget<Property>>,
) -> Widget<PropertyList> {
    let property_list = PropertyList::new(properties);

    Widget::new(property_list)
}
