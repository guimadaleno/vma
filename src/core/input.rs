//! What vma sends to a machine's mouse and keyboard: a key's scancodes, a
//! combo's and a text's, the steps of a click, a drag or a scroll with the
//! pauses a hand leaves between them, and how keys are split so the
//! machine's keyboard never drops one. It decides what goes and in what
//! order; sending it, and waiting, are the shell's.
//!
//! Keys are scancodes of set 1, as VirtualBox's `IKeyboard::putScancodes`
//! takes them; its PS/2 keyboard hands them to the machine in the set it
//! asked for (Windows asks for set 2).

/// A mouse button, by its bit in VirtualBox's `MouseButtonState`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Button {
    Left = 1,
    Right = 2,
    Middle = 4,
}

/// One thing sent to the machine, or a pause between two.
#[derive(Debug, PartialEq)]
pub enum Act {
    /// The pointer at a pixel of the screen, from 0, with `buttons` down (a
    /// sum of `Button`s) and the wheel turned `wheel` notches, down when
    /// positive.
    Mouse {
        x: u32,
        y: u32,
        buttons: u32,
        wheel: i32,
    },
    /// Scancodes, sent at the pace the machine reads them.
    Keys(Vec<u8>),
    /// Milliseconds.
    Pause(u32),
}

/// A key, by its scancode; an extended key's is 0xE000 and its code.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key(u16);

impl Key {
    fn down(self) -> Vec<u8> {
        self.codes(0)
    }

    fn up(self) -> Vec<u8> {
        self.codes(BREAK)
    }

    fn codes(self, flag: u8) -> Vec<u8> {
        let code = (self.0 & 0xFF) as u8 | flag;
        if self.0 >= 0xE000 {
            vec![EXTENDED, code]
        } else {
            vec![code]
        }
    }
}

/// A key's release is its code with this bit.
const BREAK: u8 = 0x80;
/// The code before an extended key's own.
const EXTENDED: u8 = 0xE0;
const SHIFT: Key = Key(0x2A);
const ENTER: Key = Key(0x1C);
const TAB: Key = Key(0x0F);

/// The keys of a US keyboard's rows, from the code of each row's first:
/// what each types alone, and with shift.
const ROWS: [(u16, &str, &str); 5] = [
    (0x02, "1234567890-=", "!@#$%^&*()_+"),
    (0x10, "qwertyuiop[]", "QWERTYUIOP{}"),
    (0x1E, "asdfghjkl;'`", "ASDFGHJKL:\"~"),
    (0x2B, "\\", "|"),
    (0x2C, "zxcvbnm,./", "ZXCVBNM<>?"),
];

/// Keys by name, as `keys` and `--hold` take them; `vma help` lists them.
const NAMED: [(&str, u16); 40] = [
    ("esc", 0x01),
    ("escape", 0x01),
    ("backspace", 0x0E),
    ("tab", 0x0F),
    ("enter", 0x1C),
    ("return", 0x1C),
    ("space", 0x39),
    ("capslock", 0x3A),
    ("ctrl", 0x1D),
    ("shift", 0x2A),
    ("alt", 0x38),
    ("f1", 0x3B),
    ("f2", 0x3C),
    ("f3", 0x3D),
    ("f4", 0x3E),
    ("f5", 0x3F),
    ("f6", 0x40),
    ("f7", 0x41),
    ("f8", 0x42),
    ("f9", 0x43),
    ("f10", 0x44),
    ("f11", 0x57),
    ("f12", 0x58),
    ("win", 0xE05B),
    ("altgr", 0xE038),
    ("menu", 0xE05D),
    ("insert", 0xE052),
    ("ins", 0xE052),
    ("delete", 0xE053),
    ("del", 0xE053),
    ("home", 0xE047),
    ("end", 0xE04F),
    ("pageup", 0xE049),
    ("pgup", 0xE049),
    ("pagedown", 0xE051),
    ("pgdn", 0xE051),
    ("up", 0xE048),
    ("down", 0xE050),
    ("left", 0xE04B),
    ("right", 0xE04D),
];

