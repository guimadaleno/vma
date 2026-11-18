//! VirtualBox through its COM API: its machines, made, changed, started,
//! stopped and removed, their media, a system installed on a new one with no
//! person, and, while one runs, its console: its screen, mouse and keyboard.
//! The names and numbers
//! called are VirtualBox.xidl's (7.2.8), as COM spells them. It asks and
//! reports; what is printed, and where a file the caller named is, are the
//! caller's.

use super::dispatch::{Arg, Object};
use crate::core::command::{Create, Install, Settings, Sharing};
use crate::core::input::{self, Act};
use crate::core::{install, machine, version};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// `BitmapFormat_PNG`.
const PNG: u32 = 0x2047_4E50;
/// `BitmapFormat_BGRA`: the screen's pixels as they are, 4 bytes each.
const BGRA: u32 = 0x4152_4742;
/// `VMExecutionEngine_NativeApi`: VirtualBox runs the machine through
/// Windows' hypervisor API, its NEM mode, as where Hyper-V holds VT-x.
const NATIVE_API: u32 = 3;
/// How often a wait through an install looks for a restart that hung.
const LOOK: Duration = Duration::from_secs(10);
/// `LockType_Shared`: the machine's own window and another vma keep their
/// lock alongside it, and a running machine takes changes through it.
const SHARED: u32 = 1;
/// `LockType_Write`: the one lock on a machine that is off, to change it.
const WRITE: u32 = 2;
/// `SessionState_Unlocked` and `_Locked`: a machine no session holds, and
/// a session holding its machine.
const UNLOCKED: u32 = 1;
const LOCKED: u32 = 2;
/// `PlatformArchitecture_x86`.
const X86: u32 = 1;
/// `AccessMode`.
const READ_ONLY: u32 = 1;
const READ_WRITE: u32 = 2;
/// `DeviceType`.
const DVD: u32 = 2;
const HARD_DISK: u32 = 3;
/// `CleanupMode_UnregisterOnly`.
const UNREGISTER_ONLY: u32 = 1;
/// `CleanupMode_DetachAllReturnHardDisksAndVMRemovable`: its disks, and
/// media in its own folder that only it uses (as an unattended install's),
/// are given back to be deleted; an ISO from elsewhere stays where it is.
const DISKS_AND_OWN_MEDIA: u32 = 5;
/// `MediumVariant_Standard`: a disk that grows as it fills.
const STANDARD: i32 = 0;
/// `AdditionsRunLevelType_Desktop`: someone is signed in.
const DESKTOP: u32 = 3;
/// `MachineState_Paused`.
const PAUSED: u32 = 7;
/// `MediumState_Created`: a medium no one holds.
const CREATED: u32 = 1;
/// `MediumState_Inaccessible`: as VirtualBox's service reads each medium it
/// knows when it starts again, until something checks it.
const INACCESSIBLE: u32 = 4;
/// `MachineState_Running`.
const RUNNING: u32 = 6;
const GB: i64 = 1 << 30;
/// The ports a new machine's disk controller has: its disk, its optical
/// drive, and room for disks added while it runs.
const SPARE_PORTS: u32 = 8;
const POLL: Duration = Duration::from_millis(500);
/// How long a part of keys may take to reach the PS/2 keyboard's queue
/// before it is taken as read before a look found it: a part was seen
/// there 0.35 ms after it was sent (median of 60), 16 ms at most.
const ARRIVAL: Duration = Duration::from_millis(100);
/// How long keys may wait, none taken, before vma gives up on the machine
/// reading them: a machine just installed, busy with its first sign-in in
/// VirtualBox's NEM mode, took none for over 10 s, and then all.
const STALL: Duration = Duration::from_secs(60);
/// How long a part is looked for at once after it is sent, before the
/// looks rest between them.
const QUICK: Duration = Duration::from_millis(2);

pub struct VirtualBox(Object);

pub struct Machine {
    pub name: String,
    object: Object,
}

/// A running machine's console, under a shared lock that is let go when
/// this is dropped. The console goes first, as fields drop in order.
pub struct Console {
    console: Object,
    /// The machine's name, for what is said of it.
    name: String,
    lock: Lock,
}

/// A session holding a lock on a machine, let go when dropped.
struct Lock(Object);

/// A machine's settings and media, as `status` shows them.
pub struct Details {
    /// VirtualBox's guest OS type, whose defaults it took.
    pub os: String,
    /// In MB.
    pub memory: u32,
    pub cpus: u32,
    pub cpu_cap: u32,
    /// In MB.
    pub vram: u32,
    pub clipboard: Option<Sharing>,
    pub drag_and_drop: Option<Sharing>,
    /// Each disk's size in bytes, and its file.
    pub disks: Vec<(i64, String)>,
    /// Each optical drive's file, `None` while empty.
    pub dvds: Vec<Option<String>>,
}

impl VirtualBox {
    pub fn connect() -> Result<VirtualBox, String> {
        super::dispatch::init()?;
        let root = Object::create("VirtualBox.VirtualBox")?;
        version::check(&root.get("Version")?.string()?)?;
        Ok(VirtualBox(root))
    }

