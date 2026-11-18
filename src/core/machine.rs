//! What VirtualBox says of a machine, in vma's words, and which machine a
//! call means when none is named. The states and their numbers are
//! VirtualBox's `MachineState` (VirtualBox.xidl, 7.2.8); their words are
//! VBoxManage's short ones, which base's `vm` printed, so what an agent
//! learned there still reads the same.

/// VirtualBox's `MachineState_FirstOnline` and `_LastOnline`: between them
/// the machine has a console, its screen, mouse and keyboard.
const FIRST_ONLINE: u32 = 6;
const LAST_ONLINE: u32 = 19;
/// `MachineState_Running`.
pub const RUNNING: u32 = 6;
const PAUSED: u32 = 7;

pub fn is_online(state: u32) -> bool {
    (FIRST_ONLINE..=LAST_ONLINE).contains(&state)
}

pub fn state_word(state: u32) -> &'static str {
    match state {
        1 => "poweroff",
        2 => "saved",
        3 => "teleported",
        4 => "aborted",
        5 => "aborted-saved",
        6 => "running",
        7 => "paused",
        8 => "gurumeditation",
        9 => "teleporting",
        10 => "livesnapshotting",
        11 => "starting",
        12 => "stopping",
        13 => "saving",
        14 => "restoring",
        15 => "teleportingpausedvm",
        16 => "teleportingin",
        17 => "deletingsnapshotlive",
        18 => "deletingsnapshotlivepaused",
        19 => "onlinesnapshotting",
        20 => "restoringsnapshot",
        21 => "deletingsnapshot",
        22 => "settingup",
        23 => "snapshotting",
        _ => "unknown",
    }
}

/// What a command does to the machine it means, which decides whether vma
/// may guess which one that is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Use {
    /// It only reads the machine, as status and a shot do: the one running
    /// is guessed.
    Read,
    /// It changes the machine: guessed only where it is the only one, as
    /// another there may be someone else's.
    Change,
}

