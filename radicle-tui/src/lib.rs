use std::hash::Hash;
use std::time::Duration;

use anyhow::Result;

use tuirealm::terminal::TerminalBridge;
use tuirealm::Frame;
use tuirealm::{Application, EventListenerCfg, NoUserEvent};

pub mod cob;
pub mod ui;

/// Trait that must be implemented by client applications in order to be run
/// as tui-application using tui-realm. Implementors act as models to the
/// tui-realm application that can be polled for new messages, updated
/// accordingly and rendered with new state.
///
/// Please see `examples/` for further information on how to use it.
pub trait Tui<Id, Message>
where
    Id: Eq + PartialEq + Clone + Hash,
    Message: Eq,
{
    /// Should initialize an application by mounting and activating components.
    fn init(&mut self, app: &mut Application<Id, Message, NoUserEvent>) -> Result<()>;

    /// Should update the current state by handling a message from the view. Returns true
    /// if view should be updated (e.g. a message was received and the current state changed).
    fn update(&mut self, app: &mut Application<Id, Message, NoUserEvent>) -> Result<bool>;

    /// Should draw the application to a frame.
    fn view(&mut self, app: &mut Application<Id, Message, NoUserEvent>, frame: &mut Frame);

    /// Should return true if the application is requested to quit.
    fn quit(&self) -> bool;
}

/// A tui-window using the cross-platform Terminal helper provided
/// by tui-realm.
pub struct Window {
    /// Helper around `Terminal` to quickly setup and perform on terminal.
    pub terminal: TerminalBridge,
}

impl Default for Window {
    fn default() -> Self {
        Self::new()
    }
}

/// Provides a way to create and run a new tui-application.
impl Window {
    /// Creates a tui-window using the default cross-platform Terminal
    /// helper and panics if its creation fails.
    pub fn new() -> Self {
        Self {
            terminal: TerminalBridge::new().expect("Cannot create terminal bridge"),
        }
    }

    /// Runs this tui-window with the tui-application given and performs the
    /// following steps:
    /// 1. Enter alternative terminal screen
    /// 2. Run main loop until application should quit and with each iteration
    ///    - poll new events (tick or user event)
    ///    - update application state
    ///    - redraw view
    /// 3. Leave alternative terminal screen
    pub fn run<T, Id, Message>(&mut self, tui: &mut T, interval: u64) -> Result<()>
    where
        T: Tui<Id, Message>,
        Id: Eq + PartialEq + Clone + Hash,
        Message: Eq,
    {
        let mut update = true;
        let mut app = Application::init(
            EventListenerCfg::default().default_input_listener(Duration::from_millis(interval)),
        );
        tui.init(&mut app)?;

        while !tui.quit() {
            if update {
                self.terminal
                    .raw_mut()
                    .draw(|frame| tui.view(&mut app, frame))?;
            }
            update = tui.update(&mut app)?;
        }

        Ok(())
    }
}