    /// Every machine's state word and name. A machine whose settings can't
    /// be read has no name to give, so its ID stands for it.
    pub fn list(&self) -> Result<Vec<(String, String)>, String> {
        let mut list = Vec::new();
        for object in self.0.get("Machines")?.objects()? {
            if object.get("Accessible")?.bool()? {
                let state = machine::state_word(object.get("State")?.u32()?);
                list.push((state.to_string(), object.get("Name")?.string()?));
            } else {
                list.push(("inaccessible".into(), object.get("Id")?.string()?));
            }
        }
        Ok(list)
    }

    /// The machine named or with this ID.
    pub fn find(&self, name: &str) -> Result<Machine, String> {
        let object = self
            .0
            .call("FindMachine", &mut [Arg::Str(name)])?
            .object()?
            .ok_or_else(|| format!("VirtualBox has no machine named {name}"))?;
        Ok(Machine {
            name: name.to_string(),
            object,
        })
    }

    /// The machine `named`, or the one `machine::choose` means, which `ask`
    /// says how to name where it can't choose.
    pub fn machine(&self, named: Option<&str>, ask: &str) -> Result<Machine, String> {
        if let Some(name) = named {
            return self.find(name);
        }
        let mut found = Vec::new();
        let mut listed = Vec::new();
        for object in self.0.get("Machines")?.objects()? {
            // A machine whose settings can't be read answers nothing but
            // this; it is never the one meant, as it can't run.
            if !object.get("Accessible")?.bool()? {
                continue;
            }
            listed.push((object.get("Name")?.string()?, object.get("State")?.u32()?));
            found.push(object);
        }
        let index = machine::choose(&listed, ask)?;
        Ok(Machine {
            name: listed.swap_remove(index).0,
            object: found.swap_remove(index),
        })
    }

    /// A new machine, all of it or nothing: what a failure leaves half
    /// made is removed. `iso`, and the ISO of `c`'s install, `install_iso`,
    /// are whole Windows paths.
    pub fn create(
        &self,
        c: &Create,
        iso: Option<&str>,
        install_iso: Option<&str>,
    ) -> Result<Machine, String> {
        if self.find(&c.name).is_ok() {
            return Err(format!("a machine named {} exists", c.name));
        }
        // The install's ISO is read before anything is made, so one that
        // holds no Windows, or not the image asked, changes nothing.
        let installer = match (&c.install, install_iso) {
            (Some(i), Some(path)) => Some(self.installer(i, path)?),
            _ => None,
        };
        let os_id = match (&c.os, &installer) {
            (Some(os), _) => os.clone(),
            (None, Some((_, detected))) => detected.clone(),
            (None, None) => "Windows11_64".to_string(),
        };
        let os = self.os_type(&os_id)?;
        // Opened before anything is made, so a file VirtualBox can't read
        // changes nothing.
        let medium = iso
            .map(|iso| self.medium(iso, DVD, READ_ONLY))
            .transpose()?;
        let object = self
            .0
            .call(
                "CreateMachine",
                &mut [
                    Arg::Str(""),
                    Arg::Str(&c.name),
                    Arg::U32(X86),
                    Arg::Strs(&[]),
                    Arg::Str(&os_id),
                    Arg::Str(""),
                    Arg::Str(""),
                    Arg::Str(""),
                    Arg::Str(""),
                ],
            )?
            .object()?
            .ok_or("VirtualBox made no machine")?;
        // Saved first, as VBoxManage does: the save makes the machine's
        // folder, where the defaults keep a UEFI machine's variable store.
        object.call("SaveSettings", &mut [])?;
        let registered = object
            .call("ApplyDefaults", &mut [Arg::Str("")])
            .and_then(|_| apply(&object, &c.settings))
            .and_then(|_| object.call("SaveSettings", &mut []))
            .and_then(|_| self.0.call("RegisterMachine", &mut [Arg::Object(&object)]));
        if let Err(e) = registered {
            let deleted = object
                .call("DeleteConfig", &mut [Arg::Objects(&[])])
                .and_then(|v| v.object())
                .and_then(|p| p.map_or(Ok(()), |p| wait(&p)));
            return Err(match deleted {
                Ok(()) => e,
                Err(also) => format!("{e}; removing what was made failed too: {also}"),
            });
        }
        let machine = Machine {
            name: c.name.clone(),
            object,
        };
        // The disk comes last: one attached by a change that is then
        // discarded stays bound to the machine, and VirtualBox neither gives
        // it back on removal nor lets it go until its service restarts.
        let made = machine.change(|m| {
            // An optical drive, empty if no ISO is given, so one can be put
            // in while it runs, when no drive can be added.
            match dvd_drive(m)? {
                Some((controller, port, device)) => {
                    if let Some(medium) = &medium {
                        mount(m, &controller, port, device, medium)?
                    }
                }
                None => attach(
                    m,
                    medium.as_ref(),
                    DVD,
                    os.get("RecommendedDVDStorageBus")?.u32()?,
                )?,
            }
            let bus = os.get("RecommendedHDStorageBus")?.u32()?;
            spare_ports(m, bus)?;
            let bytes = match c.disk {
                Some(gb) => gb as i64 * GB,
                None => os.get("RecommendedHDD")?.i64()?,
            };
            let (_, stem) = place(m)?;
            self.add_disk(m, &format!("{stem}.vdi"), bytes, bus)
        });
        // The install comes once the machine is whole: VirtualBox writes its
        // answer file and puts its own medium in the drive, which boots the
        // ISO with no key to press and carries the Guest Additions.
        let made = made.and_then(|_| match &installer {
            Some((u, _)) => u
                .set("Machine", Arg::Object(&machine.object))
                .and_then(|_| u.call("Prepare", &mut []))
                .and_then(|_| u.call("ConstructMedia", &mut []))
                .and_then(|_| u.call("ReconfigureVM", &mut []))
                .map(drop),
            None => Ok(()),
        });
        if let Err(e) = made {
            return Err(match self.remove(&machine.object) {
                Ok(()) => e,
                Err(also) => format!("{e}; removing what was made failed too: {also}"),
            });
        }
        Ok(machine)
    }

