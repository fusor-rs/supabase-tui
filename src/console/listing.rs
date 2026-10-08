use crate::supabase::Grid;
use hypercmd::Key;
use std::{ops::Range, rc::Rc, time::Duration};

/// A part of a project, as the dashboard's sidebar names it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Section {
    Tables,
    Sql,
    Users,
    Storage,
    Functions,
    Advisors,
    Backups,
    Migrations,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Source {
    Section(Section),
    Table {
        schema: String,
        name: String,
    },
    UserSearch(String),
    /// The files and folders under `prefix`, which is empty or ends in `/`.
    Folder {
        bucket: String,
        prefix: String,
    },
    /// Files whose path contains `text`, in `bucket` or, when it is empty, in every bucket.
    FileSearch {
        bucket: String,
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Loading {
    Idle,
    Busy,
    Done(Duration),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Listing {
    pub(crate) source: Source,
    pub(crate) grid: Rc<Grid>,
    pub(crate) loading: Loading,
}

#[derive(Clone, PartialEq)]
pub(crate) struct Cell {
    pub(crate) column: usize,
    pub(crate) text: String,
    pub(crate) flag: Flag,
}

/// How much a cell's value needs attention.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Flag {
    Plain,
    Warning,
    Danger,
}

/// The view that shows a section: its own toolbar and panels around the grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum View {
    Grid,
    Sql,
    Storage,
    Users,
    Advisors,
}

/// Values worth flagging, by column name and exact value, in every section and query.
const FLAGGED: [(&str, &str, Flag); 8] = [
    ("Status", "banned", Flag::Danger),
    ("Status", "unconfirmed", Flag::Warning),
    ("Status", "FAILED", Flag::Danger),
    ("Status", "THROTTLED", Flag::Warning),
    ("RLS", "DISABLED", Flag::Danger),
    ("Level", "ERROR", Flag::Danger),
    ("Level", "WARN", Flag::Warning),
    ("Access", "public", Flag::Warning),
];

#[derive(Clone, PartialEq)]
pub(crate) struct Row {
    pub(crate) index: usize,
    pub(crate) cells: Vec<Cell>,
    pub(crate) selected: bool,
}

/// One column of a row, opened in full.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) value: String,
}

/// The visible rows and columns of a grid and its selected row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Window {
    pub(crate) selected: usize,
    top: usize,
    rows: usize,
    left: usize,
    columns: usize,
}

impl View {
    /// Whether the view describes the selected row below the grid.
    pub(crate) fn has_details(self) -> bool {
        matches!(self, Self::Storage | Self::Users | Self::Advisors)
    }
}

impl Section {
    pub(crate) const ALL: [Self; 8] = [
        Self::Tables,
        Self::Sql,
        Self::Users,
        Self::Storage,
        Self::Functions,
        Self::Advisors,
        Self::Backups,
        Self::Migrations,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Tables => "Tables",
            Self::Sql => "SQL editor",
            Self::Users => "Users",
            Self::Storage => "Storage",
            Self::Functions => "Edge Functions",
            Self::Advisors => "Advisors",
            Self::Backups => "Backups",
            Self::Migrations => "Migrations",
        }
    }

    /// The digit that opens the section.
    pub(crate) fn shortcut(self) -> char {
        let position = Self::ALL
            .iter()
            .position(|section| *section == self)
            .expect("every section is listed in ALL");
        char::from_digit(position as u32 + 1, 10).expect("there are fewer than ten sections")
    }

    pub(crate) fn from_shortcut(key: char) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|section| section.shortcut() == key)
    }

    pub(crate) fn view(self) -> View {
        match self {
            Self::Sql => View::Sql,
            Self::Storage => View::Storage,
            Self::Users => View::Users,
            Self::Advisors => View::Advisors,
            Self::Tables | Self::Functions | Self::Backups | Self::Migrations => View::Grid,
        }
    }

    pub(crate) fn label(self) -> String {
        format!("{}  {}", self.shortcut(), self.name())
    }

    fn empty_message(self) -> &'static str {
        match self {
            Self::Tables => "No tables yet.",
            Self::Sql => "Write SQL above, then press Ctrl+R to run it.",
            Self::Users => "No users have signed up.",
            Self::Storage => "No storage buckets.",
            Self::Functions => "No Edge Functions deployed.",
            Self::Advisors => "No issues found.",
            Self::Backups => "No backups yet.",
            Self::Migrations => "No migrations applied.",
        }
    }
}

impl Field {
    pub(crate) fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

impl Source {
    pub(crate) fn section(&self) -> Section {
        match self {
            Self::Section(section) => *section,
            Self::Table { .. } => Section::Tables,
            Self::UserSearch(_) => Section::Users,
            Self::Folder { .. } | Self::FileSearch { .. } => Section::Storage,
        }
    }

