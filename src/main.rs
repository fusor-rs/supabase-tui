mod cli;
mod console;
mod supabase;
mod upgrade;

use cli::Command;
use console::{Console, Startup};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match cli::parse().command {
        None => run(),
        Some(Command::Logout) => {
            supabase::forget()?;
            println!("Logged out. The saved Supabase access token is removed from this computer.");
            Ok(())
        }
        Some(Command::Upgrade) => upgrade::run(),
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let _runtime = runtime.enter();
    let startup = Startup {
        http: reqwest::Client::new(),
        token: supabase::saved_token()?,
    };
    hypercmd::native::run(hypercmd::mount::<Console>(startup)?)?;
    Ok(())
}
