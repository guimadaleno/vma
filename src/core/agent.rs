//! The agent inside a machine, as both ends decide it: what a request and
//! its reply hold, where a path given inside the machine leads, how a window
//! reads in a line, and which control a name means. The host's end and the
//! machine's are the shell's; this names no system and touches no file.

use crate::core::json::Json;

/// Which agent answers: the one running as the person signed in, or the
/// one running elevated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    User,
    Admin,
}

impl Role {
    pub fn word(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Admin => "admin",
        }
    }

    pub fn from_word(word: &str) -> Option<Role> {
        [Role::User, Role::Admin]
            .into_iter()
            .find(|r| r.word() == word)
    }
}

/// A path inside the machine, made whole: `%VAR%` and `$env:VAR` read from
/// `env`, `~` and a relative path taken from the person's folder,
/// `profile`. A path with a drive or a share stays as it is.
pub fn place(path: &str, profile: &str, env: impl Fn(&str) -> Option<String>) -> String {
    let mut text = expand_percent(path, &env);
    text = expand_dollar(&text, &env);
    let text = text.replace('/', "\\");
    if text == "~" {
        return profile.to_string();
    }
    if let Some(rest) = text.strip_prefix("~\\") {
        return join(profile, rest);
    }
    if is_rooted(&text) {
        return text;
    }
    join(profile, &text)
}

fn join(folder: &str, rest: &str) -> String {
    format!("{}\\{}", folder.trim_end_matches('\\'), rest)
}

/// Whether a Windows path starts at a drive or a share, `C:\…`, `C:`,
/// `\\server\…`, or the current drive's root, `\…`.
fn is_rooted(path: &str) -> bool {
    let b = path.as_bytes();
    path.starts_with('\\') || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
}

