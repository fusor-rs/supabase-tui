use super::{Error, Grid, Project, ProjectRef, Service};
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};

const BASE_URL: &str = "https://api.supabase.com/v1";
const HEALTH_SERVICES: &str = "db,auth,rest,storage,realtime,pooler";
const TABLES: &str = include_str!("sql/tables.sql");
const USERS: &str = include_str!("sql/users.sql");
const USER: &str = include_str!("sql/user.sql");
const FOLDER: &str = include_str!("sql/folder.sql");
const FILES: &str = include_str!("sql/files.sql");
const PREVIEW_ROWS: usize = 100;

#[derive(Clone)]
pub(crate) struct Api {
    http: reqwest::Client,
    token: String,
}

/// Which database role runs a query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Access {
    ReadOnly,
    ReadWrite,
}

/// The service that answered, which decides what an unauthorized answer means.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Origin {
    Management,
    Project,
}

/// Management and Storage call the explanation `message`, Auth `msg` or `error_description`.
#[derive(Deserialize)]
struct Rejection {
    #[serde(alias = "msg", alias = "error_description")]
    message: String,
}

impl Api {
    pub(crate) fn new(http: reqwest::Client, token: String) -> Self {
        Self { http, token }
    }

    pub(crate) async fn projects(&self) -> Result<Vec<Project>, Error> {
        self.fetch("/projects").await
    }

    pub(crate) async fn health(&self, project: &ProjectRef) -> Result<Vec<Service>, Error> {
        self.fetch(&format!(
            "/projects/{project}/health?services={HEALTH_SERVICES}"
        ))
        .await
    }

    /// Asks Supabase to bring a paused project back up; it takes a few minutes.
    pub(crate) async fn restore(&self, project: &ProjectRef) -> Result<(), Error> {
        let request = self.request(Method::POST, &format!("/projects/{project}/restore"));
        send(request, Origin::Management).await?;
        Ok(())
    }

    /// Runs `sql` with `parameters` bound to `$1`, `$2`…
    pub(crate) async fn query(
        &self,
        project: &ProjectRef,
        sql: &str,
        parameters: &[&str],
        access: Access,
    ) -> Result<Grid, Error> {
        let endpoint = match access {
            Access::ReadOnly => "query/read-only",
            Access::ReadWrite => "query",
        };
        let path = format!("/projects/{project}/database/{endpoint}");
        let request = self
            .request(Method::POST, &path)
            .json(&json!({ "query": sql, "parameters": parameters }));
        let body = send(request, Origin::Management).await?;
        let records: Vec<Map<String, Value>> = serde_json::from_str(&body)?;
        Ok(Grid::from_records(&records))
    }

    pub(crate) async fn tables(&self, project: &ProjectRef) -> Result<Grid, Error> {
        self.query(project, TABLES, &[], Access::ReadOnly).await
    }

    /// The newest users, or those whose email, phone or ID contains `search`.
    pub(crate) async fn users(&self, project: &ProjectRef, search: &str) -> Result<Grid, Error> {
        self.query(project, USERS, &[search], Access::ReadOnly)
            .await
    }

    /// One user's account, providers, sessions and metadata, as a single row.
    pub(crate) async fn user(&self, project: &ProjectRef, id: &str) -> Result<Grid, Error> {
        self.query(project, USER, &[id], Access::ReadOnly).await
    }

    /// The files and folders directly under `prefix`, which is empty or ends in `/`.
    pub(crate) async fn folder(
        &self,
        project: &ProjectRef,
        bucket: &str,
        prefix: &str,
    ) -> Result<Grid, Error> {
        self.query(project, FOLDER, &[bucket, prefix], Access::ReadOnly)
            .await
    }

    /// Files whose path contains `search`, in `bucket` or, when it is empty, in every bucket.
    pub(crate) async fn files(
        &self,
        project: &ProjectRef,
        bucket: &str,
        search: &str,
    ) -> Result<Grid, Error> {
        self.query(project, FILES, &[bucket, search], Access::ReadOnly)
            .await
    }

