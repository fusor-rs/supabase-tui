use super::{Console, listing::Section};
use crate::supabase::{Advice, Condition, Project, Status};
use std::{rc::Rc, time::Duration};

const TICK: Duration = Duration::from_secs(1);
/// Projects and service health are checked again this often, in clock ticks.
const REFRESH_TICKS: usize = 30;

impl Console {
    pub(super) fn start_clock(self: &Rc<Self>) -> Result<(), hypercmd::Error> {
        self.services.spawn(&self.owner, self.clone().run_clock())
    }

    async fn run_clock(self: Rc<Self>) {
        // The clock stops when the app is disposed or out of timers.
        while let Ok(sleep) = self.services.sleep(TICK) {
            if sleep.await.is_err() {
                return;
            }
            let tick = self.clock.update(|clock| {
                *clock += 1;
                *clock
            });
            if tick % REFRESH_TICKS == 0 {
                self.load_projects();
            }
        }
    }

    /// Shows `projects`, keeping the selected one, or else selecting the first running one.
    pub(super) fn show_projects(self: &Rc<Self>, mut projects: Vec<Project>) {
        projects.sort_by(|first, second| {
            (&first.organization_slug, &first.name).cmp(&(&second.organization_slug, &second.name))
        });
        let projects: Vec<Rc<Project>> = projects.into_iter().map(Rc::new).collect();
        let selected = self
            .project
            .with_untracked(|project| project.as_ref().map(|project| project.reference.clone()));
        let chosen = projects
            .iter()
            .find(|project| Some(&project.reference) == selected.as_ref())
            .or_else(|| projects.iter().find(|project| project.status.is_running()))
            .or_else(|| projects.first())
            .cloned();
        self.projects.set(projects);
        match chosen {
            Some(project) => self.choose(project),
            None => self.inform("This account has no projects yet."),
        }
    }

    pub(super) fn open_project(self: &Rc<Self>, project: Rc<Project>) {
        self.choose(project);
        self.focus("grid");
    }

    /// Selects `project`, reloading the view if it is another project or its status changed.
    fn choose(self: &Rc<Self>, project: Rc<Project>) {
        let changed = self.project.with_untracked(|current| {
            current.as_ref().is_none_or(|current| {
                current.reference != project.reference || current.status != project.status
            })
        });
        self.project.set(Some(project));
        if changed {
            self.health.set(Vec::new());
            self.advice.set(None);
            self.project_api.replace(None);
            self.check_advice();
            self.history.borrow_mut().clear();
            self.reload();
        }
        self.check_health();
    }

    fn load_projects(self: &Rc<Self>) {
        let Some(api) = self.api() else {
            return;
        };
        let console = self.clone();
        self.spawn(async move {
            let projects = api.projects().await?;
            console.show_projects(projects);
            Ok(())
        });
    }

    fn check_health(self: &Rc<Self>) {
        let Some((api, project)) = self.target() else {
            return;
        };
        if !project.status.is_running() {
            return;
        }
        let console = self.clone();
        self.spawn(async move {
            let services = api.health(&project.reference).await?;
            let current = console.project.with_untracked(|current| {
                current
                    .as_ref()
                    .is_some_and(|current| current.reference == project.reference)
            });
            if current {
                console.health.set(services);
            }
            Ok(())
        });
    }

    /// Loads the projects, their health and advice, and the current listing again.
    pub(super) fn refresh(self: &Rc<Self>) {
        self.load_projects();
        self.check_advice();
        self.reload();
    }

    /// Counts the advisors' findings, announcing them the first time for a project.
    fn check_advice(self: &Rc<Self>) {
        let Some((api, project)) = self.target() else {
            return;
        };
        if !project.status.is_running() {
            return;
        }
        let console = self.clone();
        self.spawn(async move {
            let advice = api.advice(&project.reference).await?;
            let current = console.project.with_untracked(|current| {
                current
                    .as_ref()
                    .is_some_and(|current| current.reference == project.reference)
            });
            if !current {
                return Ok(());
            }
            let first = console.advice.get_untracked().is_none();
            console.advice.set(Some(advice));
            if first && advice.errors + advice.warnings > 0 {
                console.warn(format!(
                    "{} has {}; press {} to review.",
                    project.name,
                    findings(advice),
                    Section::Advisors.shortcut()
                ));
            }
            Ok(())
        });
    }

