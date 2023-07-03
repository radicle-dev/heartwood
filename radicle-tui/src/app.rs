pub mod event;
pub mod page;
pub mod subscription;

use anyhow::Result;

use radicle::cob::issue::IssueId;
use radicle::cob::patch::PatchId;
use radicle::identity::{Id, Project};
use radicle::profile::Profile;

use tuirealm::application::PollStrategy;
use tuirealm::{Application, Frame, NoUserEvent};

use radicle_tui::ui::context::Context;
use radicle_tui::ui::theme::{self, Theme};
use radicle_tui::Tui;
use radicle_tui::{cob, ui};

use page::{HomeView, PatchView};

use self::page::{IssuePage, PageStack};

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum HomeCid {
    Header,
    Dashboard,
    IssueBrowser,
    PatchBrowser,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum PatchCid {
    Header,
    Activity,
    Files,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum IssueCid {
    Header,
    List,
    Details,
    Shortcuts,
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
    Changed(IssueId),
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
            context: Context::new(profile, id, project),
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
        let repo = self.context.repository();

        if let Some(patch) = cob::patch::find(repo, &id)? {
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
        let repo = self.context.repository();

        if let Some(issue) = cob::issue::find(repo, &id)? {
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
                        Message::Issue(IssueMessage::Leave) => {
                            self.pages.pop(app)?;
                        }
                        Message::Patch(PatchMessage::Show(id)) => {
                            self.view_patch(app, id, &theme)?;
                        }
                        Message::Patch(PatchMessage::Leave) => {
                            self.pages.pop(app)?;
                        }
                        Message::Quit => self.quit = true,
                        _ => {
                            self.pages
                                .peek_mut()?
                                .update(app, &self.context, &theme, message)?;
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