    /// VirtualBox's installer for the Windows on `iso`, a whole Windows
    /// path, set for `i`, and the guest OS type of the system it installs.
    fn installer(&self, i: &Install, iso: &str) -> Result<(Object, String), String> {
        let u = self
            .0
            .call("CreateUnattendedInstaller", &mut [])?
            .object()?
            .ok_or("VirtualBox made no installer")?;
        u.set("IsoPath", Arg::Str(iso))?;
        // It fails as not implemented even where it found the system, as
        // only OS/2's detection is finished (UnattendedImpl.cpp, 7.2.8):
        // what it found is read instead, and none is the failure.
        let _ = u.call("DetectIsoOS", &mut []);
        install::check_system(&i.iso, &u.get("DetectedOSTypeId")?.string()?)?;
        let names = u.get("DetectedImageNames")?.strings()?;
        let numbers = u.get("DetectedImageIndices")?.u32s()?;
        let images: Vec<(u32, String)> = numbers.into_iter().zip(names).collect();
        u.set("ImageIndex", Arg::U32(install::image(i.image, &images)?))?;
        if let Some(user) = &i.user {
            u.set("User", Arg::Str(user))?;
        }
        u.set("UserPassword", Arg::Str(&i.password))?;
        u.set("InstallGuestAdditions", Arg::Bool(true))?;
        // US English formats and system locale, where VirtualBox would take
        // the host's; its answer file keeps the keyboard US English whatever
        // this is, and takes the host's time zone.
        u.set("Locale", Arg::Str("en_US"))?;
        // The image chosen may be of another system than the first.
        let os = u.get("DetectedOSTypeId")?.string()?;
        Ok((u, os))
    }

    /// Removes a machine that is off: its disks, settings and folder.
    pub fn destroy(&self, name: &str) -> Result<(), String> {
        let machine = self.find(name)?;
        // One whose settings can't be read can only be unregistered: it has
        // no state to ask and no media to give back, and its files, wherever
        // they went, are out of reach.
        if !machine.object.get("Accessible")?.bool()? {
            return machine
                .object
                .call("Unregister", &mut [Arg::U32(UNREGISTER_ONLY)])
                .map(drop);
        }
        if machine::is_online(machine.state()?) {
            return Err(format!("{name} is running: stop it first"));
        }
        // A machine just powered off is still let go by its process, which
        // holds a log in its folder until then: the folder would stay.
        let until = Instant::now() + Duration::from_secs(10);
        while machine.object.get("SessionState")?.u32()? != UNLOCKED && Instant::now() < until {
            sleep(Duration::from_millis(200));
        }
        self.remove(&machine.object)
    }

    /// Unregisters a machine and deletes its settings, its folder, the
    /// media `DISKS_AND_OWN_MEDIA` gives back, and its install's medium
    /// where no drive holds it, which that doesn't give back.
    fn remove(&self, object: &Object) -> Result<(), String> {
        self.delete_install_media(object)?;
        let media = object.call("Unregister", &mut [Arg::U32(DISKS_AND_OWN_MEDIA)])?;
        let progress = object
            .call("DeleteConfig", &mut [Arg::Value(&media)])?
            .object()?
            .ok_or("VirtualBox gave no progress for the removal")?;
        wait(&progress)
    }

    /// Deletes the media an install made for machine `m` that no drive holds
    /// any longer, its own medium once ejected or never put in: it is of no
    /// use outside the drive, and VirtualBox deletes with it the files it
    /// was made of, the answer file and its password among them. One the
    /// machine still holds, as it does one ejected while it runs until it
    /// stops, is left for the next time it is off.
    fn delete_install_media(&self, m: &Object) -> Result<(), String> {
        let (folder, _) = place(m)?;
        let id = m.get("Id")?.string()?;
        for medium in self.0.get("DVDImages")?.objects()? {
            let location = medium.get("Location")?.string()?;
            let Some((dir, file)) = location.rsplit_once('\\') else {
                continue;
            };
            if !dir.eq_ignore_ascii_case(&folder)
                || !install::is_install_file(file, &id)
                || !medium.get("MachineIds")?.strings()?.is_empty()
                || !matches!(medium.get("State")?.u32()?, CREATED | INACCESSIBLE)
            {
                continue;
            }
            let progress = medium
                .call("DeleteStorage", &mut [])?
                .object()?
                .ok_or("VirtualBox gave no progress for the install's medium")?;
            wait(&progress)?;
        }
        Ok(())
    }

