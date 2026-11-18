//! The Windows shell: runs a command against VirtualBox, writes what it
//! gives, and says what the caller should see: printed for the command
//! line, kept for an MCP result. Which machine is meant, how its state
//! reads, what goes to its mouse and keyboard, what an MCP client asks and
//! how paths cross from WSL are the core's.

mod agent;
mod dispatch;
mod link;
mod vbox;

use crate::core::agent::{Role, Window, plain_name, text_of};
use crate::core::command::{self, Command, Sharing};
use crate::core::input::{self, Act};
use crate::core::json::Json;
use crate::core::machine;
use crate::core::mcp::{self, Output, Reply};
use crate::core::paths::Caller;
use link::Link;
use std::fs;
use std::io::{BufRead, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::sleep;
use std::time::{Duration, Instant};
use vbox::{Console, Machine, VirtualBox};
use windows::Win32::System::SystemInformation::GetLocalTime;

/// Runs `command` on the machine `named`, or the one the command line
/// would mean, and gives the exit code it ends with: 0, or the code of a
/// script it ran inside.
pub fn run(command: Command, named: Option<&str>) -> Result<i32, String> {
    match command {
        Command::Boot(role) => return agent::boot(role).map(|()| 0),
        Command::Agent(role) => return agent::serve(role).map(|()| 0),
        Command::Install => return agent::install().map(|()| 0),
        _ => {}
    }
    let mut session = Session::new()?;
    match command {
        Command::Batch => session.batch(named).map(|()| 0),
        Command::Mcp => session.mcp().map(|()| 0),
        command => {
            session.run(command, named, &mut Sink::Print)?;
            Ok(session.code)
        }
    }
}

/// Where a command's lines and pictures go: printed as they come, a
/// picture as a file and its path, or kept for an MCP result, a picture as
/// itself.
enum Sink {
    Print,
    Keep(Output),
}

impl Sink {
    fn say(&mut self, line: &str) {
        self.text(line);
        self.text("\n");
    }

    /// Text as it is, with no line added, as a script's output.
    fn text(&mut self, text: &str) {
        match self {
            Sink::Print => {
                print!("{text}");
                let _ = std::io::stdout().flush();
            }
            Sink::Keep(output) => output.text.push_str(text),
        }
    }

    /// A script's errors: on standard error, or after its output, named.
    fn errors(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        match self {
            Sink::Print => eprint!("{text}"),
            Sink::Keep(output) => {
                if !output.text.is_empty() && !output.text.ends_with('\n') {
                    output.text.push('\n');
                }
                output.text.push_str("errors:\n");
                output.text.push_str(text);
            }
        }
    }
}

/// One connection to VirtualBox for a run, and the console last used,
/// kept for the commands after it while its machine runs.
struct Session {
    caller: Caller,
    /// Whether it serves MCP, where a call names its machine by argument.
    mcp: bool,
    /// Whether standard input is a batch's or MCP's, so a command's text
    /// can't come from it.
    stdin_taken: bool,
    /// The exit code a script run inside gave.
    code: i32,
    vbox: Option<VirtualBox>,
    console: Option<Console>,
}

impl Session {
    fn new() -> Result<Session, String> {
        let folder = std::env::current_dir()
            .map_err(|e| format!("the current folder can't be read: {e}"))?;
        Ok(Session {
            caller: Caller::new(
                &folder.to_string_lossy(),
                setting("WSL_DISTRO_NAME").as_deref(),
            ),
            mcp: false,
            stdin_taken: false,
            code: 0,
            vbox: None,
            console: None,
        })
    }

    fn vbox(&mut self) -> Result<&VirtualBox, String> {
        if self.vbox.is_none() {
            self.vbox = Some(VirtualBox::connect()?);
        }
        Ok(self.vbox.as_ref().expect("connected above"))
    }

    /// The machine `named` names, or else VMA_NAME, or else the one
    /// running, or the only one there is.
    fn machine(&mut self, named: Option<&str>) -> Result<Machine, String> {
        let named = named.map(String::from).or_else(|| setting("VMA_NAME"));
        let ask = if self.mcp {
            "give machine"
        } else {
            "name one with --machine NAME or VMA_NAME"
        };
        self.vbox()?.machine(named.as_deref(), ask)
    }

    /// The console of `machine`: the one kept, where it is that machine's
    /// and its session still holds the machine, as it does until the
    /// machine stops.
    fn console(&mut self, machine: &Machine) -> Result<&Console, String> {
        let kept = self
            .console
            .as_ref()
            .is_some_and(|c| c.name() == machine.name && c.held());
        if !kept {
            self.console = None;
            self.console = Some(machine.console()?);
        }
        Ok(self.console.as_ref().expect("opened above"))
    }

    /// Runs `command` on the machine `named`, or the one the command line
    /// would mean, saying what it gives to `out`.
    fn run(&mut self, command: Command, named: Option<&str>, out: &mut Sink) -> Result<(), String> {
        // A command that changes a machine takes locks of its own.
        if !matches!(
            command,
            Command::Status | Command::Shot(_) | Command::Input { .. }
        ) {
            self.console = None;
        }
        match command {
            Command::List => {
                for (state, name) in self.vbox()?.list()? {
                    out.say(&format!("{state:<16}{name}"));
                }
                Ok(())
            }
            Command::Create(c) => {
                // A file named is found before VirtualBox is asked anything,
                // so a wrong one fails at once and changes nothing.
                let iso = c
                    .iso
                    .as_deref()
                    .map(|f| iso_path(&self.caller, f))
                    .transpose()?;
                let install = c
                    .install
                    .as_ref()
                    .map(|i| iso_path(&self.caller, &i.iso))
                    .transpose()?;
                let machine = self
                    .vbox()?
                    .create(&c, iso.as_deref(), install.as_deref())?;
                self.status(&machine, out)
            }
            Command::Destroy(name) => {
                self.vbox()?.destroy(&name)?;
                // The folder vma shared with it goes with it.
                if let Ok(folder) = link::folder(&name) {
                    let _ = fs::remove_dir_all(folder);
                }
                Ok(())
            }
            Command::Status => {
                let machine = self.machine(named)?;
                self.status(&machine, out)
            }
            Command::Set(settings) => self.machine(named)?.set(&settings),
            Command::Start { headless, timeout } => {
                let machine = self.machine(named)?;
                let installing = machine.installing()?;
                machine.start(self.vbox()?, headless, timeout, &mut |line| out.say(line))?;
                // A machine its install brought to its desktop gets the
                // agent; one that had it already keeps it. The Guest
                // Additions an install puts in at its end load their shared
                // folders, which the agent comes through, only from the
                // next boot, so the machine is shut down and started first.
                // Its desktop came, so a failure here is said, not the
                // start's.
                if installing && !self.link(named)?.seen() {
                    self.console = None;
                    let vbox = self.vbox()?;
                    let restarted = machine.stop(vbox, false).and_then(|()| {
                        machine.start(vbox, headless, timeout, &mut |line| out.say(line))
                    });
                    if let Err(why) = restarted.and_then(|()| self.setup(named, out, false)) {
                        out.say(&format!("the agent wasn't put in: {why}"));
                    }
                }
                Ok(())
            }
            Command::Stop { force } => {
                let machine = self.machine(named)?;
                machine.stop(self.vbox()?, force)
            }
            Command::Insert(file) => {
                let iso = iso_path(&self.caller, &file)?;
                let machine = self.machine(named)?;
                machine.insert(self.vbox()?, &iso)
            }
            Command::Eject => {
                let machine = self.machine(named)?;
                machine.eject(self.vbox()?)
            }
            Command::Disk(gb) => {
                let machine = self.machine(named)?;
                machine.add_disk(self.vbox()?, gb)
            }
            Command::Shot(file) => {
                let target = file.map(|f| self.caller.windows(&f)).transpose()?;
                let machine = self.machine(named)?;
                let png = self.console(&machine)?.shot()?;
                self.picture(png, target, out)
            }
            Command::Input { acts, places, look } => {
                let machine = self.machine(named)?;
                let console = self.console(&machine)?;
                let (width, height) = console.screen()?;
                input::on_screen(&places, width, height)?;
                console.act(&acts)?;
                if look {
                    std::thread::sleep(Duration::from_millis(500));
                    let png = console.shot()?;
                    self.picture(png, None, out)?;
                }
                Ok(())
            }
            Command::Setup => self.setup(named, out, true),
            Command::Run {
                script,
                file,
                timeout,
                admin,
            } => {
                let script = match (script, file) {
                    (Some(script), _) => script,
                    (None, Some(file)) => {
                        let path = self.caller.windows(&file)?;
                        let bytes = fs::read(&path)
                            .map_err(|e| failed(&self.caller, Path::new(&path), e))?;
                        text_of(&bytes)
                    }
                    (None, None) => self.input("the script")?,
                };
                let link = self.link(named)?;
                let reply = link.ask(
                    role(admin),
                    "run",
                    vec![
                        ("script", Json::String(script)),
                        ("seconds", Json::number(timeout)),
                    ],
                    // A slow machine takes a while to end a script that
                    // ran past its time and to say so.
                    u64::from(timeout) + 30,
                )?;
                let text = |k: &str| {
                    reply
                        .get(k)
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string()
                };
                out.text(&text("out"));
                out.errors(&text("err"));
                if matches!(reply.get("late"), Some(Json::Bool(true))) {
                    return Err(format!(
                        "the script ran past {timeout} s and was ended: --timeout S gives it longer"
                    ));
                }
                let code = reply.get("code").and_then(Json::as_i64).unwrap_or(-1) as i32;
                if code != 0 {
                    match out {
                        Sink::Print => self.code = code,
                        Sink::Keep(output) => {
                            output.failed = true;
                            output.text.push_str(&format!("exit code {code}\n"));
                        }
                    }
                }
                Ok(())
            }
            Command::Read { path, admin } => {
                let reply = self.link(named)?.ask(
                    role(admin),
                    "read",
                    vec![("path", Json::String(path))],
                    60,
                )?;
                out.text(reply.get("text").and_then(Json::as_str).unwrap_or(""));
                Ok(())
            }
            Command::Write { path, text, admin } => {
                let text = match text {
                    Some(text) => text,
                    None => self.input("the text")?,
                };
                self.link(named)?.ask(
                    role(admin),
                    "write",
                    vec![("path", Json::String(path)), ("text", Json::String(text))],
                    60,
                )?;
                Ok(())
            }
            Command::Push { local, path, admin } => {
                let from = std::path::absolute(self.caller.windows(&local)?)
                    .map_err(|e| failed(&self.caller, Path::new(&local), e))?;
                if !from.exists() {
                    return Err(format!("{local} is not there"));
                }
                let name = from
                    .file_name()
                    .ok_or_else(|| format!("{local} names no file or folder"))?
                    .to_string_lossy()
                    .into_owned();
                let link = self.link(named)?;
                let id = link::transfer_id();
                let stage = link.files(&id);
                let pushed = fs::create_dir_all(&stage)
                    .map_err(|e| failed(&self.caller, &stage, e))
                    .and_then(|()| link::copy(&from, &stage.join(&name)))
                    .and_then(|()| link::readable(&stage, &stage))
                    .and_then(|()| {
                        link.ask(
                            role(admin),
                            "push",
                            vec![
                                ("from", Json::String(format!("files\\{id}\\{name}"))),
                                (
                                    "to",
                                    Json::String(path.unwrap_or_else(|| "Desktop\\".into())),
                                ),
                            ],
                            300,
                        )
                    });
                let _ = fs::remove_dir_all(&stage);
                out.say(pushed?.get("path").and_then(Json::as_str).unwrap_or(""));
                Ok(())
            }
            Command::Pull { path, local, admin } => {
                let link = self.link(named)?;
                let id = link::transfer_id();
                let stage = link.files(&id);
                let pulled = link
                    .ask(
                        role(admin),
                        "pull",
                        vec![
                            ("path", Json::String(path)),
                            ("to", Json::String(format!("files\\{id}"))),
                        ],
                        300,
                    )
                    .and_then(|_| {
                        // The name is the one thing in the folder it was put
                        // in, read here: a reply could name any path on the
                        // host.
                        let names: Vec<String> = fs::read_dir(&stage)
                            .map_err(|e| failed(&self.caller, &stage, e))?
                            .filter_map(|e| {
                                e.ok().map(|e| e.file_name().to_string_lossy().into_owned())
                            })
                            .collect();
                        let [name] = &names[..] else {
                            return Err("the agent left no single file or folder to take".into());
                        };
                        if !plain_name(name) {
                            return Err(format!("the agent left a file vma won't take: {name}"));
                        }
                        let name = name.clone();
                        let mut target = match &local {
                            Some(l) => std::path::absolute(self.caller.windows(l)?)
                                .map_err(|e| failed(&self.caller, Path::new(l), e))?,
                            None => out_folder(&self.caller)?,
                        };
                        if target.is_dir()
                            || local
                                .as_ref()
                                .is_none_or(|l| l.ends_with('/') || l.ends_with('\\'))
                        {
                            fs::create_dir_all(&target)
                                .map_err(|e| failed(&self.caller, &target, e))?;
                            target = target.join(&name);
                        }
                        if target.is_dir() {
                            fs::remove_dir_all(&target)
                                .map_err(|e| failed(&self.caller, &target, e))?;
                        } else if target.exists() {
                            fs::remove_file(&target)
                                .map_err(|e| failed(&self.caller, &target, e))?;
                        }
                        link::copy(&stage.join(&name), &target)?;
                        Ok(target)
                    });
                let _ = fs::remove_dir_all(&stage);
                out.say(&self.caller.shown(&pulled?.to_string_lossy()));
                Ok(())
            }
            Command::Paste { text, look } => {
                let text = match text {
                    Some(text) => text,
                    None => self.input("the text")?,
                };
                self.link(named)?.ask(
                    Role::User,
                    "paste",
                    vec![("text", Json::String(text))],
                    30,
                )?;
                let keys = vec![Act::Keys(input::combos(&["ctrl+v".into()])?)];
                self.run(
                    Command::Input {
                        acts: keys,
                        places: vec![],
                        look,
                    },
                    named,
                    out,
                )
            }
            Command::Windows => {
                for window in self.windows(named)? {
                    out.say(&window.line());
                }
                Ok(())
            }
            Command::Wait {
                title,
                gone,
                timeout,
            } => {
                let until = Instant::now() + Duration::from_secs(timeout.into());
                let wanted = title.to_lowercase();
                loop {
                    stopped()?;
                    let windows = self.windows(named)?;
                    let found = windows
                        .iter()
                        .find(|w| w.title.to_lowercase().contains(&wanted));
                    match (found, gone) {
                        (Some(w), false) => {
                            out.say(&w.line());
                            return Ok(());
                        }
                        (None, true) => return Ok(()),
                        _ => {}
                    }
                    if Instant::now() >= until {
                        return Err(if gone {
                            format!("a window titled *{title}* is still open after {timeout} s")
                        } else {
                            format!("no window titled *{title}* in {timeout} s")
                        });
                    }
                    sleep(Duration::from_millis(300));
                }
            }
            Command::ClickName {
                name,
                button,
                double,
                hold,
                look,
            } => {
                let reply = self.link(named)?.ask(
                    Role::User,
                    "find",
                    vec![("name", Json::String(name))],
                    30,
                )?;
                let at = |k: &str| {
                    reply
                        .get(k)
                        .and_then(Json::as_i64)
                        .and_then(|n| u32::try_from(n).ok())
                };
                let (Some(x), Some(y)) = (at("x"), at("y")) else {
                    return Err("the agent gave no place for the control".into());
                };
                self.run(
                    Command::Input {
                        acts: input::click(x, y, button, double, &hold),
                        places: vec![(x, y)],
                        look,
                    },
                    named,
                    out,
                )
            }
            Command::Batch
            | Command::Mcp
            | Command::Notices
            | Command::Boot(_)
            | Command::Agent(_)
            | Command::Install => Err("a run takes no batch, mcp or the agent's own".into()),
        }
    }

    /// Says a picture: kept as it is, and written to `file` too where one
    /// is named; printed, written to `file` or a new file, and its path said.
    fn picture(&self, png: Vec<u8>, file: Option<String>, out: &mut Sink) -> Result<(), String> {
        let written = match (file, &*out) {
            (Some(path), _) => Some(write_over(&self.caller, Path::new(&path), &png)?),
            (None, Sink::Print) => Some(write_new(&self.caller, &png)?),
            (None, Sink::Keep(_)) => None,
        };
        if let Some(path) = written {
            out.say(&self.caller.shown(&path.to_string_lossy()));
        }
        if let Sink::Keep(output) = out {
            output.pictures.push(png);
        }
        Ok(())
    }

    /// The link to the agent inside the machine `named`.
    fn link(&mut self, named: Option<&str>) -> Result<Link, String> {
        let machine = self.machine(named)?;
        Link::open(self.console(&machine)?)
    }

    /// The windows open inside, as the person's agent sees them.
    fn windows(&mut self, named: Option<&str>) -> Result<Vec<Window>, String> {
        let reply = self.link(named)?.ask(Role::User, "windows", vec![], 30)?;
        let Some(Json::Array(list)) = reply.get("windows") else {
            return Err("the agent gave no windows".into());
        };
        list.iter()
            .map(|w| {
                Window::from_json(w)
                    .ok_or_else(|| "the agent gave a window vma can't read".to_string())
            })
            .collect()
    }

    /// Text a command takes from standard input, where that is the
    /// command's own.
    fn input(&self, what: &str) -> Result<String, String> {
        if self.stdin_taken {
            return Err(format!(
                "{what} goes with the call, as standard input isn't the command's here"
            ));
        }
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|e| format!("standard input can't be read: {e}"))?;
        Ok(text_of(&bytes))
    }

    /// Puts the agent in the machine by its own keyboard, where it doesn't
    /// answer, and says so where it does and `say` asks: the Run box takes the shared vma.exe, run elevated by
    /// ctrl+shift+enter, and the UAC prompt is answered yes by alt+y, as
    /// keys of the machine's own keyboard reach it where no program's would.
    fn setup(&mut self, named: Option<&str>, out: &mut Sink, say: bool) -> Result<(), String> {
        let link = self.link(named)?;
        // An agent taking a new vma.exe, or starting with its machine, is
        // back in seconds; one that answered here once is waited for
        // longer before it is put in again by the keyboard.
        let wait = if link.seen() { 20 } else { 5 };
        if link.answering(Duration::from_secs(wait))?.len() == 2 {
            if say {
                out.say("the agent was set up already");
            }
            return Ok(());
        }
        let machine = self.machine(named)?;
        let console = self.console(&machine)?;
        let keys = |combo: &str| -> Result<(), String> {
            console.act(&[Act::Keys(input::combos(&[combo.to_string()])?)])
        };
        keys("win+r")?;
        sleep(Duration::from_millis(1500));
        console.act(&[Act::Keys(input::typed(r"\\VBoxSvr\vma\vma.exe install")?)])?;
        keys("ctrl+shift+enter")?;
        sleep(Duration::from_secs(3));
        keys("alt+y")?;
        let until = Instant::now() + Duration::from_secs(60);
        loop {
            stopped()?;
            if link.answering(Duration::from_secs(1))?.len() == 2 {
                return Ok(());
            }
            if Instant::now() >= until {
                return Err(
                    "the agent didn't start in 60 s: vma shot shows what the machine shows".into(),
                );
            }
        }
    }

    fn status(&mut self, machine: &Machine, out: &mut Sink) -> Result<(), String> {
        let state = machine.state()?;
        let mut lines = vec![
            ("machine", machine.name.clone()),
            ("state", machine::state_word(state).to_string()),
        ];
        if machine::is_online(state) {
            let console = self.console(machine)?;
            // A machine still starting has no screen yet: 0x0.
            let (width, height) = console.screen()?;
            if width > 0 && height > 0 {
                lines.push(("screen", format!("{width}x{height}")));
            }
            if let Some(version) = console.additions()? {
                lines.push(("additions", version));
            }
            if state == machine::RUNNING {
                lines.push(("agent", agent_line(console)));
            }
        }
        let d = machine.details()?;
        lines.push(("os", d.os));
        lines.push(("memory", format!("{} MB", d.memory)));
        lines.push(("cpus", d.cpus.to_string()));
        if d.cpu_cap != 100 {
            lines.push(("cpu", format!("{}% of a host core each", d.cpu_cap)));
        }
        lines.push(("vram", format!("{} MB", d.vram)));
        for (name, mode) in [
            ("clipboard", d.clipboard),
            ("drag-and-drop", d.drag_and_drop),
        ] {
            lines.push((name, mode.map_or("unknown", Sharing::word).to_string()));
        }
        for (bytes, file) in d.disks {
            lines.push((
                "disk",
                format!(
                    "{} {}",
                    machine::size_words(bytes),
                    self.caller.shown(&file)
                ),
            ));
        }
        for iso in d.dvds {
            lines.push(("dvd", iso.map_or("empty".into(), |f| self.caller.shown(&f))));
        }
        for (name, value) in lines {
            out.say(&format!("{name:<14}{value}"));
        }
        Ok(())
    }

    /// Runs the steps standard input holds, a line each, through one
    /// console, on the machine `named` unless a step names its own, and
    /// stops at the first that fails, naming its line.
    fn batch(&mut self, named: Option<&str>) -> Result<(), String> {
        self.stdin_taken = true;
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| format!("standard input can't be read: {e}"))?;
        for (i, line) in text.lines().enumerate() {
            let failed = |why: String| format!("line {}, {}: {why}", i + 1, line.trim());
            let Some((step, machine)) = command::step(line).map_err(failed)? else {
                continue;
            };
            let named = machine.as_deref().or(named);
            self.run(step, named, &mut Sink::Print).map_err(failed)?;
        }
        Ok(())
    }

    /// Serves MCP on standard input and output, a message a line, until
    /// standard input ends. A thread reads the requests and answers what
    /// needs no machine, so the client is answered and a cancel is read
    /// while a call runs; this one runs the calls, one at a time, through
    /// the one session. Nothing else is written to standard output.
    fn mcp(&mut self) -> Result<(), String> {
        self.mcp = true;
        self.stdin_taken = true;
        let out = Arc::new(Mutex::new(std::io::stdout()));
        let calls = Arc::new(Mutex::new(Calls::default()));
        let (send, queue) = mpsc::channel::<mcp::Call>();
        let reader = {
            let (out, calls) = (out.clone(), calls.clone());
            std::thread::spawn(move || read_requests(&out, &calls, send))
        };
        for call in queue {
            if CLOSING.load(Ordering::SeqCst) {
                break;
            }
            {
                let mut c = calls.lock().expect("calls");
                if c.take_cancel(call.id()) {
                    continue;
                }
                c.running = Some(call.id().clone());
                STOP.store(false, Ordering::SeqCst);
            }
            let done = command::parse(&call.words)
                .and_then(|c| c.ok_or_else(|| "a tool is a command".to_string()))
                // A tool's words never hold --machine: its machine is an
                // argument of its own.
                .and_then(|(command, _)| {
                    let mut out = Sink::Keep(Output::default());
                    self.run(command, call.machine.as_deref(), &mut out)?;
                    let Sink::Keep(output) = out else {
                        unreachable!("kept above")
                    };
                    Ok(output)
                });
            if done.is_err() {
                // A failure may be VirtualBox's service gone: the next call
                // connects again.
                self.console = None;
                self.vbox = None;
            }
            let cancelled = {
                let mut c = calls.lock().expect("calls");
                c.running = None;
                c.take_cancel(call.id())
            };
            if !cancelled {
                write(&out, &call.answer(done))?;
            }
        }
        reader
            .join()
            .map_err(|_| "the reader of standard input failed".to_string())?
    }
}

