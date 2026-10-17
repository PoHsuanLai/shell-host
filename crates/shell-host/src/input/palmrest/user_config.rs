//! The person's gesture settings, `~/.config/palmrest/gestures.toml`. palmrest is a system service
//! and cannot read a home folder, so the host reads the file and sends its text (`UserConfig`)
//! after connecting and when the file changes. The host watches no files, so a change is found by
//! looking at the file's size and modification time every [`LOOK_EVERY`].

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use palmrest_protocol::USER_CONFIG_MAX_BYTES;

/// How often the file is looked at while the daemon is connected.
pub(crate) const LOOK_EVERY: Duration = Duration::from_secs(2);

/// The file under the person's configuration folder.
const FILE: &str = "palmrest/gestures.toml";

/// Where the file is: `$XDG_CONFIG_HOME/palmrest/gestures.toml`, else
/// `~/.config/palmrest/gestures.toml`; none when neither variable is set.
pub(crate) fn path(lookup: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let base = lookup("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            lookup("HOME")
                .filter(|v| !v.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })?;
    Some(base.join(FILE))
}

/// How the file looked: what a later look is compared with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Stamp {
    Absent,
    Present {
        modified: Option<SystemTime>,
        len: u64,
    },
}

/// How the file looks now.
pub(crate) fn stamp(path: &std::path::Path) -> Stamp {
    match std::fs::metadata(path) {
        Ok(meta) => Stamp::Present {
            modified: meta.modified().ok(),
            len: meta.len(),
        },
        Err(_) => Stamp::Absent,
    }
}

/// The file's text. Empty (which takes the person's layer away at the daemon) when there is no
/// file, or when it cannot be used: not a regular file, larger than the bound, not text. Opened
/// without blocking, so a named pipe put in its place is refused instead of waited for.
pub(crate) fn read(path: &std::path::Path) -> String {
    let Ok(file) = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(path)
    else {
        return String::new();
    };
    if !file.metadata().is_ok_and(|meta| meta.is_file()) {
        eprintln!(
            "shell-host: {} is not a regular file; ignoring it",
            path.display()
        );
        return String::new();
    }
    let mut bytes = Vec::new();
    let read = file
        .take(USER_CONFIG_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes);
    match (read, String::from_utf8(bytes)) {
        (Ok(_), Ok(text)) if text.len() <= USER_CONFIG_MAX_BYTES => text,
        _ => {
            eprintln!(
                "shell-host: {} is unreadable, not text, or over {USER_CONFIG_MAX_BYTES} bytes; ignoring it",
                path.display()
            );
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |k| {
            vars.iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| OsString::from(v))
        }
    }

    /// A case's name, its environment and the file it should come to.
    type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], Option<&'a str>);

    #[test]
    fn the_file_is_under_xdg_config_home_then_home() {
        let cases: &[Case] = &[
            (
                "xdg wins",
                &[("XDG_CONFIG_HOME", "/x"), ("HOME", "/h")],
                Some("/x/palmrest/gestures.toml"),
            ),
            (
                "home",
                &[("HOME", "/h")],
                Some("/h/.config/palmrest/gestures.toml"),
            ),
            (
                "empty xdg falls to home",
                &[("XDG_CONFIG_HOME", ""), ("HOME", "/h")],
                Some("/h/.config/palmrest/gestures.toml"),
            ),
            ("neither", &[], None),
        ];
        for (name, vars, want) in cases {
            assert_eq!(path(lookup(vars)), want.map(PathBuf::from), "{name}");
        }
    }

    #[test]
    fn the_text_is_read_and_anything_unusable_is_empty() {
        let dir = std::env::temp_dir().join(format!("shell-host-gestures-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("gestures.toml");
        assert_eq!(read(&file), "", "no file");
        assert_eq!(stamp(&file), Stamp::Absent);
        std::fs::write(&file, "[scroll]\nspeed = 30\n").expect("write");
        assert_eq!(read(&file), "[scroll]\nspeed = 30\n");
        assert!(matches!(stamp(&file), Stamp::Present { len: 20, .. }));
        std::fs::write(&file, "x".repeat(USER_CONFIG_MAX_BYTES + 1)).expect("write");
        assert_eq!(read(&file), "", "over the bound");
        std::fs::write(&file, [0xff, 0xfe]).expect("write");
        assert_eq!(read(&file), "", "not text");
        assert_eq!(read(&dir), "", "not a regular file");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
