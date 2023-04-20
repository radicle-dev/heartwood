pub mod event;
pub mod page;
pub mod subscription;

use anyhow::Result;

use radicle::cob::patch::{Patch, PatchId};
use radicle::identity::{Id, Project};
use radicle::profile::Profile;

use tuirealm::application::PollStrategy;
use tuirealm::{Application, Frame, NoUserEvent};

use radicle_tui::cob::patch::{self};

use radicle_tui::ui;
use radicle_tui::ui::theme::{self, Theme};
use radicle_tui::Tui;

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
}

/// All component ids known to this application.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Cid {
    Home(HomeCid),
    Patch(PatchCid),
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
    active_page: Box<dyn page::ViewPage>,
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
            active_page: Box::<page::Home>::default(),
            quit: false,
        }
    }

    fn mount_home_view(
        &mut self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        theme: &Theme,
    ) -> Result<()> {
        self.active_page = Box::<page::Home>::default();
        self.active_page.mount(app, &self.context, theme)?;
        self.active_page.activate(app)?;

        Ok(())
    }

    fn mount_patch_view(
        &mut self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        theme: &Theme,
    ) -> Result<()> {
        self.active_page = Box::<page::PatchView>::default();
        self.active_page.mount(app, &self.context, theme)?;
        self.active_page.activate(app)?;

        Ok(())
    }
}

impl Tui<Cid, Message> for App {
    fn init(&mut self, app: &mut Application<Cid, Message, NoUserEvent>) -> Result<()> {
        self.mount_home_view(app, &self.theme.clone())?;

        // Add global key listener and subscribe to key events
        let global = ui::widget::common::global_listener().to_boxed();
        app.mount(Cid::GlobalListener, global, subscription::global())?;

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
