//! Running the tools the reviewer already has.
//!
//! Prchum never stores credentials; it borrows the CLIs you have already
//! authenticated — `git`, `gh`, `glab`, `fj`. That works because the app
//! and those tools live in the same world.
//!
//! Inside a Flatpak they do not. The sandbox has its own filesystem and
//! its own `PATH`, and none of the host's tools are on it, so a plain
//! `Command::new("git")` fails with "no such file". The portal's answer
//! is `flatpak-spawn --host`, which runs the program outside the
//! sandbox. Everything that shells out goes through here so that
//! decision is made once.

use std::path::Path;
use std::process::Command;

/// True when the process is inside a Flatpak sandbox.
///
/// The runtime writes this file into every sandbox; its presence is the
/// documented way to ask.
pub fn sandboxed() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// How to invoke `program` so that it runs on the host: the executable
/// to spawn, and the arguments that must precede the program's own.
///
/// Split out from [`command`] so the decision can be tested without a
/// sandbox to run in.
pub fn host_invocation(program: &str, sandboxed: bool) -> (String, Vec<String>) {
    if sandboxed {
        (
            "flatpak-spawn".to_string(),
            vec!["--host".to_string(), program.to_string()],
        )
    } else {
        (program.to_string(), Vec::new())
    }
}

/// A [`Command`] that runs `program` on the host, sandboxed or not.
pub fn command(program: &str) -> Command {
    let (executable, leading) = host_invocation(program, sandboxed());
    let mut command = Command::new(executable);
    command.args(leading);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_a_sandbox_the_program_is_the_program() {
        let (executable, leading) = host_invocation("git", false);
        assert_eq!(executable, "git");
        assert!(leading.is_empty());
    }

    #[test]
    fn inside_one_it_goes_through_the_portal() {
        let (executable, leading) = host_invocation("gh", true);
        assert_eq!(executable, "flatpak-spawn");
        assert_eq!(leading, vec!["--host".to_string(), "gh".to_string()]);
    }

    #[test]
    fn the_programs_own_arguments_still_follow() {
        // The caller appends its arguments after these, so an invocation
        // reads `flatpak-spawn --host git -C /repo status`. Getting the
        // order wrong would hand git's arguments to flatpak-spawn.
        let (_, leading) = host_invocation("git", true);
        let full: Vec<String> = leading
            .into_iter()
            .chain(["-C".to_string(), "/repo".to_string()])
            .collect();
        assert_eq!(full, vec!["--host", "git", "-C", "/repo"]);
    }
}
