use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{AttrValue, Attribute, Color, Props, Style};
use tuirealm::tui::layout::Rect;
use tuirealm::tui::text::{Span, Text};
use tuirealm::{Frame, MockComponent, State};

use crate::ui::widget::{Widget, WidgetComponent};

/// A label that can be styled using a foreground color and text modifiers.
/// Its height is fixed, its width depends on the length of the text it displays.
#[derive(Clone, Default)]
pub struct Label;

impl WidgetComponent for Label {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        use tui_realm_stdlib::Label;

        let content = properties
            .get_or(Attribute::Content, AttrValue::String(String::default()))
            .unwrap_string();
        let display = properties
            .get_or(Attribute::Display, AttrValue::Flag(true))
            .unwrap_flag();
        let foreground = properties
            .get_or(Attribute::Foreground, AttrValue::Color(Color::Reset))
            .unwrap_color();
        let background = properties
            .get_or(Attribute::Background, AttrValue::Color(Color::Reset))
            .unwrap_color();

        if display {
            let mut label = match properties.get(Attribute::TextProps) {
                Some(modifiers) => Label::default()
                    .foreground(foreground)
                    .background(background)
                    .modifiers(modifiers.unwrap_text_modifiers())
                    .text(content),
                None => Label::default()
                    .foreground(foreground)
                    .background(background)
                    .text(content),
            };

            label.view(frame, area);
        }
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl From<&Widget<Label>> for Span<'_> {
    fn from(label: &Widget<Label>) -> Self {
        let content = label
            .query(Attribute::Content)
            .unwrap_or(AttrValue::String(String::default()))
            .unwrap_string();

        Span::styled(content, Style::default())
    }
}

impl From<&Widget<Label>> for Text<'_> {
    fn from(label: &Widget<Label>) -> Self {
        let content = label
            .query(Attribute::Content)
            .unwrap_or(AttrValue::String(String::default()))
            .unwrap_string();
        let foreground = label
            .query(Attribute::Foreground)
            .unwrap_or(AttrValue::Color(Color::Reset))
            .unwrap_color();

        Text::styled(content, Style::default().fg(foreground))
    }
}
