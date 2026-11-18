//! vma inside a machine: the same executable, run there. `install`, run
//! elevated by the host's `setup`, copies it to the person's folder and
//! makes two tasks that start `boot` at each logon, once as the person and
//! once elevated. `boot` runs the agent the host shares, `\\VBoxSvr\vma\
//! vma.exe`, from a local copy, and again each time it ends; the agent
//! answers the requests the host writes to the shared folder, and ends when
//! the host shares a new vma.exe, which `boot` then takes.
//!
//! A request is `in\<role>\<id>.json`, written whole under another name and
//! then renamed, so it is never read half written; its reply is
//! `out\<id>.json`, written the same way. What a path inside means, how a
//! window reads and which control a name means are the core's.

use super::dispatch::{self, Arg, Object};
use crate::core::agent::{self, Control, Found, Role, Window};
use crate::core::json::Json;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute,
};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CreateMutexW, GetCurrentProcess, OpenProcess, OpenProcessToken,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::System::Variant::{VARIANT, VT_BOOL, VT_BSTR};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, PropertyConditionFlags_IgnoreCase,
    PropertyConditionFlags_None, TreeScope_Descendants, UIA_IsOffscreenPropertyId,
    UIA_NamePropertyId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, GetForegroundWindow, GetMenuItemCount, GetMenuItemRect,
    GetMenuStringW, GetWindowTextW, GetWindowThreadProcessId, HMENU, IsIconic, IsWindowVisible,
    MF_BYPOSITION, SendMessageW, SetProcessDPIAware,
};
use windows::core::{BOOL, BSTR, PCWSTR, PWSTR, w};

/// The host's shared folder, as the machine reaches it.
const SHARE: &str = r"\\VBoxSvr\vma";
/// How often the agent looks for a request: often while requests come,
/// seldom once they stop.
const QUICK: Duration = Duration::from_millis(2);
const IDLE: Duration = Duration::from_millis(100);
/// How long after a request the agent keeps looking often.
const BUSY: Duration = Duration::from_secs(3);
/// How often the agent looks for a new vma.exe, and `boot` for the share.
const CHECK: Duration = Duration::from_secs(2);
/// How long an agent ending for a new vma.exe waits for the requests it
/// took.
const DRAIN: Duration = Duration::from_secs(120);
/// How long `boot` waits before running the agent again.
const RESTART: Duration = Duration::from_millis(200);
/// `MN_GETHMENU`: the menu a menu window shows.
const MN_GETHMENU: u32 = 0x01E1;

/// The folder in the person's own that holds vma inside.
fn home() -> PathBuf {
    PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join("vma")
}

/// Makes the tasks that start `boot` at each logon, once as the person and
/// once elevated, after copying this executable to the person's folder, and
/// starts both now. Running it again puts back what it made.
pub fn install() -> Result<(), String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    if !me
        .to_string_lossy()
        .to_lowercase()
        .starts_with(&SHARE.to_lowercase())
    {
        return Err(format!(
            "install runs inside a machine, from {SHARE}, as vma setup starts it"
        ));
    }
    let folder = home();
    fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let copy = folder.join("vma.exe");
    if fs::copy(&me, &copy).is_err() {
        // A boot still runs from the copy: it goes aside, as a running
        // executable may be renamed though not written.
        let aside = folder.join(format!("vma-{}.old", std::process::id()));
        fs::rename(&copy, &aside).map_err(|e| format!("{}: {e}", copy.display()))?;
        fs::copy(&me, &copy).map_err(|e| format!("{}: {e}", copy.display()))?;
    }
    dispatch::init()?;
    let service = Object::create("Schedule.Service")?;
    service.call("Connect", &mut [])?;
    let root = service
        .call("GetFolder", &mut [Arg::Str("\\")])?
        .object()?
        .ok_or("the task scheduler has no root folder")?;
    let user = format!(
        "{}\\{}",
        std::env::var("USERDOMAIN").unwrap_or_default(),
        std::env::var("USERNAME").unwrap_or_default()
    );
    for role in [Role::User, Role::Admin] {
        let xml = task_xml(&user, role, &copy);
        let name = format!("vma agent ({})", role.word());
        // TASK_CREATE_OR_UPDATE, and TASK_LOGON_INTERACTIVE_TOKEN: the
        // person's own session, as they are signed in.
        let task = root
            .call(
                "RegisterTask",
                &mut [
                    Arg::Str(&name),
                    Arg::Str(&xml),
                    Arg::I32(6),
                    Arg::Empty,
                    Arg::Empty,
                    Arg::I32(3),
                    Arg::Empty,
                ],
            )?
            .object()?
            .ok_or("the task scheduler gave no task")?;
        task.call("Run", &mut [Arg::Empty])?;
    }
    Ok(())
}

