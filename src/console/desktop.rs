use std::{
    env,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[cfg(target_os = "macos")]
const OPENER: &str = "open";
#[cfg(not(target_os = "macos"))]
const OPENER: &str = "xdg-open";
#[cfg(target_os = "macos")]
const CLIPBOARDS: &[&[&str]] = &[&["pbcopy"]];
#[cfg(not(target_os = "macos"))]
const CLIPBOARDS: &[&[&str]] = &[&["wl-copy"], &["xclip", "-selection", "clipboard"]];
const DOWNLOADS: &str = "Downloads";

pub(super) fn open(url: &str) -> io::Result<()> {
    Command::new(OPENER)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

/// Puts `text` on the clipboard with the first clipboard tool that works.
pub(super) fn copy(text: &str) -> io::Result<()> {
    let mut failure = io::Error::other("no clipboard tool is installed");
    for command in CLIPBOARDS {
        match pipe(command, text) {
            Ok(()) => return Ok(()),
            Err(error) => failure = error,
        }
    }
    Err(failure)
}

fn pipe(command: &[&str], text: &str) -> io::Result<()> {
    let mut child = Command::new(command[0])
        .args(&command[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(text.as_bytes())?;
    if child.wait()?.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("{} failed", command[0])))
    }
}

/// A path in the Downloads folder for `name` that no file uses yet.
pub(super) fn download_path(name: &str) -> io::Result<PathBuf> {
    let home = env::var_os("HOME").ok_or_else(|| io::Error::other("HOME is not set"))?;
    let folder = Path::new(&home).join(DOWNLOADS);
    std::fs::create_dir_all(&folder)?;
    let mut copy = 0;
    loop {
        let path = folder.join(numbered(name, copy));
        if !path.try_exists()? {
            return Ok(path);
        }
        copy += 1;
    }
}

/// `path` with the home folder written as `~`.
pub(super) fn shown(path: &Path) -> String {
    let home = env::var_os("HOME").map(PathBuf::from);
    // A path outside the home folder is shown in full.
    match home
        .as_deref()
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(relative) => format!("~/{}", relative.display()),
        None => path.display().to_string(),
    }
}

/// `name` for the first copy, then `name (1).ext`, `name (2).ext`…
fn numbered(name: &str, copy: usize) -> String {
    if copy == 0 {
        return name.to_owned();
    }
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => format!("{stem} ({copy}).{extension}"),
        _ => format!("{name} ({copy})"),
    }
}

#[cfg(test)]
mod tests {
    use super::numbered;

    #[test]
    fn copies_are_numbered_before_the_extension() {
        assert_eq!(numbered("cat.png", 0), "cat.png");
        assert_eq!(numbered("cat.png", 2), "cat (2).png");
        assert_eq!(numbered("archive.tar.gz", 1), "archive.tar (1).gz");
        assert_eq!(numbered(".env", 1), ".env (1)");
        assert_eq!(numbered("README", 1), "README (1)");
    }
}
