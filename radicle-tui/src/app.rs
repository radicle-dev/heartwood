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
use radicle_tui::ui::theme::{self, Theme};
use radicle_tui::ui::widget::Widget;

use radicle_tui::subs;

use radicle_tui::Tui;

use radicle::cob::patch::{Patch, PatchId};
use radicle::identity::{Id, Project};
use radicle::profile::Profile;

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

/// Messages handled by this application.
#[derive(Debug, Eq, PartialEq)]
pub enum Message {
    NavigationChanged(u16),
    Quit,
}

pub struct Context {
    profile: Profile,
    id: Id,
    project: Project,
    patches: Vec<(PatchId, Patch)>,
}

#[allow(dead_code)]
pub struct App {
    context: Context,
    active_page: Box<dyn ViewPage>,
    theme: Theme,
    quit: bool,
}

/// Creates a new application using a tui-realm-application, mounts all
/// components and sets focus to a default one.
impl App {
    pub fn new(profile: Profile, id: Id, project: Project) -> Self {
        let patches = patch::load_all(&profile, id);
        Self {
            context: Context {
                id,
                profile,
                project,
                patches,
            },
            theme: theme::default_dark(),
            active_page: Box::<Home>::default(),
            quit: false,
        }
    }

    fn mount_home(
        &mut self,
        app: &mut Application<ComponentId, Message, NoUserEvent>,
        theme: &Theme,
    ) -> Result<()> {
        self.active_page = Box::<Home>::default();
        self.active_page.mount(app, &self.context, theme)?;
        self.active_page.activate(app)?;

        Ok(())
    }
}

impl Tui<ComponentId, Message> for App {
    fn init(&mut self, app: &mut Application<ComponentId, Message, NoUserEvent>) -> Result<()> {
        self.mount_home(app, &self.theme.clone())?;

        // Add global key listener and subscribe to key events
        let global = ui::global_listener().to_boxed();
        app.mount(ComponentId::GlobalListener, global, subs::global())?;

        Ok(())
    }

    fn view(
        &mut self,
        app: &mut Application<ComponentId, Message, NoUserEvent>,
        frame: &mut Frame,
    ) {
        self.active_page.as_mut().view(app, frame);
    }

    fn update(
        &mut self,
        app: &mut Application<ComponentId, Message, NoUserEvent>,
        interval: u64,
    ) -> Result<()> {
        if let Ok(messages) = app.tick(PollStrategy::TryFor(Duration::from_millis(interval))) {
            for message in messages {
                match message {
                    Message::Quit => self.quit = true,
                    _ => {
                        self.active_page.update(message);
                        self.active_page.activate(app)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn quit(&self) -> bool {
        self.quit
    }
}

/// `tuirealm`'s event and prop system is designed to work with flat component hierarchies.
/// Building deep nested component hierarchies would need a lot more additional effort to 
/// properly pass events and props down these hierarchies. This makes it hard to implement 
/// full app views (home, patch details etc) as components.
/// 
/// View pages take into account these flat component hierarchies, and provide
/// switchable sets of components. 
pub trait ViewPage {
    fn mount(
        &self,
        app: &mut Application<ComponentId, Message, NoUserEvent>,
        context: &Context,
        theme: &Theme,
    ) -> Result<()>;

    fn update(&mut self, message: Message);

    fn view(&mut self, app: &mut Application<ComponentId, Message, NoUserEvent>, frame: &mut Frame);

    fn activate(&self, app: &mut Application<ComponentId, Message, NoUserEvent>) -> Result<()>;
}

pub struct Home {
    active_component: ComponentId,
}

impl Default for Home {
    fn default() -> Self {
        Home {
            active_component: ComponentId::Dashboard,
        }
    }
}

impl ViewPage for Home {
    fn mount(
        &self,
        app: &mut Application<ComponentId, Message, NoUserEvent>,
        context: &Context,
        theme: &Theme,
    ) -> Result<()> {
        let navigation = ui::navigation(theme).to_boxed();

        let dashboard = ui::dashboard(theme, &context.id, &context.project).to_boxed();
        let issue_browser = Box::<Phantom>::default();
        let patch_browser = ui::patch_browser(theme, &context.patches, &context.profile).to_boxed();

        let shortcuts = ui::shortcuts(
            theme,
            vec![
                ui::shortcut(theme, "tab", "section"),
                ui::shortcut(theme, "q", "quit"),
            ],
        )
        .to_boxed();

        app.remount(ComponentId::Navigation, navigation, subs::navigation())?;

        app.remount(ComponentId::Dashboard, dashboard, vec![])?;
        app.remount(ComponentId::IssueBrowser, issue_browser, vec![])?;
        app.remount(ComponentId::PatchBrowser, patch_browser, vec![])?;

        app.remount(ComponentId::Shortcuts, shortcuts, vec![])?;
        Ok(())
    }

    fn update(&mut self, message: Message) {
        if let Message::NavigationChanged(index) = message {
            self.active_component = match index {
                0 => ComponentId::Dashboard,
                1 => ComponentId::IssueBrowser,
                2 => ComponentId::PatchBrowser,
                _ => ComponentId::Dashboard,
            };
        }
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
        app.view(&self.active_component, frame, layout[1]);
        app.view(&ComponentId::Shortcuts, frame, layout[2]);
    }

    fn activate(&self, app: &mut Application<ComponentId, Message, NoUserEvent>) -> Result<()> {
        app.active(&self.active_component)?;
        Ok(())
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
