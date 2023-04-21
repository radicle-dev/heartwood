use anyhow::Result;
use radicle_tui::ui::{layout, theme::Theme, widget};
use tuirealm::{Frame, NoUserEvent};

use super::{subscription, Application, Cid, Context, HomeCid, Message, PatchCid};

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
        let navigation = widget::home::navigation(theme).to_boxed();

        let dashboard = widget::home::dashboard(theme, &context.id, &context.project).to_boxed();
        let issue_browser = widget::home::issues(theme).to_boxed();
        let patch_browser =
            widget::home::patches(theme, &context.patches, &context.profile).to_boxed();

        app.remount(
            Cid::Home(HomeCid::Navigation),
            navigation,
            subscription::navigation(),
        )?;

        app.remount(Cid::Home(HomeCid::Dashboard), dashboard, vec![])?;
        app.remount(Cid::Home(HomeCid::IssueBrowser), issue_browser, vec![])?;
        app.remount(Cid::Home(HomeCid::PatchBrowser), patch_browser, vec![])?;

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
        let layout = layout::default_page(area);

        app.view(&Cid::Home(HomeCid::Navigation), frame, layout[0]);
        app.view(&self.active_component, frame, layout[1]);
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
            let navigation = widget::patch::navigation(theme).to_boxed();
            let activity =
                widget::patch::activity(theme, (*id, patch), &context.profile).to_boxed();
            let files = widget::patch::files(theme, (*id, patch), &context.profile).to_boxed();

            app.remount(
                Cid::Patch(PatchCid::Navigation),
                navigation,
                subscription::navigation(),
            )?;
            app.remount(Cid::Patch(PatchCid::Activity), activity, vec![])?;
            app.remount(Cid::Patch(PatchCid::Files), files, vec![])?;
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
        let layout = layout::default_page(area);

        app.view(&Cid::Patch(PatchCid::Navigation), frame, layout[0]);
        app.view(&self.active_component, frame, layout[1]);
    }

    fn activate(&self, app: &mut Application<Cid, Message, NoUserEvent>) -> Result<()> {
        app.active(&self.active_component)?;
        Ok(())
    }
}
