use anyhow::Result;

use tui_realm_stdlib::Phantom;
use tuirealm::application::PollStrategy;
use tuirealm::command::{Cmd, CmdResult, Direction as MoveDirection};
use tuirealm::event::{Event, Key, KeyEvent};
use tuirealm::props::{AttrValue, Attribute};
use tuirealm::{Application, Frame, MockComponent, NoUserEvent, State, StateValue};

use radicle_tui::cob::patch::{self};

use radicle_tui::subs;
use radicle_tui::ui;
use radicle_tui::ui::components::container::{GlobalListener, LabeledContainer, Tabs};
use radicle_tui::ui::components::context::{ContextBar, Shortcuts};
use radicle_tui::ui::components::list::PropertyList;
use radicle_tui::ui::components::workspace::{Browser, IssueBrowser, PatchActivity, PatchFiles};
use radicle_tui::ui::layout;
use radicle_tui::ui::theme::{self, Theme};
use radicle_tui::ui::widget::Widget;

use radicle_tui::Tui;

use radicle::cob::patch::{Patch, PatchId};
use radicle::identity::{Id, Project};
use radicle::profile::Profile;

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum HomeCid {
    Navigation,
    Dashboard,
    IssueBrowser,
    PatchBrowser,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum PatchCid {
    Navigation,
    Activity,
    Files,
    Context,
}

/// All component ids known to this application.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Cid {
    Home(HomeCid),
    Patch(PatchCid),
    Shortcuts,
    GlobalListener,
}

/// Messages handled by this application.
#[derive(Debug, Eq, PartialEq)]
pub enum HomeMessage {
    Show,
}

#[derive(Debug, Eq, PartialEq)]
pub enum PatchMessage {
    Show(usize),
    Leave,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Message {
    Home(HomeMessage),
    Patch(PatchMessage),
    NavigationChanged(u16),
    Tick,
    Quit,
}

pub struct Context {
    profile: Profile,
    id: Id,
    project: Project,
    selected_patch: usize,
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
                selected_patch: 0,
                patches,
            },
            theme: theme::default_dark(),
            active_page: Box::<Home>::default(),
            quit: false,
        }
    }

    fn mount_home_view(
        &mut self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        theme: &Theme,
    ) -> Result<()> {
        self.active_page = Box::<Home>::default();
        self.active_page.mount(app, &self.context, theme)?;
        self.active_page.activate(app)?;

        Ok(())
    }

    fn mount_patch_view(
        &mut self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        theme: &Theme,
    ) -> Result<()> {
        self.active_page = Box::<PatchView>::default();
        self.active_page.mount(app, &self.context, theme)?;
        self.active_page.activate(app)?;

        Ok(())
    }
}

impl Tui<Cid, Message> for App {
    fn init(&mut self, app: &mut Application<Cid, Message, NoUserEvent>) -> Result<()> {
        self.mount_home_view(app, &self.theme.clone())?;

        // Add global key listener and subscribe to key events
        let global = ui::global_listener().to_boxed();
        app.mount(Cid::GlobalListener, global, subs::global())?;

        Ok(())
    }

    fn view(&mut self, app: &mut Application<Cid, Message, NoUserEvent>, frame: &mut Frame) {
        self.active_page.as_mut().view(app, frame);
    }

