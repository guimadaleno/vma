//! The words on vma's command line, read into the command they ask for. It
//! checks their shape (how many, which options, numbers in range, a mode
//! among its words); whether a machine or a file is there is the shell's to
//! find out, and whether VirtualBox takes a value is VirtualBox's.

use crate::core::agent::Role;
use crate::core::input::{self, Act, Button};
use crate::core::install;
use crate::core::machine::Use;

/// What `vma notices` prints: they travel inside the executable, as
/// nothing ships beside it.
pub const NOTICES: &str = concat!(
    include_str!("../../LICENSE"),
    "\n\n",
    include_str!("../../THIRD-PARTY-NOTICES.md")
);

/// What `vma help` prints. Its words, as every message of vma's, are
/// English only: they are read by agents and scripts, as part of the
/// interface, like the commands' names.
pub const USAGE: &str = "\
vma COMMAND [ARGS]

The machines
  list                      every machine: its name and state
  create NAME [--os TYPE] [--disk GB] [--iso FILE] [SETTINGS]
                            a new machine with VirtualBox's defaults for its
                            system (Windows11_64 unless --os names another of
                            VirtualBox's), a new disk of GB, by default the
                            size VirtualBox recommends, and FILE in its
                            optical drive; prints its status
  create NAME --install FILE --password WORD [--user NAME] [--image N]
         [--os TYPE] [--disk GB] [SETTINGS]
                            the same, made to install the Windows on FILE at
                            its first start, with no person, and its Guest
                            Additions: image N of the ISO (1 by default), and
                            an account NAME (vboxuser by default; never the
                            computer's name, the machine's in letters, digits
                            and dashes) with the password WORD, signed in by
                            itself. Its system is the ISO's unless --os
                            names another. start then waits through the
                            install, which takes longer than its 240 s: give
                            it --timeout
  destroy NAME              remove it: its disks, settings and folder; an ISO
                            it held from elsewhere is kept. Always by name

The machine --machine NAME names, VMA_NAME by default
  status                    its state, screen, Guest Additions, system,
                            settings and media
  set SETTINGS              change its settings; some only while it is off
  start [--headless] [--timeout S]
                            turn it on and wait, 240 s by default, for its
                            desktop; through an install, it resets the
                            machine where a restart hung, as they may in
                            VirtualBox's NEM mode (where Hyper-V, as WSL 2's,
                            holds the processor's virtualization), and puts
                            vma's agent in at its end, as setup does
  stop [--force]            ask it to shut down; the power goes after 90 s,
                            or at once with --force or while it is paused
  insert FILE               an ISO in its optical drive
  eject                     its optical drive emptied; an install's own
                            medium, of no use outside it, is deleted, or,
                            as a running machine holds it until it stops,
                            once it stops
  disk GB                   a new disk, attached to it

SETTINGS
  --memory MB  --cpus N  --cpu-cap PERCENT  --vram MB
  --clipboard MODE  --drag-and-drop MODE
                            MODE: off, to-guest, to-host or both

Its screen, mouse and keyboard
  shot [FILE]               a PNG of the screen; prints its path
  click X Y [--right|--middle] [--double] [--hold KEYS]
                            KEYS held through it, as ctrl or shift to add to
                            a selection
  click NAME                the control of that name on the screen, an open
                            menu's items first, or the one in front whose
                            name starts with it
  move X Y [--left|--right|--middle]
                            with that button still down, after a press
  drag X1 Y1 X2 Y2 [--right|--middle] [--hold KEYS]
                            press, glide, release; KEYS held all along, as
                            ctrl to copy a file; down before the press, so
                            in a list they also add to what is selected
  press X Y [--right|--middle]
                            press there and keep the button down, to look at
                            what a held button shows; move carries it, and
                            release lets it go
  release X Y               every button up, there
  scroll X Y N              N notches, down when positive
  keys COMBO...             as win+r, ctrl+shift+enter, tab, f5, alt+y
  type TEXT                 as keys of a US keyboard: ASCII only
  paste [TEXT]              any text, by the clipboard and ctrl+v
  windows                   the open windows: place, size, program, title
  wait TITLE [--gone] [--timeout S]
                            until a window's title holds TITLE, or none does
  batch                     steps from standard input, one per line: these
                            commands, or pause MS; words are split at spaces
                            outside quotes, ' or \", and a line starting
                            with # is skipped; stops at the first failure

Inside it, through vma's agent there
  setup                     once: the agent, started at each logon, once as
                            the person and once elevated
  run [SCRIPT] [--file FILE] [--timeout S]
                            PowerShell in the person's session; its output,
                            errors and exit code are vma's own
  read PATH                 a file's text
  write PATH [TEXT]         TEXT, or standard input, into a file, in UTF-8
  push LOCAL [PATH]         a file or folder in; by default onto the Desktop
  pull PATH [LOCAL]         a file or folder out; by default into VMA_OUT

A relative PATH inside starts at the person's folder, and %VAR% and $env:VAR
are read; run, read, write, push and pull take --admin, to do it elevated. A
TEXT or SCRIPT left out is read from standard input.

X and Y are pixels of the screen as a picture shows it, from 0. A command of
the mouse or keyboard takes --look: a picture half a second later, its path
printed. Keys go as fast as the machine reads them, and the command returns
once it has; vma types on a PS/2 keyboard, VirtualBox's default. A key is a
letter, digit or mark of a US keyboard, unshifted, or esc, tab, enter, space,
backspace, capslock, ctrl, shift, alt, altgr, win, menu, f1 to f12, insert,
delete, home, end, pageup, pagedown, up, down, left or right.

For an agent
  mcp                       an MCP server on standard input and output: a
                            tool for each command but batch, which takes its
                            arguments by name, and machine for --machine; a
                            picture comes in the result

  notices                   vma's licence and the third-party notices

--machine NAME goes after the command, as its other options do. Named by
neither --machine nor VMA_NAME, status, shot and a batch's pause, which only
read, take the one running, or the only one there is; any other command takes
only the only one there is, as one of several may be someone else's.

A lone -- before a command's first word ends its options, so a word after it
may start with --, as in vma type -- --text. A picture or a file pulled goes
to VMA_OUT, or to TEMP\\vma, unless a FILE or LOCAL is given. vma's own
failures exit with 125.

From WSL, run in a Linux folder, vma takes Linux paths and shows its own as
Linux. Elsewhere, /mnt/c included, it does so only when WSLENV passes
WSL_DISTRO_NAME. Windows sees only the variables WSLENV names:
  export WSLENV=${WSLENV:+$WSLENV:}WSL_DISTRO_NAME:VMA_NAME:VMA_OUT/p
";

#[derive(Debug, PartialEq)]
pub enum Command {
    List,
    Create(Create),
    /// The machine's name, never taken from VMA_NAME or guessed.
    Destroy(String),
    Status,
    Set(Settings),
    Start {
        headless: bool,
        /// Seconds to wait for its desktop.
        timeout: u32,
    },
    Stop {
        force: bool,
    },
    Insert(String),
    Eject,
    /// A new disk's size in GB.
    Disk(u32),
    /// The file to write, as given; `None` takes a new name in the default
    /// folder.
    Shot(Option<String>),
    /// What goes to the mouse and keyboard, the places it was given, and
    /// whether a picture follows half a second later.
    Input {
        acts: Vec<Act>,
        places: Vec<(u32, u32)>,
        look: bool,
    },
    /// Steps from standard input, each read by `step`.
    Batch,
    /// An MCP server on standard input and output.
    Mcp,
    /// vma's licence and the third-party notices, printed.
    Notices,
    /// The agent put inside the machine and started.
    Setup,
    /// A PowerShell script run inside: `None` reads it from standard input.
    Run {
        script: Option<String>,
        file: Option<String>,
        /// Seconds it may take.
        timeout: u32,
        admin: bool,
    },
    Read {
        path: String,
        admin: bool,
    },
    /// `text`, or standard input where it is `None`, into a file inside.
    Write {
        path: String,
        text: Option<String>,
        admin: bool,
    },
    Push {
        local: String,
        path: Option<String>,
        admin: bool,
    },
    Pull {
        path: String,
        local: Option<String>,
        admin: bool,
    },
    /// Text through the clipboard and ctrl+v: `None` reads it from
    /// standard input.
    Paste {
        text: Option<String>,
        look: bool,
    },
    Windows,
    Wait {
        title: String,
        gone: bool,
        /// Seconds.
        timeout: u32,
    },
    /// A click on the control a name means.
    ClickName {
        name: String,
        button: Button,
        double: bool,
        hold: Vec<input::Key>,
        look: bool,
    },
    /// Inside the machine, started at logon: runs the agent the host
    /// shares, and again each time it ends.
    Boot(Role),
    /// Inside the machine: answers the host's requests.
    Agent(Role),
    /// Inside the machine, elevated, run by `setup`: makes the tasks that
    /// start `boot` at each logon.
    Install,
}

impl Command {
    /// What it does to the machine it acts on: status, a shot and a batch's
    /// pause, which sends nothing, only read it; anything else changes it.
    pub fn machine_use(&self) -> Use {
        match self {
            Command::Status | Command::Shot(_) => Use::Read,
            Command::Input { acts, .. } if acts.iter().all(|a| matches!(a, Act::Pause(_))) => {
                Use::Read
            }
            _ => Use::Change,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Create {
    pub name: String,
    /// VirtualBox's guest OS type, whose defaults the machine takes; `None`
    /// takes the system it installs, or else Windows11_64.
    pub os: Option<String>,
    /// The disk's size in GB; `None` takes VirtualBox's recommended size.
    pub disk: Option<u32>,
    pub iso: Option<String>,
    pub install: Option<Install>,
    pub settings: Settings,
}

/// A system the machine installs at its first start, with no person.
#[derive(Debug, PartialEq)]
pub struct Install {
    /// The ISO it is installed from.
    pub iso: String,
    /// The account signed in by itself; `None` takes VirtualBox's
    /// `install::DEFAULT_USER`.
    pub user: Option<String>,
    /// The account's password, which VirtualBox wants.
    pub password: String,
    /// Which of the ISO's images is installed; `None` takes VirtualBox's 1.
    pub image: Option<u32>,
}

/// Settings a machine takes; `None` leaves one as it is.
#[derive(Debug, Default, PartialEq)]
pub struct Settings {
    /// In MB.
    pub memory: Option<u32>,
    pub cpus: Option<u32>,
    /// The share of a host core each processor may take, 1 to 100.
    pub cpu_cap: Option<u32>,
    /// Video memory, in MB.
    pub vram: Option<u32>,
    pub clipboard: Option<Sharing>,
    pub drag_and_drop: Option<Sharing>,
}

impl Settings {
    pub fn is_empty(&self) -> bool {
        *self == Settings::default()
    }
}

/// Which way the clipboard, or drag and drop, crosses between host and
/// machine. Its number is VirtualBox's `ClipboardMode` and `DnDMode`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sharing {
    Off = 0,
    ToGuest = 1,
    ToHost = 2,
    Both = 3,
}

impl Sharing {
    pub fn word(self) -> &'static str {
        match self {
            Sharing::Off => "off",
            Sharing::ToGuest => "to-guest",
            Sharing::ToHost => "to-host",
            Sharing::Both => "both",
        }
    }

    pub fn from_number(n: u32) -> Option<Sharing> {
        [
            Sharing::Off,
            Sharing::ToGuest,
            Sharing::ToHost,
            Sharing::Both,
        ]
        .into_iter()
        .find(|s| *s as u32 == n)
    }
}

const INSTALL: [&str; 4] = ["install", "user", "password", "image"];

const SETTINGS: [&str; 6] = [
    "memory",
    "cpus",
    "cpu-cap",
    "vram",
    "clipboard",
    "drag-and-drop",
];

/// Reads the words after `vma`: `None` asks for help. A command that acts
/// on a machine takes `--machine NAME`, which comes back beside it. A
/// failure is the line vma prints.
pub fn parse(words: &[String]) -> Result<Option<(Command, Option<String>)>, String> {
    let Some((name, rest)) = words.split_first() else {
        return Ok(None);
    };
    let mut machine = None;
    // How a command that acts on a machine reads its words: as any other,
    // and --machine too.
    let mut on = |rest: &[String],
                  switches: &[&str],
                  valued: &[&str],
                  min: usize,
                  max: usize,
                  shape: &str|
     -> Result<Options, String> {
        let valued = [valued, &["machine"]].concat();
        let o = Options::read(rest, switches, &valued, min, max, shape)?;
        machine = match o.value("machine") {
            Some("") => return Err("--machine wants a machine's name".into()),
            named => named.map(String::from),
        };
        Ok(o)
    };
    let command = match name.as_str() {
        "help" | "--help" | "-h" => return Ok(None),
        "list" => {
            Options::read(rest, &[], &[], 0, 0, "list")?;
            Command::List
        }
        "create" => {
            let shape =
                "create NAME [--os TYPE] [--disk GB] [--iso FILE | --install FILE] [SETTINGS]";
            let mut valued = vec!["os", "disk", "iso"];
            valued.extend(INSTALL);
            valued.extend(SETTINGS);
            let o = Options::read(rest, &[], &valued, 1, 1, shape)?;
            Command::Create(Create {
                name: o.pos[0].clone(),
                os: o.value("os").map(String::from),
                disk: o.number("disk", 1, u32::MAX)?,
                iso: o.value("iso").map(String::from),
                install: o.install(&o.pos[0])?,
                settings: o.settings()?,
            })
        }
        "destroy" => {
            let o = Options::read(rest, &[], &[], 1, 1, "destroy NAME")?;
            Command::Destroy(o.pos[0].clone())
        }
        "status" => {
            on(rest, &[], &[], 0, 0, "status")?;
            Command::Status
        }
        "set" => {
            let o = on(rest, &[], &SETTINGS, 0, 0, "set SETTINGS")?;
            let settings = o.settings()?;
            if settings.is_empty() {
                return Err("usage: vma set SETTINGS, as vma help lists them".into());
            }
            Command::Set(settings)
        }
        "start" => {
            let o = on(
                rest,
                &["headless"],
                &["timeout"],
                0,
                0,
                "start [--headless] [--timeout S]",
            )?;
            Command::Start {
                headless: o.has("headless"),
                timeout: o.number("timeout", 1, u32::MAX)?.unwrap_or(240),
            }
        }
        "stop" => {
            let o = on(rest, &["force"], &[], 0, 0, "stop [--force]")?;
            Command::Stop {
                force: o.has("force"),
            }
        }
        "insert" => {
            let o = on(rest, &[], &[], 1, 1, "insert FILE")?;
            Command::Insert(o.pos[0].clone())
        }
        "eject" => {
            on(rest, &[], &[], 0, 0, "eject")?;
            Command::Eject
        }
        "disk" => {
            let o = on(rest, &[], &[], 1, 1, "disk GB")?;
            Command::Disk(number("GB", &o.pos[0], 1, u32::MAX)?)
        }
        "shot" => {
            let o = on(rest, &[], &[], 0, 1, "shot [FILE]")?;
            Command::Shot(o.pos.first().cloned())
        }
        "click" => {
            let o = on(
                rest,
                &["right", "middle", "double", "look"],
                &["hold"],
                1,
                2,
                "click X Y|NAME [--right|--middle] [--double] [--hold KEYS] [--look]",
            )?;
            let button = o.button(&["right", "middle"])?.unwrap_or(Button::Left);
            if o.pos.len() == 1 {
                Command::ClickName {
                    name: o.pos[0].clone(),
                    button,
                    double: o.has("double"),
                    hold: o.held()?,
                    look: o.has("look"),
                }
            } else {
                let (x, y) = o.place(0)?;
                o.input(
                    &[(x, y)],
                    input::click(x, y, button, o.has("double"), &o.held()?),
                )
            }
        }
        "move" => {
            let o = on(
                rest,
                &["left", "right", "middle", "look"],
                &[],
                2,
                2,
                "move X Y [--left|--right|--middle] [--look]",
            )?;
            let (x, y) = o.place(0)?;
            o.input(
                &[(x, y)],
                input::move_to(x, y, o.button(&["left", "right", "middle"])?),
            )
        }
        "press" => {
            let o = on(
                rest,
                &["right", "middle", "look"],
                &[],
                2,
                2,
                "press X Y [--right|--middle] [--look]",
            )?;
            let (x, y) = o.place(0)?;
            let button = o.button(&["right", "middle"])?.unwrap_or(Button::Left);
            o.input(&[(x, y)], input::press(x, y, button))
        }
        "release" => {
            let o = on(rest, &["look"], &[], 2, 2, "release X Y [--look]")?;
            let (x, y) = o.place(0)?;
            o.input(&[(x, y)], input::release(x, y))
        }
        "drag" => {
            let o = on(
                rest,
                &["right", "middle", "look"],
                &["hold"],
                4,
                4,
                "drag X1 Y1 X2 Y2 [--right|--middle] [--hold KEYS] [--look]",
            )?;
            let button = o.button(&["right", "middle"])?.unwrap_or(Button::Left);
            let (from, to) = (o.place(0)?, o.place(2)?);
            o.input(&[from, to], input::drag(from, to, button, &o.held()?))
        }
        "scroll" => {
            let o = on(rest, &["look"], &[], 3, 3, "scroll X Y N [--look]")?;
            let (x, y) = o.place(0)?;
            let notches = match o.pos[2].parse::<i32>() {
                Ok(n) if n != 0 => n,
                _ => {
                    return Err(format!(
                        "N is a whole number of notches, down when positive, not {}",
                        o.pos[2]
                    ));
                }
            };
            o.input(&[(x, y)], input::scroll(x, y, notches))
        }
        "keys" => {
            let o = on(
                rest,
                &["look"],
                &[],
                1,
                usize::MAX,
                "keys COMBO... [--look]",
            )?;
            o.input(&[], vec![Act::Keys(input::combos(&o.pos)?)])
        }
        "type" => {
            let o = on(rest, &["look"], &[], 1, usize::MAX, "type TEXT [--look]")?;
            o.input(&[], vec![Act::Keys(input::typed(&o.pos.join(" "))?)])
        }
        "batch" => {
            on(rest, &[], &[], 0, 0, "batch")?;
            Command::Batch
        }
        "mcp" => {
            Options::read(rest, &[], &[], 0, 0, "mcp")?;
            Command::Mcp
        }
        "setup" => {
            on(rest, &[], &[], 0, 0, "setup")?;
            Command::Setup
        }
        "notices" => {
            Options::read(rest, &[], &[], 0, 0, "notices")?;
            Command::Notices
        }
        "run" => {
            let o = on(
                rest,
                &["admin"],
                &["file", "timeout"],
                0,
                usize::MAX,
                "run [SCRIPT] [--file FILE] [--timeout S] [--admin]",
            )?;
            if !o.pos.is_empty() && o.value("file").is_some() {
                return Err("run takes a SCRIPT or --file, not both".into());
            }
            Command::Run {
                script: (!o.pos.is_empty()).then(|| o.pos.join(" ")),
                file: o.value("file").map(String::from),
                timeout: o.number("timeout", 1, u32::MAX)?.unwrap_or(60),
                admin: o.has("admin"),
            }
        }
        "read" => {
            let o = on(rest, &["admin"], &[], 1, 1, "read PATH [--admin]")?;
            Command::Read {
                path: o.pos[0].clone(),
                admin: o.has("admin"),
            }
        }
        "write" => {
            let o = on(
                rest,
                &["admin"],
                &[],
                1,
                usize::MAX,
                "write PATH [TEXT] [--admin]",
            )?;
            Command::Write {
                path: o.pos[0].clone(),
                text: (o.pos.len() > 1).then(|| o.pos[1..].join(" ")),
                admin: o.has("admin"),
            }
        }
        "push" => {
            let o = on(rest, &["admin"], &[], 1, 2, "push LOCAL [PATH] [--admin]")?;
            Command::Push {
                local: o.pos[0].clone(),
                path: o.pos.get(1).cloned(),
                admin: o.has("admin"),
            }
        }
        "pull" => {
            let o = on(rest, &["admin"], &[], 1, 2, "pull PATH [LOCAL] [--admin]")?;
            Command::Pull {
                path: o.pos[0].clone(),
                local: o.pos.get(1).cloned(),
                admin: o.has("admin"),
            }
        }
        "paste" => {
            let o = on(rest, &["look"], &[], 0, usize::MAX, "paste [TEXT] [--look]")?;
            Command::Paste {
                text: (!o.pos.is_empty()).then(|| o.pos.join(" ")),
                look: o.has("look"),
            }
        }
        "windows" => {
            on(rest, &[], &[], 0, 0, "windows")?;
            Command::Windows
        }
        "wait" => {
            let o = on(
                rest,
                &["gone"],
                &["timeout"],
                1,
                1,
                "wait TITLE [--gone] [--timeout S]",
            )?;
            Command::Wait {
                title: o.pos[0].clone(),
                gone: o.has("gone"),
                timeout: o.number("timeout", 1, u32::MAX)?.unwrap_or(30),
            }
        }
        "boot" | "agent" => {
            let o = Options::read(rest, &[], &["role"], 0, 0, "agent --role user|admin")?;
            let role = o
                .value("role")
                .and_then(Role::from_word)
                .ok_or("--role is user or admin")?;
            if name == "boot" {
                Command::Boot(role)
            } else {
                Command::Agent(role)
            }
        }
        "install" => {
            Options::read(rest, &[], &[], 0, 0, "install")?;
            Command::Install
        }
        _ => return Err(format!("{name} is no command: vma help")),
    };
    Ok(Some((command, machine)))
}

/// The commands a batch takes as steps, besides `pause`.
const STEPS: [&str; 12] = [
    "shot", "click", "move", "press", "release", "drag", "scroll", "keys", "type", "paste",
    "windows", "wait",
];

/// A line of a batch, read into the command it asks for and the machine
/// its `--machine` names: `None` for a blank line or a comment. A step is
/// one of `STEPS`, or `pause MS`.
pub fn step(line: &str) -> Result<Option<(Command, Option<String>)>, String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let words = words(line)?;
    if words[0] == "pause" {
        let o = Options::read(&words[1..], &[], &[], 1, 1, "pause MS")?;
        let ms = number("MS", &o.pos[0], 0, u32::MAX)?;
        let pause = Command::Input {
            acts: vec![Act::Pause(ms)],
            places: Vec::new(),
            look: false,
        };
        return Ok(Some((pause, None)));
    }
    if !STEPS.contains(&words[0].as_str()) {
        return Err(format!(
            "{} is no step: a batch takes {} and pause",
            words[0],
            STEPS.join(", ")
        ));
    }
    parse(&words)
}

/// A line's words, split at spaces outside quotes; a quote is ' or ", and
/// holds the other kind as it is.
fn words(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut quote = None;
    for c in line.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => word.get_or_insert_default().push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                word.get_or_insert_default();
            }
            None if c.is_whitespace() => words.extend(word.take()),
            None => word.get_or_insert_default().push(c),
        }
    }
    if let Some(q) = quote {
        return Err(format!("a {q} is never closed"));
    }
    words.extend(word);
    Ok(words)
}