    fn os_type(&self, id: &str) -> Result<Object, String> {
        self.0
            .call("GetGuestOSType", &mut [Arg::Str(id)])?
            .object()?
            .ok_or_else(|| format!("VirtualBox knows no system named {id}"))
    }

    /// A medium already known to VirtualBox, or one opened from its file.
    fn medium(&self, location: &str, device: u32, access: u32) -> Result<Object, String> {
        self.0
            .call(
                "OpenMedium",
                &mut [
                    Arg::Str(location),
                    Arg::U32(device),
                    Arg::U32(access),
                    Arg::Bool(false),
                ],
            )?
            .object()?
            .ok_or_else(|| format!("VirtualBox didn't open {location}"))
    }

    /// A new disk of `bytes`, growing as it fills, named `file` in the
    /// machine's folder, attached to its controller on `bus`. A disk that
    /// can't be attached is deleted.
    fn add_disk(&self, m: &Object, file: &str, bytes: i64, bus: u32) -> Result<(), String> {
        let (folder, _) = place(m)?;
        let location = format!("{folder}\\{file}");
        let disk = self
            .0
            .call(
                "CreateMedium",
                &mut [
                    Arg::Str("VDI"),
                    Arg::Str(&location),
                    Arg::U32(READ_WRITE),
                    Arg::U32(HARD_DISK),
                ],
            )?
            .object()?
            .ok_or("VirtualBox made no disk")?;
        let progress = disk
            .call(
                "CreateBaseStorage",
                &mut [Arg::I64(bytes), Arg::I32s(&[STANDARD])],
            )?
            .object()?
            .ok_or("VirtualBox gave no progress for the disk")?;
        wait(&progress)?;
        attach(m, Some(&disk), HARD_DISK, bus).inspect_err(|_| {
            if let Ok(Some(progress)) = disk.call("DeleteStorage", &mut []).and_then(|v| v.object())
            {
                let _ = wait(&progress);
            }
        })
    }
}

impl Machine {
    /// VirtualBox's `MachineState`, a number `machine::state_word` reads.
    pub fn state(&self) -> Result<u32, String> {
        self.object.get("State")?.u32()
    }

    pub fn details(&self) -> Result<Details, String> {
        let m = &self.object;
        let graphics = m
            .get("GraphicsAdapter")?
            .object()?
            .ok_or("no graphics adapter")?;
        let mut details = Details {
            os: m.get("OSTypeId")?.string()?,
            memory: m.get("MemorySize")?.u32()?,
            cpus: m.get("CPUCount")?.u32()?,
            cpu_cap: m.get("CPUExecutionCap")?.u32()?,
            vram: graphics.get("VRAMSize")?.u32()?,
            clipboard: Sharing::from_number(m.get("ClipboardMode")?.u32()?),
            drag_and_drop: Sharing::from_number(m.get("DnDMode")?.u32()?),
            disks: Vec::new(),
            dvds: Vec::new(),
        };
        for attachment in m.get("MediumAttachments")?.objects()? {
            let medium = attachment.get("Medium")?.object()?;
            match attachment.get("Type")?.u32()? {
                HARD_DISK => {
                    if let Some(disk) = medium {
                        details.disks.push((
                            disk.get("LogicalSize")?.i64()?,
                            disk.get("Location")?.string()?,
                        ));
                    }
                }
                DVD => details.dvds.push(
                    medium
                        .map(|iso| iso.get("Location")?.string())
                        .transpose()?,
                ),
                _ => {}
            }
        }
        Ok(details)
    }

    pub fn set(&self, settings: &Settings) -> Result<(), String> {
        self.change(|m| apply(m, settings))
    }

