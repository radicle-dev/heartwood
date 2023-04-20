use std::hash::Hash;

use tuirealm::event::{Key, KeyEvent, KeyModifiers};
use tuirealm::{Sub, SubClause, SubEventClause};

pub fn navigation<Id, UserEvent>() -> Vec<Sub<Id, UserEvent>>
where
    Id: Clone + Hash + Eq + PartialEq,
    UserEvent: Clone + Eq + PartialEq + PartialOrd,
{
    vec![Sub::new(
        SubEventClause::Keyboard(KeyEvent {
            code: Key::Tab,
            modifiers: KeyModifiers::NONE,
        }),
        SubClause::Always,
    )]
}

pub fn global<Id, UserEvent>() -> Vec<Sub<Id, UserEvent>>
where
    Id: Clone + Hash + Eq + PartialEq,
    UserEvent: Clone + Eq + PartialEq + PartialOrd,
{
    vec![
        Sub::new(
            SubEventClause::Keyboard(KeyEvent {
                code: Key::Char('q'),
                modifiers: KeyModifiers::NONE,
            }),
            SubClause::Always,
        ),
        Sub::new(SubEventClause::WindowResize, SubClause::Always),
    ]
}
