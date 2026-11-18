<img src="site/vma-icon.png" alt="VMA" width="96">

# VMA

VirtualBox Machine Agent: give an AI agent a VirtualBox machine. The agent
sees its screen, uses its mouse and keyboard, and runs scripts and moves
files inside it. One small Windows executable, quick to start for every
action: an MCP server for agents, a command line for scripts.

Version 0.1.0 makes, changes, starts, stops and removes
machines and their media, installs Windows on a new one with no one at it,
reads a machine's state, takes a picture of its screen, uses its mouse and
keyboard, and runs scripts, moves files and finds windows and controls
inside it through its agent there, from the command line or as an MCP
server.

## Get it

From its page, [guimadaleno.com/vma](https://guimadaleno.com/vma/), whose
button copies the line that installs it. Or by hand: the newest release's
version is in
`https://download.guimadaleno.com/vma/latest.json`, signed with the key in
`update.pub`, and its executable is
`https://download.guimadaleno.com/vma/<version>/vma.exe`, with its
`SHA256SUMS` beside it. vma doesn't update itself. From WSL, keep it on
Windows' disk, where it starts in 40 ms against 130 from a Linux folder,
and link it onto the path:

```
ln -s /mnt/c/Users/<you>/AppData/Local/Programs/vma/vma.exe ~/.local/bin/vma
```

## Use

```
vma list                      every machine: its state and name
vma create NAME [--os TYPE] [--disk GB] [--iso FILE] [SETTINGS]
vma create NAME --install FILE --password WORD [--user NAME] [--image N] ...
vma destroy NAME              its disks, settings and folder; an ISO from elsewhere stays
vma status                    state, screen, Guest Additions, system, settings, media
vma set SETTINGS              --memory MB --cpus N --cpu-cap PERCENT --vram MB
                              --clipboard MODE --drag-and-drop MODE
vma start [--headless] [--timeout S]
vma stop [--force]
vma insert FILE | eject       an ISO in its optical drive, or out
vma disk GB                   a new disk
vma shot [FILE]               a PNG of the screen; prints its path
vma click X Y [--right|--middle] [--double] [--hold KEYS]
vma move | press | release X Y
vma drag X1 Y1 X2 Y2          press, glide, release
vma scroll X Y N              N notches, down when positive
vma keys COMBO...             as win+r, ctrl+shift+enter, tab
vma type TEXT                 as keys of a US keyboard: ASCII only
vma click NAME                the control of that name on the screen
vma paste [TEXT]              any text, by the clipboard and ctrl+v
vma windows                   the open windows: place, size, program, title
vma wait TITLE [--gone]       until a window's title holds TITLE, or none does
vma setup                     once: vma's agent inside, started at each logon
vma run [SCRIPT] [--file FILE] [--timeout S] [--admin]
                              PowerShell inside; its output and exit code
vma read PATH | write PATH [TEXT]
vma push LOCAL [PATH] | pull PATH [LOCAL]
vma batch                     steps from standard input, one per line
vma mcp                       an MCP server on standard input and output
vma notices                   vma's licence and the third-party notices
vma help                      all of it, in full
```

A new machine takes VirtualBox's defaults for its system (Windows 11 unless
`--os` names another), a disk of the size VirtualBox recommends, and an
optical drive. With `--install`, it installs the Windows on `FILE` at its
first start, its Guest Additions too, and signs in by itself; `start`
waits through the install, given a `--timeout` long enough for it. Where
Hyper-V holds the processor's virtualization (WSL 2 or Windows' own
security features turn it on), VirtualBox runs a machine in its slower NEM
mode, where a restart Windows asks for may hang until the machine is
reset: through an install, `start` resets it.
X and Y are pixels of the screen as a picture shows it, from 0, and a
command of the mouse or keyboard takes `--look` for a picture half a second
later. Keys go as fast as the machine reads them, and none is lost to a slow
one: VirtualBox's PS/2 keyboard drops what its queue can't hold, so vma reads
that queue and sends the next keys once it has room. A machine paused while
keys go fails the command, and gets the keys already sent when it goes on.
vma types only on a PS/2 keyboard, VirtualBox's default.
`--machine NAME`, after the command, or else `VMA_NAME`, picks the machine
the other commands act on. Named by neither, `status`, `shot` and a batch's
`pause`, which only read, take the one running, or the only one there is;
any other command takes only the only one there is, as one of several may
be someone else's. `destroy` takes a name every time. A picture goes to
`VMA_OUT`, or to `%TEMP%\vma`, unless a `FILE` is given. vma's own failures
exit with 125.

## Inside

`setup` puts vma's agent in a Windows machine by its own keyboard: the Run
box takes `vma.exe` from a folder the host shares, run elevated, and the UAC
prompt is answered. The agent starts at each logon, once as the person and
once elevated (`--admin`), and takes a new `vma.exe` from the host by itself.
A machine `create --install` made gets it at the end of its install.
Inside the machine, any program can write to that folder, and so ask the
elevated agent, with no UAC prompt, as with base's `vm`: vma is for test
machines, with nothing to guard inside. The host takes nothing from the
machine but the files it was asked to pull, by their own names. The
host and the agent talk through that shared folder, polled: measured at
17 ms a round trip, where a serial port, quicker for a short request,
could be added only to a machine that is off and serves one vma at a time.

## MCP

`vma mcp` serves the same commands as MCP tools to an agent's client, over
standard input and output, with one connection to VirtualBox for the whole
run and pictures in the results. A tool's arguments are the command's,
by name (`x`, `y`, `button`, `look`, …), and `machine` stands for
`--machine`. It speaks MCP 2026-07-28, and 2024-11-05 to 2025-11-25 for a
client that opens with `initialize`. It answers the client while a call
runs, and a call the client cancels stops. `start` and `stop` may take
minutes, longer than some clients wait for a call: give such a client the
time (in Claude Code, `MCP_TOOL_TIMEOUT`), or start with a short `timeout`
and look with `status`. In Claude Code:

```
claude mcp add vma -- /mnt/c/path/to/vma.exe mcp
```

From WSL, run in a Linux folder, `vma.exe` takes Linux paths and prints
them back as Linux. From a Windows folder, `/mnt/c` included, it does so
only when it gets `WSL_DISTRO_NAME`. Windows sees only the variables
`WSLENV` names:

```
export WSLENV=${WSLENV:+$WSLENV:}WSL_DISTRO_NAME:VMA_NAME:VMA_OUT/p
```

## Build

On Linux or WSL, with Rust and `gcc-mingw-w64-x86-64`:

```
./dist.sh          # dist/vma.exe
cargo test         # the core's tests
```

VirtualBox 7.2 or a later 7.x on a Windows host; vma refuses other versions,
whose numbers it would misread. Linux and macOS hosts are planned (#1).

## License

MIT, in `LICENSE`. The crates it is built from are listed, with their
licences, in `THIRD-PARTY-NOTICES.md`, which `vma notices` prints.