    pub(super) fn restore(self: &Rc<Self>) {
        let Some((api, project)) = self.target() else {
            return;
        };
        if project.status != Status::Inactive {
            self.inform("Only paused projects can be restored.");
            return;
        }
        let console = self.clone();
        self.spawn(async move {
            api.restore(&project.reference).await?;
            console.inform(format!(
                "Restoring {}. It takes a few minutes; the status updates by itself.",
                project.name
            ));
            console.load_projects();
            Ok(())
        });
    }
}

impl Console {
    pub(super) fn is_project(&self, project: &Project) -> bool {
        self.project.with(|current| {
            current
                .as_ref()
                .is_some_and(|current| current.reference == project.reference)
        })
    }

    pub(super) fn project_name(&self) -> String {
        self.project.with(|project| match project {
            Some(project) => format!("{} / {}", project.organization_slug, project.name),
            None => "No project".into(),
        })
    }

    pub(super) fn project_details(&self) -> String {
        self.project.with(|project| {
            project.as_ref().map_or_else(String::new, |project| {
                format!(
                    "ref {} · {} · Postgres {}",
                    project.reference, project.region, project.database.version
                )
            })
        })
    }

    pub(super) fn project_status(&self) -> String {
        self.project.with(|project| {
            project.as_ref().map_or_else(String::new, |project| {
                format!("{} {}", marker(project.status), project.status.label())
            })
        })
    }

    pub(super) fn project_condition(&self) -> Option<Condition> {
        self.project
            .with(|project| project.as_ref().map(|project| project.status.condition()))
    }

    pub(super) fn restorable(&self) -> bool {
        self.project.with(|project| {
            project
                .as_ref()
                .is_some_and(|project| project.status == Status::Inactive)
        })
    }

    pub(super) fn advice_text(&self) -> String {
        match self.advice.get() {
            None => String::new(),
            Some(Advice {
                errors: 0,
                warnings: 0,
            }) => "✓ no advisor findings".into(),
            Some(advice) => format!(
                "⚠ {} · press {}",
                findings(advice),
                Section::Advisors.shortcut()
            ),
        }
    }

    pub(super) fn advice_has_errors(&self) -> bool {
        self.advice.get().is_some_and(|advice| advice.errors > 0)
    }

    pub(super) fn advice_has_warnings(&self) -> bool {
        self.advice
            .get()
            .is_some_and(|advice| advice.errors == 0 && advice.warnings > 0)
    }

    /// The number of advisor findings to show beside `section`; zero for other sections.
    fn findings_on(&self, section: Section) -> usize {
        match self.advice.get() {
            Some(advice) if section == Section::Advisors => advice.errors + advice.warnings,
            _ => 0,
        }
    }

    pub(super) fn section_flagged(&self, section: Section) -> bool {
        self.findings_on(section) > 0
    }

    pub(super) fn section_label(&self, section: Section) -> String {
        match self.findings_on(section) {
            0 => section.label(),
            count => format!("{}  ⚠ {count}", section.label()),
        }
    }

    pub(super) fn health_note(&self) -> &'static str {
        let running = self.project.with(|project| {
            project
                .as_ref()
                .is_some_and(|project| project.status.is_running())
        });
        match (running, self.health.with(Vec::is_empty)) {
            (false, _) => "not running",
            (true, true) => "checking…",
            (true, false) => "",
        }
    }
}

/// Such as `2 errors and 1 warning`, leaving out a count of zero.
pub(super) fn findings(advice: Advice) -> String {
    let count = |number: usize, noun: &str| match number {
        0 => None,
        1 => Some(format!("1 {noun}")),
        _ => Some(format!("{number} {noun}s")),
    };
    [
        count(advice.errors, "error"),
        count(advice.warnings, "warning"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" and ")
}

/// A project's sidebar entry: its status marker and name.
pub(super) fn project_entry(project: &Project) -> String {
    format!("{} {}", marker(project.status), project.name)
}

fn marker(status: Status) -> &'static str {
    match status.condition() {
        Condition::Up => "●",
        Condition::Changing => "◐",
        Condition::Paused => "○",
        Condition::Down => "✕",
    }
}

#[cfg(test)]
mod tests {
    use super::{Advice, findings};

    #[test]
    fn findings_name_only_what_was_found() {
        let advice = |errors, warnings| Advice { errors, warnings };
        assert_eq!(findings(advice(2, 1)), "2 errors and 1 warning");
        assert_eq!(findings(advice(1, 0)), "1 error");
        assert_eq!(findings(advice(0, 5)), "5 warnings");
    }
}
