use super::{
    Console,
    listing::{Cell, Listing, Loading, Row, Section, Source, View},
    overlay::Record,
};
use crate::supabase::{self, Api, Grid, Project};
use hypercmd::{Event, EventPayload, Input, Key};
use std::{rc::Rc, time::Instant};

const HEADER_ROWS: usize = 1;
/// The narrowest a grid column gets before columns scroll sideways.
const COLUMN_CELLS: usize = 14;

impl Console {
    /// Opens a section, forgetting where the user drilled down from.
    pub(super) fn open_section(self: &Rc<Self>, section: Section) {
        self.history.borrow_mut().clear();
        self.close_search();
        self.show(Source::Section(section));
        if section == Section::Sql {
            self.focus("sql");
        } else {
            self.focus("grid");
        }
    }

    pub(super) fn drill(self: &Rc<Self>, source: Source) {
        let current = (self.listing.get_untracked(), self.window.get_untracked());
        self.history.borrow_mut().push(current);
        self.show(source);
    }

    pub(super) fn back(self: &Rc<Self>) -> bool {
        let Some((listing, window)) = self.history.borrow_mut().pop() else {
            return false;
        };
        self.close_search();
        let interrupted = listing.loading == Loading::Busy;
        let source = listing.source.clone();
        self.next_generation();
        self.listing.set(listing);
        self.window.set(window);
        if interrupted {
            self.show(source);
        }
        true
    }

    /// Loads the current listing again, from its section if it was drilled into.
    pub(super) fn reload(self: &Rc<Self>) {
        let source = self
            .listing
            .with_untracked(|listing| listing.source.clone());
        if source.section() == Section::Sql {
            return;
        }
        self.show(source);
    }

    fn show(self: &Rc<Self>, source: Source) {
        let Some((api, project)) = self.target() else {
            return;
        };
        if source.section() == Section::Sql {
            self.next_generation();
            self.listing.set(Listing::new(source));
            self.window.update(|window| window.rewind());
            return;
        }
        let task = fetch(api, project, source.clone());
        self.load(source, task);
    }

    pub(super) fn target(&self) -> Option<(Api, Rc<Project>)> {
        Some((self.api()?, self.project.get_untracked()?))
    }

    fn next_generation(&self) -> u64 {
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        generation
    }

    /// Shows `source` with the grid `task` loads, unless the user moved on meanwhile.
    pub(super) fn load(
        self: &Rc<Self>,
        source: Source,
        task: impl Future<Output = Result<Grid, supabase::Error>> + 'static,
    ) {
        let generation = self.next_generation();
        let reloading = self
            .listing
            .with_untracked(|listing| listing.source == source);
        let mut listing = Listing::new(source);
        listing.loading = Loading::Busy;
        self.listing.set(listing);
        if !reloading {
            self.window.update(|window| window.rewind());
        }
        let console = self.clone();
        let started = Instant::now();
        let spawned = self.services.spawn(&self.owner, async move {
            let result = task.await;
            if console.generation.get() != generation {
                return;
            }
            console.listing.update(|listing| match result {
                Ok(grid) => {
                    listing.grid = Rc::new(grid);
                    listing.loading = Loading::Done(started.elapsed());
                }
                Err(error) => listing.loading = Loading::Failed(error.to_string()),
            });
            let shown = console.matches_untracked().len();
            console.window.update(|window| window.keep_within(shown));
        });
        if let Err(error) = spawned {
            self.warn(error.to_string());
        }
    }

    pub(super) fn navigate(self: &Rc<Self>, event: &Event) {
        match event.payload {
            EventPayload::Input(Input::Key {
                key: Key::Enter, ..
            }) => self.open_selected(),
            EventPayload::Input(Input::Key { key, modifiers, .. })
                if !modifiers.control && !modifiers.alt && !modifiers.super_key =>
            {
                if !matches!(
                    key,
                    Key::Up
                        | Key::Down
                        | Key::Left
                        | Key::Right
                        | Key::PageUp
                        | Key::PageDown
                        | Key::Home
                        | Key::End
                ) {
                    return;
                }
                self.move_window(key);
            }
            EventPayload::Input(Input::Scroll { rows, .. }) => {
                let key = if rows < 0 { Key::Up } else { Key::Down };
                for _ in 0..rows.unsigned_abs() {
                    self.move_window(key);
                }
            }
            _ => return,
        }
        event.prevent_default();
    }

    fn move_window(&self, key: Key) {
        let rows = self.matches_untracked().len();
        let columns = self
            .listing
            .with_untracked(|listing| listing.grid.columns.len());
        self.window
            .update(|window| window.navigate(key, (rows, columns)));
    }

    fn matches_untracked(&self) -> Vec<usize> {
        let filter = fusor::untrack(|| self.filter());
        self.listing
            .with_untracked(|listing| listing.matching(&filter))
    }

    /// The grid position of the selected row, among the rows the filter shows.
    pub(super) fn selected(&self) -> Option<usize> {
        let selected = self.window.get().selected;
        let filter = self.filter();
        self.listing
            .with(|listing| listing.matching(&filter).get(selected).copied())
    }

    pub(super) fn selected_row(&self) -> Option<usize> {
        fusor::untrack(|| self.selected())
    }

    /// The selected row's value in `column`, empty when there is none.
    pub(super) fn selected_value(&self, column: &str) -> String {
        let Some(row) = self.selected() else {
            return String::new();
        };
        self.listing.with(|listing| {
            listing
                .grid
                .value(row, column)
                .map_or_else(String::new, str::to_owned)
        })
    }

    /// The headline below the grid: an advisor finding's title, or else the selected row.
    pub(super) fn details_title(&self) -> String {
        match self.view() {
            View::Advisors => self.selected_value("Issue"),
            _ => self.selection_summary(),
        }
    }

