use std::collections::HashMap;

use anyhow::Result;

use radicle::cob::patch::{Patch, PatchId};
use tuirealm::{Frame, NoUserEvent};

use radicle_tui::ui::layout;
use radicle_tui::ui::theme::Theme;
use radicle_tui::ui::widget;

use super::{subscription, Application, Cid, Context, HomeCid, HomeMessage, Message, PatchCid};

/// `tuirealm`'s event and prop system is designed to work with flat component hierarchies.
/// Building deep nested component hierarchies would need a lot more additional effort to
/// properly pass events and props down these hierarchies. This makes it hard to implement
/// full app views (home, patch details etc) as components.
///
/// View pages take into account these flat component hierarchies, and provide
/// switchable sets of components.
pub trait ViewPage {
    /// Will be called whenever a view page is pushed onto the page stack. Should create and mount all widgets.
    fn mount(
        &self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        context: &Context,
        theme: &Theme,
    ) -> Result<()>;

    /// Will be called whenever a view page is popped from the page stack. Should unmount all widgets.
    fn unmount(&self, app: &mut Application<Cid, Message, NoUserEvent>) -> Result<()>;

    /// Will be called whenever a view page is on top of the stack and can be used to update its internal
    /// state depending on the message passed.
    fn update(
        &mut self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        message: Message,
    ) -> Result<()>;

    /// Will be called whenever a view page is on top of the page stack and needs to be rendered.
    fn view(&mut self, app: &mut Application<Cid, Message, NoUserEvent>, frame: &mut Frame);

    /// Can be used to retrieve a view page's internal state in a unified form.
    fn state(&self) -> PageState;
}

///
/// Home
///
pub struct HomeView {
    active_component: Cid,
    patches: (Vec<(PatchId, Patch)>, usize),
}

impl HomeView {
    pub fn new(patches: Vec<(PatchId, Patch)>) -> Self {
        HomeView {
            active_component: Cid::Home(HomeCid::Dashboard),
            patches: (patches, 0),
        }
    }
}

impl ViewPage for HomeView {
    fn mount(
        &self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        context: &Context,
        theme: &Theme,
    ) -> Result<()> {
        let (patches, _) = &self.patches;
        let navigation = widget::home::navigation(theme).to_boxed();

        let dashboard = widget::home::dashboard(theme, &context.id, &context.project).to_boxed();
        let issue_browser = widget::home::issues(theme).to_boxed();
        let patch_browser = widget::home::patches(theme, patches, &context.profile).to_boxed();

        app.remount(
            Cid::Home(HomeCid::Navigation),
            navigation,
            subscription::navigation(),
        )?;

        app.remount(Cid::Home(HomeCid::Dashboard), dashboard, vec![])?;
        app.remount(Cid::Home(HomeCid::IssueBrowser), issue_browser, vec![])?;
        app.remount(Cid::Home(HomeCid::PatchBrowser), patch_browser, vec![])?;

        app.active(&self.active_component)?;

        Ok(())
    }

    fn unmount(&self, app: &mut Application<Cid, Message, NoUserEvent>) -> Result<()> {
        app.umount(&Cid::Home(HomeCid::Navigation))?;
        app.umount(&Cid::Home(HomeCid::Dashboard))?;
        app.umount(&Cid::Home(HomeCid::IssueBrowser))?;
        app.umount(&Cid::Home(HomeCid::PatchBrowser))?;
        Ok(())
    }

    fn update(
        &mut self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        message: Message,
    ) -> Result<()> {
        match message {
            Message::NavigationChanged(index) => {
                self.active_component = Cid::Home(HomeCid::from(index as usize));
            }
            Message::Home(HomeMessage::PatchChanged(index)) => {
                self.patches.1 = index;
            }
            _ => {}
        }
        app.active(&self.active_component)?;

        Ok(())
    }

    fn view(&mut self, app: &mut Application<Cid, Message, NoUserEvent>, frame: &mut Frame) {
        let area = frame.size();
        let layout = layout::default_page(area);

        app.view(&Cid::Home(HomeCid::Navigation), frame, layout[0]);
        app.view(&self.active_component, frame, layout[1]);
    }

    fn state(&self) -> PageState {
        let (patches, selected) = &self.patches;
        PageState::Map(
            [(
                "patches".to_string(),
                PageStateValue::Patches(patches.clone(), *selected),
            )]
            .iter()
            .cloned()
            .collect(),
        )
    }
}

///
/// Patch detail page
///
pub struct PatchView {
    active_component: Cid,
    patch: (PatchId, Patch),
}

