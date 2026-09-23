//! The stdin command language: `[bar|dock] hover x y`, `click x y`, `leave`, `stats`,
//! `capture <path>`, `quit`. Coordinates are logical px relative to the surface.

/// Which surface a command addresses; `Primary` is the first one spawned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Primary,
    Bar,
    Dock,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    Hover(Target, f32, f32),
    Click(Target, f32, f32),
    Leave(Target),
    Stats,
    Capture(Target, String),
    Quit,
}

/// Parses one line; `Err` carries a message for stderr.
pub fn parse(line: &str) -> Result<Cmd, String> {
    let mut words: Vec<&str> = line.split_whitespace().collect();
    let target = match words.first() {
        Some(&"bar") => Target::Bar,
        Some(&"dock") => Target::Dock,
        _ => Target::Primary,
    };
    if target != Target::Primary {
        words.remove(0);
    }
    let num = |i: usize| -> Result<f32, String> {
        words
            .get(i)
            .ok_or(format!("missing argument {i} in {line:?}"))?
            .parse::<f32>()
            .map_err(|e| format!("{e} in {line:?}"))
    };
    match words.first() {
        Some(&"hover") => Ok(Cmd::Hover(target, num(1)?, num(2)?)),
        Some(&"click") => Ok(Cmd::Click(target, num(1)?, num(2)?)),
        Some(&"leave") => Ok(Cmd::Leave(target)),
        Some(&"stats") => Ok(Cmd::Stats),
        Some(&"capture") => words
            .get(1)
            .map(|p| Cmd::Capture(target, (*p).to_owned()))
            .ok_or_else(|| format!("capture needs a path: {line:?}")),
        Some(&"quit") => Ok(Cmd::Quit),
        _ => Err(format!("unknown command {line:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        let cases: &[(&str, Result<Cmd, ()>)] = &[
            ("hover 1 2.5", Ok(Cmd::Hover(Target::Primary, 1.0, 2.5))),
            ("dock hover 3 4", Ok(Cmd::Hover(Target::Dock, 3.0, 4.0))),
            ("bar click 5 6", Ok(Cmd::Click(Target::Bar, 5.0, 6.0))),
            ("leave", Ok(Cmd::Leave(Target::Primary))),
            ("stats", Ok(Cmd::Stats)),
            (
                "capture /tmp/a.png",
                Ok(Cmd::Capture(Target::Primary, "/tmp/a.png".into())),
            ),
            ("quit", Ok(Cmd::Quit)),
            ("hover 1", Err(())),
            ("fly", Err(())),
        ];
        for (line, want) in cases {
            assert_eq!(parse(line).map_err(|_| ()), *want, "case {line:?}");
        }
    }
}