    pub(crate) fn title(&self) -> String {
        match self {
            Self::Section(section) => section.name().into(),
            Self::Table { schema, name } => format!("Tables · {schema}.{name}"),
            Self::UserSearch(text) => format!("Users · matching “{text}”"),
            Self::Folder { bucket, prefix } => format!("Storage · {bucket}/{prefix}"),
            Self::FileSearch { bucket, text } if bucket.is_empty() => {
                format!("Storage · files matching “{text}”")
            }
            Self::FileSearch { bucket, text } => {
                format!("Storage · files in {bucket} matching “{text}”")
            }
        }
    }

    fn empty_message(&self) -> &'static str {
        match self {
            Self::Section(section) => section.empty_message(),
            Self::Table { .. } => "This table is empty.",
            Self::UserSearch(_) => "No user's email, phone or ID contains that.",
            Self::Folder { .. } => "This folder is empty.",
            Self::FileSearch { .. } => "No file path contains that.",
        }
    }
}

impl Listing {
    pub(crate) fn new(source: Source) -> Self {
        Self {
            source,
            grid: Rc::default(),
            loading: Loading::Idle,
        }
    }

    pub(crate) fn summary(&self) -> String {
        let Loading::Done(took) = self.loading else {
            return String::new();
        };
        let rows = match self.grid.rows.len() {
            1 => "1 row".into(),
            count => format!("{count} rows"),
        };
        format!("{rows} · {} ms", took.as_millis())
    }

    /// Text shown in place of rows: progress, an error or an empty result.
    pub(crate) fn message(&self) -> String {
        match &self.loading {
            Loading::Busy => "Loading…".into(),
            Loading::Failed(error) => error.clone(),
            Loading::Idle if self.source == Source::Section(Section::Sql) => {
                Section::Sql.empty_message().into()
            }
            Loading::Done(_) if self.grid.rows.is_empty() => self.source.empty_message().into(),
            Loading::Idle | Loading::Done(_) => String::new(),
        }
    }

    pub(crate) fn headings(&self, window: Window) -> Vec<Cell> {
        window
            .visible_columns(self.grid.columns.len())
            .map(|column| Cell {
                column,
                text: self.grid.columns[column].clone(),
                flag: Flag::Plain,
            })
            .collect()
    }

    /// The positions of the rows that contain `filter`, ignoring case; all rows when it is empty.
    pub(crate) fn matching(&self, filter: &str) -> Vec<usize> {
        let filter = filter.trim().to_lowercase();
        self.grid
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                filter.is_empty() || row.iter().any(|cell| cell.to_lowercase().contains(&filter))
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// The visible rows among `matches`, the grid positions that pass the filter.
    pub(crate) fn rows(&self, window: Window, matches: &[usize]) -> Vec<Row> {
        let visible = window.visible_rows(matches.len());
        let columns = window.visible_columns(self.grid.columns.len());
        matches[visible.clone()]
            .iter()
            .zip(visible)
            .map(|(&row, position)| Row {
                index: row,
                cells: self.cells(row, columns.clone()),
                selected: window.selected == position,
            })
            .collect()
    }

    /// The `columns` of `row`, each on one line and flagged when it needs attention.
    fn cells(&self, row: usize, columns: Range<usize>) -> Vec<Cell> {
        columns
            .map(|column| {
                let text = &self.grid.rows[row][column];
                Cell {
                    column,
                    text: text.replace(['\n', '\r'], "↵"),
                    flag: flag(&self.grid.columns[column], text),
                }
            })
            .collect()
    }

    pub(crate) fn fields(&self, row: usize) -> Vec<Field> {
        fields(&self.grid, row)
    }
}

/// Each column of `row` with its name; none when there is no such row.
pub(crate) fn fields(grid: &Grid, row: usize) -> Vec<Field> {
    let Some(values) = grid.rows.get(row) else {
        return Vec::new();
    };
    grid.columns
        .iter()
        .zip(values)
        .map(|(name, value)| Field::new(name, value))
        .collect()
}

fn flag(column: &str, value: &str) -> Flag {
    FLAGGED
        .iter()
        .find(|(name, flagged, _)| *name == column && *flagged == value)
        .map_or(Flag::Plain, |(_, _, flag)| *flag)
}

impl Default for Window {
    fn default() -> Self {
        Self {
            selected: 0,
            top: 0,
            rows: 1,
            left: 0,
            columns: 1,
        }
    }
}

impl Window {
    pub(crate) fn resize(&mut self, rows: usize, columns: usize) {
        self.rows = rows.max(1);
        self.columns = columns.max(1);
        self.reveal();
    }

    /// Returns to the first row and column, for a new grid.
    pub(crate) fn rewind(&mut self) {
        self.selected = 0;
        self.top = 0;
        self.left = 0;
    }