/// A task that starts `boot` for `role` at `user`'s logon, in a console no
/// one sees (`conhost --headless`), for as long as it runs.
fn task_xml(user: &str, role: Role, exe: &Path) -> String {
    let level = match role {
        Role::User => "LeastPrivilege",
        Role::Admin => "HighestAvailable",
    };
    let escape = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>{level}</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>conhost.exe</Command><Arguments>--headless "{exe}" boot --role {role}</Arguments></Exec></Actions>
</Task>"#,
        user = escape(user),
        exe = escape(&exe.to_string_lossy()),
        role = role.word(),
    )
}

/// Holds a named mutex for the life of the process: false where another
/// process holds it, so a second boot or agent of a role ends at once.
fn only_one(name: &str) -> bool {
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    match unsafe { CreateMutexW(None, true, PCWSTR(wide.as_ptr())) } {
        // One that already exists gives a handle, not owned.
        Ok(_) => (unsafe { GetLastError() }) != ERROR_ALREADY_EXISTS,
        Err(_) => false,
    }
}

/// Runs the agent the host shares, from a local copy, and again as soon as
/// it ends, as it does to take a new vma.exe; waits for the share while the
/// host hasn't put it there.
pub fn boot(role: Role) -> Result<(), String> {
    if !only_one(&format!("Local\\vma-boot-{}", role.word())) {
        return Ok(());
    }
    let shared = PathBuf::from(SHARE).join("vma.exe");
    let local = home().join(format!("agent-{}.exe", role.word()));
    loop {
        if !shared.is_file() {
            sleep(CHECK);
            continue;
        }
        if fs::copy(&shared, &local).is_ok() || local.is_file() {
            let _ = Command::new(&local)
                .args(["agent", "--role", role.word()])
                .creation_flags(CREATE_NO_WINDOW.0)
                .status();
        }
        // An agent that ends at once, again and again, is not run in a
        // loop that spins.
        sleep(RESTART);
    }
}

use std::os::windows::process::CommandExt;

/// What marks the shared vma.exe: a new one ends the agent.
fn mark(path: &Path) -> Option<(u64, SystemTime)> {
    let m = fs::metadata(path).ok()?;
    Some((m.len(), m.modified().ok()?))
}