/// `%NAME%` replaced by the variable's value; a name with no variable stays
/// as written, as Windows leaves it.
fn expand_percent(text: &str, env: &impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        let Some(len) = after.find('%') else {
            break;
        };
        let name = &after[..len];
        out.push_str(&rest[..start]);
        match (!name.is_empty()).then(|| env(name)).flatten() {
            Some(value) => {
                out.push_str(&value);
                rest = &after[len + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `$env:NAME` replaced by the variable's value, or by nothing, as
/// PowerShell reads it; a name is letters, digits and `_`.
fn expand_dollar(text: &str, env: &impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.to_ascii_lowercase().find("$env:") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 5..];
        let len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        if len == 0 {
            out.push_str(&rest[start..start + 5]);
        } else {
            out.push_str(&env(&after[..len]).unwrap_or_default());
        }
        rest = &after[len..];
    }
    out.push_str(rest);
    out
}

/// A window as the agent sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub title: String,
    pub program: String,
    pub front: bool,
    pub minimized: bool,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

impl Window {
    /// The window on one line, as `vm windows` printed it: `>` for the one
    /// in front, its place and size, its program and its title.
    pub fn line(&self) -> String {
        let place = if self.minimized {
            "minimized".to_string()
        } else {
            format!("{},{} {}x{}", self.x, self.y, self.width, self.height)
        };
        format!(
            "{} {place:<22}{:<16}{}",
            if self.front { '>' } else { ' ' },
            self.program,
            self.title
        )
    }

    pub fn json(&self) -> Json {
        Json::object(vec![
            ("title", Json::text(&self.title)),
            ("program", Json::text(&self.program)),
            ("front", Json::Bool(self.front)),
            ("minimized", Json::Bool(self.minimized)),
            ("x", Json::number(self.x)),
            ("y", Json::number(self.y)),
            ("width", Json::number(self.width)),
            ("height", Json::number(self.height)),
        ])
    }

    pub fn from_json(json: &Json) -> Option<Window> {
        let text = |k: &str| json.get(k).and_then(Json::as_str).map(String::from);
        let flag = |k: &str| matches!(json.get(k), Some(Json::Bool(true)));
        let number = |k: &str| json.get(k).and_then(Json::as_i64);
        Some(Window {
            title: text("title")?,
            program: text("program")?,
            front: flag("front"),
            minimized: flag("minimized"),
            x: number("x")?,
            y: number("y")?,
            width: number("width")?,
            height: number("height")?,
        })
    }
}

/// A control the agent found on the screen: its name, and the middle of
/// its place.
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    pub name: String,
    pub x: i64,
    pub y: i64,
}

/// Where the controls a name may mean were found, in the order they are
/// looked through.
pub struct Found {
    /// The items of the menus open on the screen.
    pub menus: Vec<Control>,
    /// The controls of the window in front.
    pub front: Vec<Control>,
    /// The first control with the name whole, anywhere on the screen.
    pub anywhere: Option<Control>,
}

/// The control `name` means: one with the name whole in an open menu, in
/// the window in front, or anywhere; else the one in an open menu or the
/// window in front whose name starts with it, so a name that grows
/// ("Claude" to "Claude, Connected") still finds its control. Two names
/// that start so are named back, never guessed.
pub fn choose_control(name: &str, found: &Found) -> Result<(i64, i64), String> {
    let whole = |c: &&Control| c.name.to_lowercase() == name.to_lowercase();
    if let Some(c) = found
        .menus
        .iter()
        .find(whole)
        .or(found.front.iter().find(whole))
    {
        return Ok((c.x, c.y));
    }
    if let Some(c) = &found.anywhere {
        return Ok((c.x, c.y));
    }
    let lower = name.to_lowercase();
    let starts: Vec<&Control> = found
        .menus
        .iter()
        .chain(&found.front)
        .filter(|c| c.name.to_lowercase().starts_with(&lower))
        .collect();
    let mut unique: Vec<&str> = Vec::new();
    for c in &starts {
        if !unique.contains(&c.name.as_str()) {
            unique.push(&c.name);
        }
    }
    match unique[..] {
        [] => Err(format!("no control named '{name}' is on the screen")),
        [_] => Ok((starts[0].x, starts[0].y)),
        _ => Err(format!(
            "the names of several controls start with '{name}': {}",
            unique
                .iter()
                .map(|n| format!("'{n}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// A file's or a stream's bytes as text: UTF-8 with or without its mark,
/// or UTF-16 with its mark, as Windows' own tools write them (PowerShell
/// 5.1's `Out-File` and `>` write UTF-16).
pub fn text_of(bytes: &[u8]) -> String {
    let utf16 = |rest: &[u8], unit: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = rest.as_chunks::<2>().0.iter().map(|c| unit(*c)).collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// A path quoted for PowerShell in single quotes: each of the quotes it
/// takes as one (`'`, and the typographic `‘ ’ ‚ ‛`) doubled.
pub fn single_quoted(text: &str) -> String {
    let mut out = String::from("'");
    for c in text.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// A file name the host takes from a folder it reads, as one name alone,
/// never a path: not empty, no separator, not `.` or `..`.
pub fn plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['\\', '/', ':'])
}

/// A control's name as UI Automation gives it, without the shortcut a
/// menu item's carries after a tab ("Save\tCtrl+S").
pub fn control_name(name: &str) -> &str {
    name.split('\t').next().unwrap_or("")
}

/// A menu item's text as a person reads it: `&` marks a key and goes, `&&`
/// is an `&`, and a tab starts the shortcut after it, which goes too.
pub fn menu_name(text: &str) -> String {
    let text = text.split('\t').next().unwrap_or("");
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                chars.next();
                out.push('&');
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> Option<String> {
        match name.to_ascii_uppercase().as_str() {
            "USERPROFILE" => Some(r"C:\Users\dev".into()),
            "TEMP" => Some(r"C:\Users\dev\AppData\Local\Temp".into()),
            "APPDATA" => Some(r"C:\Users\dev\AppData\Roaming".into()),
            _ => None,
        }
    }

    const HOME: &str = r"C:\Users\dev";

    #[test]
    fn a_path_is_made_whole_from_the_persons_folder() {
        assert_eq!(
            place(r"Desktop\a.txt", HOME, env),
            r"C:\Users\dev\Desktop\a.txt"
        );
        assert_eq!(
            place("Desktop/a.txt", HOME, env),
            r"C:\Users\dev\Desktop\a.txt"
        );
        assert_eq!(place("~", HOME, env), HOME);
        assert_eq!(place(r"~\x", HOME, env), r"C:\Users\dev\x");
        assert_eq!(place(r"C:\Windows\x.txt", HOME, env), r"C:\Windows\x.txt");
        assert_eq!(place(r"\\server\share\f", HOME, env), r"\\server\share\f");
        assert_eq!(
            place(r"%TEMP%\f", HOME, env),
            r"C:\Users\dev\AppData\Local\Temp\f"
        );
        assert_eq!(
            place(r"$env:APPDATA\f", HOME, env),
            r"C:\Users\dev\AppData\Roaming\f"
        );
        assert_eq!(place(r"%NOPE%\f", HOME, env), r"C:\Users\dev\%NOPE%\f");
        assert_eq!(place(r"50% off\f", HOME, env), r"C:\Users\dev\50% off\f");
        assert_eq!(place(r"$env:NOPE\f", HOME, env), r"\f");
    }

    #[test]
    fn a_window_reads_as_vm_printed_it() {
        let w = Window {
            title: "Untitled - Notepad".into(),
            program: "notepad".into(),
            front: true,
            minimized: false,
            x: 10,
            y: 20,
            width: 800,
            height: 600,
        };
        assert_eq!(
            w.line(),
            "> 10,20 800x600         notepad         Untitled - Notepad"
        );
        assert_eq!(Window::from_json(&w.json()), Some(w.clone()));
        let hidden = Window {
            front: false,
            minimized: true,
            ..w
        };
        assert!(hidden.line().starts_with("  minimized "));
    }

    fn c(name: &str, x: i64) -> Control {
        Control {
            name: name.into(),
            x,
            y: 1,
        }
    }

    #[test]
    fn a_name_means_its_control_whole_before_one_that_starts_with_it() {
        let found = Found {
            menus: vec![c("Save", 1), c("Save As...", 2)],
            front: vec![c("File", 3), c("Save", 4)],
            anywhere: None,
        };
        assert_eq!(choose_control("save", &found), Ok((1, 1)));
        assert_eq!(choose_control("File", &found), Ok((3, 1)));
        assert_eq!(choose_control("Save As", &found), Ok((2, 1)));
        assert_eq!(
            choose_control("Sav", &found),
            Err("the names of several controls start with 'Sav': 'Save', 'Save As...'".into())
        );
        assert_eq!(
            choose_control("Nope", &found),
            Err("no control named 'Nope' is on the screen".into())
        );
        let elsewhere = Found {
            menus: vec![],
            front: vec![],
            anywhere: Some(c("Recycle Bin", 9)),
        };
        assert_eq!(choose_control("Recycle Bin", &elsewhere), Ok((9, 1)));
    }

    #[test]
    fn text_is_read_with_or_without_its_mark() {
        assert_eq!(text_of(b"plain"), "plain");
        assert_eq!(text_of(b"\xEF\xBB\xBFmarked"), "marked");
        assert_eq!(text_of(&[0xFF, 0xFE, b'h', 0, b'i', 0]), "hi");
        assert_eq!(text_of(&[0xFE, 0xFF, 0, b'h', 0, b'i']), "hi");
        assert_eq!(text_of("é".as_bytes()), "é");
    }

    #[test]
    fn a_path_is_quoted_for_powershell() {
        assert_eq!(single_quoted(r"C:\Temp\a.ps1"), r"'C:\Temp\a.ps1'");
        assert_eq!(single_quoted("O'Brien"), "'O''Brien'");
        assert_eq!(single_quoted("O\u{2019}Brien"), "'O\u{2019}\u{2019}Brien'");
    }

    #[test]
    fn a_name_from_a_folder_is_one_name() {
        assert!(plain_name("one.bin"));
        assert!(plain_name("pd"));
        for bad in ["", ".", "..", r"C:\Users", r"..\x", "a/b", "C:x"] {
            assert!(!plain_name(bad), "{bad}");
        }
    }

    #[test]
    fn a_controls_name_leaves_its_shortcut() {
        assert_eq!(control_name("Save\tCtrl+S"), "Save");
        assert_eq!(control_name("R&D"), "R&D");
    }

    #[test]
    fn menu_items_read_as_a_person_does() {
        assert_eq!(menu_name("&Save\tCtrl+S"), "Save");
        assert_eq!(menu_name("Save &As..."), "Save As...");
        assert_eq!(menu_name("R&&D"), "R&D");
    }

    #[test]
    fn roles_have_words() {
        assert_eq!(Role::from_word("admin"), Some(Role::Admin));
        assert_eq!(Role::User.word(), "user");
        assert_eq!(Role::from_word("root"), None);
    }
}