    /// The text under the headline: an advisor finding's explanation.
    pub(super) fn details_text(&self) -> String {
        match self.view() {
            View::Advisors => self.selected_value("Detail"),
            _ => String::new(),
        }
    }

    /// The selected row's non-empty values on one line.
    fn selection_summary(&self) -> String {
        let Some(row) = self.selected() else {
            return String::new();
        };
        self.listing.with(|listing| {
            listing.grid.rows[row]
                .iter()
                .filter(|value| !value.is_empty())
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" · ")
        })
    }

    pub(super) fn view(&self) -> View {
        self.listing.with(|listing| listing.source.section().view())
    }

    /// Goes back to `source` if the user came through it, or else opens it from its section.
    pub(super) fn return_to(self: &Rc<Self>, source: Source) {
        if self
            .listing
            .with_untracked(|listing| listing.source == source)
        {
            return;
        }
        let found = self
            .history
            .borrow()
            .iter()
            .rposition(|(listing, _)| listing.source == source);
        match found {
            Some(position) => {
                self.history.borrow_mut().truncate(position + 1);
                self.back();
            }
            None => {
                self.open_section(source.section());
                if source != Source::Section(source.section()) {
                    self.drill(source);
                }
            }
        }
        self.focus("grid");
    }

    pub(super) fn resize(&self, event: &Event) {
        if let EventPayload::Resize { width, height } = event.payload {
            let rows = usize::from(height).saturating_sub(HEADER_ROWS);
            let columns = usize::from(width) / COLUMN_CELLS;
            self.window.update(|window| window.resize(rows, columns));
        }
    }

    /// Opens what the selected row stands for, or else the row in full.
    fn open_selected(self: &Rc<Self>) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let source = self
            .listing
            .with_untracked(|listing| listing.source.clone());
        match source.section() {
            Section::Tables => self.open_table(row),
            Section::Storage => self.open_storage_row(row),
            Section::Users => self.open_user(row),
            _ => {
                let fields = self.listing.with_untracked(|listing| listing.fields(row));
                self.show_record(Record::new(source.title(), fields));
            }
        }
    }

    fn open_table(self: &Rc<Self>, row: usize) {
        let table = self.listing.with_untracked(|listing| {
            if listing.source != Source::Section(Section::Tables) {
                return None;
            }
            Some(Source::Table {
                schema: listing.grid.value(row, "Schema")?.to_owned(),
                name: listing.grid.value(row, "Table")?.to_owned(),
            })
        });
        match table {
            Some(table) => self.drill(table),
            None => {
                let fields = self.listing.with_untracked(|listing| listing.fields(row));
                self.show_record(Record::new(self.listing_title(), fields));
            }
        }
    }

    pub(super) fn headings(&self) -> Vec<Cell> {
        let window = self.window.get();
        self.listing.with(|listing| listing.headings(window))
    }

    pub(super) fn rows(&self) -> Vec<Row> {
        let window = self.window.get();
        let filter = self.filter();
        self.listing.with(|listing| {
            let matches = listing.matching(&filter);
            listing.rows(window, &matches)
        })
    }

    pub(super) fn listing_title(&self) -> String {
        self.listing.with(|listing| listing.source.title())
    }

    pub(super) fn listing_summary(&self) -> String {
        self.listing.with(Listing::summary)
    }

    pub(super) fn listing_message(&self) -> String {
        let filter = self.filter();
        self.listing.with(|listing| {
            let message = listing.message();
            if message.is_empty() && listing.matching(&filter).is_empty() {
                return format!("No row contains “{}”.", filter.trim());
            }
            message
        })
    }

    pub(super) fn listing_failed(&self) -> bool {
        self.listing
            .with(|listing| matches!(listing.loading, Loading::Failed(_)))
    }

    pub(super) fn is_section(&self, section: Section) -> bool {
        self.listing
            .with(|listing| listing.source.section() == section)
    }

    pub(super) fn position(&self) -> String {
        let window = self.window.get();
        let filter = self.filter();
        let (shown, total, width) = self.listing.with(|listing| {
            let shown = listing.matching(&filter).len();
            (shown, listing.grid.rows.len(), listing.grid.columns.len())
        });
        let mut position = window.position(shown, width);
        if shown < total {
            position += &format!(" · {total} before filtering");
        }
        if !position.is_empty() && !self.history.borrow().is_empty() {
            position += " · Esc back";
        }
        position
    }
}

async fn fetch(api: Api, project: Rc<Project>, source: Source) -> Result<Grid, supabase::Error> {
    if !project.status.is_running() {
        return Err(supabase::Error::NotRunning(
            project.name.clone(),
            project.status.label(),
        ));
    }
    let reference = &project.reference;
    match source {
        Source::Table { schema, name } => api.table_rows(reference, &schema, &name).await,
        Source::Section(Section::Tables) => api.tables(reference).await,
        Source::Section(Section::Users) => api.users(reference, "").await,
        Source::UserSearch(text) => api.users(reference, &text).await,
        Source::Folder { bucket, prefix } => api.folder(reference, &bucket, &prefix).await,
        Source::FileSearch { bucket, text } => api.files(reference, &bucket, &text).await,
        Source::Section(Section::Storage) => api.buckets(reference).await,
        Source::Section(Section::Functions) => api.functions(reference).await,
        Source::Section(Section::Advisors) => api.advisors(reference).await,
        Source::Section(Section::Backups) => api.backups(reference).await,
        Source::Section(Section::Migrations) => api.migrations(reference).await,
        // The SQL editor shows only the results of queries the user runs.
        Source::Section(Section::Sql) => Ok(Grid::default()),
    }
}
