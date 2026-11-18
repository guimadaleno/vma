//! What vma decides of an install with no person, from what VirtualBox read
//! on its ISO: whether it holds a system vma installs, which of its images is
//! meant, which files in a machine's folder the install made, and when a
//! restart of the install has hung. Reading the ISO, making and deleting the
//! install's files, and looking at the machine, are VirtualBox's.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Duration;

/// Whether the system VirtualBox found on `iso`, whose guest OS type is
/// `os`, is one vma installs: Windows only, as only Windows runs what vma
/// does inside a machine.
pub fn check_system(iso: &str, os: &str) -> Result<(), String> {
    if os.starts_with("Windows") {
        Ok(())
    } else if os.is_empty() {
        Err(format!(
            "{iso} holds no system VirtualBox knows, and vma installs Windows"
        ))
    } else {
        Err(format!("{iso} holds {os}, and vma installs only Windows"))
    }
}

/// The image of the ISO to install: the one `asked`, or VirtualBox's 1, if
/// it is among `images`, each one's number and name. An ISO that lists none,
/// as an older Windows, has only one.
pub fn image(asked: Option<u32>, images: &[(u32, String)]) -> Result<u32, String> {
    let index = asked.unwrap_or(1);
    if images.is_empty() || images.iter().any(|(i, _)| *i == index) {
        return Ok(index);
    }
    let listed: Vec<String> = images
        .iter()
        .map(|(i, name)| format!("{i} {name}"))
        .collect();
    Err(format!(
        "the ISO has no image {index}; its images are {}",
        listed.join(", ")
    ))
}

/// The account VirtualBox makes where none is named.
pub const DEFAULT_USER: &str = "vboxuser";

/// The name Windows gives the computer of a machine named `machine`, as
/// VirtualBox makes it (UnattendedImpl.cpp and UnattendedScript.cpp,
/// 7.2.8): its ASCII letters and digits, punctuation between them as one
/// `-`, `-vm` after a name of under 3, cut to 15. `None` where it keeps
/// nothing, as VirtualBox then names it after the machine's ID.
pub fn computer_name(machine: &str) -> Option<String> {
    let mut name = String::new();
    for c in machine.chars() {
        if c.is_ascii_alphanumeric() {
            name.push(c);
        } else if !name.is_empty() && c.is_ascii_punctuation() && !name.ends_with('-') {
            name.push('-');
        }
    }
    if name.is_empty() {
        return None;
    }
    if name.len() < 3 {
        name.push_str("-vm");
    }
    name.truncate(15);
    Some(name)
}

/// Whether `user` may be the account on a machine named `machine`: not the
/// computer's name, for which Windows makes the account but leaves it out
/// of Administrators, so nothing on the machine can be elevated, its Guest
/// Additions' install included.
pub fn check_user(user: &str, machine: &str) -> Result<(), String> {
    match computer_name(machine) {
        Some(computer) if computer.eq_ignore_ascii_case(user) => Err(format!(
            "--user can't be {user}, the name Windows gives the computer: Windows makes the account then, but not an administrator"
        )),
        _ => Ok(()),
    }
}

/// How long a restart's screen stays the same, with no byte read or written
/// by the machine's disks, before it counts as hung: the hangs seen lasted
/// until a reset; a restart that goes on moves its disks or its screen, a
/// spinner's dots, within seconds.
pub const HUNG_AFTER: Duration = Duration::from_secs(120);

/// The resets a wait gives an install before it takes the machine for stuck
/// another way, as on an error Setup shows: more than an install's restarts.
pub const MAX_RESETS: u32 = 6;

/// Watches an install for a restart that hung, as most restarts Windows
/// asked for did under VirtualBox's NEM mode, Hyper-V holding VT-x (7.2.8):
/// Windows shut its devices down and its processors spun, the screen frozen,
/// black or on "Restarting", until the machine was reset, and then the
/// install went on.
#[derive(Default)]
pub struct Restarts {
    /// Since when, in the wait's time, nothing has moved, and the disks'
    /// bytes and the screen's picture then.
    still: Option<(Duration, u64, u64)>,
    /// The resets so far.
    resets: u32,
}

impl Restarts {
    /// After a look at `at`, the wait's time, that saw the disks' bytes so
    /// far, `bytes`, and the screen's `picture`: whether the restart hung,
    /// both the same for `HUNG_AFTER`.
    pub fn hung(&mut self, at: Duration, bytes: u64, picture: u64) -> bool {
        match self.still {
            Some((since, b, p)) if b == bytes && p == picture => at - since >= HUNG_AFTER,
            _ => {
                self.still = Some((at, bytes, picture));
                false
            }
        }
    }

    /// Forgets what it saw, after a reset, and counts it.
    pub fn reset(&mut self) {
        self.still = None;
        self.resets += 1;
    }

    /// Forgets what it saw, after a look that saw nothing.
    pub fn forget(&mut self) {
        self.still = None;
    }

    /// Whether it has reset the machine `MAX_RESETS` times, so a hang seen
    /// again is not a restart's.
    pub fn gave_up(&self) -> bool {
        self.resets >= MAX_RESETS
    }
}

