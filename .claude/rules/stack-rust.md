---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
  - ".cargo/**"
  - "build.rs"
  - "dist.sh"
  - ".github/**"
---

# Stack: Rust

On-demand module of `CONVENTIONS.md` — read before writing Rust or touching a
`Cargo.toml`. The file's existing style still comes first (see *Scope*). A
Windows desktop program also reads `stack-win32.md`.

Placeholder: `<APP>` (the binary's name; upper-cased in environment variables).

## Style
- **`rustfmt` with no configuration.** No `rustfmt.toml`: the default is the one every reader already knows.
- **The current edition for a new crate.** An existing crate moves editions in a commit of its own.
- **Every file opens with `//!`** saying what it is for and where its job stops — "Nothing here decides what a failure *means*; a probe only reports what happened." The boundary is the part a reader can't get from the code.
- **`///` on items whose name doesn't carry the why** — what a variant means to the user, what a function blocks on, what a field holds when it isn't obvious.

## Dependencies
The core rule (*Code*: every dependency earns its place), applied to Cargo:
- **A comment above every crate in `Cargo.toml`** saying why the program can't do without it. A crate without one is a crate to remove.
- **`default-features = false`, then only the features used** — and the comment says what was left out and why (key generation, PEM parsing, a localisation runtime, legacy decoders).
- **Platform crates under `[target.'cfg(<os>)'.dependencies]`**, so the core builds and tests on any machine.
- **The system before a crate** (what Windows offers: `stack-win32.md`). When a crate does fill a gap, its comment names the gap ("Windows has no Ed25519 of its own").
- **No async runtime.** Blocking I/O on threads. With few concurrent jobs, a thread per job beats a pool: a pool small enough to be cheap lets one stuck job delay every other.
- **No error-handling crates.** An error the program reacts to or shows the user is a closed enum it matches on (`enum Failure { Timeout, Refused, Dns, Http(u16), … }`), because user-facing words come from the translation table and a timeout and a refusal must be told apart by code, not by parsing text. `Result<_, String>` is fine where the error is only passed along.
- **Nothing ships beside the executable.** A C library is compiled in (`bundled`), never loaded as a DLL; build-time tools stay in `[build-dependencies]`.

## Architecture
- **A core that names no platform.** Policy, formats, protocol, and state live in `src/core/` — or, once the program is large, in a `core/` workspace crate, which makes the boundary the compiler's to enforce. The shell (`src/win/`, `src/ui/`, …) is the only code that reaches the OS.
- **One native shell per platform, chosen by `cfg` in `main.rs`.** A platform without one yet gets a terminal front (`src/cli.rs`) over the same core, so the core runs end to end there; its shell, when it comes, is `src/<platform>/` beside `src/win/`.
- **The core never touches a socket, a thread, or the clock.** The shell asks it for jobs, runs each, hands back results, and passes the time in. That is what makes the policy testable with a fake clock, and what a port to another OS reuses unchanged.
- **One owner of mutable state.** One app object on one thread holds it; everything else reads from it and sends changes back through it. Worker threads report back by message, never by sharing the state.

## Build profile
- **Release is tuned for size by default:** `opt-level = "z"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true`, with a comment tying it to the budget.
- **A deviation carries its reason in a comment.** Realtime work (audio, video) earns `opt-level = 3`. A program that must survive one thread's panic — something live on air — earns `panic = "unwind"`, with the thread restarted.
- **`[profile.test] opt-level = 2`** when tests do real-size work (large dumps, key derivation) and would otherwise make the suite a chore.

## Tests
- **Unit tests beside the code** (`#[cfg(test)] mod tests`); a module's tests move to `<module>/tests.rs` when they outgrow it. `tests/` is for integration against real systems.
- **The core's tests run natively on Linux and macOS;** a shell's tests run on its platform. From WSL, `cargo test --target x86_64-pc-windows-gnu` runs the Windows tests through interop — and an environment variable reaches the Windows process only when `WSLENV` names it (`WSLENV=<APP>_LANG`, or `<APP>_HOME/p` for a path).
- **Real systems are opt-in** through `<APP>_TEST_*` variables (see *Debug and test*). When they need setting up, a script in `tools/` starts them and prints the variables to `eval`.
- **A shell is used, not only pictured.** A picture from `--shot` never passes through the window: a button whose hit test misses it, a child window lying over it, or a list whose wheel goes nowhere looks right in every one. The desktop template is gaining two flags that go through the window (`guimadaleno/base#17`), and has neither yet: `--drive <script>` opens the demonstration in its real window and uses it step by step through the input a person's takes (`click <target>`, `wheel <target> <notches>`, `key <name>`, `type <text>`, `shot <file.png>`), and after each step, once what moves is at rest, writes the core's view and where each scrolling area stands. `--sweep` does it unasked in every scene: it presses every target with an action and scrolls every area taller than its view, and fails on one that changes nothing and isn't marked inert; CI runs it on each shell's platform. A target has a name and is found where the paint put it: the paint lists each as it draws it, and the hit test reads the same list. The script is read by the core; a shell only finds the targets and sends the input, so a new shell is not done without both.
- **A program without them takes them from the desktop template in the first change that touches its interface,** never all programs at once. Until the template has them (`guimadaleno/base#17`), such a change is used from outside (`stack-win32.md`, *Seeing it without a person*).

## Cross-building
- **Windows builds come from Linux, WSL, or macOS through mingw-w64** (`apt install gcc-mingw-w64-x86-64`, `brew install mingw-w64`). `.cargo/config.toml` points `x86_64-pc-windows-gnu` at `x86_64-w64-mingw32-gcc`/`-ar` and defines `cargo win` as the release cross build. On Windows, MSVC ignores it and a plain `cargo build --release` works.
- **Scripts run on all of them:** bash without GNU-only flags (`wc -c`, not `stat -c`), Python with only the standard library.
- **Paths for Windows tools are converted:** a Windows program driven from WSL (signtool) can't read a WSL path — `wslpath -w` first.

## CI
What every workflow follows is `stack-actions.md`; this is what Cargo adds to it.
- **Three jobs:** core tests on Linux; the cross build and its budget on Linux; the full suite on Windows. A macOS or Linux job comes with that platform's shell; until then it waits, commented.
- **`THIRD-PARTY-NOTICES.md` is compiled into the executable,** which is why CI's ignore list names its markdown files one by one.
- **Each job caches Cargo's downloads and `target/`, keyed by `Cargo.lock`.** Without it every job builds every dependency from scratch, the largest part of the minutes.
- **The runner image's mingw builds the executable the budget measures,** which is why its image is pinned like the rest.

## A new program starts from the template
A desktop program starts from `guimadaleno/base`'s `templates/rust-desktop/`:
`python3 base/templates/rust-desktop/tools/new.py <id> [<Name>]`, in the folder that
holds `base/`, copies it into `<id>/`, renames every spelling and gives the install
folder an ASCII spelling of the name. `Cargo.toml`'s layout,
`.cargo/config.toml`, `build.rs`, the packaging script, the CI and release workflows,
the application manifest, and the install code come from there — never retyped from
memory. Each
carries fixes that aren't obvious from its shape, and a rewrite loses them silently. A
fix to one of them in a program goes to the template too.