/// Answers the host's requests to `role` until the host shares a new
/// vma.exe, or the share goes.
pub fn serve(role: Role) -> Result<(), String> {
    if !only_one(&format!("Local\\vma-agent-{}", role.word())) {
        return Ok(());
    }
    unsafe {
        let _ = SetProcessDPIAware();
    }
    let share = PathBuf::from(SHARE);
    let inbox = share.join("in").join(role.word());
    let shared = share.join("vma.exe");
    // The shared vma.exe as it is now: one that differs later is a new
    // one. Compared with its own copy instead, a copy that came short or
    // stale would end every agent boot starts, and none would answer;
    // this way a vma.exe shared between boot's copy and this start runs
    // only at the change after.
    let version = mark(&shared);
    let running = Arc::new(AtomicUsize::new(0));
    let mut checked = Instant::now();
    let mut last = Instant::now();
    loop {
        if checked.elapsed() >= CHECK {
            checked = Instant::now();
            if shared.is_file() && mark(&shared) != version {
                break;
            }
        }
        let Ok(entries) = fs::read_dir(&inbox) else {
            sleep(IDLE);
            continue;
        };
        let mut requests: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        requests.sort();
        if !requests.is_empty() && shared.is_file() && mark(&shared) != version {
            // A request that came with a new version is the new one's.
            break;
        }
        for file in requests {
            last = Instant::now();
            let Ok(text) = fs::read(&file) else { continue };
            let _ = fs::remove_file(&file);
            let Ok(request) = Json::parse(&String::from_utf8_lossy(&text)) else {
                continue;
            };
            // Each request on a thread of its own, so a long script holds
            // back no other.
            let (share, running) = (share.clone(), running.clone());
            running.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(move || {
                handle(role, &request, &share);
                running.fetch_sub(1, Ordering::SeqCst);
            });
        }
        sleep(if last.elapsed() < BUSY { QUICK } else { IDLE });
    }
    // The requests taken are answered before the new version starts, but
    // one stuck holds it back only so long.
    let until = Instant::now() + DRAIN;
    while running.load(Ordering::SeqCst) > 0 && Instant::now() < until {
        sleep(QUICK);
    }
    Ok(())
}

/// Answers one request, its reply written whole under another name and
/// then renamed.
fn handle(role: Role, request: &Json, share: &Path) {
    let Some(id) = request.get("id").and_then(Json::as_str) else {
        return;
    };
    let _ = dispatch::init();
    let reply = answer(role, request, share).unwrap_or_else(|why| {
        let mut why = why;
        if role == Role::User && why.contains("Access is denied") {
            why.push_str(": --admin does it elevated");
        }
        Json::object(vec![("error", Json::String(why))])
    });
    let outbox = share.join("out");
    let temp = outbox.join(format!("{id}.tmp"));
    if fs::write(&temp, reply.write()).is_ok() {
        let _ = fs::rename(&temp, outbox.join(format!("{id}.json")));
    }
}

fn field<'a>(request: &'a Json, name: &str) -> Result<&'a str, String> {
    request
        .get(name)
        .and_then(Json::as_str)
        .ok_or_else(|| format!("the request has no {name}"))
}

/// A path inside, as the person's session reads it.
fn place(path: &str) -> PathBuf {
    let profile = std::env::var("USERPROFILE").unwrap_or_default();
    PathBuf::from(agent::place(path, &profile, |name| {
        std::env::var(name).ok()
    }))
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> String + '_ {
    move |e| format!("{}: {e}", path.display())
}

fn answer(role: Role, request: &Json, share: &Path) -> Result<Json, String> {
    let ok = |members: Vec<(&str, Json)>| Ok(Json::object(members));
    match field(request, "op")? {
        "ping" => ok(vec![
            ("role", Json::text(role.word())),
            (
                "user",
                Json::String(std::env::var("USERNAME").unwrap_or_default()),
            ),
            ("elevated", Json::Bool(elevated())),
        ]),
        "run" => {
            let seconds = request.get("seconds").and_then(Json::as_i64).unwrap_or(60);
            run(
                field(request, "id")?,
                field(request, "script")?,
                seconds as u64,
            )
        }
        "read" => {
            let path = place(field(request, "path")?);
            if path.is_dir() {
                return Err(format!(
                    "{} is a folder: vma pull copies it",
                    path.display()
                ));
            }
            let bytes = fs::read(&path).map_err(io(&path))?;
            ok(vec![("text", Json::String(agent::text_of(&bytes)))])
        }
        "write" => {
            let path = place(field(request, "path")?);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(io(parent))?;
            }
            fs::write(&path, field(request, "text")?).map_err(io(&path))?;
            ok(vec![("path", Json::String(path.display().to_string()))])
        }
        "push" => {
            let from = share.join(field(request, "from")?);
            let asked = field(request, "to")?;
            let mut to = place(asked);
            if to.is_dir() || asked.ends_with('\\') || asked.ends_with('/') {
                to = to.join(from.file_name().unwrap_or_default());
            }
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(io(parent))?;
            }
            if to.is_dir() {
                fs::remove_dir_all(&to).map_err(io(&to))?;
            } else if to.exists() {
                fs::remove_file(&to).map_err(io(&to))?;
            }
            copy(&from, &to)?;
            ok(vec![("path", Json::String(to.display().to_string()))])
        }
        "pull" => {
            let from = place(field(request, "path")?);
            if !from.exists() {
                return Err(format!("{} is not there", from.display()));
            }
            let name = from
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "drive".into());
            let to = share.join(field(request, "to")?);
            fs::create_dir_all(&to).map_err(io(&to))?;
            copy(&from, &to.join(&name))?;
            ok(vec![("name", Json::String(name))])
        }
        "paste" => {
            clipboard(field(request, "text")?)?;
            ok(vec![])
        }
        "windows" => ok(vec![(
            "windows",
            Json::Array(windows().iter().map(Window::json).collect()),
        )]),
        "find" => {
            let (x, y) = find(field(request, "name")?)?;
            ok(vec![("x", Json::number(x)), ("y", Json::number(y))])
        }
        other => Err(format!("{other} is no request this agent knows")),
    }
}

