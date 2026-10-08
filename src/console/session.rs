use super::{Console, Screen};
use crate::supabase::{self, Api, Project};
use hypercmd::{Error, Event, EventPayload, Input, Key};
use std::rc::Rc;

impl Console {
    /// Captures the root on first presentation and connects with a saved token.
    pub(super) fn attach(self: &Rc<Self>, event: &Event) -> Result<(), Error> {
        if self.root.replace(Some(event.target.clone())).is_some() {
            return Ok(());
        }
        let Some(token) = self.saved_token.take() else {
            return Ok(());
        };
        let console = self.clone();
        self.start(async move {
            let api = Api::new(console.http.clone(), token);
            let projects = api.projects().await?;
            console.enter(api, projects);
            Ok(())
        })
    }

    pub(super) fn submit_token(self: &Rc<Self>, event: &Event) -> Result<(), Error> {
        if let EventPayload::Input(Input::Key {
            key: Key::Enter, ..
        }) = event.payload
        {
            event.prevent_default();
            return self.log_in();
        }
        Ok(())
    }

    /// Checks the pasted token against Supabase and saves it once it works.
    pub(super) fn log_in(self: &Rc<Self>) -> Result<(), Error> {
        let token = self
            .token_draft
            .with_untracked(|draft| draft.trim().to_owned());
        if token.is_empty() {
            self.screen.set(Screen::Failed(
                "Paste a personal access token first.".into(),
            ));
            return Ok(());
        }
        self.screen.set(Screen::Connecting);
        let console = self.clone();
        self.start(async move {
            let api = Api::new(console.http.clone(), token.clone());
            let projects = api.projects().await?;
            supabase::store_token(&token)?;
            console.token_draft.set(String::new());
            console.enter(api, projects);
            Ok(())
        })
    }

    /// Runs a login step, showing its failure on the welcome screen.
    fn start(
        &self,
        task: impl Future<Output = Result<(), supabase::Error>> + 'static,
    ) -> Result<(), Error> {
        let screen = self.screen.clone();
        self.services.spawn(&self.owner, async move {
            if let Err(error) = task.await {
                screen.set(Screen::Failed(error.to_string()));
            }
        })
    }

    fn enter(self: &Rc<Self>, api: Api, projects: Vec<Project>) {
        self.api.replace(Some(api));
        self.screen.set(Screen::Console);
        if let Err(error) = self.start_clock() {
            self.warn(format!("Status updates stopped: {error}"));
        }
        self.show_projects(projects);
        self.focus("grid");
    }

    pub(super) fn asks_for_token(&self) -> bool {
        matches!(self.screen.get(), Screen::LoggedOut | Screen::Failed(_))
    }

    pub(super) fn welcome_status(&self) -> String {
        match self.screen.get() {
            Screen::Connecting => "Connecting to Supabase…".into(),
            Screen::LoggedOut => "Paste it once; it is kept in the system keychain.".into(),
            Screen::Failed(error) => error,
            Screen::Console => String::new(),
        }
    }
}
