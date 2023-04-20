use radicle::cob::patch::{Patch, PatchId};

use tui_realm_stdlib::Phantom;
use tuirealm::command::{Cmd, CmdResult, Direction as MoveDirection};
use tuirealm::event::{Event, Key, KeyEvent};
use tuirealm::{MockComponent, NoUserEvent, State, StateValue};

use radicle_tui::ui::components::container::{GlobalListener, LabeledContainer, Tabs};
use radicle_tui::ui::components::context::{ContextBar, Shortcuts};
use radicle_tui::ui::components::list::PropertyList;
use radicle_tui::ui::components::workspace::{
    Browser, Dashboard, IssueBrowser, PatchActivity, PatchFiles,
};

use radicle_tui::ui::widget::Widget;

use super::{Message, PatchMessage};

/// Since the framework does not know the type of messages that are being
/// passed around in the app, the following handlers need to be implemented for
/// each component used.
impl tuirealm::Component<Message, NoUserEvent> for Widget<GlobalListener> {
    fn on(&mut self, event: Event<NoUserEvent>) -> Option<Message> {
        match event {
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
                self.perform(Cmd::Move(MoveDirection::Up));
                None
            }
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            }) => {
                self.perform(Cmd::Move(MoveDirection::Down));
                None
            }
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => match self.perform(Cmd::Submit) {
                CmdResult::Submit(State::One(StateValue::Usize(index))) => {
                    Some(Message::Patch(PatchMessage::Show(index)))
                }
                _ => None,
            },
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

impl tuirealm::Component<Message, NoUserEvent> for Widget<PatchActivity> {
    fn on(&mut self, event: Event<NoUserEvent>) -> Option<Message> {
        match event {
            Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => {
                Some(Message::Patch(PatchMessage::Leave))
            }
            _ => None,
        }
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<PatchFiles> {
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

impl tuirealm::Component<Message, NoUserEvent> for Phantom {
    fn on(&mut self, _event: Event<NoUserEvent>) -> Option<Message> {
        None
    }
}
