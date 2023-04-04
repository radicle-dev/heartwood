use std::time::Duration;

use anyhow::Result;

use tui_realm_stdlib::Phantom;
use tuirealm::application::PollStrategy;
use tuirealm::command::{Cmd, CmdResult, Direction as MoveDirection};
use tuirealm::event::{Event, Key, KeyEvent};
use tuirealm::props::{AttrValue, Attribute};
use tuirealm::tui::layout::{Constraint, Direction, Layout};
use tuirealm::{Application, Component, Frame, MockComponent, NoUserEvent, State, StateValue};

use radicle_tui::cob::patch::{self};

use radicle_tui::ui;
use radicle_tui::ui::components::container::{GlobalListener, LabeledContainer, Tabs};
use radicle_tui::ui::components::context::Shortcuts;
use radicle_tui::ui::components::list::PropertyList;
use radicle_tui::ui::components::workspace::Browser;
use radicle_tui::ui::theme;
use radicle_tui::ui::widget::Widget;

use radicle_tui::subs;

use radicle_tui::Tui;

use radicle::cob::patch::{Patch, PatchId};
use radicle::identity::{Id, Project};
use radicle::profile::Profile;

/// Messages handled by this application.
#[derive(Debug, Eq, PartialEq)]
pub enum Message {
    NavigationChanged(u16),
    Quit,
}

/// All components known to this application.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum ComponentId {
    Navigation,
    Dashboard,
    IssueBrowser,
    PatchBrowser,
    Shortcuts,
    GlobalListener,
}

#[allow(dead_code)]
pub struct App {
    profile: Profile,
    id: Id,
    project: Project,
    patches: Vec<(PatchId, Patch)>,
    active: ComponentId,
    quit: bool,
}

/// Creates a new application using a tui-realm-application, mounts all
/// components and sets focus to a default one.
impl App {
    pub fn new(profile: Profile, id: Id, project: Project) -> Self {
        let patches = patch::load_all(&profile, id);
        Self {
            id,
            profile,
            project,
            patches,
            active: ComponentId::Dashboard,
            quit: false,
        }
    }
}

impl Tui<ComponentId, Message> for App {
    fn init(&mut self, app: &mut Application<ComponentId, Message, NoUserEvent>) -> Result<()> {
        let theme = theme::default_dark();

        let navigation = ui::navigation(&theme).to_boxed();

        let dashboard = ui::dashboard(&theme, &self.id, &self.project).to_boxed();
        let issue_browser = Box::<Phantom>::default();
        let patch_browser = ui::patch_browser(&theme, &self.patches, &self.profile).to_boxed();

        let global_listener = ui::global_listener().to_boxed();

        let shortcuts = ui::shortcuts(
            &theme,
            vec![
                ui::shortcut(&theme, "tab", "section"),
                ui::shortcut(&theme, "q", "quit"),
            ],
        )
        .to_boxed();

        app.mount(
            ComponentId::Navigation,
            navigation,
            vec![subs::navigation()],
        )?;

        app.mount(ComponentId::Dashboard, dashboard, vec![])?;
        app.mount(ComponentId::IssueBrowser, issue_browser, vec![])?;
        app.mount(ComponentId::PatchBrowser, patch_browser, vec![])?;

        app.mount(ComponentId::Shortcuts, shortcuts, vec![])?;

        // Add global key listener and subscribe to key events
        app.mount(
            ComponentId::GlobalListener,
            global_listener,
            vec![subs::global()],
        )?;

        // We need to give focus to a component then
        app.active(&ComponentId::Dashboard)?;

        Ok(())
    }

    fn view(
        &mut self,
        app: &mut Application<ComponentId, Message, NoUserEvent>,
        frame: &mut Frame,
    ) {
        let area = frame.size();
        let margin_h = 1u16;
        let navigation_h = 2u16;
        let shortcuts_h = app
            .query(&ComponentId::Shortcuts, Attribute::Height)
            .ok()
            .flatten()
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let workspaces_h = area.height.saturating_sub(
            shortcuts_h
                .saturating_add(navigation_h)
                .saturating_add(margin_h),
        );

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .horizontal_margin(margin_h)
            .constraints(
                [
                    Constraint::Length(navigation_h),
                    Constraint::Length(workspaces_h),
                    Constraint::Length(shortcuts_h),
                ]
                .as_ref(),
            )
            .split(area);

        app.view(&ComponentId::Navigation, frame, layout[0]);
        app.view(&self.active, frame, layout[1]);
        app.view(&ComponentId::Shortcuts, frame, layout[2]);
    }

    fn update(
        &mut self,
        app: &mut Application<ComponentId, Message, NoUserEvent>,
        interval: u64,
    ) -> Result<()> {
        if let Ok(messages) = app.tick(PollStrategy::TryFor(Duration::from_millis(interval))) {
            for message in messages {
                match message {
                    Message::NavigationChanged(index) => {
                        self.active = match index {
                            0 => ComponentId::Dashboard,
                            1 => ComponentId::IssueBrowser,
                            2 => ComponentId::PatchBrowser,
                            _ => ComponentId::Dashboard,
                        };
                        app.active(&self.active)?;
                    }
                    Message::Quit => self.quit = true,
                }
            }
        }

        Ok(())
    }

    fn quit(&self) -> bool {
        self.quit
    }
}

/// Since the framework does not know the type of messages that are being
/// passed around in the app, the following handlers need to be implemented for
/// each component used.
impl Component<Message, NoUserEvent> for Widget<GlobalListener> {
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

impl Component<Message, NoUserEvent> for Widget<Tabs> {
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

impl Component<Message, NoUserEvent> for Widget<Browser<(PatchId, Patch)>> {
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
            _ => None,
        }
    }
}

impl Component<Message, NoUserEvent> for Widget<LabeledContainer> {
    fn on(&mut self, _event: Event<NoUserEvent>) -> Option<Message> {
        None
    }
}

impl Component<Message, NoUserEvent> for Widget<PropertyList> {
    fn on(&mut self, _event: Event<NoUserEvent>) -> Option<Message> {
        None
    }
}

impl Component<Message, NoUserEvent> for Widget<Shortcuts> {
    fn on(&mut self, _event: Event<NoUserEvent>) -> Option<Message> {
        None
    }
}

impl Component<Message, NoUserEvent> for Phantom {
    fn on(&mut self, _event: Event<NoUserEvent>) -> Option<Message> {
        None
    }
}