/// A command's words split into options and the rest. An option is a word
/// that starts with `--`; one of `valued` takes the word after it. Any other
/// word, a lone `-` or a negative number too, is positional, and so is every
/// word after a lone `--` that comes before the first of them; past the
/// first, `--` is a word like another, as in a text typed.
struct Options {
    pos: Vec<String>,
    switches: Vec<String>,
    values: Vec<(String, String)>,
}

impl Options {
    fn read(
        rest: &[String],
        switches: &[&str],
        valued: &[&str],
        min: usize,
        max: usize,
        shape: &str,
    ) -> Result<Options, String> {
        let mut o = Options {
            pos: Vec::new(),
            switches: Vec::new(),
            values: Vec::new(),
        };
        let mut words = rest.iter();
        while let Some(word) = words.next() {
            if word == "--" {
                if o.pos.is_empty() {
                    o.pos.extend(words.by_ref().cloned());
                    break;
                }
                o.pos.push(word.clone());
                continue;
            }
            let Some(name) = word.strip_prefix("--") else {
                o.pos.push(word.clone());
                continue;
            };
            if valued.contains(&name) {
                let value = words
                    .next()
                    .ok_or_else(|| format!("{word} wants a value: vma {shape}"))?;
                if o.values.iter().any(|(n, _)| n == name) {
                    return Err(format!("{word} is given twice"));
                }
                o.values.push((name.to_string(), value.clone()));
            } else if switches.contains(&name) {
                o.switches.push(name.to_string());
            } else {
                return Err(format!("{word} is not an option here: vma {shape}"));
            }
        }
        if o.pos.len() < min || o.pos.len() > max {
            return Err(format!("usage: vma {shape}"));
        }
        Ok(o)
    }

