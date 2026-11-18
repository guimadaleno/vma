//! vma: one executable to use a VirtualBox machine as a person at it would.
//! This reads the command line and hands the command to the platform's
//! shell; it decides only what a failure prints and the exit code, 125 for
//! vma's own, so a command's own codes stay apart from it.

// Away from Windows only the core's tests use the core, until #1 brings a
// shell there.
#![cfg_attr(not(windows), allow(dead_code))]

mod core;
#[cfg(windows)]
mod win;

use crate::core::command::{self, Command};
use std::process::ExitCode;

fn main() -> ExitCode {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let done = command::parse(&words).and_then(|parsed| match parsed {
        None => {
            print!("{}", command::USAGE);
            Ok(0)
        }
        Some((Command::Notices, _)) => {
            print!("{}", command::NOTICES);
            Ok(0)
        }
        Some((command, machine)) => run(command, machine),
    });
    match done {
        Ok(0) => ExitCode::SUCCESS,
        // A script's own code, which may be any a process has.
        Ok(code) => std::process::exit(code),
        Err(why) => {
            eprintln!("vma: {why}");
            ExitCode::from(125)
        }
    }
}

#[cfg(windows)]
fn run(command: Command, machine: Option<String>) -> Result<i32, String> {
    win::run(command, machine.as_deref())
}

// Linux and macOS hosts are #1; until then the core builds and is tested
// there, and vma says plainly that it cannot reach VirtualBox.
#[cfg(not(windows))]
fn run(_: Command, _: Option<String>) -> Result<i32, String> {
    Err("VirtualBox is reached from a Windows host only, for now".into())
}
