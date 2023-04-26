use radicle::cob::patch::{Patch, PatchId};

use tuirealm::command::{Cmd, CmdResult, Direction as MoveDirection};
use tuirealm::event::{Event, Key, KeyEvent};
use tuirealm::{MockComponent, NoUserEvent, State, StateValue};

use radicle_tui::ui::components::common::container::{GlobalListener, LabeledContainer, Tabs};
use radicle_tui::ui::components::common::context::{ContextBar, Shortcuts};
use radicle_tui::ui::components::common::list::PropertyList;
use radicle_tui::ui::components::common::Browser;
use radicle_tui::ui::components::home::{Dashboard, IssueBrowser};
use radicle_tui::ui::components::patch;

use radicle_tui::ui::widget::Widget;

use super::{HomeMessage, Message, PatchMessage};

/// Since the framework does not know the type of messages that are being
/// passed around in the app, the following handlers need to be implemented for
/// each component used.
impl tuirealm::Component<Message, NoUserEvent> for Widget<GlobalListener> {
    fn on(&mut self, event: Event<NoUserEvent>) -> Option<Message> {
        match event {
            Event::WindowResize(_, _) => Some(Message::Tick),
            Event::Keyboard(KeyEvent {
                code: Key::Char('q'),
                ..
            }) => Some(Message::Quit),
            _ => None,
        }
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<Tabs> {
    fn on(&mut self, event: Event<NoUserEvent>) -> Option<Message> {
        match event {
            Event::Keyboard(KeyEvent { code: Key::Tab, .. }) => {
                match self.perform(Cmd::Move(MoveDirection::Right)) {
                    CmdResult::Changed(State::One(StateValue::U16(index))) => {
                        Some(Message::NavigationChanged(index))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<Browser<(PatchId, Patch)>> {
    fn on(&mut self, event: Event<NoUserEvent>) -> Option<Message> {
        match event {
            Event::Keyboard(KeyEvent { code: Key::Up, .. }) => {
                match self.perform(Cmd::Move(MoveDirection::Up)) {
                    CmdResult::Changed(State::One(StateValue::Usize(index))) => {
                        Some(Message::Home(HomeMessage::PatchChanged(index)))
                    }
                    _ => Some(Message::Tick),
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            }) => match self.perform(Cmd::Move(MoveDirection::Down)) {
                CmdResult::Changed(State::One(StateValue::Usize(index))) => {
                    Some(Message::Home(HomeMessage::PatchChanged(index)))
                }
                _ => Some(Message::Tick),
            },
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => Some(Message::Patch(PatchMessage::Show)),
            _ => None,
        }
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<Dashboard> {
    fn on(&mut self, _event: Event<NoUserEvent>) -> Option<Message> {
        None
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<IssueBrowser> {
    fn on(&mut self, _event: Event<NoUserEvent>) -> Option<Message> {
        None
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<patch::Activity> {
    fn on(&mut self, event: Event<NoUserEvent>) -> Option<Message> {
        match event {
            Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => {
                Some(Message::Patch(PatchMessage::Leave))
            }
            _ => None,
        }
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<patch::Files> {
    fn on(&mut self, event: Event<NoUserEvent>) -> Option<Message> {
        match event {
            Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => {
                Some(Message::Patch(PatchMessage::Leave))
            }
            _ => None,
        }
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<LabeledContainer> {
    fn on(&mut self, _event: Event<NoUserEvent>) -> Option<Message> {
        None
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<PropertyList> {
    fn on(&mut self, _event: Event<NoUserEvent>) -> Option<Message> {
        None
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<ContextBar> {
    fn on(&mut self, _event: Event<NoUserEvent>) -> Option<Message> {
        None
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<Shortcuts> {
    fn on(&mut self, _event: Event<NoUserEvent>) -> Option<Message> {
        None
    }
}
