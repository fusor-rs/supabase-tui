use super::{Console, desktop, projects::findings};
use crate::supabase::Advice;

impl Console {
    /// Such as `1 error and 3 warnings`, or empty until the advisors answer.
    pub(super) fn advice_summary(&self) -> String {
        match self.advice.get() {
            None => String::new(),
            Some(Advice {
                errors: 0,
                warnings: 0,
            }) => "No errors or warnings".into(),
            Some(advice) => findings(advice),
        }
    }

    pub(super) fn has_fix(&self) -> bool {
        !self.selected_value("Fix").is_empty()
    }

    /// Opens the selected finding's remediation guide in the browser.
    pub(super) fn open_fix(&self) {
        let url = fusor::untrack(|| self.selected_value("Fix"));
        if url.is_empty() {
            self.inform("Select a finding first.");
            return;
        }
        if let Err(error) = desktop::open(&url) {
            self.warn(format!(
                "Cannot open the browser ({error}); the link is {url}"
            ));
        }
    }
}
