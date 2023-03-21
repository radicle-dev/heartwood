use std::time::Duration;

use anyhow::Result;

use tui_realm_stdlib::Phantom;
use tuirealm::application::PollStrategy;
use tuirealm::command::{Cmd, Direction as MoveDirection};
use tuirealm::event::{Event, Key, KeyEvent, KeyModifiers};
use tuirealm::props::{AttrValue, Attribute};
use tuirealm::tui::layout::{Constraint, Direction, Layout};
use tuirealm::{
    Application, Component, Frame, MockComponent, NoUserEvent, Sub, SubClause, SubEventClause,
};

use radicle_tui::ui;
use radicle_tui::ui::components::container::{GlobalListener, LabeledContainer};
use radicle_tui::ui::components::context::Shortcuts;
use radicle_tui::ui::components::list::PropertyList;
use radicle_tui::ui::components::workspace::Workspaces;
use radicle_tui::ui::theme;
use radicle_tui::ui::widget::Widget;

use radicle_tui::Tui;

use radicle::identity::{Id, Project};

#[allow(dead_code)]
pub struct App {
    id: Id,
    project: Project,
    quit: bool,
}

/// Messages handled by this application.
#[derive(Debug, Eq, PartialEq)]
pub enum Message {
    Quit,
}

/// All components known to this application.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum ComponentId {
    Workspaces,
    Shortcuts,
    GlobalListener,
}

/// Creates a new application using a tui-realm-application, mounts all
/// components and sets focus to a default one.
impl App {
    pub fn new(id: Id, project: Project) -> Self {
        Self {
            id,
            project,
            quit: false,
        }
    }
}

impl Tui<ComponentId, Message> for App {
    fn init(&mut self, app: &mut Application<ComponentId, Message, NoUserEvent>) -> Result<()> {
        let theme = theme::default_dark();

        let dashboard = ui::labeled_container(
            &theme,
            "about",
            ui::property_list(
                &theme,
                vec![
                    ui::property(&theme, "id", &self.id.to_string()),
                    ui::property(&theme, "name", self.project.name()),
                    ui::property(&theme, "description", self.project.description()),
                ],
            )
            .to_boxed(),
        )
        .to_boxed();

        app.mount(
            ComponentId::Workspaces,
            ui::workspaces(
                &theme,
                self.project.name(),
                ui::tabs(
                    &theme,
                    vec![
                        ui::label("dashboard"),
                        ui::label("issues"),
                        ui::label("patches"),
                    ],
                ),
                vec![
                    dashboard,
                    Box::<Phantom>::default(),
                    Box::<Phantom>::default(),
                ],
            )
            .to_boxed(),
            vec![],
        )?;

        app.mount(
            ComponentId::Shortcuts,
            ui::shortcuts(
                &theme,
                vec![
                    ui::shortcut(&theme, "tab", "section"),
                    ui::shortcut(&theme, "q", "quit"),
                ],
            )
            .to_boxed(),
            vec![],
        )?;

        // Add global key listener and subscribe to key events
        app.mount(
            ComponentId::GlobalListener,
            ui::global_listener().to_boxed(),
            vec![Sub::new(
                SubEventClause::Keyboard(KeyEvent {
                    code: Key::Char('q'),
                    modifiers: KeyModifiers::NONE,
                }),
                SubClause::Always,
            )],
        )?;

        // We need to give focus to a component then
        app.active(&ComponentId::Workspaces)?;

        Ok(())
    }

    fn view(
        &mut self,
        app: &mut Application<ComponentId, Message, NoUserEvent>,
        frame: &mut Frame,
    ) {
        let area = frame.size();
        let margin_h = 1u16;
        let shortcuts_h = app
            .query(&ComponentId::Shortcuts, Attribute::Height)
            .ok()
            .flatten()
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let workspaces_h = area
            .height
            .saturating_sub(shortcuts_h.saturating_add(margin_h));

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .horizontal_margin(margin_h)
            .constraints(
                [
                    Constraint::Length(workspaces_h),
                    Constraint::Length(shortcuts_h),
                ]
                .as_ref(),
            )
            .split(area);

        app.view(&ComponentId::Workspaces, frame, layout[0]);
        app.view(&ComponentId::Shortcuts, frame, layout[1]);
    }

    fn update(&mut self, app: &mut Application<ComponentId, Message, NoUserEvent>, interval: u64) {
        if let Ok(messages) = app.tick(PollStrategy::TryFor(Duration::from_millis(interval))) {
            for message in messages {
                match message {
                    Message::Quit => self.quit = true,
                }
            }
        }
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

impl Component<Message, NoUserEvent> for Widget<Workspaces> {
    fn on(&mut self, event: Event<NoUserEvent>) -> Option<Message> {
        match event {
            Event::Keyboard(KeyEvent { code: Key::Tab, .. }) => {
                self.perform(Cmd::Move(MoveDirection::Right));
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
