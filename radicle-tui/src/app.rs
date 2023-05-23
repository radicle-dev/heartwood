pub mod event;
pub mod page;
pub mod subscription;

use anyhow::Result;

use radicle::cob::issue::{IssueId, Issues};
use radicle::cob::patch::{PatchId, Patches};
use radicle::identity::{Id, Project};
use radicle::profile::Profile;
use radicle::storage::ReadStorage;

use tuirealm::application::PollStrategy;
use tuirealm::{Application, Frame, NoUserEvent};

use radicle_tui::ui;
use radicle_tui::ui::theme::{self, Theme};
use radicle_tui::Tui;

use page::{HomeView, PatchView};

use self::page::{IssuePage, PageStack};

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

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum IssueCid {
    List,
}

/// All component ids known to this application.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Cid {
    Home(HomeCid),
    Issue(IssueCid),
    Patch(PatchCid),
    GlobalListener,
}

/// Messages handled by this application.
#[derive(Debug, Eq, PartialEq)]
pub enum HomeMessage {}

#[derive(Debug, Eq, PartialEq)]
pub enum IssueMessage {
    Show(IssueId),
    Leave,
}

#[derive(Debug, Eq, PartialEq)]
pub enum PatchMessage {
    Show(PatchId),
    Leave,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Message {
    Home(HomeMessage),
    Issue(IssueMessage),
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
        let home = Box::<HomeView>::default();
        self.pages.push(home, app, &self.context, theme)?;

        Ok(())
    }

    fn view_patch(
        &mut self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        id: PatchId,
        theme: &Theme,
    ) -> Result<()> {
        let repo = self
            .context
            .profile
            .storage
            .repository(self.context.id)
            .unwrap();
        let patches = Patches::open(&repo).unwrap();

        if let Some(patch) = patches.get(&id)? {
            let view = Box::new(PatchView::new((id, patch)));
            self.pages.push(view, app, &self.context, theme)?;

            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Could not mount 'page::PatchView'. Patch not found."
            ))
        }
    }

    fn view_issue(
        &mut self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        id: IssueId,
        theme: &Theme,
    ) -> Result<()> {
        let repo = self
            .context
            .profile
            .storage
            .repository(self.context.id)
            .unwrap();
        let issues = Issues::open(&repo).unwrap();

        if let Some(issue) = issues.get(&id)? {
            let view = Box::new(IssuePage::new((id, issue)));
            self.pages.push(view, app, &self.context, theme)?;

            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Could not mount 'page::IssueView'. Issue not found."
            ))
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
                        Message::Issue(IssueMessage::Show(id)) => {
                            self.view_issue(app, id, &theme)?;
                        }
                        Message::Patch(PatchMessage::Show(id)) => {
                            self.view_patch(app, id, &theme)?;
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
