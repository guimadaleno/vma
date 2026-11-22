<img src="site/vma-icon.png" alt="VMA" width="96">

# VMA

VirtualBox Machine Agent: give an AI agent a VirtualBox machine. The agent
sees its screen, uses its mouse and keyboard, and runs scripts and moves
files inside it. It also makes, changes and removes machines, and installs
Windows on a new one with no one at it. One small Windows executable, quick
to start for every action: an MCP server for agents, a command line for
scripts.

## Requirements

A Windows host with VirtualBox 7.2 or a later 7.x; vma refuses other
versions, whose numbers it would misread. Linux and macOS hosts are planned
(#1). Where Hyper-V holds the processor's virtualization (WSL 2 or Windows'
own security features turn it on), VirtualBox runs machines in its slower
NEM mode, where a restart Windows asks for may hang until the machine is
reset: through an install, `start` resets it.

## Get it

From its page, [guimadaleno.com/vma](https://guimadaleno.com/vma/), whose
button copies the line that installs it. Or by hand: the newest release's
version is in `https://download.guimadaleno.com/vma/latest.json`, signed
with the key in `update.pub`, and its executable is
`https://download.guimadaleno.com/vma/<version>/vma.exe`, with its
`SHA256SUMS` beside it. vma doesn't update itself.

From WSL, keep it on Windows' disk, where it starts faster than from a Linux
folder, and link it onto the path:

```
ln -s /mnt/c/Users/<you>/AppData/Local/Programs/vma/vma.exe ~/.local/bin/vma
```

Run in a Linux folder, `vma.exe` takes Linux paths and prints them back as
Linux. From a Windows folder, `/mnt/c` included, it does so only when it gets
`WSL_DISTRO_NAME`. Windows sees only the variables `WSLENV` names:

```
export WSLENV=${WSLENV:+$WSLENV:}WSL_DISTRO_NAME:VMA_NAME:VMA_OUT/p
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

`--machine NAME`, after the command, or else `VMA_NAME`, picks the machine a
command acts on; `vma help` says which one it takes when neither names one.
With `--install`, a new machine installs the Windows on `FILE` at its first
start, its Guest Additions too, and signs in by itself; give `start` a
`--timeout` long enough for it. vma types only on a PS/2 keyboard,
VirtualBox's default.

## Inside

`setup` puts vma's agent in a Windows machine by its own keyboard, and a
machine `create --install` made gets it at the end of its install. The agent
starts at each logon, once as the person and once elevated (`--admin`), and
takes a new `vma.exe` from the host by itself. The host and the agent talk
through a folder the host shares. Inside the machine, any program can write
to that folder, and so ask the elevated agent, with no UAC prompt: vma is
for test machines, with nothing to guard inside. The host takes nothing from
the machine but the files it was asked to pull, by their own names.

## MCP

`vma mcp` serves the same commands as MCP tools, over standard input and
output, with one connection to VirtualBox for the whole run and pictures in
the results. A tool's arguments are the command's, by name, and `machine`
stands for `--machine`. `start` and `stop` may take minutes, longer than
some clients wait for a call: give such a client the time (in Claude Code,
`MCP_TOOL_TIMEOUT`), or start with a short `timeout` and look with `status`.
In Claude Code:

```
claude mcp add vma -- /mnt/c/path/to/vma.exe mcp
```

## Claude Code plugin

A skill for an agent using vma: `vma help` first, a look at the screen
before and after each act, the agent inside over pixels. It runs `vma`;
with vma missing, and only with your OK, the line on guimadaleno.com/vma,
which downloads `vma.exe` from download.guimadaleno.com:

```
/plugin install vma --marketplace guimadaleno/vma
```

## Build

On Linux or WSL, with Rust and `gcc-mingw-w64-x86-64`:

```
./dist.sh          # dist/vma.exe
cargo test         # the core's tests
```

## License

MIT, in `LICENSE`. The crates it is built from are listed, with their
licences, in `THIRD-PARTY-NOTICES.md`, which `vma notices` prints.