/// Copies a file, or a folder and all it holds.
fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if from.is_dir() {
        fs::create_dir_all(to).map_err(io(to))?;
        for entry in fs::read_dir(from).map_err(io(from))? {
            let entry = entry.map_err(io(from))?;
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to)
            .map(drop)
            .map_err(|e| format!("{} to {}: {e}", from.display(), to.display()))
    }
}

/// Whether this process runs elevated.
fn elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = 0;
        let got = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );
        let _ = CloseHandle(token);
        got.is_ok() && elevation.TokenIsElevated != 0
    }
}

/// A PowerShell script, run in the person's folder with no window: its
/// output and errors read as they come, so a program it leaves running with
/// the same pipes can't hold the answer back, and the whole of it ended
/// once `seconds` pass.
fn run(id: &str, script: &str, seconds: u64) -> Result<Json, String> {
    let folder = std::env::temp_dir().join("vma");
    fs::create_dir_all(&folder).map_err(io(&folder))?;
    let file = folder.join(format!("{id}.ps1"));
    // Windows PowerShell reads a script without a mark as ANSI, and writes
    // to a pipe in the console's code page: both made UTF-8, the second
    // before the script is called, so its lines keep their numbers.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(script.trim_start_matches('\u{feff}').as_bytes());
    fs::write(&file, bytes).map_err(io(&file))?;
    let call = format!(
        "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); $OutputEncoding = [Console]::OutputEncoding; & {}; exit $LASTEXITCODE",
        agent::single_quoted(&file.display().to_string())
    );
    let child = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &call,
        ])
        .current_dir(std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into()))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW.0)
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            let _ = fs::remove_file(&file);
            return Err(format!("PowerShell didn't start: {e}"));
        }
    };
    let collect = |stream: Option<Box<dyn Read + Send>>| {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        if let Some(mut stream) = stream {
            let buffer = buffer.clone();
            std::thread::spawn(move || {
                let mut chunk = [0u8; 8192];
                while let Ok(n) = stream.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    buffer
                        .lock()
                        .expect("output")
                        .extend_from_slice(&chunk[..n]);
                }
            });
        }
        buffer
    };
    let out = collect(
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    );
    let err = collect(
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    );
    let until = Instant::now() + Duration::from_secs(seconds);
    let (code, late) = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break (status.code().unwrap_or(-1), false);
        }
        if Instant::now() >= until {
            let _ = Command::new("taskkill.exe")
                .args(["/T", "/F", "/PID", &child.id().to_string()])
                .creation_flags(CREATE_NO_WINDOW.0)
                .status();
            let _ = child.wait();
            break (-1, true);
        }
        sleep(Duration::from_millis(5));
    };
    // What the pipes still carry comes at once; a program that kept them
    // open is not waited for.
    sleep(Duration::from_millis(100));
    let _ = fs::remove_file(&file);
    let text =
        |b: &Arc<Mutex<Vec<u8>>>| String::from_utf8_lossy(&b.lock().expect("output")).into_owned();
    Ok(Json::object(vec![
        ("code", Json::number(code)),
        ("out", Json::String(text(&out))),
        ("err", Json::String(text(&err))),
        ("late", Json::Bool(late)),
    ]))
}