    fn update(&mut self, app: &mut Application<Cid, Message, NoUserEvent>) -> Result<bool> {
        match app.tick(PollStrategy::Once) {
            Ok(messages) if !messages.is_empty() => {
                let theme = theme::default_dark();
                for message in messages {
                    match message {
                        Message::Home(HomeMessage::Show) => {
                            self.mount_home_view(app, &theme)?;
                        }
                        Message::Patch(PatchMessage::Show(index)) => {
                            self.context.selected_patch = index;
                            self.mount_patch_view(app, &theme)?;
                        }
                        Message::Patch(PatchMessage::Leave) => {
                            self.mount_home_view(app, &theme)?;
                        }
                        Message::Quit => self.quit = true,
                        _ => {
                            self.active_page.update(message);
                            self.active_page.activate(app)?;
                        }
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
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
        app: &mut Application<Cid, Message, NoUserEvent>,
        context: &Context,
        theme: &Theme,
    ) -> Result<()>;

    fn update(&mut self, message: Message);

    fn view(&mut self, app: &mut Application<Cid, Message, NoUserEvent>, frame: &mut Frame);

    fn activate(&self, app: &mut Application<Cid, Message, NoUserEvent>) -> Result<()>;
}

///
/// Home
///
pub struct Home {
    active_component: Cid,
}

impl Default for Home {
    fn default() -> Self {
        Home {
            active_component: Cid::Home(HomeCid::Dashboard),
        }
    }
}

impl ViewPage for Home {
    fn mount(
        &self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        context: &Context,
        theme: &Theme,
    ) -> Result<()> {
        let navigation = ui::home_navigation(theme).to_boxed();

        let dashboard = ui::dashboard(theme, &context.id, &context.project).to_boxed();
        let issue_browser = ui::issue_browser(theme).to_boxed();
        let patch_browser = ui::patch_browser(theme, &context.patches, &context.profile).to_boxed();

        let shortcuts = ui::shortcuts(
            theme,
            vec![
                ui::shortcut(theme, "tab", "section"),
                ui::shortcut(theme, "q", "quit"),
            ],
        )
        .to_boxed();

        app.remount(
            Cid::Home(HomeCid::Navigation),
            navigation,
            subs::navigation(),
        )?;

        app.remount(Cid::Home(HomeCid::Dashboard), dashboard, vec![])?;
        app.remount(Cid::Home(HomeCid::IssueBrowser), issue_browser, vec![])?;
        app.remount(Cid::Home(HomeCid::PatchBrowser), patch_browser, vec![])?;

        app.remount(Cid::Shortcuts, shortcuts, vec![])?;
        Ok(())
    }

    fn update(&mut self, message: Message) {
        if let Message::NavigationChanged(index) = message {
            self.active_component = match index {
                0 => Cid::Home(HomeCid::Dashboard),
                1 => Cid::Home(HomeCid::IssueBrowser),
                2 => Cid::Home(HomeCid::PatchBrowser),
                _ => Cid::Home(HomeCid::Dashboard),
            };
        }
    }

    fn view(&mut self, app: &mut Application<Cid, Message, NoUserEvent>, frame: &mut Frame) {
        let area = frame.size();
        let navigation_h = 2u16;
        let shortcuts_h = app
            .query(&Cid::Shortcuts, Attribute::Height)
            .ok()
            .flatten()
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let layout = layout::default_page(area, navigation_h, shortcuts_h);

        app.view(&Cid::Home(HomeCid::Navigation), frame, layout[0]);
        app.view(&self.active_component, frame, layout[1]);
        app.view(&Cid::Shortcuts, frame, layout[2]);
    }

    fn activate(&self, app: &mut Application<Cid, Message, NoUserEvent>) -> Result<()> {
        app.active(&self.active_component)?;
        Ok(())
    }
}

///
/// Patch detail page
///
pub struct PatchView {
    active_component: Cid,
}

impl Default for PatchView {
    fn default() -> Self {
        PatchView {
            active_component: Cid::Patch(PatchCid::Activity),
        }
    }
}

impl ViewPage for PatchView {
    fn mount(
        &self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        context: &Context,
        theme: &Theme,
    ) -> Result<()> {
        if let Some((id, patch)) = context.patches.get(context.selected_patch) {
            let navigation = ui::patch_navigation(theme).to_boxed();
            let activity = ui::patch_activity(theme).to_boxed();
            let files = ui::patch_files(theme).to_boxed();
            let context = ui::patch_context(theme, (*id, patch), &context.profile).to_boxed();
            let shortcuts = ui::shortcuts(
                theme,
                vec![
                    ui::shortcut(theme, "esc", "back"),
                    ui::shortcut(theme, "tab", "section"),
                    ui::shortcut(theme, "q", "quit"),
                ],
            )
            .to_boxed();

            app.remount(
                Cid::Patch(PatchCid::Navigation),
                navigation,
                subs::navigation(),
            )?;
            app.remount(Cid::Patch(PatchCid::Activity), activity, vec![])?;
            app.remount(Cid::Patch(PatchCid::Files), files, vec![])?;
            app.remount(Cid::Patch(PatchCid::Context), context, vec![])?;
            app.remount(Cid::Shortcuts, shortcuts, vec![])?;
        }
        Ok(())
    }

    fn update(&mut self, message: Message) {
        if let Message::NavigationChanged(index) = message {
            self.active_component = match index {
                0 => Cid::Patch(PatchCid::Activity),
                1 => Cid::Patch(PatchCid::Files),
                _ => Cid::Patch(PatchCid::Activity),
            };
        }
    }

    fn view(&mut self, app: &mut Application<Cid, Message, NoUserEvent>, frame: &mut Frame) {
        let area = frame.size();
        let navigation_h = 2u16;
        let context_h = 1u16;
        let shortcuts_h = app
            .query(&Cid::Shortcuts, Attribute::Height)
            .ok()
            .flatten()
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let layout = layout::page_with_context(area, navigation_h, context_h, shortcuts_h);

        app.view(&Cid::Patch(PatchCid::Navigation), frame, layout[0]);
        app.view(&self.active_component, frame, layout[1]);
        app.view(&Cid::Patch(PatchCid::Context), frame, layout[2]);
        app.view(&Cid::Shortcuts, frame, layout[3]);
    }

    fn activate(&self, app: &mut Application<Cid, Message, NoUserEvent>) -> Result<()> {
        app.active(&self.active_component)?;
        Ok(())
    }
}

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
            Event::WindowResize(_, _) => Some(Message::Tick),
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