/// The key that types `c`, and whether shift goes with it.
fn char_key(c: char) -> Option<(Key, bool)> {
    if c == ' ' {
        return Some((Key(0x39), false));
    }
    ROWS.iter().find_map(|&(first, plain, shifted)| {
        let at = |row: &str| row.chars().position(|r| r == c);
        at(plain)
            .map(|i| (Key(first + i as u16), false))
            .or_else(|| at(shifted).map(|i| (Key(first + i as u16), true)))
    })
}

/// A key by its name, or by the character it types without shift.
pub fn key(name: &str) -> Result<Key, String> {
    let lower = name.to_lowercase();
    if let Some(&(_, code)) = NAMED.iter().find(|(n, _)| *n == lower) {
        return Ok(Key(code));
    }
    let mut chars = lower.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        match char_key(c) {
            Some((key, false)) => return Ok(key),
            Some((_, true)) => {
                return Err(format!(
                    "{name} is a shifted key: write shift and the key under it"
                ));
            }
            None => {}
        }
    }
    Err(format!("{name} is no key vma knows: vma help lists them"))
}

/// The keys of a combo such as `ctrl+shift+enter`, in the order they go
/// down.
pub fn combo(text: &str) -> Result<Vec<Key>, String> {
    text.split('+')
        .map(|name| {
            if name.is_empty() {
                Err(format!("{text} has an empty key"))
            } else {
                key(name)
            }
        })
        .collect()
}

/// The codes of combos pressed one after another: each one's keys go down
/// in order and up in reverse.
pub fn combos(texts: &[String]) -> Result<Vec<u8>, String> {
    let mut codes = Vec::new();
    for text in texts {
        let keys = combo(text)?;
        codes.extend(keys.iter().flat_map(|k| k.down()));
        codes.extend(keys.iter().rev().flat_map(|k| k.up()));
    }
    Ok(codes)
}

/// The codes that type `text` on a US keyboard: ASCII, with a new line as
/// enter and a tab as tab; a carriage return is left out.
pub fn typed(text: &str) -> Result<Vec<u8>, String> {
    let mut codes = Vec::new();
    for c in text.chars() {
        let (key, shifted) = match c {
            '\r' => continue,
            '\n' => (ENTER, false),
            '\t' => (TAB, false),
            _ => char_key(c).ok_or_else(|| {
                format!("{c:?} has no key on a US keyboard: type sends ASCII only")
            })?,
        };
        if shifted {
            codes.extend(SHIFT.down());
        }
        codes.extend(key.down());
        codes.extend(key.up());
        if shifted {
            codes.extend(SHIFT.up());
        }
    }
    Ok(codes)
}

fn keys_down(hold: &[Key]) -> Option<Act> {
    (!hold.is_empty()).then(|| Act::Keys(hold.iter().flat_map(|k| k.down()).collect()))
}

fn keys_up(hold: &[Key]) -> Option<Act> {
    (!hold.is_empty()).then(|| Act::Keys(hold.iter().rev().flat_map(|k| k.up()).collect()))
}

fn mouse(x: u32, y: u32, buttons: u32) -> Act {
    Act::Mouse {
        x,
        y,
        buttons,
        wheel: 0,
    }
}

/// A click, or two for a double one, at x, y. The pointer gets there
/// first, and the keys held go down before the press and up after the
/// release, as ctrl to add to a selection.
pub fn click(x: u32, y: u32, button: Button, double: bool, hold: &[Key]) -> Vec<Act> {
    let mut acts = vec![mouse(x, y, 0), Act::Pause(40)];
    acts.extend(keys_down(hold));
    for _ in 0..if double { 2 } else { 1 } {
        acts.extend([
            mouse(x, y, button as u32),
            Act::Pause(50),
            mouse(x, y, 0),
            Act::Pause(60),
        ]);
    }
    acts.extend(keys_up(hold));
    acts
}

/// The pointer to x, y with `button` down, or none.
pub fn move_to(x: u32, y: u32, button: Option<Button>) -> Vec<Act> {
    vec![mouse(x, y, button.map_or(0, |b| b as u32))]
}

/// A press at x, y, the button kept down.
pub fn press(x: u32, y: u32, button: Button) -> Vec<Act> {
    vec![mouse(x, y, 0), Act::Pause(60), mouse(x, y, button as u32)]
}

/// Every button up, at x, y.
pub fn release(x: u32, y: u32) -> Vec<Act> {
    vec![mouse(x, y, 0)]
}

