use super::{
    Console,
    listing::{Section, Source},
};
use hypercmd::{Event, EventPayload, Input, Key};
use std::rc::Rc;

/// What the search bar above the grid does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Search {
    Closed,
    /// Hides the rows of the current list that don't contain the text.
    Filter,
    /// Asks Supabase for the users or files that match, beyond the rows loaded.
    Find,
}

impl Console {
    pub(super) fn open_filter(&self) {
        self.search.set(Search::Filter);
        self.focus("search");
    }

    pub(super) fn open_find(&self) {
        if self.found(String::new()).is_none() {
            self.inform("f finds users or storage files; press / to filter this list.");
            return;
        }
        self.search.set(Search::Find);
        self.focus("search");
    }

    pub(super) fn close_search(&self) {
        self.search.set(Search::Closed);
        self.search_text.set(String::new());
    }

    pub(super) fn submit_search(self: &Rc<Self>, event: &Event) {
        let EventPayload::Input(Input::Key { key, .. }) = event.payload else {
            return;
        };
        match key {
            Key::Enter => self.apply_search(),
            Key::Escape => {
                self.close_search();
                self.focus("grid");
            }
            _ => return,
        }
        event.prevent_default();
    }

    fn apply_search(self: &Rc<Self>) {
        if self.search.get_untracked() == Search::Filter {
            self.focus("grid");
            return;
        }
        let text = self
            .search_text
            .with_untracked(|text| text.trim().to_owned());
        if text.is_empty() {
            return;
        }
        if let Some(source) = self.found(text) {
            self.close_search();
            self.drill(source);
            self.focus("grid");
        }
    }

    /// Where finding `text` leads from the current list, if it can find anything.
    fn found(&self, text: String) -> Option<Source> {
        self.listing
            .with_untracked(|listing| match &listing.source {
                Source::Section(Section::Users) | Source::UserSearch(_) => {
                    Some(Source::UserSearch(text))
                }
                Source::Section(Section::Storage) => Some(Source::FileSearch {
                    bucket: String::new(),
                    text,
                }),
                Source::Folder { bucket, .. } | Source::FileSearch { bucket, .. } => {
                    Some(Source::FileSearch {
                        bucket: bucket.clone(),
                        text,
                    })
                }
                Source::Section(_) | Source::Table { .. } => None,
            })
    }

    /// The text rows must contain to be shown, empty when every row is.
    pub(super) fn filter(&self) -> String {
        if self.search.get() == Search::Filter {
            self.search_text.get()
        } else {
            String::new()
        }
    }

    pub(super) fn filter_changed(&self) {
        self.window.update(|window| window.rewind());
    }

    pub(super) fn searching(&self) -> bool {
        self.search.get() != Search::Closed
    }

    pub(super) fn search_prompt(&self) -> &'static str {
        match (
            self.search.get(),
            self.listing.with(|listing| listing.source.section()),
        ) {
            (Search::Find, Section::Users) => "Find users by email, phone or ID, then Enter",
            (Search::Find, _) => "Find files by path, then Enter",
            (Search::Filter | Search::Closed, _) => {
                "Filter these rows · Enter keeps it, Esc clears"
            }
        }
    }
}