/// Set while the call running is to stop: its client cancelled it, or
/// standard input ended. The long waits look at it.
static STOP: AtomicBool = AtomicBool::new(false);
/// Set once standard input has ended: no call waiting runs.
static CLOSING: AtomicBool = AtomicBool::new(false);

/// A failure where the call running is to stop.
fn stopped() -> Result<(), String> {
    if STOP.load(Ordering::SeqCst) {
        Err("cancelled".into())
    } else {
        Ok(())
    }
}

/// The MCP call running and the calls cancelled, as both threads see them.
#[derive(Default)]
struct Calls {
    running: Option<Json>,
    cancelled: Vec<Json>,
}

impl Calls {
    /// Whether the call `id` was cancelled, forgetting it.
    fn take_cancel(&mut self, id: &Json) -> bool {
        let before = self.cancelled.len();
        self.cancelled.retain(|c| c != id);
        self.cancelled.len() != before
    }
}

/// How many cancels are kept for calls not seen yet: one for a call that
/// already ended is never taken.
const CANCELS_KEPT: usize = 64;

/// Reads requests until standard input ends: answers those that need no
/// machine, queues the calls, and marks what a cancel names.
fn read_requests(
    out: &Mutex<std::io::Stdout>,
    calls: &Mutex<Calls>,
    send: mpsc::Sender<mcp::Call>,
) -> Result<(), String> {
    let mut protocol = mcp::Session::default();
    let read = std::io::stdin().lock().split(b'\n').try_for_each(|line| {
        let line = line.map_err(|e| format!("standard input can't be read: {e}"))?;
        let line = String::from_utf8_lossy(&line);
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            return Ok(());
        }
        match protocol.handle(line) {
            Reply::Nothing => Ok(()),
            Reply::Send(message) => write(out, &message),
            Reply::Run(call) => {
                // The other end gone, there is nothing left to serve.
                let _ = send.send(call);
                Ok(())
            }
            Reply::Cancel(id) => {
                let mut c = calls.lock().expect("calls");
                if c.running.as_ref() == Some(&id) {
                    STOP.store(true, Ordering::SeqCst);
                }
                if c.cancelled.len() == CANCELS_KEPT {
                    c.cancelled.remove(0);
                }
                c.cancelled.push(id);
                Ok(())
            }
        }
    });
    CLOSING.store(true, Ordering::SeqCst);
    STOP.store(true, Ordering::SeqCst);
    read
}

