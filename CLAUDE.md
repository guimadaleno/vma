# VMA

VMA (VirtualBox Machine Agent) lets an agent use a VirtualBox machine as a
person at it would (its screen, mouse and keyboard), and do inside it what a
test needs. It is one small Windows executable, `vma.exe`, that is both a CLI
and an MCP server (`vma mcp`), and it replaced base's `vm`. What is left
is in the issues: the goal #8 next, then the plans, #1 to #3.

## Decided

- **Host:** Windows, through VirtualBox's COM API, called **by name** through
  IDispatch (`src/win/dispatch.rs`), which keeps vma apart from the
  interfaces' IDs, new whenever an interface changes. Enum numbers and
  method arguments still change between versions (a running machine is 5 in
  6.1, 6 from 7.0), so vma takes VirtualBox 7.2 and later 7.x, the versions
  it is checked against, and refuses others (`src/core/version.rs`). No
  PowerShell, no VBoxManage at run time. Linux and macOS hosts: #1.
- **Guest:** Windows only, for what runs inside. The screen, mouse and
  keyboard work with any guest, because they are VirtualBox's own devices.
- **Agent:** the same `vma.exe` inside (`boot`, `agent`, `install`),
  reached through a folder the host shares, polled: chosen by measurement
  (17 ms a round trip, against 4 ms for a serial port that only a machine
  off takes and one vma at a time can use).
- **Words:** English only, and not in a locale table. vma's readers are
  agents and scripts, and its messages are part of its interface, like the
  commands' names. This overrides *i18n* for this project.
- **Repository:** public, so everything in it follows *Openable*. Machines
  are "a machine", never their names. The names it can't do without:
  guimadaleno.com and its download host, the R2 account's endpoint in
  `site.yml`, and the owner as the author (`LICENSE`, the page's footer).

## Layout

| Path | What |
| --- | --- |
| `src/core/` | What vma decides without a system: the command line, which machine is meant and how its state reads, what an install takes from its ISO, what goes to the mouse and keyboard, MCP and its JSON, the agent's paths, windows and controls, paths between WSL and Windows. Tested on Linux. |
| `src/win/` | The Windows shell: COM by name (`dispatch.rs`), VirtualBox's machines, media, installer, console, mouse and keyboard (`vbox.rs`), the host's end of the agent, its shared folder (`link.rs`), the agent inside a machine (`agent.rs`), the commands, `batch` and the MCP server's loop, one session for a run (`mod.rs`). |
| `dist.sh` | Builds `dist/vma.exe` and holds it to its 1 MB budget. |
| `THIRD-PARTY-NOTICES.md` | The crates the Windows build links and their licences, compiled in and printed by `vma notices`: a crate added is added there in the same commit. |
| `.github/workflows/release.yml` | A `v*` tag builds `dist.sh`'s executable and publishes it through guimadaleno/download's `publish` to `download.guimadaleno.com/vma/`, and only on the owner's word. |
| `.github/actions/publish/` | guimadaleno/download's `publish/`, unchanged at the tag `release.yml` names, as a public repository can't use a private one's action: a change there is copied whole. |
| `site/` | The page at `guimadaleno.com/vma/`, one file with no build step, and its icons; `.github/workflows/site.yml` publishes it to the bucket's `page/` on a push that touches it. Its version, size and checksum come from `latest.json`, so a release needs no change here. |
| `tools/icon.py` | Draws the mark and writes the page's icons: run it after a change to the mark, and commit what it writes. |
| `update.pub` | The public half of vma's release key, which the publish action checks the manifest's signature against; the private half is the `UPDATE_KEY` secret and the owner's offline backup. |

## VirtualBox's API

The source of every name and number vma calls is `VirtualBox.xidl` at the tag
of the version on the host (7.2.8 when this was written):
`https://raw.githubusercontent.com/VirtualBox/virtualbox/v<version>/src/VBox/Main/idl/VirtualBox.xidl`.
Read a method or an enum there before calling it; COM spells the names with
a capital first letter (`takeScreenShotToArray` is `TakeScreenShotToArray`).
VBoxManage's own words (state names) are in
`src/VBox/Frontends/VBoxManage/` at the same tag.

## Called from WSL

A Windows process started from a Linux folder gets that folder as its
current directory, `\\wsl.localhost\<distribution>\…`, and no WSL variable.
That is how vma knows it was called from WSL: it takes Linux paths and shows
the paths it writes as Linux (`src/core/paths.rs`). Started from a Windows
folder, `/mnt/c` included, it can tell only from `WSL_DISTRO_NAME`, and
environment variables reach it only when `WSLENV` names them
(`export WSLENV=${WSLENV:+$WSLENV:}WSL_DISTRO_NAME:VMA_NAME:VMA_OUT/p`).

## Build and check

Cargo is in `~/.cargo/bin`, which a non-interactive shell may not have on
`PATH`. Every build and test runs under the memory cap.

- `cargo test`: the core's tests, on Linux.
- `cargo clippy --all-targets -- -D warnings`, and again with
  `--target x86_64-pc-windows-gnu`: CI runs both.
- `cargo win`, or `./dist.sh`: `target/x86_64-pc-windows-gnu/release/vma.exe`,
  run straight from WSL. While a `vma.exe` started from a path still runs
  (a `start` waiting through an install), Windows runs the old build for
  every new start from that path, rebuilt or not: test a rebuild from a
  copy at a new path.

No CI runner has a VirtualBox, so a change that reaches VirtualBox is seen
working against a real machine before its push: each step of its issue's
list, run with the built `vma.exe`, and its output or picture read.
Test machines are made and removed with vma itself (`vma create t1
--memory 512`, `vma destroy t1`); with no system on its disk, one boots to
its firmware's screen, enough for the screen. An ISO a test inserts or
removes is a copy (VirtualBox's own `VBoxGuestAdditions.iso` will do), never
one of the owner's. Another session may be using the machines: agree turns
with it first (*Virtual machine*).

Where Hyper-V holds the processor's virtualization (WSL 2 turns it on),
VirtualBox runs machines in its NEM mode (`Snail execution mode` in a
machine's `VBox.log`): slow, and a restart Windows asks for may hang (four
of an install's first five did), its screen frozen, black or on
"Restarting", and its disks still, until the machine is reset. `vma start`
resets an install's; any other needs `vma stop --force` and `vma start`.

## Figures

Median of five calls from WSL, against base's `vm`: `shot` 84 ms against
521 ms, a machine running; `status` 68 ms against 487 ms, the machine off
(running, `vm` waits 4 s for its agent, which vma doesn't have yet).
`create` with its 80 GB disk: 0.1 s. An install of Windows 11 from its
ISO to the desktop with no person, in VirtualBox's NEM mode: 22 minutes,
two of its restarts hung and reset by `start`. Input, median of five
against `vm`: `click` 222 ms against 787 (150 of them its own pauses),
`keys enter` 82 against 657, `type` of 99 characters 575 against 1717, a
`batch` of 20 moves 93 against 688; 4000 characters 25 s against 61, and
at a 20% CPU cap 59 s with none lost, where `vm` lost 18%. Through MCP,
median of five calls on one server: `click` 140 ms, `keys enter` 6 ms,
`shot` 23 ms, `type` of 99 characters 529 ms. Inside, against `vm`'s
agent: `read` 119 ms against 744, `windows` 118 against 756, `run` of a
one-line script 3.1 s against 3.3 (PowerShell's own start). The
executable: 647 KB, as released.
