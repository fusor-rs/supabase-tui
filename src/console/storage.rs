use super::{
    Console, desktop,
    listing::{Field, Section, Source},
    overlay::Record,
};
use crate::supabase::{SIGNED_URL_LIFETIME, StoredFile, unix_timestamp};
use std::{
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

const PREVIEW_BYTES: usize = 64 * 1024;
const TEXT_TYPES: [&str; 8] = [
    "json",
    "xml",
    "javascript",
    "csv",
    "yaml",
    "sql",
    "markdown",
    "toml",
];
const TEXT_EXTENSIONS: [&str; 16] = [
    "txt", "md", "json", "csv", "tsv", "log", "sql", "yml", "yaml", "toml", "xml", "html", "css",
    "js", "ts", "env",
];
const SIZE_UNITS: [&str; 4] = ["kB", "MB", "GB", "TB"];
const SIZE_STEP: f64 = 1000.0;

/// One step of the storage path above the grid, and the list it leads back to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Crumb {
    pub(crate) position: usize,
    pub(crate) label: String,
    pub(crate) target: Source,
}

impl Console {
    pub(super) fn crumbs(&self) -> Vec<Crumb> {
        self.listing.with(|listing| crumbs(&listing.source))
    }

    pub(super) fn file_selected(&self) -> bool {
        self.selected().and_then(|row| self.file_at(row)).is_some()
    }

    /// Opens the selected bucket or folder, or previews the selected file.
    pub(super) fn open_storage_row(self: &Rc<Self>, row: usize) {
        let opened = self.listing.with_untracked(|listing| {
            let name = |column| listing.grid.value(row, column).map(str::to_owned);
            match &listing.source {
                Source::Section(Section::Storage) => Some(Source::Folder {
                    bucket: name("Bucket")?,
                    prefix: String::new(),
                }),
                Source::Folder { bucket, prefix } => {
                    let folder = name("Name").filter(|name| name.ends_with('/'))?;
                    Some(Source::Folder {
                        bucket: bucket.clone(),
                        prefix: format!("{prefix}{folder}"),
                    })
                }
                _ => None,
            }
        });
        match (opened, self.file_at(row)) {
            (Some(folder), _) => self.drill(folder),
            (None, Some(file)) => self.preview(row, file),
            (None, None) => {}
        }
    }

    fn file_at(&self, row: usize) -> Option<StoredFile> {
        self.listing.with_untracked(|listing| {
            let grid = &listing.grid;
            match &listing.source {
                Source::Folder { bucket, prefix } => {
                    let name = grid
                        .value(row, "Name")
                        .filter(|name| !name.ends_with('/'))?;
                    Some(StoredFile {
                        bucket: bucket.clone(),
                        path: format!("{prefix}{name}"),
                    })
                }
                Source::FileSearch { .. } => Some(StoredFile {
                    bucket: grid.value(row, "Bucket")?.to_owned(),
                    path: grid.value(row, "Path")?.to_owned(),
                }),
                _ => None,
            }
        })
    }

    fn selected_file(&self) -> Option<StoredFile> {
        let file = self.selected_row().and_then(|row| self.file_at(row));
        if file.is_none() {
            self.inform("Select a file first.");
        }
        file
    }

    fn preview(self: &Rc<Self>, row: usize, file: StoredFile) {
        let (fields, kind) = self.listing.with_untracked(|listing| {
            let kind = listing.grid.value(row, "Type").map(str::to_owned);
            (listing.fields(row), kind)
        });
        let title = format!("{}/{}", file.bucket, file.path);
        let mut record = Record::new(title.clone(), fields);
        if !is_text(&file.path, kind.as_deref()) {
            record.body = "No preview for this type.\n\
                           o opens it in your browser · d downloads it · u makes a shareable link"
                .into();
            self.show_record(record);
            return;
        }
        record.body = "Loading…".into();
        self.show_record(record);
        let console = self.clone();
        self.spawn(async move {
            let loaded = match console.project_api().await {
                Ok(api) => api.head(&file, PREVIEW_BYTES).await,
                Err(error) => Err(error),
            };
            let body = match loaded {
                Ok((head, false)) => String::from_utf8_lossy(&head).into_owned(),
                Ok((head, true)) => format!(
                    "{}\n… the preview shows the first {}",
                    String::from_utf8_lossy(&head),
                    size(PREVIEW_BYTES as u64)
                ),
                Err(error) => error.to_string(),
            };
            console.revise_record(&title, |record| record.body = body);
            Ok(())
        });
    }

    pub(super) fn open_file(self: &Rc<Self>) {
        let Some(file) = self.selected_file() else {
            return;
        };
        let console = self.clone();
        self.spawn(async move {
            let url = console.project_api().await?.signed_url(&file).await?;
            desktop::open(&url)?;
            console.inform(format!("Opened {} in your browser.", file.path));
            Ok(())
        });
    }

