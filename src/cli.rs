use crate::supabase::TOKEN_VARIABLE;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

/// Manage your Supabase projects in the terminal.
///
/// Without a command, opens the console; it asks for an access token the first time.
#[derive(Parser)]
#[command(version)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Remove the saved access token from this computer
    Logout,
    /// Upgrade to the latest stable release
    Upgrade,
}

/// Parses the arguments, printing help, the version or a usage error and exiting when asked.
pub(crate) fn parse() -> Cli {
    let command = Cli::command().after_help(format!(
        "{TOKEN_VARIABLE}, when set, is used instead of the saved token.\n\
         Press ? in the console for keys."
    ));
    Cli::from_arg_matches(&command.get_matches()).unwrap_or_else(|error| error.exit())
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command};
    use clap::Parser;

    #[test]
    fn reads_commands_and_rejects_unknown_ones() {
        let command = |arguments: &[&str]| {
            Cli::try_parse_from(arguments).map(|cli| match cli.command {
                None => "console",
                Some(Command::Logout) => "logout",
                Some(Command::Upgrade) => "upgrade",
            })
        };
        assert_eq!(command(&["supatui"]).unwrap(), "console");
        assert_eq!(command(&["supatui", "logout"]).unwrap(), "logout");
        assert_eq!(command(&["supatui", "upgrade"]).unwrap(), "upgrade");
        assert!(command(&["supatui", "update"]).is_err());
        assert!(command(&["supatui", "upgrade", "now"]).is_err());
    }
}