/// Writes `message` as one line on standard output.
fn write(out: &Mutex<std::io::Stdout>, message: &Json) -> Result<(), String> {
    let mut out = out.lock().expect("standard output");
    writeln!(out, "{}", message.write())
        .and_then(|()| out.flush())
        .map_err(|e| format!("standard output can't be written: {e}"))
}

/// Which agents answer, for `status`: given a few seconds where the share
/// or its vma.exe came only now, and never failing the status. A machine
/// without its folder never had the agent, and is left as it was: nothing
/// shared with it, copied or asked.
fn agent_line(console: &Console) -> String {
    match link::folder(console.name()) {
        Ok(folder) if !folder.is_dir() => return "not answering: vma setup".into(),
        Ok(_) => {}
        Err(why) => return format!("not reachable: {why}"),
    }
    let link = match Link::open(console) {
        Ok(link) => link,
        Err(why) => return format!("not reachable: {why}"),
    };
    let wait = Duration::from_secs(if link.fresh { 6 } else { 1 });
    match link.answering(wait) {
        Ok(up) if !up.is_empty() => up.iter().map(|r| r.word()).collect::<Vec<_>>().join(", "),
        Ok(_) if link.seen() => "not answering yet: starting, or its machine busy".into(),
        Ok(_) => "not answering: vma setup".into(),
        Err(why) => format!("not reachable: {why}"),
    }
}

