mod advisors;
mod browse;
mod desktop;
mod editor;
mod keys;
mod listing;
mod overlay;
mod projects;
mod search;
mod session;
mod storage;
mod users;
mod view;

use crate::supabase::{self, Advice, Api, Project, ProjectApi, Service};
use fusor::{FromInputs, OwnerHandle, Signal, signal};
use hypercmd::{Error, Node, Services};
use listing::{Listing, Section, Source, Window};
use overlay::Overlay;
use search::Search;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// How long a notice stays on screen, in clock ticks.
const NOTICE_TICKS: usize = 8;

pub(crate) struct Startup {
    pub(crate) http: reqwest::Client,
    pub(crate) token: Option<String>,
}

#[derive(Clone, PartialEq)]
pub(crate) enum Screen {
    Connecting,
    LoggedOut,
    Failed(String),
    Console,
}

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Info,
    Error,
}

#[derive(Clone, PartialEq)]
struct Notice {
    text: String,
    tone: Tone,
    shown: usize,
}

pub(crate) struct Console {
    owner: OwnerHandle,
    services: Services,
    http: reqwest::Client,
    api: RefCell<Option<Api>>,
    project_api: RefCell<Option<ProjectApi>>,
    screen: Signal<Screen>,
    token_draft: Signal<String>,
    projects: Signal<Vec<Rc<Project>>>,
    project: Signal<Option<Rc<Project>>>,
    health: Signal<Vec<Service>>,
    advice: Signal<Option<Advice>>,
    listing: Signal<Listing>,
    window: Signal<Window>,
    history: RefCell<Vec<(Listing, Window)>>,
    generation: Cell<u64>,
    search: Signal<Search>,
    search_text: Signal<String>,
    invite_draft: Signal<String>,
    sql: Signal<String>,
    read_only: Signal<bool>,
    overlay: Signal<Overlay>,
    clock: Signal<usize>,
    notice: Signal<Option<Notice>>,
    root: RefCell<Option<Node>>,
    saved_token: RefCell<Option<String>>,
}

impl FromInputs for Console {
    type Inputs = Startup;
    type Error = Error;

    fn from_inputs(startup: Startup, owner: OwnerHandle) -> Result<Self, Error> {
        let screen = match startup.token {
            Some(_) => Screen::Connecting,
            None => Screen::LoggedOut,
        };
        Ok(Self {
            services: Services::from_owner(&owner)?,
            owner,
            http: startup.http,
            api: RefCell::new(None),
            project_api: RefCell::new(None),
            screen: signal(screen),
            token_draft: signal(String::new()),
            projects: signal(Vec::new()),
            project: signal(None),
            health: signal(Vec::new()),
            advice: signal(None),
            listing: signal(Listing::new(Source::Section(Section::Tables))),
            window: signal(Window::default()),
            history: RefCell::new(Vec::new()),
            generation: Cell::new(0),
            search: signal(Search::Closed),
            search_text: signal(String::new()),
            invite_draft: signal(String::new()),
            sql: signal(String::new()),
            read_only: signal(true),
            overlay: signal(Overlay::None),
            clock: signal(0),
            notice: signal(None),
            root: RefCell::new(None),
            saved_token: RefCell::new(startup.token),
        })
    }
}

impl Console {
    fn api(&self) -> Option<Api> {
        self.api.borrow().clone()
    }

    /// Runs Supabase work in the background and reports its failure as a notice.
    fn spawn(&self, task: impl Future<Output = Result<(), supabase::Error>> + 'static) {
        let notice = self.notice.clone();
        let clock = self.clock.clone();
        let started = self.services.spawn(&self.owner, async move {
            if let Err(error) = task.await {
                notify(&notice, &clock, error.to_string(), Tone::Error);
            }
        });
        if let Err(error) = started {
            self.warn(error.to_string());
        }
    }

    /// The selected project's Storage and Auth APIs, fetching its key the first time.
    async fn project_api(&self) -> Result<ProjectApi, supabase::Error> {
        let (api, project) = self.target().ok_or(supabase::Error::NoProject)?;
        let cached = self.project_api.borrow().clone();
        if let Some(cached) = cached
            && cached.reference == project.reference
        {
            return Ok(cached);
        }
        let fetched = api.project_api(&project.reference).await?;
        self.project_api.replace(Some(fetched.clone()));
        Ok(fetched)
    }

    fn inform(&self, text: impl Into<String>) {
        notify(&self.notice, &self.clock, text.into(), Tone::Info);
    }

    fn warn(&self, text: String) {
        notify(&self.notice, &self.clock, text, Tone::Error);
    }

    fn notice_text(&self) -> String {
        let now = self.clock.get();
        self.notice.with(|notice| {
            notice
                .as_ref()
                .filter(|notice| now.saturating_sub(notice.shown) < NOTICE_TICKS)
                .map_or_else(String::new, |notice| notice.text.clone())
        })
    }

    fn notice_is_error(&self) -> bool {
        self.notice.with(|notice| {
            notice
                .as_ref()
                .is_some_and(|notice| notice.tone == Tone::Error)
        })
    }

    fn focus(&self, id: &str) {
        if let Some(node) = self.root.borrow().as_ref().and_then(|root| root.find(id)) {
            node.request_focus();
        }
    }
}

fn notify(notice: &Signal<Option<Notice>>, clock: &Signal<usize>, text: String, tone: Tone) {
    notice.set(Some(Notice {
        text,
        tone,
        shown: clock.get_untracked(),
    }));
}
