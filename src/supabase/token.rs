use super::Error;
use std::env::{self, VarError};

pub(crate) const TOKENS_PAGE: &str = "https://supabase.com/dashboard/account/tokens";
/// The variable the Supabase CLI reads its access token from.
pub(crate) const TOKEN_VARIABLE: &str = "SUPABASE_ACCESS_TOKEN";
const KEYRING_SERVICE: &str = "supabase-tui";
const KEYRING_ACCOUNT: &str = "access-token";

/// The access token from the environment, or else the one saved in the keychain.
pub(crate) fn saved_token() -> Result<Option<String>, Error> {
    match env::var(TOKEN_VARIABLE) {
        Ok(token) if !token.trim().is_empty() => return Ok(Some(token.trim().to_owned())),
        // Without the variable, the keychain holds the token.
        Ok(_) | Err(VarError::NotPresent) => {}
        Err(error) => return Err(error.into()),
    }
    match keyring_entry()?.get_password() {
        Ok(token) => Ok(Some(token)),
        // Not being logged in yet is the expected first-run state.
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn store_token(token: &str) -> Result<(), Error> {
    keyring_entry()?.set_password(token)?;
    Ok(())
}

pub(crate) fn forget() -> Result<(), Error> {
    match keyring_entry()?.delete_credential() {
        // Logging out twice leaves the intended state.
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn keyring_entry() -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
}