/// The agent that answers: the elevated one where `admin` asks for it.
fn role(admin: bool) -> Role {
    if admin { Role::Admin } else { Role::User }
}

/// The folder a picture or a file pulled goes to by default: VMA_OUT, or
/// TEMP\vma.
fn out_folder(caller: &Caller) -> Result<PathBuf, String> {
    match setting("VMA_OUT") {
        Some(out) => std::path::absolute(caller.windows(&out)?)
            .map_err(|e| failed(caller, Path::new(&out), e)),
        None => Ok(std::env::temp_dir().join("vma")),
    }
}

/// An environment variable vma reads, unset when empty.
fn setting(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// An ISO's whole Windows path, as VirtualBox's service, which has a
/// current folder of its own, must be given it.
fn iso_path(caller: &Caller, file: &str) -> Result<String, String> {
    let path = std::path::absolute(caller.windows(file)?)
        .map_err(|e| failed(caller, Path::new(file), e))?;
    if !path.is_file() {
        return Err(format!("{file} is not a file"));
    }
    Ok(path.to_string_lossy().into_owned())
}

/// Writes `bytes` to `path`, over a file there, and gives its whole path.
fn write_over(caller: &Caller, path: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let whole = std::path::absolute(path).map_err(|e| failed(caller, path, e))?;
    if let Some(parent) = whole.parent() {
        fs::create_dir_all(parent).map_err(|e| failed(caller, parent, e))?;
    }
    fs::write(&whole, bytes).map_err(|e| failed(caller, &whole, e))?;
    Ok(whole)
}

/// A file's failure, its path shown as the caller knows it.
fn failed(caller: &Caller, path: &Path, error: std::io::Error) -> String {
    format!("{}: {error}", caller.shown(&path.to_string_lossy()))
}

/// Writes `bytes` to a new file in VMA_OUT, or TEMP\vma, named by the local
/// time to the millisecond, so pictures sort as they were taken; a second
/// one in the same millisecond takes `-2`.
fn write_new(caller: &Caller, bytes: &[u8]) -> Result<PathBuf, String> {
    let folder = out_folder(caller)?;
    fs::create_dir_all(&folder).map_err(|e| failed(caller, &folder, e))?;
    let t = unsafe { GetLocalTime() };
    let stem = format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}-{:03}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    );
    let mut n = 1;
    loop {
        let name = if n == 1 {
            format!("{stem}.png")
        } else {
            format!("{stem}-{n}.png")
        };
        let path = folder.join(name);
        match fs::File::create_new(&path) {
            Ok(mut file) => {
                file.write_all(bytes)
                    .map_err(|e| failed(caller, &path, e))?;
                return Ok(path);
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(failed(caller, &path, e)),
        }
    }
}
