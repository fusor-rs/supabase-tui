use super::{Console, Screen, listing::Section, overlay::Overlay, users::UserAction};
use hypercmd::{Error, ErrorKind, Event, EventPayload, Input, Key};
use std::rc::Rc;

impl Console {
    pub(super) fn shortcut(self: &Rc<Self>, event: &Event) -> Result<(), Error> {
        let EventPayload::Input(Input::Key { key, modifiers, .. }) = event.payload else {
            return Ok(());
        };
        if self.screen.get_untracked() != Screen::Console {
            return Ok(());
        }
        if self.overlay.get_untracked() != Overlay::None {
            if self.overlay_key(key) {
                event.prevent_default();
            }
            return Ok(());
        }
        if modifiers.control || modifiers.alt || modifiers.super_key {
            if key == Key::Char('r') && modifiers.control && self.in_sql_editor() {
                self.run_sql();
                event.prevent_default();
            }
            return Ok(());
        }
        if matches!(event.target.tag(), "input" | "textarea") {
            if key == Key::Escape {
                self.focus("grid");
                event.prevent_default();
            }
            return Ok(());
        }
        if self.single_key(key)? || self.section_key(key) {
            event.prevent_default();
        }
        Ok(())
    }

    /// Handles a key while an overlay is open; returns whether it was one.
    fn overlay_key(self: &Rc<Self>, key: Key) -> bool {
        let closes = matches!(key, Key::Escape);
        match (self.overlay.get_untracked(), key) {
            (Overlay::Confirm(_), Key::Char('y')) => self.confirm(),
            (Overlay::Confirm(_), Key::Char('n')) => self.close_overlay(),
            (Overlay::Help | Overlay::Record(_), Key::Char('q') | Key::Backspace) => {
                self.close_overlay();
            }
            (Overlay::Record(_), _) if !closes => return self.section_key(key),
            _ if closes => self.close_overlay(),
            _ => return false,
        }
        true
    }

    /// Handles a single-key shortcut; returns whether `key` was one.
    fn single_key(self: &Rc<Self>, key: Key) -> Result<bool, Error> {
        match key {
            Key::Char('r') => self.refresh(),
            Key::Char('R') => self.restore(),
            Key::Char('/') => self.open_filter(),
            Key::Char('f') => self.open_find(),
            Key::Char('?') => self.open_help(),
            Key::Char('q') => quit()?,
            Key::Char(digit) => match Section::from_shortcut(digit) {
                Some(section) => self.open_section(section),
                None => return Ok(false),
            },
            Key::Escape | Key::Backspace if self.searching() => self.close_search(),
            Key::Escape | Key::Backspace => return Ok(self.back()),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Handles a key that acts on the selected user or file; returns whether it was one.
    fn section_key(self: &Rc<Self>, key: Key) -> bool {
        let section = self
            .listing
            .with_untracked(|listing| listing.source.section());
        match (section, key) {
            (Section::Users, Key::Char('i')) => self.open_invite(),
            (Section::Users, Key::Char('p')) => self.ask(UserAction::SendRecovery),
            (Section::Users, Key::Char('m')) => self.ask(UserAction::SendMagicLink),
            (Section::Users, Key::Char('b')) => self.ask(UserAction::Ban),
            (Section::Users, Key::Char('D')) => self.ask(UserAction::Delete),
            (Section::Storage, Key::Char('o')) => self.open_file(),
            (Section::Storage, Key::Char('u')) => self.share_file(),
            (Section::Storage, Key::Char('d')) => self.download_file(),
            (Section::Advisors, Key::Char('o')) => self.open_fix(),
            _ => return false,
        }
        true
    }

    /// Tab leaves the sidebar for the grid instead of visiting every project.
    pub(super) fn leave_sidebar(&self, event: &Event) {
        if let EventPayload::Input(Input::Key {
            key: Key::Tab,
            modifiers,
            ..
        }) = event.payload
            && !modifiers.shift
        {
            self.focus(if self.in_sql_editor() { "sql" } else { "grid" });
            event.prevent_default();
        }
    }
}

/// Asks the terminal runner for the same orderly shutdown as Ctrl+C.
fn quit() -> Result<(), Error> {
    signal_hook::low_level::raise(signal_hook::consts::SIGTERM)
        .map_err(|error| Error::new(ErrorKind::Terminal, format!("cannot quit: {error}")))
}
