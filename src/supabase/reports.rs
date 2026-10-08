use super::{
    Api, Error, Grid, ProjectRef,
    grid::{timestamp, unix_timestamp},
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Function {
    name: String,
    slug: String,
    status: String,
    version: u64,
    updated_at: u64,
    verify_jwt: Option<bool>,
}

#[derive(Deserialize)]
struct Bucket {
    name: String,
    public: bool,
    created_at: String,
    updated_at: String,
}

#[derive(Deserialize)]
struct Lints {
    lints: Vec<Lint>,
}

/// How many advisor findings need attention.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Advice {
    pub(crate) errors: usize,
    pub(crate) warnings: usize,
}

#[derive(Deserialize)]
struct Lint {
    level: Level,
    title: String,
    detail: String,
    remediation: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "UPPERCASE")]
enum Level {
    Error,
    Warn,
    Info,
}

#[derive(Deserialize)]
struct Backups {
    backups: Vec<Backup>,
}

#[derive(Deserialize)]
struct Backup {
    status: String,
    is_physical_backup: bool,
    inserted_at: String,
}

#[derive(Deserialize)]
struct Migration {
    version: String,
    name: Option<String>,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
        }
    }
}

/// The advisor kinds the dashboard shows, by the path segment that lists them.
const ADVISORS: [(&str, &str); 2] = [("security", "Security"), ("performance", "Performance")];

impl Api {
    pub(crate) async fn functions(&self, project: &ProjectRef) -> Result<Grid, Error> {
        let functions: Vec<Function> = self
            .fetch(&format!("/projects/{project}/functions"))
            .await?;
        let rows = functions
            .into_iter()
            .map(|function| {
                let jwt = match function.verify_jwt {
                    Some(true) => "required",
                    Some(false) => "not required",
                    None => "",
                };
                vec![
                    function.name,
                    function.slug,
                    function.status,
                    function.version.to_string(),
                    jwt.into(),
                    unix_timestamp(function.updated_at),
                ]
            })
            .collect();
        let columns = ["Name", "Slug", "Status", "Version", "JWT", "Updated (UTC)"];
        Ok(Grid::new(&columns, rows))
    }

    pub(crate) async fn buckets(&self, project: &ProjectRef) -> Result<Grid, Error> {
        let buckets: Vec<Bucket> = self
            .fetch(&format!("/projects/{project}/storage/buckets"))
            .await?;
        let rows = buckets
            .into_iter()
            .map(|bucket| {
                let access = if bucket.public { "public" } else { "private" };
                vec![
                    bucket.name,
                    access.into(),
                    timestamp(&bucket.created_at),
                    timestamp(&bucket.updated_at),
                ]
            })
            .collect();
        let columns = ["Bucket", "Access", "Created (UTC)", "Updated (UTC)"];
        Ok(Grid::new(&columns, rows))
    }

    /// Security and performance advice, most severe first.
    pub(crate) async fn advisors(&self, project: &ProjectRef) -> Result<Grid, Error> {
        let rows = self
            .lints(project)
            .await?
            .into_iter()
            .map(|(lint, kind)| {
                vec![
                    lint.level.label().into(),
                    kind.into(),
                    lint.title,
                    lint.detail,
                    lint.remediation,
                ]
            })
            .collect();
        let columns = ["Level", "Kind", "Issue", "Detail", "Fix"];
        Ok(Grid::new(&columns, rows))
    }

    pub(crate) async fn advice(&self, project: &ProjectRef) -> Result<Advice, Error> {
        let lints = self.lints(project).await?;
        let count = |level| lints.iter().filter(|(lint, _)| lint.level == level).count();
        Ok(Advice {
            errors: count(Level::Error),
            warnings: count(Level::Warn),
        })
    }

    async fn lints(&self, project: &ProjectRef) -> Result<Vec<(Lint, &'static str)>, Error> {
        let mut lints = Vec::new();
        for (path, kind) in ADVISORS {
            let found: Lints = self
                .fetch(&format!("/projects/{project}/advisors/{path}"))
                .await?;
            lints.extend(found.lints.into_iter().map(|lint| (lint, kind)));
        }
        lints.sort_by_key(|(lint, _)| lint.level);
        Ok(lints)
    }

    pub(crate) async fn backups(&self, project: &ProjectRef) -> Result<Grid, Error> {
        let listing: Backups = self
            .fetch(&format!("/projects/{project}/database/backups"))
            .await?;
        let rows = listing
            .backups
            .into_iter()
            .map(|backup| {
                let kind = if backup.is_physical_backup {
                    "physical"
                } else {
                    "logical"
                };
                vec![timestamp(&backup.inserted_at), backup.status, kind.into()]
            })
            .collect();
        Ok(Grid::new(&["Taken (UTC)", "Status", "Kind"], rows))
    }

    pub(crate) async fn migrations(&self, project: &ProjectRef) -> Result<Grid, Error> {
        let mut migrations: Vec<Migration> = self
            .fetch(&format!("/projects/{project}/database/migrations"))
            .await?;
        migrations.sort_by(|first, second| second.version.cmp(&first.version));
        let rows = migrations
            .into_iter()
            // Migrations applied without a name have none to show.
            .map(|migration| vec![migration.version, migration.name.unwrap_or_default()])
            .collect();
        Ok(Grid::new(&["Version", "Name"], rows))
    }
}
