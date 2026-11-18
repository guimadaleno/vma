//! The host's end of vma's agent inside a machine: a folder of the host's,
//! `%LOCALAPPDATA%\vma\<machine>`, shared with the machine as `\\VBoxSvr\
//! vma` while it runs, holding the vma.exe the agent runs, the requests
//! written to it and the replies read back. Polled, as VirtualBox's shared
//! folders carry no change notifications: a request's round trip was
//! measured at 17 ms through it, before the agent was built. What a
//! request does inside is the agent's (`agent.rs`).

use super::vbox::Console;
use crate::core::agent::Role;
use crate::core::json::Json;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

/// The shared folder's name, as the machine reaches it, `\\VBoxSvr\vma`.
const SHARE: &str = "vma";
/// How often the host looks for a reply.
const LOOK: Duration = Duration::from_millis(1);
/// How old a reply or a file left behind may get before it is swept.
const STALE: Duration = Duration::from_secs(30 * 60);
/// A file that says an agent answered through the folder once.
const SEEN: &str = "seen";

pub struct Link {
    folder: PathBuf,
    /// Whether the share or its vma.exe was put there by this open, so
    /// the agent may still be on its way.
    pub fresh: bool,
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> String + '_ {
    move |e| format!("{}: {e}", path.display())
}

impl Link {
    /// The machine's folder, made and shared where it isn't, with this
    /// vma.exe in it, which an agent running another takes.
    pub fn open(console: &Console) -> Result<Link, String> {
        let folder = folder(console.name())?;
        for sub in ["in\\user", "in\\admin", "out", "files"] {
            let path = folder.join(sub);
            fs::create_dir_all(&path).map_err(io(&path))?;
        }
        sweep(&folder.join("out"));
        sweep(&folder.join("files"));
        let put = put_executable(&folder)?;
        let shared = console.share(SHARE, &folder.to_string_lossy())?;
        Ok(Link {
            folder,
            fresh: put || shared,
        })
    }

    /// Whether an agent ever answered through this folder: one that
    /// doesn't now is starting or busy, not missing.
    pub fn seen(&self) -> bool {
        self.folder.join(SEEN).is_file()
    }

    /// Where a file going in or out waits, under a name of its own.
    pub fn files(&self, id: &str) -> PathBuf {
        self.folder.join("files").join(id)
    }

    /// Asks `role`'s agent `op` with `fields`, and gives its reply, or its
    /// failure; one not answered in `seconds` is taken back.
    pub fn ask(
        &self,
        role: Role,
        op: &str,
        fields: Vec<(&str, Json)>,
        seconds: u64,
    ) -> Result<Json, String> {
        let id = self.send(role, op, fields)?;
        match self.reply(role, &id, Duration::from_secs(seconds))? {
            Some(reply) => Ok(reply),
            None => Err(format!(
                "the agent ({}) didn't answer in {seconds} s: vma status, or vma setup",
                role.word()
            )),
        }
    }

    /// Writes a request to `role`'s agent and gives its id.
    pub fn send(&self, role: Role, op: &str, fields: Vec<(&str, Json)>) -> Result<String, String> {
        let id = new_id();
        let mut members = vec![("id", Json::text(&id)), ("op", Json::text(op))];
        members.extend(fields);
        let inbox = self.folder.join("in").join(role.word());
        let temp = inbox.join(format!("{id}.tmp"));
        fs::write(&temp, Json::object(members).write()).map_err(io(&temp))?;
        let file = inbox.join(format!("{id}.json"));
        fs::rename(&temp, &file).map_err(io(&file))?;
        Ok(id)
    }

    /// The reply to request `id` of `role`'s, once it comes, or `None`
    /// after `wait`, its request taken back; a reply that says it failed is
    /// the failure.
    pub fn reply(&self, role: Role, id: &str, wait: Duration) -> Result<Option<Json>, String> {
        let file = self.folder.join("out").join(format!("{id}.json"));
        let until = Instant::now() + wait;
        let request = self
            .folder
            .join("in")
            .join(role.word())
            .join(format!("{id}.json"));
        while !file.is_file() {
            // A request given up is taken back, so it never runs late.
            if let Err(why) = super::stopped() {
                let _ = fs::remove_file(&request);
                return Err(why);
            }
            if Instant::now() >= until {
                let _ = fs::remove_file(&request);
                return Ok(None);
            }
            sleep(LOOK);
        }
        let _ = fs::write(self.folder.join(SEEN), "");
        let text = fs::read(&file).map_err(io(&file))?;
        let _ = fs::remove_file(&file);
        let reply = Json::parse(&String::from_utf8_lossy(&text))
            .map_err(|e| format!("the agent's reply isn't JSON: {e}"))?;
        match reply.get("error").and_then(Json::as_str) {
            Some(why) => Err(why.to_string()),
            None => Ok(Some(reply)),
        }
    }

