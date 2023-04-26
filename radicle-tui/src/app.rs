pub mod event;
pub mod page;
pub mod subscription;

use anyhow::Result;

use radicle::identity::{Id, Project};
use radicle::profile::Profile;

use tuirealm::application::PollStrategy;
use tuirealm::{Application, Frame, NoUserEvent};

use radicle_tui::cob::patch::{self};

use radicle_tui::ui;
use radicle_tui::ui::theme::{self, Theme};
use radicle_tui::Tui;

use page::{HomeView, PatchView};

use self::page::PageStack;

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
    PatchChanged(usize),
}

#[derive(Debug, Eq, PartialEq)]
pub enum PatchMessage {
    Show,
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
}

#[allow(dead_code)]
pub struct App {
    context: Context,
    pages: PageStack,
    theme: Theme,
    quit: bool,
}

/// Creates a new application using a tui-realm-application, mounts all
/// components and sets focus to a default one.
impl App {
    pub fn new(profile: Profile, id: Id, project: Project) -> Self {
        Self {
            context: Context {
                id,
                profile,
                project,
            },
            pages: PageStack::default(),
            theme: theme::default_dark(),
            quit: false,
        }
    }

    fn view_home(
        &mut self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        theme: &Theme,
    ) -> Result<()> {
        let patches = patch::load_all(&self.context.profile, self.context.id);
        let home = Box::new(HomeView::new(patches));
        self.pages.push(home, app, &self.context, theme)?;

        Ok(())
    }

    fn view_patch(
        &mut self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        theme: &Theme,
    ) -> Result<()> {
        let page = self.pages.peek_mut()?;
        let state = page.state().unwrap_map();
        let patches = state
            .and_then(|mut values| values.remove("patches"))
            .and_then(|value| value.unwrap_patches());

        match patches {
            Some((patches, selection)) => match patches.get(selection) {
                Some((id, patch)) => {
                    let view = Box::new(PatchView::new((*id, patch.clone())));
                    self.pages.push(view, app, &self.context, theme)?;

                    Ok(())
                }
                None => Err(anyhow::anyhow!(
                    "Could not mount 'page::PatchView'. Patch not found."
                )),
            },
            None => Err(anyhow::anyhow!(
                "Could not mount 'page::PatchView'. No state value for 'patches' found."
            )),
        }
    }
}

impl Tui<Cid, Message> for App {
    fn init(&mut self, app: &mut Application<Cid, Message, NoUserEvent>) -> Result<()> {
        self.view_home(app, &self.theme.clone())?;

        // Add global key listener and subscribe to key events
        let global = ui::widget::common::global_listener().to_boxed();
        app.mount(Cid::GlobalListener, global, subscription::global())?;

        Ok(())
    }

    fn view(&mut self, app: &mut Application<Cid, Message, NoUserEvent>, frame: &mut Frame) {
        if let Ok(page) = self.pages.peek_mut() {
            page.view(app, frame);
        }
    }

    fn update(&mut self, app: &mut Application<Cid, Message, NoUserEvent>) -> Result<bool> {
        match app.tick(PollStrategy::Once) {
            Ok(messages) if !messages.is_empty() => {
                let theme = theme::default_dark();
                for message in messages {
                    match message {
                        Message::Patch(PatchMessage::Show) => {
                            self.view_patch(app, &theme)?;
                        }
                        Message::Patch(PatchMessage::Leave) => {
                            self.pages.pop(app)?;
                        }
                        Message::Quit => self.quit = true,
                        _ => {
                            self.pages.peek_mut()?.update(app, message)?;
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