    /// Moves the selection or the columns; `size` is the grid's (rows, columns).
    pub(crate) fn navigate(&mut self, key: Key, size: (usize, usize)) {
        let (length, width) = size;
        let target = match key {
            Key::Left => {
                self.left = self.left.saturating_sub(1);
                return;
            }
            Key::Right => {
                self.left = (self.left + 1).min(width.saturating_sub(self.columns));
                return;
            }
            Key::Up => self.selected.saturating_sub(1),
            Key::Down => self.selected + 1,
            Key::PageUp => self.selected.saturating_sub(self.rows),
            Key::PageDown => self.selected + self.rows,
            Key::Home => 0,
            Key::End => length,
            _ => return,
        };
        self.selected = target;
        self.keep_within(length);
    }

    /// Keeps the selection on one of `length` rows, as after a reload removed some.
    pub(crate) fn keep_within(&mut self, length: usize) {
        self.selected = self.selected.min(length.saturating_sub(1));
        self.reveal();
    }

    pub(crate) fn visible_rows(&self, length: usize) -> Range<usize> {
        self.top.min(length)..(self.top + self.rows).min(length)
    }

    pub(crate) fn visible_columns(&self, width: usize) -> Range<usize> {
        let left = self.left.min(width.saturating_sub(self.columns));
        left..(left + self.columns).min(width)
    }

    /// Where the window is, such as `row 3 of 40 · columns 2–5 of 9`.
    pub(crate) fn position(&self, length: usize, width: usize) -> String {
        if length == 0 {
            return String::new();
        }
        let mut position = format!("row {} of {length}", self.selected + 1);
        let columns = self.visible_columns(width);
        if columns.len() < width {
            position += &format!(
                " · columns {}–{} of {width}",
                columns.start + 1,
                columns.end
            );
        }
        position
    }

    fn reveal(&mut self) {
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + self.rows {
            self.top = self.selected + 1 - self.rows;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Flag, Key, Listing, Section, Source, Window, flag};
    use crate::supabase::Grid;
    use std::rc::Rc;

    #[test]
    fn filters_rows_by_any_cell_ignoring_case() {
        let mut listing = Listing::new(Source::Section(Section::Storage));
        let rows = [
            ["avatars", "public"],
            ["Invoices", "private"],
            ["logs", "private"],
        ];
        listing.grid = Rc::new(Grid::new(
            &["Bucket", "Access"],
            rows.iter()
                .map(|row| row.map(String::from).to_vec())
                .collect(),
        ));
        assert_eq!(listing.matching(""), [0, 1, 2]);
        assert_eq!(listing.matching(" inv "), [1]);
        assert_eq!(listing.matching("PRIVATE"), [1, 2]);
        assert_eq!(listing.matching("nothing"), [0; 0]);
    }

    #[test]
    fn window_keeps_the_selection_visible() {
        let mut window = Window::default();
        window.resize(3, 2);
        for _ in 0..4 {
            window.navigate(Key::Down, (10, 5));
        }
        assert_eq!(window.selected, 4);
        assert_eq!(window.visible_rows(10), 2..5);
        window.navigate(Key::End, (10, 5));
        assert_eq!(window.visible_rows(10), 7..10);
        window.navigate(Key::PageUp, (10, 5));
        assert_eq!((window.selected, window.visible_rows(10)), (6, 6..9));
        window.navigate(Key::Home, (10, 5));
        assert_eq!(window.visible_rows(10), 0..3);
    }

    #[test]
    fn window_pans_across_wide_grids() {
        let mut window = Window::default();
        window.resize(5, 2);
        for _ in 0..9 {
            window.navigate(Key::Right, (1, 5));
        }
        assert_eq!(window.visible_columns(5), 3..5);
        assert_eq!(window.position(1, 5), "row 1 of 1 · columns 4–5 of 5");
        assert_eq!(window.visible_columns(1), 0..1);
        assert_eq!(window.position(1, 1), "row 1 of 1");
        window.navigate(Key::Left, (1, 5));
        assert_eq!(window.visible_columns(5), 2..4);
    }

    #[test]
    fn flags_values_by_column() {
        assert_eq!(flag("Status", "banned"), Flag::Danger);
        assert_eq!(flag("Level", "WARN"), Flag::Warning);
        assert_eq!(flag("Email", "banned"), Flag::Plain);
        assert_eq!(flag("RLS", "enabled"), Flag::Plain);
    }

    #[test]
    fn digits_open_sections_in_sidebar_order() {
        assert_eq!(Section::from_shortcut('1'), Some(Section::Tables));
        assert_eq!(Section::from_shortcut('2'), Some(Section::Sql));
        assert_eq!(Section::from_shortcut('8'), Some(Section::Migrations));
        assert_eq!(Section::from_shortcut('9'), None);
    }
}