impl PatchView {
    pub fn new(patch: (PatchId, Patch)) -> Self {
        PatchView {
            active_component: Cid::Patch(PatchCid::Activity),
            patch,
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
        let (id, patch) = &self.patch;
        let navigation = widget::patch::navigation(theme).to_boxed();
        let activity = widget::patch::activity(theme, (*id, patch), &context.profile).to_boxed();
        let files = widget::patch::files(theme, (*id, patch), &context.profile).to_boxed();

        app.remount(
            Cid::Patch(PatchCid::Navigation),
            navigation,
            subscription::navigation(),
        )?;
        app.remount(Cid::Patch(PatchCid::Activity), activity, vec![])?;
        app.remount(Cid::Patch(PatchCid::Files), files, vec![])?;

        app.active(&self.active_component)?;

        Ok(())
    }

    fn unmount(&self, app: &mut Application<Cid, Message, NoUserEvent>) -> Result<()> {
        app.umount(&Cid::Patch(PatchCid::Navigation))?;
        app.umount(&Cid::Patch(PatchCid::Activity))?;
        app.umount(&Cid::Patch(PatchCid::Files))?;
        Ok(())
    }

    fn update(
        &mut self,
        app: &mut Application<Cid, Message, NoUserEvent>,
        message: Message,
    ) -> Result<()> {
        if let Message::NavigationChanged(index) = message {
            self.active_component = Cid::Patch(PatchCid::from(index as usize));
        }
        app.active(&self.active_component)?;

        Ok(())
    }

    fn view(&mut self, app: &mut Application<Cid, Message, NoUserEvent>, frame: &mut Frame) {
        let area = frame.size();
        let layout = layout::default_page(area);

        app.view(&Cid::Patch(PatchCid::Navigation), frame, layout[0]);
        app.view(&self.active_component, frame, layout[1]);
    }

    fn state(&self) -> PageState {
        PageState::None
    }
}

/// Represents a state value that can be retrieved from a view page.
#[derive(Clone)]
pub enum PageStateValue {
    /// List of patches and its selected patch
    Patches(Vec<(PatchId, Patch)>, usize),
}

impl PageStateValue {
    pub fn unwrap_patches(self) -> Option<(Vec<(PatchId, Patch)>, usize)> {
        match self {
            PageStateValue::Patches(patches, selection) => Some((patches, selection)),
        }
    }
}

/// View pages provide a way to retrieve their state in a unified manner
/// in case that state needs to be passed to other pages.
#[derive(Clone)]
pub enum PageState {
    None,
    Map(HashMap<String, PageStateValue>),
}

impl PageState {
    pub fn unwrap_map(self) -> Option<HashMap<String, PageStateValue>> {
        match self {
            PageState::Map(map) => Some(map),
            _ => None,
        }
    }
}

/// View pages need to preserve their state (e.g. selected navigation tab, contents
/// and the selected row of a table). Therefor they should not be (re-)created
/// each time they are displayed.
/// Instead the application can push a new page onto the page stack if it needs to
/// be displayed. Its components are then created using the internal state. If a
/// new page needs to be displayed, it will also be pushed onto the stack. Leaving
/// that page again will pop it from the stack. The application can then return to
/// the previously displayed page in the state it was left.
#[derive(Default)]
pub struct PageStack {
    pages: Vec<Box<dyn ViewPage>>,
}

impl PageStack {
    pub fn push(
        &mut self,
        page: Box<dyn ViewPage>,
        app: &mut Application<Cid, Message, NoUserEvent>,
        context: &Context,
        theme: &Theme,
    ) -> Result<()> {
        page.mount(app, context, theme)?;
        self.pages.push(page);

        Ok(())
    }

    pub fn pop(&mut self, app: &mut Application<Cid, Message, NoUserEvent>) -> Result<()> {
        self.peek_mut()?.unmount(app)?;
        self.pages.pop();

        Ok(())
    }

    pub fn peek_mut(&mut self) -> Result<&mut Box<dyn ViewPage>> {
        match self.pages.last_mut() {
            Some(page) => Ok(page),
            None => Err(anyhow::anyhow!(
                "Could not peek active page. Page stack is empty."
            )),
        }
    }
}

impl From<usize> for HomeCid {
    fn from(index: usize) -> Self {
        match index {
            0 => HomeCid::Dashboard,
            1 => HomeCid::IssueBrowser,
            2 => HomeCid::PatchBrowser,
            _ => HomeCid::Dashboard,
        }
    }
}

impl From<usize> for PatchCid {
    fn from(index: usize) -> Self {
        match index {
            0 => PatchCid::Activity,
            1 => PatchCid::Files,
            _ => PatchCid::Activity,
        }
    }
}
