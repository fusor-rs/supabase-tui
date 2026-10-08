use serde::Deserialize;
use std::fmt;

/// A project's reference ID, the subdomain of its API URL.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(transparent)]
pub(crate) struct ProjectRef(String);

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Project {
    #[serde(rename = "ref")]
    pub(crate) reference: ProjectRef,
    pub(crate) name: String,
    pub(crate) organization_slug: String,
    pub(crate) region: String,
    pub(crate) status: Status,
    pub(crate) database: Database,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Database {
    pub(crate) version: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Status {
    ActiveHealthy,
    ActiveUnhealthy,
    Inactive,
    ComingUp,
    GoingDown,
    Pausing,
    Restarting,
    Restoring,
    Resizing,
    Upgrading,
    InitFailed,
    PauseFailed,
    RestoreFailed,
    Removed,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Condition {
    Up,
    Changing,
    Paused,
    Down,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Service {
    pub(crate) name: String,
    pub(crate) healthy: bool,
}

impl fmt::Display for ProjectRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Status {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ActiveHealthy => "healthy",
            Self::ActiveUnhealthy => "unhealthy",
            Self::Inactive => "paused",
            Self::ComingUp => "coming up",
            Self::GoingDown => "going down",
            Self::Pausing => "pausing",
            Self::Restarting => "restarting",
            Self::Restoring => "restoring",
            Self::Resizing => "resizing",
            Self::Upgrading => "upgrading",
            Self::InitFailed => "setup failed",
            Self::PauseFailed => "pause failed",
            Self::RestoreFailed => "restore failed",
            Self::Removed => "removed",
            Self::Unknown => "unknown",
        }
    }

    pub(crate) fn condition(self) -> Condition {
        match self {
            Self::ActiveHealthy => Condition::Up,
            Self::Inactive | Self::Removed => Condition::Paused,
            Self::ComingUp
            | Self::GoingDown
            | Self::Pausing
            | Self::Restarting
            | Self::Restoring
            | Self::Resizing
            | Self::Upgrading => Condition::Changing,
            Self::ActiveUnhealthy
            | Self::InitFailed
            | Self::PauseFailed
            | Self::RestoreFailed
            | Self::Unknown => Condition::Down,
        }
    }

    /// Whether the database answers queries and health checks.
    pub(crate) fn is_running(self) -> bool {
        matches!(self, Self::ActiveHealthy | Self::ActiveUnhealthy)
    }
}

#[cfg(test)]
mod tests {
    use super::{Project, Status};

    #[test]
    fn projects_read_from_the_management_api() {
        let projects: Vec<Project> = serde_json::from_str(
            r#"[{
              "id": "abcdefghijklmnopqrst", "ref": "abcdefghijklmnopqrst",
              "organization_id": "org", "organization_slug": "acme", "name": "Shop",
              "region": "us-east-1", "created_at": "2024-01-01T00:00:00Z",
              "status": "INACTIVE",
              "database": {"host": "db.abcdefghijklmnopqrst.supabase.co", "version": "15.6.1.143",
                           "postgres_engine": "15", "release_channel": "ga"}
            }, {
              "id": "b", "ref": "b", "organization_id": "org", "organization_slug": "acme",
              "name": "Next", "region": "eu-west-2", "created_at": "2024-01-01T00:00:00Z",
              "status": "A_NEW_STATUS",
              "database": {"host": "h", "version": "17.4.1", "postgres_engine": "17",
                           "release_channel": "ga"}
            }]"#,
        )
        .unwrap();
        let read: Vec<_> = projects
            .iter()
            .map(|project| {
                (
                    project.reference.to_string(),
                    project.status,
                    project.database.version.as_str(),
                )
            })
            .collect();
        assert_eq!(
            read,
            [
                (
                    "abcdefghijklmnopqrst".into(),
                    Status::Inactive,
                    "15.6.1.143"
                ),
                ("b".into(), Status::Unknown, "17.4.1"),
            ]
        );
    }
}
