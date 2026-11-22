# VMA

VMA (VirtualBox Machine Agent) lets an agent use a VirtualBox machine as a
person at it would (its screen, mouse and keyboard), and run scripts and move
files inside it. One Windows executable, `vma.exe`, is both a CLI and an MCP
server (`vma mcp`).

## Decided

- **Host:** Windows, through VirtualBox's COM API called by name through
  IDispatch (`src/win/dispatch.rs`), so interface IDs don't matter. No
  PowerShell, no VBoxManage at run time. A VirtualBox vma wasn't checked
  against is refused, not misread (`src/core/version.rs`).
- **Guest:** Windows only, for what runs inside. The screen, mouse and
  keyboard work with any guest: they are VirtualBox's own devices.
- **Agent:** the same `vma.exe` inside (`boot`, `agent`, `install`), reached
  through a folder the host shares, polled; not a serial port, which is
  quicker but is added only to a machine that is off and serves one vma at a
  time.
- **Words:** English only, no locale table: vma's readers are agents and
  scripts, and its messages are part of its interface, like the commands'
  names. Departs from "the product speaks its users' language".
- **Public:** a machine is "a machine", never its name. The only outside
  names: guimadaleno.com and its download host, the R2 endpoint in
  `site.yml`, and the owner as the author (`LICENSE`, the page's footer,
  the plugin's manifests).
- **Plugin:** `plugin/`, listed by `.claude-plugin/marketplace.json`, holds
  only Markdown, JSON and images: Anthropic's plugin directory takes nothing
  else from an unverified publisher.
- **Notices:** `THIRD-PARTY-NOTICES.md`, compiled in and printed by
  `vma notices`, lists the crates the Windows build links: a crate added is
  added there in the same commit.

## Build and check

- `cargo test`: the core's tests, on Linux.
- `cargo clippy --all-targets -- -D warnings`, and again with
  `--target x86_64-pc-windows-gnu`: CI runs both.
- `./dist.sh`: `dist/vma.exe`, held to its size budget, run straight from WSL.
- While a `vma.exe` started from a path still runs (a `start` waiting through
  an install), Windows runs the old build for every new start from that path,
  rebuilt or not: test a rebuild from a copy at a new path.

## Seen working

No CI runner has a VirtualBox: a change that reaches VirtualBox is seen
working against a real machine, with the built `vma.exe`, before its push.

- Test machines are made and removed with vma itself (`vma create t1
  --memory 512`, `vma destroy t1`); with no system on its disk, one boots to
  its firmware's screen, enough for the screen.
- An ISO a test inserts or removes is a copy (VirtualBox's own
  `VBoxGuestAdditions.iso` will do), never one of the owner's.
- Another session may be using the machines: agree turns with it first.
- VirtualBox's NEM mode shows as `Snail execution mode` in a machine's
  `VBox.log`.