    pub(crate) async fn table_rows(
        &self,
        project: &ProjectRef,
        schema: &str,
        table: &str,
    ) -> Result<Grid, Error> {
        let sql = format!(
            "select * from {}.{} limit {PREVIEW_ROWS}",
            quote_identifier(schema),
            quote_identifier(table)
        );
        self.query(project, &sql, &[], Access::ReadOnly).await
    }

    pub(super) async fn fetch<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        let body = send(self.request(Method::GET, path), Origin::Management).await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub(super) fn http(&self) -> reqwest::Client {
        self.http.clone()
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, format!("{BASE_URL}{path}"))
            .bearer_auth(&self.token)
    }
}

/// reqwest needs the Tokio runtime; requests run there, not on the UI thread.
pub(super) async fn send(request: RequestBuilder, origin: Origin) -> Result<String, Error> {
    tokio::spawn(async move { Ok(accept(request.send().await?, origin).await?.text().await?) })
        .await?
}

/// Passes a successful response through and turns any other into an error.
pub(super) async fn accept(response: Response, origin: Origin) -> Result<Response, Error> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await?;
    Err(rejection(status, &body, origin))
}

fn rejection(status: StatusCode, body: &str, origin: Origin) -> Error {
    match (status, origin) {
        (StatusCode::UNAUTHORIZED, Origin::Management) => Error::Unauthorized,
        (StatusCode::TOO_MANY_REQUESTS, Origin::Management) => Error::RateLimited,
        _ => match serde_json::from_str::<Rejection>(body) {
            Ok(rejection) => Error::Rejected(rejection.message),
            // Gateways answer some failures with plain text, an HTML page or an empty body.
            Err(_) if body.trim().is_empty() || body.trim_start().starts_with('<') => {
                Error::Rejected(format!("Supabase answered {status}"))
            }
            Err(_) => Error::Rejected(body.trim().to_owned()),
        },
    }
}

fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::{Error, Origin, StatusCode, quote_identifier, rejection};

    #[test]
    fn identifiers_are_quoted_for_postgres() {
        assert_eq!(quote_identifier("orders"), "\"orders\"");
        assert_eq!(
            quote_identifier("My \"odd\" table"),
            "\"My \"\"odd\"\" table\""
        );
    }

    #[test]
    fn rejections_show_supabase_messages() {
        let message =
            r#"{"message":"Failed to run sql query: ERROR: relation \"x\" does not exist"}"#;
        let auth = r#"{"code":429,"error_code":"over_email_send_rate_limit",
                       "msg":"email rate limit exceeded"}"#;
        let shown = [
            rejection(StatusCode::BAD_REQUEST, message, Origin::Management),
            rejection(
                StatusCode::BAD_GATEWAY,
                " upstream timeout\n",
                Origin::Management,
            ),
            rejection(StatusCode::INTERNAL_SERVER_ERROR, "", Origin::Management),
            rejection(
                StatusCode::BAD_GATEWAY,
                "<!DOCTYPE HTML>\n<html>",
                Origin::Project,
            ),
            rejection(StatusCode::TOO_MANY_REQUESTS, auth, Origin::Project),
            rejection(StatusCode::UNAUTHORIZED, message, Origin::Project),
        ]
        .map(|error| error.to_string());
        assert_eq!(
            shown,
            [
                "Failed to run sql query: ERROR: relation \"x\" does not exist",
                "upstream timeout",
                "Supabase answered 500 Internal Server Error",
                "Supabase answered 502 Bad Gateway",
                "email rate limit exceeded",
                "Failed to run sql query: ERROR: relation \"x\" does not exist",
            ]
        );
        assert!(matches!(
            rejection(StatusCode::UNAUTHORIZED, message, Origin::Management),
            Error::Unauthorized
        ));
    }
}
