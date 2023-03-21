use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{AttrValue, Attribute, Color, Props, Style};
use tuirealm::tui::layout::Rect;
use tuirealm::tui::text::Span;
use tuirealm::{Frame, MockComponent, State, StateValue};

use crate::ui::widget::{Widget, WidgetComponent};

/// A label that can be styled using a foreground color and text modifiers.
/// Its height is fixed, its width depends on the length of the text it displays.
#[derive(Clone)]
pub struct Label {
    content: StateValue,
}

impl Label {
    pub fn new(content: StateValue) -> Self {
        Self { content }
    }
}

impl WidgetComponent for Label {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect) {
        use tui_realm_stdlib::Label;

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
                    .text(self.content.clone().unwrap_string()),
                None => Label::default()
                    .foreground(foreground)
                    .background(background)
                    .text(self.content.clone().unwrap_string()),
            };

            label.view(frame, area);
        }
    }

    fn state(&self) -> State {
        State::One(self.content.clone())
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl From<&Widget<Label>> for Span<'_> {
    fn from(label: &Widget<Label>) -> Self {
        Span::styled(label.content.clone().unwrap_string(), Style::default())
    }
}