    /// Which agents answer a ping within `wait`, both asked at once.
    pub fn answering(&self, wait: Duration) -> Result<Vec<Role>, String> {
        let roles = [Role::User, Role::Admin];
        let ids: Vec<String> = roles
            .iter()
            .map(|&r| self.send(r, "ping", vec![]))
            .collect::<Result<_, _>>()?;
        let until = Instant::now() + wait;
        let mut up = Vec::new();
        for (role, id) in roles.into_iter().zip(ids) {
            let left = until.saturating_duration_since(Instant::now());
            if self.reply(role, &id, left).ok().flatten().is_some() {
                up.push(role);
            }
        }
        Ok(up)
    }
}

/// An id of its own for a request or a file: the time and a count, as
/// two in one process never share one.
fn new_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{nanos:x}-{:x}-{:x}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Removes what a call left in `folder` long ago: a reply that came after
/// its call gave up, a file a call didn't take back.
fn sweep(folder: &Path) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > STALE);
        if old {
            let path = entry.path();
            let _ = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
        }
    }
}

/// Puts this vma.exe in the folder where it is newer than the one there,
/// and says whether it did: the agent then runs the newest any vma on the
/// host has shared, and two builds used by turns don't restart it at each
/// call. Written whole under a name of its own, with this one's time, and
/// moved over, a few times while the machine copies the old one.
fn put_executable(folder: &Path) -> Result<bool, String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let mine = fs::metadata(&me).map_err(io(&me))?;
    let built = mine.modified().map_err(io(&me))?;
    let target = folder.join("vma.exe");
    if let Ok(there) = fs::metadata(&target) {
        let same = there.len() == mine.len() && there.modified().ok() == Some(built);
        if same || there.modified().is_ok_and(|t| t > built) {
            return Ok(false);
        }
    }
    let temp = folder.join(format!("vma-{}.tmp", new_id()));
    fs::copy(&me, &temp).map_err(io(&temp))?;
    let stamped = fs::File::options()
        .write(true)
        .open(&temp)
        .and_then(|f| f.set_modified(built));
    if let Err(e) = stamped {
        let _ = fs::remove_file(&temp);
        return Err(format!("{}: {e}", temp.display()));
    }
    for attempt in 1.. {
        match fs::rename(&temp, &target) {
            Ok(()) => return Ok(true),
            Err(_) if attempt < 20 => sleep(Duration::from_millis(100)),
            Err(e) => {
                let _ = fs::remove_file(&temp);
                return Err(format!("{}: {e}", target.display()));
            }
        }
    }
    unreachable!("the loop returns")
}

/// Copies a file, or a folder and all it holds, on the host.
pub fn copy(from: &Path, to: &Path) -> Result<(), String> {
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

/// Opens each file under `path`, as the host's antivirus may take a file
/// once it lands in the folder, and VirtualBox then tells the machine only
/// "The parameter is incorrect": opened here first, the refusal comes with
/// its reason. Names a file from `base`.
pub fn readable(path: &Path, base: &Path) -> Result<(), String> {
    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(io(path))? {
            readable(&entry.map_err(io(path))?.path(), base)?;
        }
        return Ok(());
    }
    fs::File::open(path).map(drop).map_err(|e| {
        let name = path.strip_prefix(base).unwrap_or(path);
        format!(
            "the host can't read its copy of {}, as its antivirus may have taken it: {e}",
            name.display()
        )
    })
}

/// The host's folder vma shares with the machine `name`.
pub fn folder(name: &str) -> Result<PathBuf, String> {
    let safe: String = name
        .chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
    let local = std::env::var("LOCALAPPDATA").map_err(|_| "LOCALAPPDATA is not set")?;
    Ok(PathBuf::from(local).join("vma").join(safe))
}

/// The id a new file transfer goes under.
pub fn transfer_id() -> String {
    new_id()
}
