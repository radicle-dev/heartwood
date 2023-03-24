use std::ops::Deref;

use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{AttrValue, Attribute, Color, Props};
use tuirealm::tui::layout::Rect;
use tuirealm::{Frame, MockComponent, State};

pub type BoxedWidget<T> = Box<Widget<T>>;

pub trait WidgetComponent {
    fn view(&mut self, properties: &Props, frame: &mut Frame, area: Rect);

    fn state(&self) -> State;

    fn perform(&mut self, cmd: Cmd) -> CmdResult;
}

#[derive(Clone)]
pub struct Widget<T: WidgetComponent> {
    component: T,
    properties: Props,
}

impl<T: WidgetComponent> Deref for Widget<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.component
    }
}

impl<T: WidgetComponent> Widget<T> {
    pub fn new(component: T) -> Self {
        Widget {
            component,
            properties: Props::default(),
        }
    }

    pub fn foreground(mut self, fg: Color) -> Self {
        self.attr(Attribute::Foreground, AttrValue::Color(fg));
        self
    }

    pub fn highlight(mut self, fg: Color) -> Self {
        self.attr(Attribute::HighlightedColor, AttrValue::Color(fg));
        self
    }

    pub fn background(mut self, bg: Color) -> Self {
        self.attr(Attribute::Background, AttrValue::Color(bg));
        self
    }

    pub fn height(mut self, h: u16) -> Self {
        self.attr(Attribute::Height, AttrValue::Size(h));
        self
    }

    pub fn width(mut self, w: u16) -> Self {
        self.attr(Attribute::Width, AttrValue::Size(w));
        self
    }

    pub fn content(mut self, content: AttrValue) -> Self {
        self.attr(Attribute::Content, content);
        self
    }

    pub fn to_boxed(self) -> Box<Self> {
        Box::new(self)
    }
}

impl<T: WidgetComponent> MockComponent for Widget<T> {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        self.component.view(&self.properties, frame, area)
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.properties.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.properties.set(attr, value)
    }

    fn state(&self) -> State {
        self.component.state()
    }

    fn perform(&mut self, cmd: Cmd) -> CmdResult {
        self.component.perform(cmd)
    }
}