/// Steps a drag glides through, one every 20 ms.
const GLIDE: u32 = 24;

/// Pressed a moment before moving, and moved in steps, as a hand drags: a
/// drag starts only once the pointer has left the press by a few pixels,
/// and a target takes it only after the pointer has rested on it. Keys
/// held, as ctrl to copy, go down before the press and up after the
/// release, so the program reads them all along.
pub fn drag(from: (u32, u32), to: (u32, u32), button: Button, hold: &[Key]) -> Vec<Act> {
    let b = button as u32;
    let mut acts = vec![mouse(from.0, from.1, 0), Act::Pause(60)];
    acts.extend(keys_down(hold));
    acts.extend([mouse(from.0, from.1, b), Act::Pause(120)]);
    let along = |a: u32, z: u32, i: u32| {
        (a as i64 + (z as i64 - a as i64) * i as i64 / GLIDE as i64) as u32
    };
    for i in 1..=GLIDE {
        acts.extend([
            mouse(along(from.0, to.0, i), along(from.1, to.1, i), b),
            Act::Pause(20),
        ]);
    }
    acts.extend([Act::Pause(150), mouse(to.0, to.1, 0), Act::Pause(60)]);
    acts.extend(keys_up(hold));
    acts
}

/// The wheel turned `notches` at x, y, one notch every 30 ms, down when
/// positive.
pub fn scroll(x: u32, y: u32, notches: i32) -> Vec<Act> {
    let mut acts = vec![mouse(x, y, 0), Act::Pause(40)];
    for i in 0..notches.unsigned_abs() {
        if i > 0 {
            acts.push(Act::Pause(30));
        }
        acts.push(Act::Mouse {
            x,
            y,
            buttons: 0,
            wheel: notches.signum(),
        });
    }
    acts
}

/// Whether every place a command was given is on a screen of `width` by
/// `height` pixels; nothing is sent where one is off it. The pointer only
/// passes between places given, so it stays on the screen with them.
pub fn on_screen(places: &[(u32, u32)], width: u32, height: u32) -> Result<(), String> {
    for &(x, y) in places {
        if x >= width || y >= height {
            return Err(if width == 0 || height == 0 {
                "the screen isn't set yet, as early in a machine's start".into()
            } else {
                format!(
                    "{x} {y} is off the screen, {width}x{height}: places go from 0 0 to {} {}",
                    width - 1,
                    height - 1
                )
            });
        }
    }
    Ok(())
}

/// The PS/2 keyboard's queue holds 63 bytes: a key goes in while it and
/// those waiting fit (VirtualBox's `ps2kR3InsertStrQueue`), and one that
/// doesn't is dropped with no word.
const QUEUE: usize = 63;
/// The most bytes a part of keys may take, so the part still on its way
/// to the queue and the next one fit in it together.
const PART: usize = QUEUE / 2;

/// The most bytes a key's press or release takes in the PS/2 queue, in
/// set 2, the larger: a release is F0 and its code, after E0 for an
/// extended key, and an arrow or an editing key may add a false shift's
/// press and release around it (VirtualBox's `ps2kR3ProcessKeyEvent`).
fn queue_bytes(extended: bool) -> usize {
    if extended { 7 } else { 2 }
}

/// Each key's press or release in `codes`: where it starts, how many
/// codes it takes (two for an extended key), and its key.
fn events(codes: &[u8]) -> impl Iterator<Item = (usize, usize, u16)> + '_ {
    let mut i = 0;
    std::iter::from_fn(move || {
        let at = i;
        let extended = *codes.get(at)? == EXTENDED && at + 1 < codes.len();
        let len = if extended { 2 } else { 1 };
        i += len;
        let key = u16::from(codes[at + len - 1] & !BREAK) | if extended { 0xE000 } else { 0 };
        Some((at, len, key))
    })
}

/// The most bytes `codes` take in the PS/2 queue.
fn cost(codes: &[u8]) -> usize {
    events(codes).map(|(_, len, _)| queue_bytes(len == 2)).sum()
}