    fn has(&self, switch: &str) -> bool {
        self.switches.iter().any(|s| s == switch)
    }

    /// The input command these options end, the places it was given, and
    /// its `--look`.
    fn input(&self, places: &[(u32, u32)], acts: Vec<Act>) -> Command {
        Command::Input {
            acts,
            places: places.to_vec(),
            look: self.has("look"),
        }
    }

    /// The place given by the positional words at `at` and the one after.
    fn place(&self, at: usize) -> Result<(u32, u32), String> {
        Ok((pixel(&self.pos[at])?, pixel(&self.pos[at + 1])?))
    }

    /// The button the one switch of `choices` given names, if any.
    fn button(&self, choices: &[&str]) -> Result<Option<Button>, String> {
        let given: Vec<&str> = choices.iter().copied().filter(|c| self.has(c)).collect();
        match given[..] {
            [] => Ok(None),
            [one] => Ok(Some(match one {
                "left" => Button::Left,
                "right" => Button::Right,
                _ => Button::Middle,
            })),
            _ => Err(format!("--{} are one or the other", given.join(" and --"))),
        }
    }

    /// The keys `--hold` names, as ctrl+shift.
    fn held(&self) -> Result<Vec<input::Key>, String> {
        self.value("hold").map_or(Ok(Vec::new()), input::combo)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn number(&self, name: &str, min: u32, max: u32) -> Result<Option<u32>, String> {
        self.value(name)
            .map(|v| number(&format!("--{name}"), v, min, max))
            .transpose()
    }

    fn sharing(&self, name: &str) -> Result<Option<Sharing>, String> {
        self.value(name)
            .map(|v| match v {
                "off" => Ok(Sharing::Off),
                "to-guest" => Ok(Sharing::ToGuest),
                "to-host" => Ok(Sharing::ToHost),
                "both" => Ok(Sharing::Both),
                _ => Err(format!(
                    "--{name} is off, to-guest, to-host or both, not {v}"
                )),
            })
            .transpose()
    }

    /// The install asked for on a machine named `machine`, with what only
    /// an install takes.
    fn install(&self, machine: &str) -> Result<Option<Install>, String> {
        let Some(iso) = self.value("install") else {
            if let Some(name) = INSTALL[1..].iter().find(|n| self.value(n).is_some()) {
                return Err(format!("--{name} goes with --install"));
            }
            return Ok(None);
        };
        if self.value("iso").is_some() {
            return Err("--iso and --install are one or the other: an install puts its own medium in the drive".into());
        }
        let user = self.value("user").map(windows_user).transpose()?;
        install::check_user(user.as_deref().unwrap_or(install::DEFAULT_USER), machine)?;
        // VirtualBox refuses an empty one only once the machine is made.
        let password = match self.value("password") {
            None => {
                return Err(
                    "--install wants --password: VirtualBox makes no account without one".into(),
                );
            }
            Some("") => {
                return Err(
                    "--password can't be empty: VirtualBox makes no account without one".into(),
                );
            }
            Some(password) => password,
        };
        Ok(Some(Install {
            iso: iso.to_string(),
            user,
            password: password.to_string(),
            image: self.number("image", 1, u32::MAX)?,
        }))
    }

    fn settings(&self) -> Result<Settings, String> {
        Ok(Settings {
            memory: self.number("memory", 1, u32::MAX)?,
            cpus: self.number("cpus", 1, u32::MAX)?,
            cpu_cap: self.number("cpu-cap", 1, 100)?,
            vram: self.number("vram", 1, u32::MAX)?,
            clipboard: self.sharing("clipboard")?,
            drag_and_drop: self.sharing("drag-and-drop")?,
        })
    }
}

/// A name Windows takes for a local account, checked here as the account is
/// made only near the install's end (the answer file's oobeSystem pass): up
/// to 20 characters (`net user`), none of those the answer file and the
/// account's name forbid, and not `NONE`.
fn windows_user(name: &str) -> Result<String, String> {
    const FORBIDDEN: &str = "\"/\\[]:;|=,+*?<>%@";
    let length = name.chars().count();
    if length == 0 {
        return Err("--user wants a name".into());
    }
    if length > 20 {
        return Err(format!(
            "--user is 1 to 20 characters, not {length}: Windows takes no longer name"
        ));
    }
    if let Some(c) = name
        .chars()
        .find(|c| FORBIDDEN.contains(*c) || c.is_control())
    {
        return Err(format!(
            "--user can't hold {c:?}: Windows takes none of {FORBIDDEN} in an account's name"
        ));
    }
    if name.eq_ignore_ascii_case("none") {
        return Err("--user can't be NONE: Windows keeps that name".into());
    }
    Ok(name.to_string())
}

/// A pixel of the screen, from 0; whether the screen reaches it is the
/// shell's to find out.
fn pixel(text: &str) -> Result<u32, String> {
    match text.parse::<i64>() {
        Ok(n) if n < 0 => Err(format!("{text} is off the screen: places start at 0")),
        Ok(n) => u32::try_from(n).map_err(|_| format!("{text} is off the screen")),
        Err(_) => Err(format!(
            "{text} is not a place: X and Y are whole numbers of pixels"
        )),
    }
}

fn number(name: &str, text: &str, min: u32, max: u32) -> Result<u32, String> {
    let range = if max == u32::MAX {
        format!("{min} or more")
    } else {
        format!("{min} to {max}")
    };
    match text.parse::<u32>() {
        Ok(n) if (min..=max).contains(&n) => Ok(n),
        _ => Err(format!("{name} is a whole number, {range}, not {text}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    fn parsed(line: &str) -> Command {
        parse(&words(line)).unwrap().unwrap().0
    }

    /// The machine `--machine` names in `line`.
    fn machine(line: &str) -> Option<String> {
        parse(&words(line)).unwrap().unwrap().1
    }

    fn failure(line: &str) -> String {
        parse(&words(line)).unwrap_err()
    }

    #[test]
    fn nothing_and_help_ask_for_help() {
        assert_eq!(parse(&[]), Ok(None));
        for w in ["help", "--help", "-h"] {
            assert_eq!(parse(&words(w)), Ok(None));
        }
    }

    #[test]
    fn status_takes_nothing() {
        assert_eq!(parsed("status"), Command::Status);
        assert_eq!(failure("status extra"), "usage: vma status");
        assert_eq!(
            failure("status --look"),
            "--look is not an option here: vma status"
        );
    }

    #[test]
    fn shot_takes_a_file_or_none() {
        assert_eq!(parsed("shot"), Command::Shot(None));
        assert_eq!(
            parsed("shot pics/a.png"),
            Command::Shot(Some("pics/a.png".into()))
        );
        assert_eq!(failure("shot a.png b.png"), "usage: vma shot [FILE]");
    }

    #[test]
    fn a_command_of_a_machine_takes_its_name() {
        assert_eq!(machine("status"), None);
        assert_eq!(machine("status --machine t1"), Some("t1".into()));
        assert_eq!(parsed("status --machine t1"), Command::Status);
        assert_eq!(machine("stop --machine t1 --force"), Some("t1".into()));
        assert_eq!(machine("batch --machine t1"), Some("t1".into()));
        // Wherever options go, and never past a lone -- that ends them.
        assert_eq!(machine("type hello --machine t1"), Some("t1".into()));
        assert_eq!(parsed("type hello --machine t1"), parsed("type hello"),);
        assert_eq!(machine("type -- --machine t1"), None);
        assert_eq!(
            parsed("type -- --machine t1"),
            Command::Input {
                acts: vec![Act::Keys(input::typed("--machine t1").unwrap())],
                places: vec![],
                look: false
            }
        );
        let Command::Run { script, .. } = parsed("run --timeout 5 -- --machine t1") else {
            panic!("not run");
        };
        assert_eq!(script.as_deref(), Some("--machine t1"));
        assert_eq!(machine("run --timeout 5 -- --machine t1"), None);
        assert_eq!(
            failure("status --machine"),
            "--machine wants a value: vma status"
        );
        assert_eq!(
            failure("status --machine a --machine b"),
            "--machine is given twice"
        );
        assert_eq!(
            parse(&["status".into(), "--machine".into(), "".into()]),
            Err("--machine wants a machine's name".into())
        );
    }

    #[test]
    fn only_status_a_shot_and_a_pause_read_their_machine() {
        assert_eq!(parsed("status").machine_use(), Use::Read);
        assert_eq!(parsed("shot a.png").machine_use(), Use::Read);
        let (pause, _) = step("pause 500").unwrap().unwrap();
        assert_eq!(pause.machine_use(), Use::Read);
        for line in [
            "set --memory 512",
            "start",
            "stop",
            "eject",
            "click 1 2",
            "click OK",
            "keys enter",
            "type a",
            "paste a",
            "setup",
            "read a.txt",
            "windows",
            "wait Notepad",
        ] {
            assert_eq!(parsed(line).machine_use(), Use::Change, "{line}");
        }
    }

    #[test]
    fn a_command_of_no_one_machine_takes_none() {
        for line in [
            "list --machine t1",
            "create t2 --machine t1",
            "destroy t2 --machine t1",
            "mcp --machine t1",
            "notices --machine t1",
            "install --machine t1",
        ] {
            assert!(
                failure(line).starts_with("--machine is not an option here"),
                "{line}"
            );
        }
    }

    #[test]
    fn an_unknown_command_is_named() {
        assert_eq!(failure("nope"), "nope is no command: vma help");
    }

    #[test]
    fn create_takes_a_name_and_leaves_the_system_to_the_shell() {
        assert_eq!(
            parsed("create t1"),
            Command::Create(Create {
                name: "t1".into(),
                os: None,
                disk: None,
                iso: None,
                install: None,
                settings: Settings::default(),
            })
        );
        assert_eq!(
            parsed("create t2 --memory 512 --cpus 2 --disk 8 --iso a.iso --os Other"),
            Command::Create(Create {
                name: "t2".into(),
                os: Some("Other".into()),
                disk: Some(8),
                iso: Some("a.iso".into()),
                install: None,
                settings: Settings {
                    memory: Some(512),
                    cpus: Some(2),
                    ..Settings::default()
                },
            })
        );
        assert_eq!(
            failure("create"),
            "usage: vma create NAME [--os TYPE] [--disk GB] [--iso FILE | --install FILE] [SETTINGS]"
        );
    }

    #[test]
    fn create_takes_an_install_and_what_goes_with_it() {
        let Command::Create(c) =
            parsed("create t --install w.iso --user tester --password pw --image 2 --memory 4096")
        else {
            panic!("not a create");
        };
        assert_eq!(
            c.install,
            Some(Install {
                iso: "w.iso".into(),
                user: Some("tester".into()),
                password: "pw".into(),
                image: Some(2),
            })
        );
        assert_eq!(c.iso, None);
        assert_eq!(c.settings.memory, Some(4096));
        let Command::Create(c) = parsed("create t --install w.iso --password pw") else {
            panic!("not a create");
        };
        assert_eq!(
            c.install,
            Some(Install {
                iso: "w.iso".into(),
                user: None,
                password: "pw".into(),
                image: None,
            })
        );
        assert_eq!(
            failure("create t --install w.iso --user dev"),
            "--install wants --password: VirtualBox makes no account without one"
        );
        assert_eq!(
            parse(&[
                "create".into(),
                "t".into(),
                "--install".into(),
                "w.iso".into(),
                "--password".into(),
                "".into(),
            ])
            .unwrap_err(),
            "--password can't be empty: VirtualBox makes no account without one"
        );
    }

    #[test]
    fn what_only_an_install_takes_wants_one() {
        assert_eq!(
            failure("create t --iso a.iso --install w.iso"),
            "--iso and --install are one or the other: an install puts its own medium in the drive"
        );
        for option in ["--user u", "--password p", "--image 2"] {
            let name = option.split(' ').next().unwrap();
            assert_eq!(
                failure(&format!("create t {option}")),
                format!("{name} goes with --install")
            );
        }
        assert_eq!(
            failure("create t --install w.iso --password pw --image 0"),
            "--image is a whole number, 1 or more, not 0"
        );
    }

    #[test]
    fn a_user_is_a_name_windows_takes() {
        let user = |name: &str| {
            parse(&[
                "create".into(),
                "t".into(),
                "--install".into(),
                "w.iso".into(),
                "--password".into(),
                "pw".into(),
                "--user".into(),
                name.into(),
            ])
        };
        assert!(user("dev").is_ok());
        assert!(user("José da Silva").is_ok());
        assert!(user(&"a".repeat(20)).is_ok());
        assert_eq!(
            user(&"a".repeat(21)).unwrap_err(),
            "--user is 1 to 20 characters, not 21: Windows takes no longer name"
        );
        assert_eq!(user("").unwrap_err(), "--user wants a name");
        assert_eq!(
            user("a/b").unwrap_err(),
            "--user can't hold '/': Windows takes none of \"/\\[]:;|=,+*?<>%@ in an account's name"
        );
        assert!(user("a@b").is_err());
        assert!(user("tab\there").is_err());
        assert_eq!(
            user("None").unwrap_err(),
            "--user can't be NONE: Windows keeps that name"
        );
    }

    #[test]
    fn a_user_is_not_named_like_the_computer() {
        let refusal = "--user can't be LAB-PC, the name Windows gives the computer: Windows makes the account then, but not an administrator";
        assert_eq!(
            failure("create LAB-PC --install w.iso --password pw --user LAB-PC"),
            refusal
        );
        assert_eq!(
            failure("create LAB-PC --install w.iso --password pw --user lab-pc"),
            refusal.replace("can't be LAB-PC", "can't be lab-pc")
        );
        assert!(
            parse(&words(
                "create LAB-PC --install w.iso --password pw --user dev"
            ))
            .is_ok()
        );
        assert_eq!(
            failure("create vboxuser --install w.iso --password pw"),
            "--user can't be vboxuser, the name Windows gives the computer: Windows makes the account then, but not an administrator"
        );
    }

    #[test]
    fn destroy_wants_a_name_every_time() {
        assert_eq!(parsed("destroy t1"), Command::Destroy("t1".into()));
        assert_eq!(failure("destroy"), "usage: vma destroy NAME");
    }

    #[test]
    fn set_takes_at_least_one_setting() {
        assert_eq!(
            parsed("set --vram 64 --clipboard both --drag-and-drop to-guest --cpu-cap 50"),
            Command::Set(Settings {
                cpu_cap: Some(50),
                vram: Some(64),
                clipboard: Some(Sharing::Both),
                drag_and_drop: Some(Sharing::ToGuest),
                ..Settings::default()
            })
        );
        assert_eq!(
            failure("set"),
            "usage: vma set SETTINGS, as vma help lists them"
        );
    }

    #[test]
    fn wrong_values_are_named() {
        assert_eq!(
            failure("set --memory abc"),
            "--memory is a whole number, 1 or more, not abc"
        );
        assert_eq!(
            failure("set --cpus 0"),
            "--cpus is a whole number, 1 or more, not 0"
        );
        assert_eq!(
            failure("set --cpu-cap 101"),
            "--cpu-cap is a whole number, 1 to 100, not 101"
        );
        assert_eq!(
            failure("create t --disk -1"),
            "--disk is a whole number, 1 or more, not -1"
        );
        assert_eq!(
            failure("set --clipboard sideways"),
            "--clipboard is off, to-guest, to-host or both, not sideways"
        );
        assert_eq!(
            failure("set --memory"),
            "--memory wants a value: vma set SETTINGS"
        );
        assert_eq!(
            failure("set --memory 1 --memory 2"),
            "--memory is given twice"
        );
        assert_eq!(failure("disk 0"), "GB is a whole number, 1 or more, not 0");
    }

    #[test]
    fn start_and_stop_take_their_switches() {
        assert_eq!(
            parsed("start"),
            Command::Start {
                headless: false,
                timeout: 240
            }
        );
        assert_eq!(
            parsed("start --headless --timeout 20"),
            Command::Start {
                headless: true,
                timeout: 20
            }
        );
        assert_eq!(parsed("stop --force"), Command::Stop { force: true });
        assert_eq!(
            failure("stop --headless"),
            "--headless is not an option here: vma stop [--force]"
        );
    }

    #[test]
    fn media_take_a_file_or_a_size() {
        assert_eq!(parsed("insert a.iso"), Command::Insert("a.iso".into()));
        assert_eq!(parsed("eject"), Command::Eject);
        assert_eq!(parsed("disk 2"), Command::Disk(2));
        assert_eq!(parsed("list"), Command::List);
    }

    #[test]
    fn sharing_numbers_are_virtualbox_modes() {
        assert_eq!(Sharing::from_number(3), Some(Sharing::Both));
        assert_eq!(Sharing::from_number(0).map(Sharing::word), Some("off"));
        assert_eq!(Sharing::from_number(4), None);
    }

    fn acts(line: &str) -> Vec<Act> {
        match parsed(line) {
            Command::Input { acts, .. } => acts,
            other => panic!("not input: {other:?}"),
        }
    }

    #[test]
    fn a_click_takes_its_button_and_keys() {
        assert_eq!(
            acts("click 10 20"),
            input::click(10, 20, Button::Left, false, &[])
        );
        assert_eq!(
            acts("click 10 20 --right --double --hold ctrl+shift"),
            input::click(
                10,
                20,
                Button::Right,
                true,
                &input::combo("ctrl+shift").unwrap()
            )
        );
        assert_eq!(
            parsed("click 1 2 --look"),
            Command::Input {
                acts: input::click(1, 2, Button::Left, false, &[]),
                places: vec![(1, 2)],
                look: true
            }
        );
        assert_eq!(
            failure("click 1 2 --right --middle"),
            "--right and --middle are one or the other"
        );
        assert_eq!(
            failure("click 1 2 --hold ctrl+nope"),
            "nope is no key vma knows: vma help lists them"
        );
        assert_eq!(
            failure("click"),
            "usage: vma click X Y|NAME [--right|--middle] [--double] [--hold KEYS] [--look]"
        );
    }

    #[test]
    fn places_are_whole_pixels_from_0() {
        assert_eq!(
            failure("click -1 5"),
            "-1 is off the screen: places start at 0"
        );
        assert_eq!(
            failure("click a 5"),
            "a is not a place: X and Y are whole numbers of pixels"
        );
        assert_eq!(
            failure("click 1.5 5"),
            "1.5 is not a place: X and Y are whole numbers of pixels"
        );
        assert_eq!(
            failure("drag 1 2 3"),
            "usage: vma drag X1 Y1 X2 Y2 [--right|--middle] [--hold KEYS] [--look]"
        );
        assert_eq!(
            acts("drag 1 2 3 4 --middle"),
            input::drag((1, 2), (3, 4), Button::Middle, &[])
        );
        let Command::Input { places, .. } = parsed("drag 1 2 3 4") else {
            panic!("not input");
        };
        assert_eq!(places, vec![(1, 2), (3, 4)]);
    }

    #[test]
    fn move_press_and_release_carry_a_button() {
        assert_eq!(acts("move 5 6"), input::move_to(5, 6, None));
        assert_eq!(
            acts("move 5 6 --left"),
            input::move_to(5, 6, Some(Button::Left))
        );
        assert_eq!(
            acts("press 5 6 --middle"),
            input::press(5, 6, Button::Middle)
        );
        assert_eq!(acts("release 5 6"), input::release(5, 6));
        assert_eq!(
            failure("release 5 6 --right"),
            "--right is not an option here: vma release X Y [--look]"
        );
    }

    #[test]
    fn a_scroll_takes_notches_but_none() {
        assert_eq!(acts("scroll 1 2 -3"), input::scroll(1, 2, -3));
        assert_eq!(
            failure("scroll 1 2 0"),
            "N is a whole number of notches, down when positive, not 0"
        );
        assert!(parse(&words("scroll 1 2 x")).is_err());
    }

    #[test]
    fn keys_and_text_become_scancodes() {
        assert_eq!(
            acts("keys win+r enter"),
            vec![Act::Keys(
                input::combos(&["win+r".into(), "enter".into()]).unwrap()
            )]
        );
        assert_eq!(
            acts("type a b"),
            vec![Act::Keys(input::typed("a b").unwrap())]
        );
        assert_eq!(failure("keys"), "usage: vma keys COMBO... [--look]");
        assert_eq!(failure("type"), "usage: vma type TEXT [--look]");
        assert_eq!(
            failure("keys ctrl+!"),
            "! is a shifted key: write shift and the key under it"
        );
        assert_eq!(parsed("batch"), Command::Batch);
        assert_eq!(parsed("mcp"), Command::Mcp);
        assert_eq!(
            parsed("type -- --look a"),
            Command::Input {
                acts: vec![Act::Keys(input::typed("--look a").unwrap())],
                places: vec![],
                look: false
            }
        );
        assert_eq!(
            acts("click --right -- 1 2"),
            input::click(1, 2, Button::Right, false, &[])
        );
        // Past the first word, -- is text.
        assert_eq!(
            acts("type git checkout -- file.txt"),
            vec![Act::Keys(input::typed("git checkout -- file.txt").unwrap())]
        );
    }

    #[test]
    fn the_agents_commands_take_their_words() {
        assert_eq!(
            parsed("run Get-Date; $x --timeout 5 --admin"),
            Command::Run {
                script: Some("Get-Date; $x".into()),
                file: None,
                timeout: 5,
                admin: true
            }
        );
        assert_eq!(
            parsed("run"),
            Command::Run {
                script: None,
                file: None,
                timeout: 60,
                admin: false
            }
        );
        assert_eq!(
            failure("run a --file b.ps1"),
            "run takes a SCRIPT or --file, not both"
        );
        assert_eq!(
            parsed("write Desktop\\a.txt"),
            Command::Write {
                path: "Desktop\\a.txt".into(),
                text: None,
                admin: false
            }
        );
        assert_eq!(
            parsed("pull a b --admin"),
            Command::Pull {
                path: "a".into(),
                local: Some("b".into()),
                admin: true
            }
        );
        assert_eq!(failure("push"), "usage: vma push LOCAL [PATH] [--admin]");
        assert_eq!(
            parsed("wait Notepad --gone"),
            Command::Wait {
                title: "Notepad".into(),
                gone: true,
                timeout: 30
            }
        );
        assert_eq!(
            parsed("click Save --double"),
            Command::ClickName {
                name: "Save".into(),
                button: Button::Left,
                double: true,
                hold: vec![],
                look: false
            }
        );
        assert_eq!(parsed("agent --role admin"), Command::Agent(Role::Admin));
        assert_eq!(failure("boot --role root"), "--role is user or admin");
        assert_eq!(parsed("setup"), Command::Setup);
        assert_eq!(parsed("notices"), Command::Notices);
        assert!(NOTICES.starts_with("MIT License"));
        assert!(NOTICES.contains("windows-core"));
    }

    #[test]
    fn a_batch_line_is_a_step_or_nothing() {
        assert_eq!(step(""), Ok(None));
        assert_eq!(step("   # a note"), Ok(None));
        assert_eq!(
            step("pause 500"),
            Ok(Some((
                Command::Input {
                    acts: vec![Act::Pause(500)],
                    places: vec![],
                    look: false
                },
                None
            )))
        );
        assert_eq!(
            step("  type \"a  b\" 'say \"hi\"'"),
            Ok(Some((
                Command::Input {
                    acts: vec![Act::Keys(input::typed("a  b say \"hi\"").unwrap())],
                    places: vec![],
                    look: false
                },
                None
            )))
        );
        assert_eq!(
            step("shot ''"),
            Ok(Some((Command::Shot(Some("".into())), None)))
        );
        assert_eq!(
            step("click 1 2 --machine t1"),
            Ok(Some((
                Command::Input {
                    acts: input::click(1, 2, Button::Left, false, &[]),
                    places: vec![(1, 2)],
                    look: false
                },
                Some("t1".into())
            )))
        );
        assert_eq!(
            step("status"),
            Err("status is no step: a batch takes shot, click, move, press, release, drag, scroll, keys, type, paste, windows, wait and pause".into())
        );
        assert!(step("batch").is_err());
        assert_eq!(step("type 'open"), Err("a ' is never closed".into()));
        assert_eq!(step("pause"), Err("usage: vma pause MS".into()));
    }
}
