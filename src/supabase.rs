mod api;
mod grid;
mod model;
mod project;
mod reports;
mod token;

pub(crate) use api::{Access, Api};
pub(crate) use grid::{Grid, unix_timestamp};
pub(crate) use model::{Condition, Project, ProjectRef, Service, Status};
pub(crate) use project::{Ban, Email, ProjectApi, SIGNED_URL_LIFETIME, StoredFile};
pub(crate) use reports::Advice;
pub(crate) use token::{TOKEN_VARIABLE, TOKENS_PAGE, forget, saved_token, store_token};

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("Cannot reach Supabase; check your network connection ({0})")]
    Network(#[from] reqwest::Error),
    #[error("Supabase rejected the access token; create a new one at {TOKENS_PAGE}")]
    Unauthorized,
    #[error("Supabase is limiting requests from this token; wait a minute, then press r")]
    RateLimited,
    #[error("{0}")]
    Rejected(String),
    #[error("{0} is {1}; its database answers once the project is running again")]
    NotRunning(String, &'static str),
    #[error("Cannot read Supabase's response ({0})")]
    Json(#[from] serde_json::Error),
    #[error("Cannot use the system keychain for your access token ({0})")]
    Keychain(#[from] keyring::Error),
    #[error("Cannot read {TOKEN_VARIABLE} ({0})")]
    Environment(#[from] std::env::VarError),
    #[error("This project has no secret API key; create one under Project Settings → API Keys")]
    NoSecretKey,
    #[error("Choose a project first")]
    NoProject,
    #[error("Cannot write the downloaded file ({0})")]
    File(#[from] std::io::Error),
    #[error("A Supabase request stopped unexpectedly ({0})")]
    Stopped(#[from] tokio::task::JoinError),
}