    /// Creates a signed link to the selected file and copies it to the clipboard.
    pub(super) fn share_file(self: &Rc<Self>) {
        let Some(file) = self.selected_file() else {
            return;
        };
        let console = self.clone();
        self.spawn(async move {
            let url = console.project_api().await?.signed_url(&file).await?;
            let expires = SystemTime::now() + SIGNED_URL_LIFETIME;
            // Any clock that reaches Supabase is past 1970.
            let expires = expires.duration_since(UNIX_EPOCH).unwrap_or_default();
            let fields = vec![
                Field::new("Bucket", &file.bucket),
                Field::new("Path", &file.path),
                Field::new("Expires (UTC)", unix_timestamp(expires.as_millis() as u64)),
            ];
            let mut record = Record::new("Signed URL", fields);
            record.body.clone_from(&url);
            record.link.clone_from(&url);
            console.show_record(record);
            match desktop::copy(&url) {
                Ok(()) => console.inform("Copied the signed URL to the clipboard."),
                Err(error) => {
                    console.warn(format!("Cannot copy the URL ({error}); select it above."))
                }
            }
            Ok(())
        });
    }

    pub(super) fn download_file(self: &Rc<Self>) {
        let Some(file) = self.selected_file() else {
            return;
        };
        // Splitting always yields at least one part.
        let name = file
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&file.path)
            .to_owned();
        let destination = match desktop::download_path(&name) {
            Ok(destination) => destination,
            Err(error) => {
                self.warn(format!("Cannot choose where to save {name} ({error})"));
                return;
            }
        };
        self.inform(format!("Downloading {name}…"));
        let shown = desktop::shown(&destination);
        let console = self.clone();
        self.spawn(async move {
            let saved = console
                .project_api()
                .await?
                .download(&file, &destination)
                .await?;
            console.inform(format!("Saved {shown} ({}).", size(saved)));
            Ok(())
        });
    }
}

/// The path from the bucket list to `source`: buckets, the bucket, each folder, a search.
fn crumbs(source: &Source) -> Vec<Crumb> {
    let mut steps = vec![("Buckets".to_owned(), Source::Section(Section::Storage))];
    let folder = |bucket: &str, prefix: String| Source::Folder {
        bucket: bucket.to_owned(),
        prefix,
    };
    match source {
        Source::Folder { bucket, prefix } => {
            steps.push((bucket.clone(), folder(bucket, String::new())));
            let mut path = String::new();
            for name in prefix.split_terminator('/') {
                path = format!("{path}{name}/");
                steps.push((name.to_owned(), folder(bucket, path.clone())));
            }
        }
        Source::FileSearch { bucket, text } => {
            if !bucket.is_empty() {
                steps.push((bucket.clone(), folder(bucket, String::new())));
            }
            steps.push((format!("“{text}”"), source.clone()));
        }
        _ => {}
    }
    steps
        .into_iter()
        .enumerate()
        .map(|(position, (label, target))| Crumb {
            position,
            label,
            target,
        })
        .collect()
}

/// Whether a file is text worth previewing, by its type or else its extension.
fn is_text(path: &str, kind: Option<&str>) -> bool {
    // Files uploaded without a type are judged by their extension alone.
    let kind = kind.unwrap_or_default();
    if kind.starts_with("text/") || TEXT_TYPES.iter().any(|text| kind.contains(text)) {
        return true;
    }
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| TEXT_EXTENSIONS.contains(&extension.to_lowercase().as_str()))
}

/// Formats a byte count in decimal units, such as `1.5 MB`.
fn size(bytes: u64) -> String {
    let mut value = bytes as f64;
    if value < SIZE_STEP {
        return format!("{bytes} bytes");
    }
    let mut unit = SIZE_UNITS[0];
    for next in SIZE_UNITS {
        value /= SIZE_STEP;
        unit = next;
        if value < SIZE_STEP {
            break;
        }
    }
    format!("{value:.1} {unit}")
}

#[cfg(test)]
mod tests {
    use super::{Section, Source, crumbs, is_text, size};

    #[test]
    fn crumbs_lead_back_through_each_folder() {
        let folder = |prefix: &str| Source::Folder {
            bucket: "docs".into(),
            prefix: prefix.into(),
        };
        let steps: Vec<_> = crumbs(&folder("2024/invoices/"))
            .into_iter()
            .map(|crumb| (crumb.label, crumb.target))
            .collect();
        assert_eq!(
            steps,
            [
                ("Buckets".into(), Source::Section(Section::Storage)),
                ("docs".into(), folder("")),
                ("2024".into(), folder("2024/")),
                ("invoices".into(), folder("2024/invoices/")),
            ]
        );
        let search = Source::FileSearch {
            bucket: String::new(),
            text: "cat".into(),
        };
        let labels: Vec<_> = crumbs(&search)
            .into_iter()
            .map(|crumb| crumb.label)
            .collect();
        assert_eq!(labels, ["Buckets", "“cat”"]);
    }

    #[test]
    fn text_files_are_known_by_type_or_extension() {
        assert!(is_text("notes/today", Some("text/plain")));
        assert!(is_text("data", Some("application/json; charset=utf-8")));
        assert!(is_text(
            "exports/Orders.CSV",
            Some("application/octet-stream")
        ));
        assert!(is_text("schema.sql", None));
        assert!(!is_text("avatars/cat.png", Some("image/png")));
        assert!(!is_text("archive.tar.gz", None));
    }

    #[test]
    fn sizes_use_decimal_units() {
        assert_eq!(size(999), "999 bytes");
        assert_eq!(size(1_000), "1.0 kB");
        assert_eq!(size(1_536_000), "1.5 MB");
        assert_eq!(size(2_000_000_000_000), "2.0 TB");
    }
}