/// `codes` in parts of at most `PART` bytes in the PS/2 queue, each ending
/// where no key is held, so a machine that takes only some parts is left
/// with no key down; only keys held longer than a part are split. An
/// extended key's two codes are never apart.
pub fn parts(codes: &[u8]) -> Vec<&[u8]> {
    let mut parts = Vec::new();
    let mut held: Vec<u16> = Vec::new();
    // Where the part being made starts, and the last place after it where
    // no key is held.
    let (mut start, mut free) = (0, 0);
    for (at, len, key) in events(codes) {
        if cost(&codes[start..at + len]) > PART {
            let end = if free > start { free } else { at };
            parts.push(&codes[start..end]);
            start = end;
        }
        if codes[at + len - 1] & BREAK != 0 {
            held.retain(|&k| k != key);
        } else if !held.contains(&key) {
            held.push(key);
        }
        if held.is_empty() {
            free = at + len;
        }
    }
    if start < codes.len() {
        parts.push(&codes[start..]);
    }
    parts
}

/// The bytes waiting in the PS/2 keyboard's queue, as VirtualBox's
/// debugger reports them (`info ps2k`: "Input queue  : 3 items (64 max)").
pub fn queued(info: &str) -> Option<u32> {
    info.lines()
        .find_map(|line| line.strip_prefix("Input queue"))
        .and_then(|rest| rest.split(':').nth(1))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_found_by_name_or_character() {
        assert_eq!(key("enter"), Ok(Key(0x1C)));
        assert_eq!(key("Ctrl"), Ok(Key(0x1D)));
        assert_eq!(key("a"), Ok(Key(0x1E)));
        assert_eq!(key("A"), Ok(Key(0x1E)));
        assert_eq!(key("/"), Ok(Key(0x35)));
        assert_eq!(key("`"), Ok(Key(0x29)));
        assert_eq!(key("win"), Ok(Key(0xE05B)));
        assert_eq!(
            key("!"),
            Err("! is a shifted key: write shift and the key under it".into())
        );
        assert_eq!(
            key("nope"),
            Err("nope is no key vma knows: vma help lists them".into())
        );
        assert!(key("é").is_err());
    }

    #[test]
    fn a_combo_goes_down_in_order_and_up_in_reverse() {
        assert_eq!(
            combos(&["ctrl+shift+enter".into()]),
            Ok(vec![0x1D, 0x2A, 0x1C, 0x9C, 0xAA, 0x9D])
        );
        assert_eq!(
            combos(&["win+r".into(), "tab".into()]),
            Ok(vec![0xE0, 0x5B, 0x13, 0x93, 0xE0, 0xDB, 0x0F, 0x8F])
        );
        assert_eq!(
            combos(&["ctrl+".into()]),
            Err("ctrl+ has an empty key".into())
        );
        assert!(combos(&["ctrl+nope".into()]).is_err());
    }

    #[test]
    fn text_is_typed_with_shift_where_it_takes_it() {
        assert_eq!(typed("a"), Ok(vec![0x1E, 0x9E]));
        assert_eq!(typed("A"), Ok(vec![0x2A, 0x1E, 0x9E, 0xAA]));
        assert_eq!(
            typed("\r\n\t "),
            Ok(vec![0x1C, 0x9C, 0x0F, 0x8F, 0x39, 0xB9])
        );
        assert_eq!(typed("~"), Ok(vec![0x2A, 0x29, 0xA9, 0xAA]));
        assert_eq!(
            typed("é"),
            Err("'é' has no key on a US keyboard: type sends ASCII only".into())
        );
    }

    #[test]
    fn every_printable_ascii_character_has_a_key() {
        for c in ' '..='~' {
            assert!(typed(&c.to_string()).is_ok(), "{c}");
        }
    }

    #[test]
    fn a_click_holds_its_keys_around_the_press() {
        let ctrl = key("ctrl").unwrap();
        let acts = click(5, 6, Button::Right, false, &[ctrl]);
        assert_eq!(acts[0], mouse(5, 6, 0));
        assert_eq!(acts[2], Act::Keys(vec![0x1D]));
        assert_eq!(acts[3], mouse(5, 6, 2));
        assert_eq!(acts.last(), Some(&Act::Keys(vec![0x9D])));
        let presses = |acts: &[Act]| {
            acts.iter()
                .filter(|a| matches!(a, Act::Mouse { buttons: 1, .. }))
                .count()
        };
        assert_eq!(presses(&click(1, 1, Button::Left, true, &[])), 2);
        assert_eq!(presses(&click(1, 1, Button::Left, false, &[])), 1);
    }

    #[test]
    fn a_drag_glides_to_its_end_with_the_button_down() {
        let acts = drag((100, 10), (4, 58), Button::Left, &[]);
        let moves: Vec<_> = acts
            .iter()
            .filter_map(|a| match a {
                Act::Mouse { x, y, buttons, .. } => Some((*x, *y, *buttons)),
                _ => None,
            })
            .collect();
        assert_eq!(moves[0], (100, 10, 0));
        assert_eq!(moves[1], (100, 10, 1));
        assert_eq!(moves[GLIDE as usize + 1], (4, 58, 1));
        assert_eq!(moves.last(), Some(&(4, 58, 0)));
        assert_eq!(moves.len(), GLIDE as usize + 3);
    }

    #[test]
    fn a_scroll_turns_one_notch_at_a_time() {
        let wheel: Vec<i32> = scroll(3, 4, -3)
            .iter()
            .filter_map(|a| match a {
                Act::Mouse { wheel, .. } => Some(*wheel),
                _ => None,
            })
            .collect();
        assert_eq!(wheel, vec![0, -1, -1, -1]);
    }

    #[test]
    fn places_off_the_screen_are_refused_before_anything_goes() {
        assert_eq!(on_screen(&[(1023, 767)], 1024, 768), Ok(()));
        assert_eq!(
            on_screen(&[(1024, 0)], 1024, 768),
            Err("1024 0 is off the screen, 1024x768: places go from 0 0 to 1023 767".into())
        );
        assert_eq!(
            on_screen(&[(10, 10), (1100, 10)], 1024, 768),
            Err("1100 10 is off the screen, 1024x768: places go from 0 0 to 1023 767".into())
        );
        assert!(on_screen(&[], 0, 0).is_ok());
        assert_eq!(
            on_screen(&[(0, 0)], 0, 0),
            Err("the screen isn't set yet, as early in a machine's start".into())
        );
    }

    /// Whether some key is still down at the end of `codes`.
    fn holds(codes: &[u8]) -> bool {
        let mut held = Vec::new();
        for (at, len, key) in events(codes) {
            if codes[at + len - 1] & BREAK != 0 {
                held.retain(|&k| k != key);
            } else {
                held.push(key);
            }
        }
        !held.is_empty()
    }

    #[test]
    fn parts_fit_the_queue_and_end_with_no_key_down() {
        let codes = typed(&"x".repeat(40)).unwrap();
        let parts = parts(&codes);
        assert_eq!(parts.concat(), codes);
        assert!(parts.iter().all(|p| cost(p) <= PART));
        assert_eq!(parts[0].len(), 14);
        let codes = typed("ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghij").unwrap();
        let parts = super::parts(&codes);
        assert_eq!(parts.concat(), codes);
        assert!(parts.len() > 3);
        for part in &parts {
            assert!(cost(part) <= PART);
            assert!(!holds(part), "{part:x?}");
        }
        assert!(super::parts(&[]).is_empty());
    }

    #[test]
    fn extended_keys_stay_whole_and_long_holds_are_split() {
        let arrows = combos(&vec!["down".to_string(); 5]).unwrap();
        for part in parts(&arrows) {
            assert_eq!(part[0], EXTENDED);
            assert!(cost(part) <= PART);
        }
        let long = combos(&["ctrl+shift+alt+a+b+c+d+e+f+g+h+i+j+k+l+m".into()]).unwrap();
        let split = parts(&long);
        assert_eq!(split.concat(), long);
        assert!(split.len() > 1);
        assert!(split.iter().all(|p| cost(p) <= PART));
        assert_eq!(cost(&[EXTENDED, 0x50, 0x1E]), 9);
    }

    #[test]
    fn the_queue_is_read_from_the_debuggers_words() {
        let info = "PS/2 Keyboard: scan set 2, scanning enabled, serial line enabled\n\
                    Command queue: 0 items (4 max)\n\
                    Input queue  : 47 items (64 max)\n";
        assert_eq!(queued(info), Some(47));
        assert_eq!(queued("nothing"), None);
    }
}
