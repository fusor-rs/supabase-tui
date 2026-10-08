use super::{Console, listing::Field, users::Confirmation};
use std::rc::Rc;

/// What covers the project view, if anything.
#[derive(Clone, PartialEq)]
pub(crate) enum Overlay {
    None,
    Help,
    Record(Rc<Record>),
    Confirm(Rc<Confirmation>),
    Invite,
}

/// Something opened in full: its fields, then text and a link when it has them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Record {
    pub(crate) title: String,
    pub(crate) fields: Vec<Field>,
    pub(crate) body: String,
    pub(crate) link: String,
}

impl Record {
    pub(crate) fn new(title: impl Into<String>, fields: Vec<Field>) -> Self {
        Self {
            title: title.into(),
            fields,
            body: String::new(),
            link: String::new(),
        }
    }
}

impl Console {
    pub(super) fn show_record(&self, record: Record) {
        self.overlay.set(Overlay::Record(Rc::new(record)));
    }

    /// Changes the open record, unless the user closed it or opened another meanwhile.
    pub(super) fn revise_record(&self, title: &str, revise: impl FnOnce(&mut Record)) {
        self.overlay.update(|overlay| {
            if let Overlay::Record(record) = overlay
                && record.title == title
            {
                revise(Rc::make_mut(record));
            }
        });
    }

    pub(super) fn open_help(&self) {
        self.overlay.set(Overlay::Help);
    }

    pub(super) fn close_overlay(&self) {
        self.overlay.set(Overlay::None);
        self.focus("grid");
    }

    pub(super) fn record(&self) -> Option<Rc<Record>> {
        self.overlay.with(|overlay| match overlay {
            Overlay::Record(record) => Some(record.clone()),
            Overlay::None | Overlay::Help | Overlay::Confirm(_) | Overlay::Invite => None,
        })
    }

    pub(super) fn record_title(&self) -> String {
        self.record()
            .map_or_else(String::new, |record| record.title.clone())
    }

    pub(super) fn record_fields(&self) -> Vec<Field> {
        self.record()
            .map_or_else(Vec::new, |record| record.fields.clone())
    }

    pub(super) fn record_body(&self) -> String {
        self.record()
            .map_or_else(String::new, |record| record.body.clone())
    }

    pub(super) fn record_link(&self) -> String {
        self.record()
            .map_or_else(String::new, |record| record.link.clone())
    }
}
