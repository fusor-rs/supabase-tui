use super::{
    Console,
    listing::{Section, fields},
    overlay::{Overlay, Record},
};
use crate::supabase::{Ban, Email};
use hypercmd::{Event, EventPayload, Input, Key};
use std::rc::Rc;

const USER_KEYS: &str = "p password reset · m magic link · b ban or unban · D delete";

/// The Auth user a row shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Account {
    id: String,
    email: String,
    banned: bool,
}

/// What can be done to a user once confirmed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum UserAction {
    SendRecovery,
    SendMagicLink,
    Ban,
    Unban,
    Delete,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Confirmation {
    action: UserAction,
    account: Account,
}

impl Account {
    /// How to name the user in a sentence: the email, or the ID for phone-only users.
    fn label(&self) -> &str {
        if self.email.is_empty() {
            &self.id
        } else {
            &self.email
        }
    }
}

impl Confirmation {
    pub(crate) fn question(&self) -> String {
        let user = self.account.label();
        match self.action {
            UserAction::SendRecovery => format!("Email a password reset link to {user}?"),
            UserAction::SendMagicLink => format!("Email a sign-in link to {user}?"),
            UserAction::Ban => {
                format!("Ban {user}?\nThey can't sign in or refresh a session until unbanned.")
            }
            UserAction::Unban => format!("Lift the ban on {user}?"),
            UserAction::Delete => {
                format!("Delete {user}?\nTheir account and identities are removed for good.")
            }
        }
    }

    fn outcome(&self) -> String {
        let user = self.account.label();
        match self.action {
            UserAction::SendRecovery => format!("Sent a password reset link to {user}."),
            UserAction::SendMagicLink => format!("Sent a sign-in link to {user}."),
            UserAction::Ban => format!("Banned {user}."),
            UserAction::Unban => format!("Lifted the ban on {user}."),
            UserAction::Delete => format!("Deleted {user}."),
        }
    }
}

impl Console {
    fn account_at(&self, row: usize) -> Option<Account> {
        self.listing.with_untracked(|listing| {
            if listing.source.section() != Section::Users {
                return None;
            }
            let grid = &listing.grid;
            Some(Account {
                id: grid.value(row, "ID")?.to_owned(),
                email: grid.value(row, "Email")?.to_owned(),
                banned: grid.value(row, "Status")? == "banned",
            })
        })
    }

    /// Shows the row at once, then the account's full details when they load.
    pub(super) fn open_user(self: &Rc<Self>, row: usize) {
        let Some(account) = self.account_at(row) else {
            return;
        };
        let title = account.label().to_owned();
        let mut record = Record::new(
            title.clone(),
            self.listing.with_untracked(|listing| listing.fields(row)),
        );
        record.body = "Loading the account's details…".into();
        self.show_record(record);
        let Some((api, project)) = self.target() else {
            return;
        };
        let console = self.clone();
        self.spawn(async move {
            let details = api.user(&project.reference, &account.id).await;
            console.revise_record(&title, |record| match details {
                Ok(grid) => {
                    record.fields = fields(&grid, 0);
                    record.body = USER_KEYS.into();
                }
                Err(error) => record.body = error.to_string(),
            });
            Ok(())
        });
    }

    pub(super) fn user_selected(&self) -> bool {
        self.selected()
            .and_then(|row| self.account_at(row))
            .is_some()
    }

    pub(super) fn ban_label(&self) -> &'static str {
        if self.selected_value("Status") == "banned" {
            "Unban  b"
        } else {
            "Ban  b"
        }
    }

    pub(super) fn ask(&self, action: UserAction) {
        let Some(account) = self.selected_row().and_then(|row| self.account_at(row)) else {
            self.inform("Select a user first.");
            return;
        };
        let emails = matches!(action, UserAction::SendRecovery | UserAction::SendMagicLink);
        if emails && account.email.is_empty() {
            self.inform("This user has no email address.");
            return;
        }
        let action = match (action, account.banned) {
            (UserAction::Ban, true) => UserAction::Unban,
            (other, _) => other,
        };
        let confirmation = Confirmation { action, account };
        self.overlay.set(Overlay::Confirm(Rc::new(confirmation)));
    }

    pub(super) fn confirm(self: &Rc<Self>) {
        let Overlay::Confirm(confirmation) = self.overlay.get_untracked() else {
            return;
        };
        self.close_overlay();
        let console = self.clone();
        self.spawn(async move {
            let api = console.project_api().await?;
            let account = &confirmation.account;
            match confirmation.action {
                UserAction::SendRecovery => {
                    api.send_email(Email::PasswordRecovery, &account.email)
                        .await?;
                }
                UserAction::SendMagicLink => {
                    api.send_email(Email::MagicLink, &account.email).await?
                }
                UserAction::Ban => api.ban(&account.id, Ban::Forever).await?,
                UserAction::Unban => api.ban(&account.id, Ban::Lifted).await?,
                UserAction::Delete => api.delete_user(&account.id).await?,
            }
            console.inform(confirmation.outcome());
            console.reload();
            Ok(())
        });
    }

    pub(super) fn confirmation_question(&self) -> String {
        self.overlay.with(|overlay| match overlay {
            Overlay::Confirm(confirmation) => confirmation.question(),
            Overlay::None | Overlay::Help | Overlay::Record(_) | Overlay::Invite => String::new(),
        })
    }

    pub(super) fn open_invite(&self) {
        self.invite_draft.set(String::new());
        self.overlay.set(Overlay::Invite);
    }

    pub(super) fn submit_invite(self: &Rc<Self>, event: &Event) {
        let EventPayload::Input(Input::Key {
            key: Key::Enter, ..
        }) = event.payload
        else {
            return;
        };
        event.prevent_default();
        self.send_invite();
    }

    pub(super) fn send_invite(self: &Rc<Self>) {
        let email = self
            .invite_draft
            .with_untracked(|draft| draft.trim().to_owned());
        if email.is_empty() {
            self.inform("Type the email address to invite.");
            return;
        }
        self.close_overlay();
        let console = self.clone();
        self.spawn(async move {
            let api = console.project_api().await?;
            api.send_email(Email::Invitation, &email).await?;
            console.inform(format!("Invited {email}."));
            console.reload();
            Ok(())
        });
    }
}