    /// Turns it on, or lets a paused one go on, then waits for its desktop,
    /// saying to `say` what it does on the way.
    pub fn start(
        &self,
        vbox: &VirtualBox,
        headless: bool,
        timeout: u32,
        say: &mut dyn FnMut(&str),
    ) -> Result<(), String> {
        let state = self.state()?;
        if state == PAUSED {
            self.console()?.console.call("Resume", &mut [])?;
        } else if !machine::is_online(state) {
            // An install's medium ejected while it ran is let go by now.
            vbox.delete_install_media(&self.object)?;
            let session = Object::create("VirtualBox.Session")?;
            let front = if headless { "headless" } else { "gui" };
            let progress = self
                .object
                .call(
                    "LaunchVMProcess",
                    &mut [Arg::Object(&session), Arg::Str(front), Arg::Strs(&[])],
                )?
                .object()?
                .ok_or("VirtualBox gave no progress for the start")?;
            let lock = Lock(session);
            wait(&progress)?;
            drop(lock);
        }
        let until = Instant::now() + Duration::from_secs(timeout.into());
        let console = self.console()?;
        let guest = console
            .console
            .get("Guest")?
            .object()?
            .ok_or("the machine has no guest")?;
        let stopped = |state| {
            format!(
                "{} stopped before its desktop came: {}",
                self.name,
                machine::state_word(state)
            )
        };
        // A machine installing with no person, under the mode where its
        // restarts hang, is reset where one hung, as a person would.
        let mut restarts = (self.installing()? && console.engine()? == NATIVE_API)
            .then(install::Restarts::default);
        let begun = Instant::now();
        let mut look = begun;
        loop {
            super::stopped()?;
            // The state first: the guest of a machine gone off answers
            // nothing.
            let state = self.state()?;
            if !machine::is_online(state) {
                return Err(stopped(state));
            }
            // VirtualBox pauses a machine itself, as when the host's disk is
            // full, and a paused one gets no further.
            if state == PAUSED {
                return Err(format!(
                    "{} paused before its desktop came: vma start lets it go on",
                    self.name
                ));
            }
            match guest.get("AdditionsRunLevel").and_then(|v| v.u32()) {
                Ok(level) if level >= DESKTOP => return Ok(()),
                // Restarts come before the Guest Additions run, and a
                // machine they run on is alive; only a running machine is
                // reset, as one saving or taking a snapshot is busy.
                Ok(0) if Instant::now() >= look && state == RUNNING => {
                    look += LOOK;
                    if let Some(r) = &mut restarts {
                        // A look that fails, as while the screen changes
                        // its mode, saw nothing: the watch starts over.
                        let hung = match (console.storage_bytes(), console.picture()) {
                            (Ok(bytes), Ok(picture)) => r.hung(begun.elapsed(), bytes, picture),
                            _ => {
                                r.forget();
                                false
                            }
                        };
                        if hung && r.gave_up() {
                            return Err(format!(
                                "{}'s screen and disks stood still again after {} resets: vma shot shows what it shows",
                                self.name,
                                install::MAX_RESETS
                            ));
                        }
                        if hung {
                            console.console.call("Reset", &mut [])?;
                            r.reset();
                            say(&format!(
                                "{}'s screen and disks stood still for {} s, as a restart's do when it hangs in VirtualBox's NEM mode: reset",
                                self.name,
                                install::HUNG_AFTER.as_secs()
                            ));
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    let state = self.state()?;
                    return Err(if machine::is_online(state) {
                        e
                    } else {
                        stopped(state)
                    });
                }
            }
            if Instant::now() >= until {
                return Err(format!(
                    "{} is on, but its desktop didn't come in {timeout} s: vma shot shows what it shows",
                    self.name
                ));
            }
            sleep(POLL);
        }
    }

    /// Asks it to shut down, as its power button would, and cuts the power
    /// after 90 s, or at once when `force` or while it is paused, as a
    /// paused machine can't hear the button.
    pub fn stop(&self, vbox: &VirtualBox, force: bool) -> Result<(), String> {
        self.power_off(force)?;
        // An install's medium ejected while it ran is let go once it is off.
        vbox.delete_install_media(&self.object)
    }

    fn power_off(&self, force: bool) -> Result<(), String> {
        let state = self.state()?;
        if !machine::is_online(state) {
            return Ok(());
        }
        let console = self.console()?;
        if !force && state != PAUSED {
            console.console.call("PowerButton", &mut [])?;
            let until = Instant::now() + Duration::from_secs(90);
            while Instant::now() < until {
                super::stopped()?;
                if !machine::is_online(self.state()?) {
                    return Ok(());
                }
                sleep(POLL);
            }
        }
        let progress = console
            .console
            .call("PowerDown", &mut [])?
            .object()?
            .ok_or("VirtualBox gave no progress for the power")?;
        wait(&progress)
    }

    /// Puts the ISO at `iso`, a whole Windows path, in its optical drive,
    /// adding one to a machine that is off and has none.
    pub fn insert(&self, vbox: &VirtualBox, iso: &str) -> Result<(), String> {
        let medium = vbox.medium(iso, DVD, READ_ONLY)?;
        let os = vbox.os_type(&self.object.get("OSTypeId")?.string()?)?;
        self.change(|m| match dvd_drive(m)? {
            Some((controller, port, device)) => mount(m, &controller, port, device, &medium),
            None => attach(
                m,
                Some(&medium),
                DVD,
                os.get("RecommendedDVDStorageBus")?.u32()?,
            ),
        })
    }

    /// Empties its optical drive, and deletes an install's own medium
    /// taken out of it.
    pub fn eject(&self, vbox: &VirtualBox) -> Result<(), String> {
        self.change(|m| {
            let (controller, port, device) =
                dvd_drive(m)?.ok_or_else(|| format!("{} has no optical drive", self.name))?;
            m.call(
                "UnmountMedium",
                &mut [
                    Arg::Str(&controller),
                    Arg::I32(port),
                    Arg::I32(device),
                    Arg::Bool(false),
                ],
            )
            .map(drop)
        })?;
        vbox.delete_install_media(&self.object)
    }

    /// A new disk of `gb`, named after the machine and the first number
    /// free in its folder.
    pub fn add_disk(&self, vbox: &VirtualBox, gb: u32) -> Result<(), String> {
        let os = vbox.os_type(&self.object.get("OSTypeId")?.string()?)?;
        let bus = os.get("RecommendedHDStorageBus")?.u32()?;
        let (folder, stem) = place(&self.object)?;
        let file = (2..)
            .map(|n| format!("{stem}-{n}.vdi"))
            .find(|f| !std::path::Path::new(&format!("{folder}\\{f}")).exists())
            .expect("a free name before the numbers run out");
        self.change(|m| vbox.add_disk(m, &file, gb as i64 * GB, bus))
    }

    pub fn console(&self) -> Result<Console, String> {
        if !machine::is_online(self.state()?) {
            return Err(format!("{} is not running", self.name));
        }
        let session = Object::create("VirtualBox.Session")?;
        self.object.call(
            "LockMachine",
            &mut [Arg::Object(&session), Arg::U32(SHARED)],
        )?;
        let lock = Lock(session);
        let console = lock
            .0
            .get("Console")?
            .object()?
            .ok_or_else(|| format!("{} is not running", self.name))?;
        Ok(Console {
            console,
            name: self.name.clone(),
            lock,
        })
    }

    /// Whether its optical drive holds the medium of its install, so it is
    /// installing, or about to.
    pub fn installing(&self) -> Result<bool, String> {
        let id = self.object.get("Id")?.string()?;
        for attachment in self.object.get("MediumAttachments")?.objects()? {
            if attachment.get("Type")?.u32()? != DVD {
                continue;
            }
            if let Some(medium) = attachment.get("Medium")?.object()? {
                let location = medium.get("Location")?.string()?;
                let file = location.rsplit('\\').next().unwrap_or(&location);
                if install::is_install_file(file, &id) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Runs `change` on the machine as a session holds it, under a shared
    /// lock while it runs and a write lock while it is off, and saves what
    /// it changed; a failure discards it.
    fn change<R>(&self, change: impl FnOnce(&Object) -> Result<R, String>) -> Result<R, String> {
        let lock_type = if machine::is_online(self.state()?) {
            SHARED
        } else {
            WRITE
        };
        let session = Object::create("VirtualBox.Session")?;
        self.object.call(
            "LockMachine",
            &mut [Arg::Object(&session), Arg::U32(lock_type)],
        )?;
        let lock = Lock(session);
        let m = lock
            .0
            .get("Machine")?
            .object()?
            .ok_or("the session has no machine")?;
        let done = change(&m).and_then(|r| m.call("SaveSettings", &mut []).map(|_| r));
        if done.is_err() {
            let _ = m.call("DiscardSettings", &mut []);
        }
        done
    }
}

impl Console {
    fn display(&self) -> Result<Object, String> {
        self.console
            .get("Display")?
            .object()?
            .ok_or_else(|| "the machine has no display".to_string())
    }

    /// The first screen's width and height, in pixels.
    pub fn screen(&self) -> Result<(u32, u32), String> {
        screen(&self.display()?)
    }

    /// A PNG of the first screen, at its size.
    pub fn shot(&self) -> Result<Vec<u8>, String> {
        let display = self.display()?;
        let (width, height) = screen(&display)?;
        display
            .call(
                "TakeScreenShotToArray",
                &mut [
                    Arg::U32(0),
                    Arg::U32(width),
                    Arg::U32(height),
                    Arg::U32(PNG),
                ],
            )?
            .bytes()
    }

    fn debugger(&self) -> Result<Object, String> {
        self.console
            .get("Debugger")?
            .object()?
            .ok_or_else(|| "the machine has no debugger".to_string())
    }

    /// VirtualBox's `VMExecutionEngine` for the machine.
    fn engine(&self) -> Result<u32, String> {
        self.debugger()?.get("ExecutionEngine")?.u32()
    }

    /// The bytes all its disks have read and written since it started.
    fn storage_bytes(&self) -> Result<u64, String> {
        let stats = self
            .debugger()?
            .call(
                "GetStats",
                &mut [Arg::Str("/Public/Storage/*Bytes*"), Arg::Bool(false)],
            )?
            .string()?;
        Ok(install::storage_bytes(&stats))
    }

    /// What its first screen shows, as `install::picture` reads it; one not
    /// set shows nothing, as no pixels.
    fn picture(&self) -> Result<u64, String> {
        let display = self.display()?;
        let (width, height) = screen(&display)?;
        if width == 0 || height == 0 {
            return Ok(install::picture(&[]));
        }
        let pixels = display
            .call(
                "TakeScreenShotToArray",
                &mut [
                    Arg::U32(0),
                    Arg::U32(width),
                    Arg::U32(height),
                    Arg::U32(BGRA),
                ],
            )?
            .bytes()?;
        Ok(install::picture(&pixels))
    }

    /// Sends `acts` to the mouse and keyboard in order, waiting through
    /// their pauses, to a machine that is running. Should one fail, the
    /// keys it left down are let go.
    pub fn act(&self, acts: &[Act]) -> Result<(), String> {
        if acts.iter().any(|a| !matches!(a, Act::Pause(_))) {
            machine::takes_input(&self.name, self.state()?)?;
        }
        if acts.iter().any(|a| matches!(a, Act::Keys(_))) {
            let m = self.console.get("Machine")?.object()?;
            let m = m.ok_or("the console has no machine")?;
            machine::types_on(&self.name, m.get("KeyboardHIDType")?.u32()?)?;
        }
        let device = |name: &str| {
            self.console
                .get(name)?
                .object()?
                .ok_or_else(|| format!("the machine has no {name}"))
        };
        let (mouse, keyboard, debugger) = (device("Mouse")?, device("Keyboard")?, self.debugger()?);
        let mut doubt = false;
        let done = acts.iter().try_for_each(|act| match act {
            // VirtualBox's pixels start at 1.
            Act::Mouse {
                x,
                y,
                buttons,
                wheel,
            } => mouse
                .call(
                    "PutMouseEventAbsolute",
                    &mut [
                        Arg::I32(*x as i32 + 1),
                        Arg::I32(*y as i32 + 1),
                        Arg::I32(*wheel),
                        Arg::I32(0),
                        Arg::I32(*buttons as i32),
                    ],
                )
                .map(drop),
            Act::Keys(codes) => self.keys(&keyboard, &debugger, codes, &mut doubt),
            Act::Pause(ms) => {
                sleep(Duration::from_millis((*ms).into()));
                Ok(())
            }
        });
        if done.is_err() {
            let _ = keyboard.call("ReleaseKeys", &mut []);
        }
        done
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The host's folder `path` shared with the machine as `name`, for as
    /// long as it runs, writable; one of that name sharing another folder
    /// gives way. Says whether it shared it now.
    pub fn share(&self, name: &str, path: &str) -> Result<bool, String> {
        for folder in self.console.get("SharedFolders")?.objects()? {
            if folder.get("Name")?.string()? != name {
                continue;
            }
            if folder.get("HostPath")?.string()?.eq_ignore_ascii_case(path) {
                return Ok(false);
            }
            self.console
                .call("RemoveSharedFolder", &mut [Arg::Str(name)])?;
        }
        self.console
            .call(
                "CreateSharedFolder",
                &mut [
                    Arg::Str(name),
                    Arg::Str(path),
                    Arg::Bool(true),
                    Arg::Bool(false),
                    Arg::Str(""),
                ],
            )
            .map(|_| true)
    }

    /// Whether its session still holds the machine: VirtualBox lets it go
    /// when the machine stops, and a console kept from before is no use.
    pub fn held(&self) -> bool {
        self.lock.0.get("State").and_then(|s| s.u32()).ok() == Some(LOCKED)
    }

    /// `MachineState`, as the console sees it.
    fn state(&self) -> Result<u32, String> {
        self.console.get("State")?.u32()
    }

    /// Sends `codes` as fast as the machine reads them: its PS/2 keyboard
    /// drops a key its queue can't hold, with no word, so each part goes
    /// once the queue reads empty, and the last has been read when this
    /// returns. `doubt` is whether the part sent before was never seen in
    /// the queue.
    fn keys(
        &self,
        keyboard: &Object,
        debugger: &Object,
        codes: &[u8],
        doubt: &mut bool,
    ) -> Result<(), String> {
        for part in input::parts(codes) {
            let part: Vec<i32> = part.iter().map(|&c| c.into()).collect();
            let stored = keyboard
                .call("PutScancodes", &mut [Arg::I32s(&part)])?
                .u32()?;
            if stored as usize != part.len() {
                return Err(format!(
                    "{}'s keyboard took {stored} of {} keys",
                    self.name,
                    part.len()
                ));
            }
            self.read_out(debugger, doubt)?;
        }
        Ok(())
    }

    /// Waits until a part just sent has reached the PS/2 queue and the
    /// machine has read the queue empty. A part not seen there in
    /// `ARRIVAL` was read before a look found it, or is still on its way,
    /// held by a machine that stopped running: one that stopped fails
    /// here, and its keys go to it when it goes on. Two parts not seen in
    /// a row are waited for until one shows, so no more than two are ever
    /// on their way, which the queue holds together.
    fn read_out(&self, debugger: &Object, doubt: &mut bool) -> Result<(), String> {
        let sent = Instant::now();
        let (mut seen, mut checked) = (false, false);
        let (mut last, mut since) = (0, sent);
        loop {
            super::stopped()?;
            let info = debugger
                .call("Info", &mut [Arg::Str("ps2k"), Arg::Str("")])?
                .string()?;
            let waiting = input::queued(&info)
                .ok_or("VirtualBox's debugger didn't tell the PS/2 keyboard's queue")?;
            seen |= waiting > 0;
            if waiting != last {
                (last, since) = (waiting, Instant::now());
            }
            if waiting == 0 && seen {
                *doubt = false;
                return Ok(());
            }
            if since.elapsed() >= STALL {
                return Err(format!(
                    "{} took no key in {} s",
                    self.name,
                    STALL.as_secs()
                ));
            }
            if waiting > 0 {
                // The machine reads a byte a millisecond at most.
                sleep(Duration::from_millis(waiting.into()));
                continue;
            }
            if !checked && sent.elapsed() >= ARRIVAL {
                machine::takes_input(&self.name, self.state()?)?;
                checked = true;
                if !*doubt {
                    *doubt = true;
                    return Ok(());
                }
            }
            if sent.elapsed() >= QUICK {
                sleep(Duration::from_millis(1));
            }
        }
    }

    /// The Guest Additions' version, where they run inside.
    pub fn additions(&self) -> Result<Option<String>, String> {
        let Some(guest) = self.console.get("Guest")?.object()? else {
            return Ok(None);
        };
        let version = guest.get("AdditionsVersion")?.string()?;
        Ok((!version.is_empty()).then_some(version))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.0.call("UnlockMachine", &mut []);
    }
}

fn screen(display: &Object) -> Result<(u32, u32), String> {
    let (mut width, mut height, mut bits) = (0u32, 0u32, 0u32);
    let (mut x, mut y, mut status) = (0i32, 0i32, 0i32);
    display.call(
        "GetScreenResolution",
        &mut [
            Arg::U32(0),
            Arg::OutU32(&mut width),
            Arg::OutU32(&mut height),
            Arg::OutU32(&mut bits),
            Arg::OutI32(&mut x),
            Arg::OutI32(&mut y),
            Arg::OutI32(&mut status),
        ],
    )?;
    Ok((width, height))
}

/// Sets on `m`, a machine that takes changes, each setting given.
fn apply(m: &Object, s: &Settings) -> Result<(), String> {
    if let Some(mb) = s.memory {
        m.set("MemorySize", Arg::U32(mb))?;
    }
    if let Some(n) = s.cpus {
        m.set("CPUCount", Arg::U32(n))?;
    }
    if let Some(percent) = s.cpu_cap {
        m.set("CPUExecutionCap", Arg::U32(percent))?;
    }
    if let Some(mb) = s.vram {
        m.get("GraphicsAdapter")?
            .object()?
            .ok_or("no graphics adapter")?
            .set("VRAMSize", Arg::U32(mb))?;
    }
    if let Some(mode) = s.clipboard {
        m.set("ClipboardMode", Arg::U32(mode as u32))?;
    }
    if let Some(mode) = s.drag_and_drop {
        m.set("DnDMode", Arg::U32(mode as u32))?;
    }
    Ok(())
}

/// The machine's folder, and the name of its settings file without
/// `.vbox`: its name as VirtualBox made it safe for a file, which its own
/// files are named after.
fn place(m: &Object) -> Result<(String, String), String> {
    let settings = m.get("SettingsFilePath")?.string()?;
    let (folder, file) = settings
        .rsplit_once('\\')
        .ok_or_else(|| format!("{settings} is no settings file"))?;
    let stem = file.strip_suffix(".vbox").unwrap_or(file);
    Ok((folder.to_string(), stem.to_string()))
}

/// Opens ports on the controller on `bus` up to `SPARE_PORTS`, as many as
/// it takes, so a disk can be added while the machine runs, when no port
/// can be opened.
fn spare_ports(m: &Object, bus: u32) -> Result<(), String> {
    for c in m.get("StorageControllers")?.objects()? {
        if c.get("Bus")?.u32()? == bus {
            let ports = SPARE_PORTS.min(c.get("MaxPortCount")?.u32()?);
            if c.get("PortCount")?.u32()? < ports {
                c.set("PortCount", Arg::U32(ports))?;
            }
            return Ok(());
        }
    }
    Ok(())
}

/// Blocks until a VirtualBox progress ends, and gives its failure.
fn wait(progress: &Object) -> Result<(), String> {
    progress.call("WaitForCompletion", &mut [Arg::I32(-1)])?;
    if progress.get("ResultCode")?.i32()? == 0 {
        return Ok(());
    }
    let info = progress.get("ErrorInfo")?.object()?;
    Err(match info {
        Some(info) => info.get("Text")?.string()?,
        None => format!("{} failed", progress.get("Description")?.string()?),
    })
}

/// The first optical drive's controller, port and device.
fn dvd_drive(m: &Object) -> Result<Option<(String, i32, i32)>, String> {
    for attachment in m.get("MediumAttachments")?.objects()? {
        if attachment.get("Type")?.u32()? == DVD {
            return Ok(Some((
                attachment.get("Controller")?.string()?,
                attachment.get("Port")?.i32()?,
                attachment.get("Device")?.i32()?,
            )));
        }
    }
    Ok(None)
}

fn mount(
    m: &Object,
    controller: &str,
    port: i32,
    device: i32,
    medium: &Object,
) -> Result<(), String> {
    m.call(
        "MountMedium",
        &mut [
            Arg::Str(controller),
            Arg::I32(port),
            Arg::I32(device),
            Arg::Object(medium),
            Arg::Bool(false),
        ],
    )
    .map(drop)
}

/// Attaches `medium` as a `device`, or an empty one, on the first free port
/// of the machine's controller on `bus`, opening one more port where all
/// are taken.
fn attach(m: &Object, medium: Option<&Object>, device: u32, bus: u32) -> Result<(), String> {
    let mut controller = None;
    for c in m.get("StorageControllers")?.objects()? {
        if c.get("Bus")?.u32()? == bus {
            controller = Some(c);
            break;
        }
    }
    let controller = controller.ok_or("the machine has no storage controller for this medium")?;
    let name = controller.get("Name")?.string()?;
    let mut taken = Vec::new();
    for attachment in m.get("MediumAttachments")?.objects()? {
        if attachment.get("Controller")?.string()? == name {
            taken.push(attachment.get("Port")?.i32()?);
        }
    }
    let ports = controller.get("PortCount")?.u32()? as i32;
    let port = (0..).find(|p| !taken.contains(p)).expect("a free port");
    if port >= ports {
        controller.set("PortCount", Arg::U32(port as u32 + 1))?;
    }
    m.call(
        "AttachDevice",
        &mut [
            Arg::Str(&name),
            Arg::I32(port),
            Arg::I32(0),
            Arg::U32(device),
            medium.map_or(Arg::NoObject, Arg::Object),
        ],
    )
    .map(drop)
}