/// The bytes all disks of a machine read and wrote, from VirtualBox's
/// statistics of them, an XML whose counters each hold theirs in `c`.
pub fn storage_bytes(stats: &str) -> u64 {
    stats
        .split(" c=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next()?.parse::<u64>().ok())
        .sum()
}

/// A screen's pixels, read down to a number that tells one picture from
/// another.
pub fn picture(pixels: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    pixels.hash(&mut hasher);
    hasher.finish()
}

/// Whether `file`, in the folder of the machine whose ID is `id`, is one an
/// install made there: VirtualBox names them `Unattended-<ID>-`.
pub fn is_install_file(file: &str, id: &str) -> bool {
    let prefix = format!("Unattended-{id}-");
    file.len() > prefix.len()
        && file
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(&prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_windows_is_installed() {
        assert_eq!(check_system("w.iso", "Windows11_64"), Ok(()));
        assert_eq!(
            check_system("u.iso", "Ubuntu_64"),
            Err("u.iso holds Ubuntu_64, and vma installs only Windows".into())
        );
        assert_eq!(
            check_system("a.iso", ""),
            Err("a.iso holds no system VirtualBox knows, and vma installs Windows".into())
        );
    }

    #[test]
    fn an_image_is_one_the_iso_has() {
        let images = [
            (1, "Windows 11 Enterprise LTSC Evaluation".to_string()),
            (2, "Windows 11 Enterprise N LTSC Evaluation".to_string()),
        ];
        assert_eq!(image(None, &images), Ok(1));
        assert_eq!(image(Some(2), &images), Ok(2));
        assert_eq!(
            image(Some(9), &images),
            Err("the ISO has no image 9; its images are 1 Windows 11 Enterprise LTSC Evaluation, 2 Windows 11 Enterprise N LTSC Evaluation".into())
        );
        assert_eq!(image(Some(3), &[]), Ok(3));
    }

    #[test]
    fn the_computer_is_named_as_virtualbox_names_it() {
        assert_eq!(computer_name("LAB-PC").as_deref(), Some("LAB-PC"));
        assert_eq!(computer_name("t1").as_deref(), Some("t1-vm"));
        assert_eq!(computer_name("teste ção").as_deref(), Some("testeo"));
        assert_eq!(computer_name("My..PC!").as_deref(), Some("My-PC-"));
        assert_eq!(
            computer_name("a very long machine name").as_deref(),
            Some("averylongmachin")
        );
        assert_eq!(computer_name("-- çã"), None);
    }

    #[test]
    fn a_restart_hangs_once_nothing_moves_long_enough() {
        let s = Duration::from_secs;
        let black = picture(&[0; 8]);
        let restarting = picture(&[0, 0, 0, 0, 255, 255, 255, 0]);
        let mut r = Restarts::default();
        assert!(!r.hung(s(0), 100, black));
        assert!(!r.hung(s(119), 100, black));
        assert!(r.hung(s(120), 100, black));
        r.reset();
        assert!(!r.hung(s(130), 100, black));
        // The disks moving, or the screen changing, starts over.
        assert!(!r.hung(s(250), 101, black));
        assert!(!r.hung(s(300), 101, restarting));
        assert!(!r.hung(s(419), 101, restarting));
        assert!(r.hung(s(420), 101, restarting));
    }

    #[test]
    fn it_gives_up_after_its_resets() {
        let mut r = Restarts::default();
        for _ in 1..MAX_RESETS {
            r.reset();
        }
        assert!(!r.gave_up());
        r.forget();
        assert!(!r.gave_up());
        r.reset();
        assert!(r.gave_up());
    }

    #[test]
    fn storage_bytes_add_up_every_counter() {
        let stats = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Statistics>
<Counter c="5608806400" unit="bytes" vis="used" name="/Public/Storage/AHCI0/Port0/BytesRead"/>
<Counter c="1285053440" unit="bytes" vis="used" name="/Public/Storage/AHCI0/Port1/BytesRead"/>
<Counter c="10211771904" unit="bytes" vis="used" name="/Public/Storage/AHCI0/Port1/BytesWritten"/>
</Statistics>"#;
        assert_eq!(storage_bytes(stats), 5608806400 + 1285053440 + 10211771904);
        assert_eq!(storage_bytes("<Statistics/>"), 0);
    }

    #[test]
    fn pictures_tell_screens_apart() {
        assert_eq!(picture(&[1, 2, 3, 4]), picture(&[1, 2, 3, 4]));
        assert_ne!(picture(&[1, 2, 3, 4]), picture(&[1, 2, 3, 5]));
    }

    #[test]
    fn install_files_are_named_after_the_machine() {
        let id = "0b5e7c2a-1111-2222-3333-444455556666";
        assert!(is_install_file(
            "Unattended-0b5e7c2a-1111-2222-3333-444455556666-autounattend.xml",
            id
        ));
        assert!(is_install_file(
            "unattended-0B5E7C2A-1111-2222-3333-444455556666-aux-iso.viso",
            id
        ));
        assert!(!is_install_file(
            "Unattended-ffffffff-1111-2222-3333-444455556666-autounattend.xml",
            id
        ));
        assert!(!is_install_file(
            "Unattended-0b5e7c2a-1111-2222-3333-444455556666-",
            id
        ));
        assert!(!is_install_file("t1.vbox", id));
    }
}