/// The clipboard holds `text`; a program holding it turns a try away for a
/// moment.
fn clipboard(text: &str) -> Result<(), String> {
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    for attempt in 1.. {
        let opened = unsafe { OpenClipboard(None) };
        if opened.is_ok() {
            let done = unsafe {
                EmptyClipboard()
                    .and_then(|()| GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2))
                    .and_then(|memory| {
                        let target = GlobalLock(memory) as *mut u16;
                        std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
                        let _ = GlobalUnlock(memory);
                        SetClipboardData(CF_UNICODETEXT.0.into(), Some(HANDLE(memory.0)))
                    })
            };
            unsafe {
                let _ = CloseClipboard();
            }
            return done
                .map(drop)
                .map_err(|e| format!("the clipboard took no text: {}", e.message()));
        }
        if attempt == 5 {
            return Err("the clipboard is held by another program".into());
        }
        sleep(Duration::from_millis(100));
    }
    unreachable!("the loop returns")
}

/// The windows a person sees, front to back: visible, titled, and not
/// cloaked (a cloaked window is one of Windows' own, kept out of sight).
fn windows() -> Vec<Window> {
    let mut handles: Vec<HWND> = Vec::new();
    unsafe extern "system" fn each(window: HWND, list: LPARAM) -> BOOL {
        unsafe { (*(list.0 as *mut Vec<HWND>)).push(window) };
        BOOL(1)
    }
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut handles as *mut _ as isize));
    }
    let front = unsafe { GetForegroundWindow() };
    let mut list = Vec::new();
    for window in handles {
        unsafe {
            if !IsWindowVisible(window).as_bool() {
                continue;
            }
            let mut title = [0u16; 512];
            let len = GetWindowTextW(window, &mut title);
            if len == 0 {
                continue;
            }
            let mut cloaked = 0u32;
            if DwmGetWindowAttribute(window, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4)
                .is_ok()
                && cloaked != 0
            {
                continue;
            }
            let mut r = RECT::default();
            let _ = DwmGetWindowAttribute(
                window,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut r as *mut _ as *mut _,
                size_of::<RECT>() as u32,
            );
            let mut process = 0u32;
            GetWindowThreadProcessId(window, Some(&mut process));
            list.push(Window {
                title: String::from_utf16_lossy(&title[..len as usize]),
                program: program(process),
                front: window == front,
                minimized: IsIconic(window).as_bool(),
                x: r.left.into(),
                y: r.top.into(),
                width: (r.right - r.left).into(),
                height: (r.bottom - r.top).into(),
            });
        }
    }
    list
}

/// A process's program, by its file's name without `.exe`.
fn program(process: u32) -> String {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process) else {
            return String::new();
        };
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        let got = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        if got.is_err() {
            return String::new();
        }
        let path = String::from_utf16_lossy(&path[..len as usize]);
        let file = path.rsplit('\\').next().unwrap_or(&path);
        file.strip_suffix(".exe")
            .or_else(|| file.strip_suffix(".EXE"))
            .unwrap_or(file)
            .to_string()
    }
}