/// The machine a call means when it names none: the only one there is,
/// or, for a call that only reads, the only one online. `machines` holds
/// each one's name and state; the answer is an index into it. `ask` says
/// how a call names one, as "name one with --machine NAME or VMA_NAME".
pub fn choose(machines: &[(String, u32)], how: Use, ask: &str) -> Result<usize, String> {
    let online: Vec<usize> = (0..machines.len())
        .filter(|&i| is_online(machines[i].1))
        .collect();
    let names = |indexes: &mut dyn Iterator<Item = usize>| {
        indexes
            .map(|i| machines[i].0.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    match (online.len(), machines.len()) {
        (_, 0) => Err("VirtualBox has no machine".into()),
        (_, 1) => Ok(0),
        _ if how == Use::Change => Err(format!(
            "this changes a machine, and there are several ({}): {ask}",
            names(&mut (0..machines.len()))
        )),
        (1, _) => Ok(online[0]),
        (0, _) => Err(format!(
            "no machine is running, and there are several ({}): {ask}",
            names(&mut (0..machines.len()))
        )),
        _ => Err(format!(
            "several machines are running ({}): {ask}",
            names(&mut online.into_iter())
        )),
    }
}

/// Whether a machine in `state` takes the mouse and keyboard. A paused
/// one holds a few dozen keys and gives them all at once when it goes on,
/// so none is sent to it.
pub fn takes_input(name: &str, state: u32) -> Result<(), String> {
    match state {
        RUNNING => Ok(()),
        PAUSED => Err(format!("{name} is paused: vma start lets it go on")),
        _ => Err(format!(
            "{name} is {}: only a running machine takes input",
            state_word(state)
        )),
    }
}

/// VirtualBox's `KeyboardHIDType`s.
const NO_KEYBOARD: u32 = 1;
const PS2_KEYBOARD: u32 = 2;
const USB_KEYBOARD: u32 = 3;

/// Whether vma types on a machine whose keyboard is the `KeyboardHIDType`
/// `kind`. It paces keys by the PS/2 keyboard's queue, VirtualBox's
/// default, and VirtualBox sends them to a USB keyboard instead where the
/// machine has one, whose queue vma can't read.
pub fn types_on(name: &str, kind: u32) -> Result<(), String> {
    let other = match kind {
        PS2_KEYBOARD => return Ok(()),
        NO_KEYBOARD => "none",
        USB_KEYBOARD => "USB",
        _ => "USB and PS/2",
    };
    Err(format!(
        "{name}'s keyboard is {other}, and vma types only on a PS/2 one, whose queue \
         tells how fast the machine reads: VBoxManage's modifyvm --keyboard ps2, \
         while it is off"
    ))
}

/// A disk's size, in whole GB where it is one, as disks are made, and to a
/// tenth otherwise.
pub fn size_words(bytes: i64) -> String {
    const GB: i64 = 1 << 30;
    if bytes % GB == 0 {
        format!("{} GB", bytes / GB)
    } else {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASK: &str = "name one with --machine NAME or VMA_NAME";

    fn machines(list: &[(&str, u32)]) -> Vec<(String, u32)> {
        list.iter().map(|&(n, s)| (n.to_string(), s)).collect()
    }

    #[test]
    fn online_is_between_virtualbox_bounds() {
        assert!(!is_online(1)); // poweroff
        assert!(is_online(6)); // running
        assert!(is_online(7)); // paused
        assert!(is_online(19)); // onlinesnapshotting
        assert!(!is_online(20)); // restoringsnapshot, offline
    }

    #[test]
    fn the_one_running_is_meant_for_a_read() {
        let m = machines(&[("a", 1), ("b", 6), ("c", 2)]);
        assert_eq!(choose(&m, Use::Read, ASK), Ok(1));
    }

    #[test]
    fn the_only_machine_is_meant_even_off() {
        for how in [Use::Read, Use::Change] {
            assert_eq!(choose(&machines(&[("a", 1)]), how, ASK), Ok(0));
            assert_eq!(choose(&machines(&[("a", 6)]), how, ASK), Ok(0));
        }
    }

    #[test]
    fn a_change_never_guesses_among_several() {
        assert_eq!(
            choose(&machines(&[("a", 1), ("b", 6)]), Use::Change, ASK),
            Err("this changes a machine, and there are several (a, b): name one with --machine NAME or VMA_NAME".into())
        );
        assert_eq!(
            choose(
                &machines(&[("a", 1), ("b", 2)]),
                Use::Change,
                "give machine"
            ),
            Err("this changes a machine, and there are several (a, b): give machine".into())
        );
    }

    #[test]
    fn otherwise_a_name_is_asked_for() {
        for how in [Use::Read, Use::Change] {
            assert_eq!(
                choose(&[], how, ASK),
                Err("VirtualBox has no machine".into())
            );
        }
        assert_eq!(
            choose(&machines(&[("a", 1), ("b", 2)]), Use::Read, ASK),
            Err("no machine is running, and there are several (a, b): name one with --machine NAME or VMA_NAME".into())
        );
        assert_eq!(
            choose(
                &machines(&[("a", 6), ("b", 1), ("c", 7)]),
                Use::Read,
                "give machine"
            ),
            Err("several machines are running (a, c): give machine".into())
        );
    }

    #[test]
    fn only_a_running_machine_takes_input() {
        assert_eq!(takes_input("a", 6), Ok(()));
        assert_eq!(
            takes_input("a", 7),
            Err("a is paused: vma start lets it go on".into())
        );
        assert_eq!(
            takes_input("a", 8),
            Err("a is gurumeditation: only a running machine takes input".into())
        );
    }

    #[test]
    fn keys_go_to_a_ps2_keyboard_only() {
        assert_eq!(types_on("a", 2), Ok(()));
        assert_eq!(
            types_on("a", 3),
            Err("a's keyboard is USB, and vma types only on a PS/2 one, whose queue tells how fast the machine reads: VBoxManage's modifyvm --keyboard ps2, while it is off".into())
        );
        assert!(types_on("a", 4).unwrap_err().contains("USB and PS/2"));
        assert!(types_on("a", 1).unwrap_err().contains("is none"));
    }

    #[test]
    fn sizes_read_in_gb() {
        assert_eq!(size_words(8 << 30), "8 GB");
        assert_eq!(size_words(80 << 30), "80 GB");
        assert_eq!(size_words(3 << 29), "1.5 GB");
    }

    #[test]
    fn states_read_as_vboxmanage_says_them() {
        assert_eq!(state_word(1), "poweroff");
        assert_eq!(state_word(8), "gurumeditation");
        assert_eq!(state_word(99), "unknown");
    }
}
