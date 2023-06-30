use tuirealm::command::{Cmd, CmdResult, Direction as MoveDirection};
use tuirealm::event::{Event, Key, KeyEvent};
use tuirealm::{MockComponent, NoUserEvent, State, StateValue};

use radicle_tui::ui::widget::common::container::{AppHeader, GlobalListener, LabeledContainer};
use radicle_tui::ui::widget::common::context::{ContextBar, Shortcuts};
use radicle_tui::ui::widget::common::list::PropertyList;
use radicle_tui::ui::widget::home::{Dashboard, IssueBrowser, PatchBrowser};
use radicle_tui::ui::widget::{issue, patch};

use radicle_tui::ui::widget::Widget;

use super::{IssueMessage, Message, PatchMessage};

/// Since the framework does not know the type of messages that are being
/// passed around in the app, the following handlers need to be implemented for
/// each component used.
///
/// TODO: should handle `Event::WindowResize`, which is not emitted by `termion`.
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

impl tuirealm::Component<Message, NoUserEvent> for Widget<AppHeader> {
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

impl tuirealm::Component<Message, NoUserEvent> for Widget<issue::LargeList> {
    fn on(&mut self, event: Event<NoUserEvent>) -> Option<Message> {
        match event {
            Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => {
                Some(Message::Issue(IssueMessage::Leave))
            }
            Event::Keyboard(KeyEvent { code: Key::Up, .. }) => {
                let result = self.perform(Cmd::Move(MoveDirection::Up));
                match result {
                    CmdResult::Changed(State::One(StateValue::Usize(selected))) => {
                        let item = self.items().get(selected)?;
                        Some(Message::Issue(IssueMessage::Changed(item.id().to_owned())))
                    }
                    _ => None,
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            }) => {
                let result = self.perform(Cmd::Move(MoveDirection::Down));
                match result {
                    CmdResult::Changed(State::One(StateValue::Usize(selected))) => {
                        let item = self.items().get(selected)?;
                        Some(Message::Issue(IssueMessage::Changed(item.id().to_owned())))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<issue::Details> {
    fn on(&mut self, _event: Event<NoUserEvent>) -> Option<Message> {
        None
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<PatchBrowser> {
    fn on(&mut self, event: Event<NoUserEvent>) -> Option<Message> {
        match event {
            Event::Keyboard(KeyEvent { code: Key::Up, .. }) => {
                self.perform(Cmd::Move(MoveDirection::Up));
                Some(Message::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            }) => {
                self.perform(Cmd::Move(MoveDirection::Down));
                Some(Message::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => {
                let result = self.perform(Cmd::Submit);
                match result {
                    CmdResult::Submit(State::One(StateValue::Usize(selected))) => {
                        let item = self.items().get(selected)?;
                        Some(Message::Patch(PatchMessage::Show(item.id().to_owned())))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<IssueBrowser> {
    fn on(&mut self, event: Event<NoUserEvent>) -> Option<Message> {
        match event {
            Event::Keyboard(KeyEvent { code: Key::Up, .. }) => {
                self.perform(Cmd::Move(MoveDirection::Up));
                Some(Message::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            }) => {
                self.perform(Cmd::Move(MoveDirection::Down));
                Some(Message::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => {
                let result = self.perform(Cmd::Submit);
                match result {
                    CmdResult::Submit(State::One(StateValue::Usize(selected))) => {
                        let item = self.items().get(selected)?;
                        Some(Message::Issue(IssueMessage::Show(item.id().to_owned())))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

impl tuirealm::Component<Message, NoUserEvent> for Widget<Dashboard> {
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