/// The items of the menus open on the screen, by Windows' own menu calls:
/// a classic menu, as a right click or a drop opens, shows none of them to
/// UI Automation.
fn menu_items() -> Vec<Control> {
    let mut items = Vec::new();
    let mut after: Option<HWND> = None;
    loop {
        let found = unsafe { FindWindowExW(None, after, w!("#32768"), PCWSTR::null()) };
        let Ok(window) = found else { break };
        if window.is_invalid() {
            break;
        }
        after = Some(window);
        unsafe {
            if !IsWindowVisible(window).as_bool() {
                continue;
            }
            let menu =
                HMENU(SendMessageW(window, MN_GETHMENU, Some(WPARAM(0)), Some(LPARAM(0))).0 as _);
            if menu.is_invalid() {
                continue;
            }
            for i in 0..GetMenuItemCount(Some(menu)).max(0) as u32 {
                let mut text = [0u16; 256];
                let len = GetMenuStringW(menu, i, Some(&mut text), MF_BYPOSITION);
                if len <= 0 {
                    continue;
                }
                let mut r = RECT::default();
                if GetMenuItemRect(Some(window), menu, i, &mut r).is_err() {
                    continue;
                }
                items.push(Control {
                    name: agent::menu_name(&String::from_utf16_lossy(&text[..len as usize])),
                    x: ((r.left + r.right) / 2).into(),
                    y: ((r.top + r.bottom) / 2).into(),
                });
            }
        }
    }
    items
}

/// A VARIANT of a string, as a condition on a name is made.
fn text_variant(text: &str) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_BSTR;
        inner.Anonymous.bstrVal = std::mem::ManuallyDrop::new(BSTR::from(text));
    }
    v
}

fn bool_variant(value: bool) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_BOOL;
        inner.Anonymous.boolVal = if value {
            windows::Win32::Foundation::VARIANT_TRUE
        } else {
            windows::Win32::Foundation::VARIANT_FALSE
        };
    }
    v
}

fn middle(element: &IUIAutomationElement) -> Option<Control> {
    unsafe {
        let r = element.CurrentBoundingRectangle().ok()?;
        if r.right <= r.left || r.bottom <= r.top {
            return None;
        }
        Some(Control {
            name: element
                .CurrentName()
                .map(|n| agent::control_name(&n.to_string()).to_string())
                .unwrap_or_default(),
            x: ((r.left + r.right) / 2).into(),
            y: ((r.top + r.bottom) / 2).into(),
        })
    }
}

/// The middle of the control `name` means: the menus' items and the
/// window in front's controls as the core weighs them.
fn find(name: &str) -> Result<(i64, i64), String> {
    let failed = |e: windows::core::Error| format!("UI Automation failed: {}", e.message());
    let uia: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.map_err(failed)?;
    let shown = unsafe {
        uia.CreatePropertyConditionEx(
            UIA_IsOffscreenPropertyId,
            &bool_variant(false),
            PropertyConditionFlags_None,
        )
    }
    .map_err(failed)?;
    let named = unsafe {
        uia.CreatePropertyConditionEx(
            UIA_NamePropertyId,
            &text_variant(name),
            PropertyConditionFlags_IgnoreCase,
        )
    }
    .map_err(failed)?;
    let whole = unsafe { uia.CreateAndCondition(&named, &shown) }.map_err(failed)?;
    let window = unsafe { GetForegroundWindow() };
    let front = (!window.is_invalid())
        .then(|| unsafe { uia.ElementFromHandle(window) }.ok())
        .flatten();
    let mut controls = Vec::new();
    let mut anywhere = None;
    if let Some(front) = &front
        && let Ok(all) = unsafe { front.FindAll(TreeScope_Descendants, &shown) }
    {
        for i in 0..unsafe { all.Length() }.unwrap_or(0) {
            if let Some(c) = unsafe { all.GetElement(i) }.ok().as_ref().and_then(middle)
                && !c.name.is_empty()
            {
                controls.push(c);
            }
        }
    }
    let menus = menu_items();
    let wanted = name.to_lowercase();
    let known = menus
        .iter()
        .chain(&controls)
        .any(|c| c.name.to_lowercase() == wanted);
    if !known && let Ok(root) = unsafe { uia.GetRootElement() } {
        anywhere = unsafe { root.FindFirst(TreeScope_Descendants, &whole) }
            .ok()
            .as_ref()
            .and_then(middle);
    }
    agent::choose_control(
        name,
        &Found {
            menus,
            front: controls,
            anywhere,
        },
    )
}
