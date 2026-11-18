//! MCP, the protocol an agent's client speaks to vma over standard input
//! and output, one JSON-RPC message a line. vma speaks the stateless
//! revision (2026-07-28: each request carries its version in `_meta`, and
//! `server/discover` tells what the server speaks) and the earlier ones,
//! whose client opens with `initialize`. A tool is a command: its arguments
//! become the command's words, read by `command::parse`, so a tool checks
//! and words its failures as the command line does. Running it is the
//! shell's; this decides what a line asks and what goes back.

use crate::core::json::{self, Json};
use crate::core::machine::Use;

/// The stateless revision, the only one a request's `_meta` may name.
pub const MODERN: &str = "2026-07-28";
/// The revisions whose client opens with `initialize`, newest first.
const LEGACY: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const UNSUPPORTED_PROTOCOL_VERSION: i32 = -32022;

const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";
const CAPABILITIES_KEY: &str = "io.modelcontextprotocol/clientCapabilities";
const SERVER_KEY: &str = "io.modelcontextprotocol/serverInfo";

/// How long a client may keep the tools and what discovery said: they
/// change only with vma itself, a day.
const CACHE_MS: u32 = 86_400_000;

const INSTRUCTIONS: &str = "vma uses a VirtualBox machine as a person at it would: its screen, \
mouse and keyboard. A tool acts on the machine its `machine` names, or else the one VMA_NAME \
names where the server has it. Without either, status and shot, which only read, take the one \
running, or the only one there is; any other tool takes only the only one there is. x and y are pixels of the screen as a shot shows it, from 0. With \
`look`, a picture of the screen half a second after comes in the result. Keys go as fast as \
the machine reads them.";

/// What a command gives back: its lines, and the pictures it took.
#[derive(Debug, Default, PartialEq)]
pub struct Output {
    pub text: String,
    /// Each a PNG.
    pub pictures: Vec<Vec<u8>>,
    /// Whether it ended failing, though it gave all this, as a script run
    /// inside that exits with a code of its own.
    pub failed: bool,
}

/// The revision a client chose with `initialize`, whose later requests
/// carry none of their own.
#[derive(Debug, Default)]
pub struct Session {
    legacy: Option<&'static str>,
}

/// What a line asks of the server.
#[derive(Debug, PartialEq)]
pub enum Reply {
    /// Nothing: a notification, or a client's response.
    Nothing,
    /// This message, written as a line.
    Send(Json),
    /// A tool run as the command `words`, on the machine named, if any.
    Run(Call),
    /// The request of this id stopped, waiting or running, and answered
    /// no more.
    Cancel(Json),
}

#[derive(Debug, PartialEq)]
pub struct Call {
    id: Json,
    modern: bool,
    pub words: Vec<String>,
    pub machine: Option<String>,
}

impl Call {
    pub fn id(&self) -> &Json {
        &self.id
    }

    /// The response to the call, from what its command did: a failure is
    /// a result the agent reads, as the specification asks of a tool's.
    pub fn answer(&self, done: Result<Output, String>) -> Json {
        let (output, failed) = match done {
            Ok(output) => {
                let failed = output.failed;
                (output, failed)
            }
            Err(why) => (
                Output {
                    text: why,
                    ..Output::default()
                },
                true,
            ),
        };
        result(self.id.clone(), self.modern, tool_result(output, failed))
    }
}

fn tool_result(output: Output, failed: bool) -> Vec<(&'static str, Json)> {
    let mut content = Vec::new();
    let text = output.text.trim_end();
    if !text.is_empty() || output.pictures.is_empty() {
        content.push(Json::object(vec![
            ("type", Json::text("text")),
            ("text", Json::text(text)),
        ]));
    }
    for png in &output.pictures {
        content.push(Json::object(vec![
            ("type", Json::text("image")),
            ("data", Json::String(json::base64(png))),
            ("mimeType", Json::text("image/png")),
        ]));
    }
    vec![
        ("content", Json::Array(content)),
        ("isError", Json::Bool(failed)),
    ]
}

impl Session {
    /// What to do with a line that came on standard input.
    pub fn handle(&mut self, line: &str) -> Reply {
        let message = match Json::parse(line) {
            Ok(m) => m,
            Err(why) => return error(Json::Null, PARSE_ERROR, &format!("not JSON: {why}")),
        };
        let id = message.get("id").cloned();
        let Some(method) = message.get("method").and_then(Json::as_str) else {
            // A response of the client's, or nothing that makes sense.
            return match (&id, message.get("result").or(message.get("error"))) {
                (Some(_), Some(_)) => Reply::Nothing,
                _ => error(id.unwrap_or(Json::Null), INVALID_REQUEST, "no method"),
            };
        };
        let Some(id) = id else {
            // A notification: initialized needs nothing, and cancelled
            // names the request to stop.
            if method == "notifications/cancelled"
                && let Some(id) = message.get("params").and_then(|p| p.get("requestId"))
            {
                return Reply::Cancel(id.clone());
            }
            return Reply::Nothing;
        };
        let empty = Json::Object(Vec::new());
        let params = message.get("params").unwrap_or(&empty);
        if method == "initialize" {
            return self.initialize(id, params);
        }
        let modern = match params.get("_meta").and_then(|m| m.get(VERSION_KEY)) {
            Some(version) => {
                let Some(version) = version.as_str() else {
                    return error(id, INVALID_PARAMS, &format!("{VERSION_KEY} is a string"));
                };
                if version != MODERN {
                    return unsupported(id, version);
                }
                if params
                    .get("_meta")
                    .and_then(|m| m.get(CAPABILITIES_KEY))
                    .is_none_or(|c| !matches!(c, Json::Object(_)))
                {
                    return error(
                        id,
                        INVALID_PARAMS,
                        &format!("the request's _meta has no {CAPABILITIES_KEY}"),
                    );
                }
                true
            }
            None if self.legacy.is_some() => false,
            None => {
                return error(
                    id,
                    INVALID_PARAMS,
                    &format!(
                        "the request's _meta has no {VERSION_KEY}, and no initialize came before"
                    ),
                );
            }
        };
        match method {
            "server/discover" if modern => Reply::Send(result(
                id,
                true,
                vec![
                    ("supportedVersions", Json::Array(vec![Json::text(MODERN)])),
                    ("capabilities", capabilities()),
                    ("instructions", Json::text(INSTRUCTIONS)),
                    ("ttlMs", Json::number(CACHE_MS)),
                    ("cacheScope", Json::text("public")),
                ],
            )),
            "ping" if !modern => Reply::Send(result(id, false, vec![])),
            "tools/list" => {
                let mut members =
                    vec![("tools", Json::Array(TOOLS.iter().map(Tool::json).collect()))];
                if modern {
                    members.push(("ttlMs", Json::number(CACHE_MS)));
                    members.push(("cacheScope", Json::text("public")));
                }
                Reply::Send(result(id, modern, members))
            }
            "tools/call" => call(id, modern, params),
            _ => error(id, METHOD_NOT_FOUND, &format!("vma has no method {method}")),
        }
    }

    fn initialize(&mut self, id: Json, params: &Json) -> Reply {
        let asked = params.get("protocolVersion").and_then(Json::as_str);
        let version = LEGACY
            .into_iter()
            .find(|v| Some(*v) == asked)
            .unwrap_or(LEGACY[0]);
        self.legacy = Some(version);
        Reply::Send(result(
            id,
            false,
            vec![
                ("protocolVersion", Json::text(version)),
                ("capabilities", capabilities()),
                ("serverInfo", server_info()),
                ("instructions", Json::text(INSTRUCTIONS)),
            ],
        ))
    }
}

fn capabilities() -> Json {
    Json::object(vec![("tools", Json::Object(Vec::new()))])
}

fn server_info() -> Json {
    Json::object(vec![
        ("name", Json::text("vma")),
        ("version", Json::text(env!("CARGO_PKG_VERSION"))),
    ])
}

/// A response with `members` as its result; a stateless one says it is
/// whole and who answers.
fn result(id: Json, modern: bool, mut members: Vec<(&str, Json)>) -> Json {
    if modern {
        members.insert(0, ("resultType", Json::text("complete")));
        members.push(("_meta", Json::object(vec![(SERVER_KEY, server_info())])));
    }
    Json::object(vec![
        ("jsonrpc", Json::text("2.0")),
        ("id", id),
        ("result", Json::object(members)),
    ])
}

fn error(id: Json, code: i32, message: &str) -> Reply {
    error_with(id, code, message, None)
}

fn error_with(id: Json, code: i32, message: &str, data: Option<Json>) -> Reply {
    let mut error = vec![
        ("code", Json::number(code)),
        ("message", Json::text(message)),
    ];
    if let Some(data) = data {
        error.push(("data", data));
    }
    Reply::Send(Json::object(vec![
        ("jsonrpc", Json::text("2.0")),
        ("id", id),
        ("error", Json::object(error)),
    ]))
}

fn unsupported(id: Json, asked: &str) -> Reply {
    error_with(
        id,
        UNSUPPORTED_PROTOCOL_VERSION,
        "Unsupported protocol version",
        Some(Json::object(vec![
            ("supported", Json::Array(vec![Json::text(MODERN)])),
            ("requested", Json::text(asked)),
        ])),
    )
}

/// A `tools/call`: an unknown tool is the protocol's error, arguments the
/// tool can't take are the tool's.
fn call(id: Json, modern: bool, params: &Json) -> Reply {
    let Some(name) = params.get("name").and_then(Json::as_str) else {
        return error(id, INVALID_PARAMS, "tools/call names no tool");
    };
    let Some(tool) = TOOLS.iter().find(|t| t.name == name) else {
        return error(id, INVALID_PARAMS, &format!("vma has no tool {name}"));
    };
    let empty = Json::Object(Vec::new());
    let arguments = params.get("arguments").unwrap_or(&empty);
    match tool.words(arguments) {
        Ok((words, machine)) => Reply::Run(Call {
            id,
            modern,
            words,
            machine,
        }),
        Err(why) => Reply::Send(result(
            id,
            modern,
            tool_result(
                Output {
                    text: why,
                    ..Output::default()
                },
                true,
            ),
        )),
    }
}

/// What an argument is, and how it becomes the command's words.
#[derive(Clone, Copy)]
enum Kind {
    /// A pixel of the screen: a whole number from 0, a positional word.
    Place,
    /// A whole number, a positional word.
    Whole,
    /// Text, a positional word.
    Word,
    /// Several texts, a positional word each.
    Words,
    /// Text after `--NAME`.
    Value,
    /// A whole number after `--NAME`.
    Number,
    /// `--NAME` when true.
    Switch,
    /// One of these values, each the switch it gives, if any.
    Choice(&'static [(&'static str, Option<&'static str>)]),
}

#[derive(Clone, Copy)]
struct Param {
    /// Its name as an argument, and, for an option, as the command's.
    name: &'static str,
    option: &'static str,
    kind: Kind,
    required: bool,
    about: &'static str,
}

struct Tool {
    name: &'static str,
    about: &'static str,
    params: &'static [Param],
    /// What it does to the machine `machine` names, where it acts on one.
    machine: Option<Use>,
}

const fn p(name: &'static str, kind: Kind, required: bool, about: &'static str) -> Param {
    Param {
        name,
        option: name,
        kind,
        required,
        about,
    }
}

const fn opt(name: &'static str, option: &'static str, kind: Kind, about: &'static str) -> Param {
    Param {
        name,
        option,
        kind,
        required: false,
        about,
    }
}

const SHARING: Kind = Kind::Choice(&[
    ("off", Some("off")),
    ("to-guest", Some("to-guest")),
    ("to-host", Some("to-host")),
    ("both", Some("both")),
]);
const PRESSED: Kind = Kind::Choice(&[
    ("left", None),
    ("right", Some("right")),
    ("middle", Some("middle")),
]);
const LOOK: Param = p(
    "look",
    Kind::Switch,
    false,
    "a picture of the screen half a second after, in the result",
);
const HOLD: Param = p(
    "hold",
    Kind::Value,
    false,
    "keys held through it, as ctrl, or ctrl+shift; down before the press, so in a list they also add to what is selected",
);
const ADMIN: Param = p("admin", Kind::Switch, false, "done elevated");
const INSIDE: &str =
    "a path inside; a relative one starts at the person's folder, and %VAR% and $env:VAR are read";
const SETTINGS: [Param; 6] = [
    p("memory", Kind::Number, false, "in MB"),
    p("cpus", Kind::Number, false, "processors"),
    opt(
        "cpu_cap",
        "cpu-cap",
        Kind::Number,
        "how much of a host core each processor may take, 1 to 100",
    ),
    p("vram", Kind::Number, false, "video memory, in MB"),
    p(
        "clipboard",
        SHARING,
        false,
        "which way the clipboard crosses",
    ),
    opt(
        "drag_and_drop",
        "drag-and-drop",
        SHARING,
        "which way drag and drop crosses",
    ),
];

/// The tools, in the order `vma help` lists the commands. `batch` is the
/// command line's way to one console for many steps, which a client has
/// for its whole run, and `mcp` is this. A text the command line reads
/// from standard input is an argument here, as standard input is MCP's.
const TOOLS: [Tool; 28] = [
    Tool {
        name: "list",
        about: "Every machine: its state and name.",
        params: &[],
        machine: None,
    },
    Tool {
        name: "create",
        about: "A new machine with VirtualBox's defaults for its system (Windows11_64 unless os names another of VirtualBox's), a new disk of disk GB, by default the size VirtualBox recommends, and iso in its optical drive; its status in the result. With install, it installs the Windows on that ISO at its first start, with no person, and its Guest Additions, with an account user (vboxuser by default) and password, signed in by itself; start then waits through the install, which takes longer than its 240 s: give it timeout.",
        params: &[
            p("name", Kind::Word, true, "the machine's name"),
            p("os", Kind::Value, false, "VirtualBox's guest OS type"),
            p("disk", Kind::Number, false, "the disk's size in GB"),
            p("iso", Kind::Value, false, "an ISO for its optical drive"),
            p(
                "install",
                Kind::Value,
                false,
                "a Windows ISO to install from",
            ),
            p(
                "password",
                Kind::Value,
                false,
                "the account's password, with install",
            ),
            p(
                "user",
                Kind::Value,
                false,
                "the account's name, with install",
            ),
            p(
                "image",
                Kind::Number,
                false,
                "which image of the ISO, with install; 1 by default",
            ),
            SETTINGS[0],
            SETTINGS[1],
            SETTINGS[2],
            SETTINGS[3],
            SETTINGS[4],
            SETTINGS[5],
        ],
        machine: None,
    },
    Tool {
        name: "destroy",
        about: "Remove a machine: its disks, settings and folder; an ISO it held from elsewhere is kept. Always by name.",
        params: &[p("name", Kind::Word, true, "the machine's name")],
        machine: None,
    },
    Tool {
        name: "status",
        about: "The machine's state, screen, Guest Additions, system, settings and media.",
        params: &[],
        machine: Some(Use::Read),
    },
    Tool {
        name: "set",
        about: "Change the machine's settings; some only while it is off.",
        params: &SETTINGS,
        machine: Some(Use::Change),
    },
    Tool {
        name: "start",
        about: "Turn the machine on and wait, 240 s by default, for its desktop; through an install, it resets the machine where a restart hung, as they may in VirtualBox's NEM mode.",
        params: &[
            p(
                "headless",
                Kind::Switch,
                false,
                "with no window on the host",
            ),
            p(
                "timeout",
                Kind::Number,
                false,
                "seconds to wait for the desktop",
            ),
        ],
        machine: Some(Use::Change),
    },
    Tool {
        name: "stop",
        about: "Ask the machine to shut down; the power goes after 90 s, or at once with force or while it is paused.",
        params: &[p("force", Kind::Switch, false, "cut the power at once")],
        machine: Some(Use::Change),
    },
    Tool {
        name: "insert",
        about: "An ISO in the machine's optical drive.",
        params: &[p("file", Kind::Word, true, "the ISO")],
        machine: Some(Use::Change),
    },
    Tool {
        name: "eject",
        about: "The machine's optical drive emptied; an install's own medium is deleted, once the machine stops if it runs.",
        params: &[],
        machine: Some(Use::Change),
    },
    Tool {
        name: "disk",
        about: "A new disk, attached to the machine.",
        params: &[p("gb", Kind::Whole, true, "its size in GB")],
        machine: Some(Use::Change),
    },
    Tool {
        name: "shot",
        about: "A picture of the machine's screen, at its size, in the result.",
        params: &[p("file", Kind::Word, false, "a PNG to write it to as well")],
        machine: Some(Use::Read),
    },
    Tool {
        name: "click",
        about: "A click at x, y, or on the control of that name on the screen: an open menu's items first, or the one in front whose name starts with it.",
        params: &[
            p("x", Kind::Place, false, "pixels from the left, from 0"),
            p("y", Kind::Place, false, "pixels from the top, from 0"),
            p(
                "name",
                Kind::Word,
                false,
                "a control's name, in place of x and y",
            ),
            p("button", PRESSED, false, "left by default"),
            p("double", Kind::Switch, false, "two clicks"),
            HOLD,
            LOOK,
        ],
        machine: Some(Use::Change),
    },
    Tool {
        name: "move",
        about: "The pointer to x, y, with button still down, after a press.",
        params: &[
            p("x", Kind::Place, true, "pixels from the left, from 0"),
            p("y", Kind::Place, true, "pixels from the top, from 0"),
            p(
                "button",
                Kind::Choice(&[
                    ("none", None),
                    ("left", Some("left")),
                    ("right", Some("right")),
                    ("middle", Some("middle")),
                ]),
                false,
                "none by default",
            ),
            LOOK,
        ],
        machine: Some(Use::Change),
    },
    Tool {
        name: "drag",
        about: "Press at x1, y1, glide to x2, y2, release; hold keys all along, as ctrl to copy a file.",
        params: &[
            p("x1", Kind::Place, true, "where it starts, from the left"),
            p("y1", Kind::Place, true, "where it starts, from the top"),
            p("x2", Kind::Place, true, "where it ends, from the left"),
            p("y2", Kind::Place, true, "where it ends, from the top"),
            p("button", PRESSED, false, "left by default"),
            HOLD,
            LOOK,
        ],
        machine: Some(Use::Change),
    },
    Tool {
        name: "press",
        about: "Press at x, y and keep the button down, to look at what a held button shows; move carries it, and release lets it go.",
        params: &[
            p("x", Kind::Place, true, "pixels from the left, from 0"),
            p("y", Kind::Place, true, "pixels from the top, from 0"),
            p("button", PRESSED, false, "left by default"),
            LOOK,
        ],
        machine: Some(Use::Change),
    },
    Tool {
        name: "release",
        about: "Every button up, at x, y.",
        params: &[
            p("x", Kind::Place, true, "pixels from the left, from 0"),
            p("y", Kind::Place, true, "pixels from the top, from 0"),
            LOOK,
        ],
        machine: Some(Use::Change),
    },
    Tool {
        name: "scroll",
        about: "The wheel turned at x, y.",
        params: &[
            p("x", Kind::Place, true, "pixels from the left, from 0"),
            p("y", Kind::Place, true, "pixels from the top, from 0"),
            p(
                "notches",
                Kind::Whole,
                true,
                "down when positive, up when negative",
            ),
            LOOK,
        ],
        machine: Some(Use::Change),
    },
    Tool {
        name: "keys",
        about: "Combos pressed one after another, as win+r, ctrl+shift+enter, tab, f5, alt+y. A key is a letter, digit or mark of a US keyboard, unshifted, or esc, tab, enter, space, backspace, capslock, ctrl, shift, alt, altgr, win, menu, f1 to f12, insert, delete, home, end, pageup, pagedown, up, down, left or right.",
        params: &[p("combos", Kind::Words, true, "the combos, in order"), LOOK],
        machine: Some(Use::Change),
    },
    Tool {
        name: "type",
        about: "Text typed as keys of a US keyboard: ASCII only, a new line as enter.",
        params: &[p("text", Kind::Word, true, "the text"), LOOK],
        machine: Some(Use::Change),
    },
    Tool {
        name: "paste",
        about: "Any text, by the clipboard and ctrl+v.",
        params: &[p("text", Kind::Word, true, "the text"), LOOK],
        machine: Some(Use::Change),
    },
    Tool {
        name: "windows",
        about: "The open windows inside: place, size, program, title; > marks the one in front.",
        params: &[],
        machine: Some(Use::Change),
    },
    Tool {
        name: "wait",
        about: "Until a window's title holds title, or, with gone, none does.",
        params: &[
            p("title", Kind::Word, true, "part of a window's title"),
            p(
                "gone",
                Kind::Switch,
                false,
                "until no window's title holds it",
            ),
            p("timeout", Kind::Number, false, "seconds, 30 by default"),
        ],
        machine: Some(Use::Change),
    },
    Tool {
        name: "setup",
        about: "Once: vma's agent inside the machine, started at each logon, once as the person and once elevated; run, read, write, push, pull, paste, windows, wait and click by name need it.",
        params: &[],
        machine: Some(Use::Change),
    },
    Tool {
        name: "run",
        about: "A PowerShell script, in the person's session inside; its output, errors and exit code in the result.",
        params: &[
            p("script", Kind::Word, false, "the script"),
            p(
                "file",
                Kind::Value,
                false,
                "a host's file holding the script, in place of script",
            ),
            p(
                "timeout",
                Kind::Number,
                false,
                "seconds it may take, 60 by default",
            ),
            ADMIN,
        ],
        machine: Some(Use::Change),
    },
    Tool {
        name: "read",
        about: "A file's text, inside.",
        params: &[p("path", Kind::Word, true, INSIDE), ADMIN],
        machine: Some(Use::Change),
    },
    Tool {
        name: "write",
        about: "Text into a file inside, in UTF-8.",
        params: &[
            p("path", Kind::Word, true, INSIDE),
            p("text", Kind::Word, true, "the text"),
            ADMIN,
        ],
        machine: Some(Use::Change),
    },
    Tool {
        name: "push",
        about: "A host's file or folder copied in.",
        params: &[
            p("local", Kind::Word, true, "the host's file or folder"),
            p(
                "path",
                Kind::Word,
                false,
                "where inside; the Desktop by default",
            ),
            ADMIN,
        ],
        machine: Some(Use::Change),
    },
    Tool {
        name: "pull",
        about: "A file or folder inside copied out to the host; its path there in the result.",
        params: &[
            p("path", Kind::Word, true, INSIDE),
            p(
                "local",
                Kind::Word,
                false,
                "where on the host; VMA_OUT, or TEMP\\vma, by default",
            ),
            ADMIN,
        ],
        machine: Some(Use::Change),
    },
];

impl Tool {
    fn json(&self) -> Json {
        let mut properties: Vec<(&str, Json)> = self
            .params
            .iter()
            .map(|param| (param.name, param.schema()))
            .collect();
        if let Some(how) = self.machine {
            let about = match how {
                Use::Read => {
                    "the machine; by default the one VMA_NAME names, or else the one running, or the only one there is"
                }
                Use::Change => {
                    "the machine; by default the one VMA_NAME names, or else the only one there is"
                }
            };
            properties.push((
                "machine",
                Json::object(vec![
                    ("type", Json::text("string")),
                    ("description", Json::text(about)),
                ]),
            ));
        }
        let required: Vec<Json> = self
            .params
            .iter()
            .filter(|param| param.required)
            .map(|param| Json::text(param.name))
            .collect();
        let mut schema = vec![
            ("type", Json::text("object")),
            ("properties", Json::object(properties)),
        ];
        if !required.is_empty() {
            schema.push(("required", Json::Array(required)));
        }
        schema.push(("additionalProperties", Json::Bool(false)));
        Json::object(vec![
            ("name", Json::text(self.name)),
            ("description", Json::text(self.about)),
            ("inputSchema", Json::object(schema)),
        ])
    }

    /// The command's words for `arguments`, options before `--` and the
    /// positional words after it, and the machine named, if any.
    fn words(&self, arguments: &Json) -> Result<(Vec<String>, Option<String>), String> {
        let Json::Object(given) = arguments else {
            return Err(format!("{}'s arguments are an object", self.name));
        };
        let mut machine = None;
        for (key, value) in given {
            if self.machine.is_some() && key == "machine" {
                let name = value
                    .as_str()
                    .ok_or_else(|| "machine is a machine's name".to_string())?;
                machine = Some(name.to_string());
            } else if !self.params.iter().any(|p| p.name == key) {
                return Err(format!("{} takes no argument {key}", self.name));
            }
        }
        if self.name == "click" {
            let has = |k: &str| arguments.get(k).is_some();
            match (has("name"), has("x"), has("y")) {
                (true, false, false) | (false, true, true) => {}
                (true, _, _) => return Err("click takes name, or x and y, not both".into()),
                _ => return Err("click wants x and y, or name".into()),
            }
        }
        let (mut options, mut positional) = (vec![self.name.to_string()], Vec::new());
        for param in self.params {
            let Some(value) = arguments.get(param.name) else {
                if param.required {
                    return Err(format!("{} wants {}", self.name, param.name));
                }
                continue;
            };
            let whole = || {
                value
                    .as_i64()
                    .map(|n| n.to_string())
                    .ok_or_else(|| format!("{} is a whole number", param.name))
            };
            let text = || {
                value
                    .as_str()
                    .map(String::from)
                    .ok_or_else(|| format!("{} is text", param.name))
            };
            let option = format!("--{}", param.option);
            match param.kind {
                Kind::Place | Kind::Whole => positional.push(whole()?),
                Kind::Word => positional.push(text()?),
                Kind::Words => {
                    let Json::Array(items) = value else {
                        return Err(format!("{} is a list of texts", param.name));
                    };
                    for item in items {
                        let item = item
                            .as_str()
                            .ok_or_else(|| format!("{} is a list of texts", param.name))?;
                        positional.push(item.to_string());
                    }
                }
                Kind::Value => options.extend([option, text()?]),
                Kind::Number => options.extend([option, whole()?]),
                Kind::Switch => match value {
                    Json::Bool(true) => options.push(option),
                    Json::Bool(false) => {}
                    _ => return Err(format!("{} is true or false", param.name)),
                },
                Kind::Choice(choices) => {
                    let asked = text()?;
                    let Some(&(_, word)) = choices.iter().find(|(c, _)| *c == asked) else {
                        let names: Vec<&str> = choices.iter().map(|(c, _)| *c).collect();
                        return Err(format!(
                            "{} is one of {}, not {asked}",
                            param.name,
                            names.join(", ")
                        ));
                    };
                    match word {
                        // A choice of settings is the option's value; one
                        // of buttons is a switch of its own.
                        Some(w) if param.name == "button" => options.push(format!("--{w}")),
                        Some(w) => options.extend([option, w.to_string()]),
                        None => {}
                    }
                }
            }
        }
        options.push("--".into());
        options.extend(positional);
        Ok((options, machine))
    }
}

impl Param {
    fn schema(&self) -> Json {
        let mut schema = match self.kind {
            Kind::Place => vec![
                ("type", Json::text("integer")),
                ("minimum", Json::number(0)),
            ],
            Kind::Whole | Kind::Number => vec![("type", Json::text("integer"))],
            Kind::Word | Kind::Value => vec![("type", Json::text("string"))],
            Kind::Words => vec![
                ("type", Json::text("array")),
                ("items", Json::object(vec![("type", Json::text("string"))])),
                ("minItems", Json::number(1)),
            ],
            Kind::Switch => vec![("type", Json::text("boolean"))],
            Kind::Choice(choices) => vec![
                ("type", Json::text("string")),
                (
                    "enum",
                    Json::Array(choices.iter().map(|(c, _)| Json::text(c)).collect()),
                ),
            ],
        };
        schema.push(("description", Json::text(self.about)));
        Json::object(schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::command::{self, Command};

    fn modern(id: u32, method: &str, params: &str) -> String {
        let meta = format!(r#""_meta":{{"{VERSION_KEY}":"{MODERN}","{CAPABILITIES_KEY}":{{}}}}"#);
        let params = if params.is_empty() {
            format!("{{{meta}}}")
        } else {
            format!("{{{params},{meta}}}")
        };
        format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"{method}","params":{params}}}"#)
    }

    fn sent(reply: Reply) -> Json {
        match reply {
            Reply::Send(json) => json,
            other => panic!("not sent: {other:?}"),
        }
    }

    fn code(reply: Reply) -> i64 {
        let json = sent(reply);
        json.get("error")
            .and_then(|e| e.get("code"))
            .and_then(Json::as_i64)
            .unwrap_or_else(|| panic!("no error: {}", json.write()))
    }

    #[test]
    fn a_modern_client_discovers_and_lists() {
        let mut s = Session::default();
        let found = sent(s.handle(&modern(1, "server/discover", "")));
        let result = found.get("result").unwrap();
        assert_eq!(result.get("resultType"), Some(&Json::text("complete")));
        assert_eq!(
            result.get("supportedVersions"),
            Some(&Json::Array(vec![Json::text(MODERN)]))
        );
        assert!(result.get("capabilities").unwrap().get("tools").is_some());
        let server = result.get("_meta").unwrap().get(SERVER_KEY).unwrap();
        assert_eq!(server.get("name"), Some(&Json::text("vma")));
        let listed = sent(s.handle(&modern(2, "tools/list", "")));
        let result = listed.get("result").unwrap();
        assert_eq!(result.get("cacheScope"), Some(&Json::text("public")));
        let Some(Json::Array(tools)) = result.get("tools") else {
            panic!("no tools");
        };
        let names: Vec<&str> = tools
            .iter()
            .map(|t| t.get("name").and_then(Json::as_str).unwrap())
            .collect();
        assert_eq!(names[0], "list");
        assert!(names.contains(&"click") && names.contains(&"type"));
        assert!(!names.contains(&"batch") && !names.contains(&"mcp"));
    }

    #[test]
    fn a_legacy_client_opens_with_initialize() {
        let mut s = Session::default();
        assert_eq!(
            code(s.handle(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)),
            -32602
        );
        let opened = sent(s.handle(
            r#"{"jsonrpc":"2.0","id":"a","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"c","version":"1"}}}"#,
        ));
        assert_eq!(opened.get("id"), Some(&Json::text("a")));
        let result = opened.get("result").unwrap();
        assert_eq!(
            result.get("protocolVersion"),
            Some(&Json::text("2025-06-18"))
        );
        assert!(result.get("resultType").is_none());
        assert_eq!(
            s.handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
            Reply::Nothing
        );
        let listed = sent(s.handle(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#));
        assert!(listed.get("result").unwrap().get("ttlMs").is_none());
        let pong = sent(s.handle(r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#));
        assert_eq!(pong.get("result"), Some(&Json::Object(vec![])));
        let newest = sent(Session::default().handle(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2099-01-01"}}"#,
        ));
        assert_eq!(
            newest.get("result").unwrap().get("protocolVersion"),
            Some(&Json::text("2025-11-25"))
        );
    }

    #[test]
    fn wrong_requests_get_the_protocols_errors() {
        let mut s = Session::default();
        assert_eq!(code(s.handle("not json")), -32700);
        assert_eq!(code(s.handle(r#"{"jsonrpc":"2.0","id":1}"#)), -32600);
        assert_eq!(code(s.handle(&modern(1, "prompts/list", ""))), -32601);
        assert_eq!(code(s.handle(&modern(1, "ping", ""))), -32601);
        let old = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"1900-01-01","io.modelcontextprotocol/clientCapabilities":{}}}}"#;
        let refused = sent(s.handle(old));
        let error = refused.get("error").unwrap();
        assert_eq!(error.get("code").and_then(Json::as_i64), Some(-32022));
        assert_eq!(
            error.get("data").unwrap().get("requested"),
            Some(&Json::text("1900-01-01"))
        );
        let no_capabilities = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#;
        assert_eq!(code(s.handle(no_capabilities)), -32602);
        assert_eq!(
            code(s.handle(&modern(1, "tools/call", r#""name":"nope""#))),
            -32602
        );
        assert_eq!(
            s.handle(r#"{"jsonrpc":"2.0","id":5,"result":{}}"#),
            Reply::Nothing
        );
        assert_eq!(
            s.handle(r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"r7","reason":"late"}}"#),
            Reply::Cancel(Json::text("r7"))
        );
    }

    fn run(arguments: &str, tool: &str) -> Call {
        let line = modern(
            9,
            "tools/call",
            &format!(r#""name":"{tool}","arguments":{arguments}"#),
        );
        match Session::default().handle(&line) {
            Reply::Run(call) => call,
            other => panic!("not run: {other:?}"),
        }
    }

    fn failure_text(arguments: &str, tool: &str) -> String {
        let line = modern(
            9,
            "tools/call",
            &format!(r#""name":"{tool}","arguments":{arguments}"#),
        );
        let json = sent(Session::default().handle(&line));
        let result = json.get("result").unwrap();
        assert_eq!(result.get("isError"), Some(&Json::Bool(true)));
        let Some(Json::Array(content)) = result.get("content") else {
            panic!("no content");
        };
        content[0]
            .get("text")
            .and_then(Json::as_str)
            .unwrap()
            .to_string()
    }

    #[test]
    fn arguments_become_the_commands_words() {
        let call = run(
            r#"{"x":10,"y":20,"button":"right","double":true,"hold":"ctrl","look":true,"machine":"m"}"#,
            "click",
        );
        assert_eq!(call.machine, Some("m".into()));
        assert_eq!(
            command::parse(&call.words).unwrap().unwrap(),
            command::parse(
                &"click 10 20 --right --double --hold ctrl --look"
                    .split(' ')
                    .map(String::from)
                    .collect::<Vec<_>>()
            )
            .unwrap()
            .unwrap()
        );
        let typed = run(r#"{"text":"--look \"a\"\n"}"#, "type");
        let Command::Input { look, .. } = command::parse(&typed.words).unwrap().unwrap().0 else {
            panic!("not input");
        };
        assert!(!look);
        assert_eq!(typed.words, ["type", "--", "--look \"a\"\n"]);
        let set = run(r#"{"cpu_cap":50,"drag_and_drop":"both"}"#, "set");
        assert_eq!(
            set.words,
            ["set", "--cpu-cap", "50", "--drag-and-drop", "both", "--"]
        );
        let moved = run(r#"{"x":1,"y":2,"button":"left"}"#, "move");
        assert_eq!(moved.words, ["move", "--left", "--", "1", "2"]);
        assert_eq!(
            run(r#"{"name":"Save As..."}"#, "click").words,
            ["click", "--", "Save As..."]
        );
        assert_eq!(
            run(r#"{"path":"a.txt","text":"--x"}"#, "write").words,
            ["write", "--", "a.txt", "--x"]
        );
        let keys = run(r#"{"combos":["win+r","enter"]}"#, "keys");
        assert_eq!(keys.words, ["keys", "--", "win+r", "enter"]);
        for (tool, _) in [("list", ()), ("status", ()), ("eject", ())] {
            assert!(command::parse(&run("{}", tool).words).is_ok());
        }
    }

    #[test]
    fn arguments_a_tool_cant_take_are_its_failure() {
        assert_eq!(
            failure_text(r#"{"x":1}"#, "click"),
            "click wants x and y, or name"
        );
        assert_eq!(
            failure_text(r#"{"x":1,"y":2,"name":"OK"}"#, "click"),
            "click takes name, or x and y, not both"
        );
        assert_eq!(
            failure_text(r#"{"x":1.5,"y":2}"#, "click"),
            "x is a whole number"
        );
        assert_eq!(
            failure_text(r#"{"x":1,"y":2,"z":3}"#, "click"),
            "click takes no argument z"
        );
        assert_eq!(
            failure_text(r#"{"x":1,"y":2,"button":"up"}"#, "click"),
            "button is one of left, right, middle, not up"
        );
        assert_eq!(
            failure_text(r#"{"machine":"m"}"#, "list"),
            "list takes no argument machine"
        );
        assert_eq!(
            failure_text(r#"{"combos":"enter"}"#, "keys"),
            "combos is a list of texts"
        );
    }

    #[test]
    fn a_call_answers_with_text_and_pictures() {
        let call = run("{}", "status");
        let answer = call.answer(Ok(Output {
            text: "state running\n".into(),
            pictures: vec![b"png".to_vec()],
            failed: false,
        }));
        let result = answer.get("result").unwrap();
        assert_eq!(result.get("isError"), Some(&Json::Bool(false)));
        let Some(Json::Array(content)) = result.get("content") else {
            panic!("no content");
        };
        assert_eq!(content[0].get("text"), Some(&Json::text("state running")));
        assert_eq!(content[1].get("data"), Some(&Json::text("cG5n")));
        assert_eq!(content[1].get("mimeType"), Some(&Json::text("image/png")));
        let failed = call.answer(Err("m is not running".into()));
        assert_eq!(
            failed.get("result").unwrap().get("isError"),
            Some(&Json::Bool(true))
        );
    }

    #[test]
    fn tools_come_in_the_helps_order() {
        let at = |name: &str| {
            command::USAGE
                .find(&format!("\n  {name} "))
                .unwrap_or_else(|| panic!("{name} is not in the help"))
        };
        let places: Vec<usize> = TOOLS.iter().map(|t| at(t.name)).collect();
        let mut sorted = places.clone();
        sorted.sort();
        assert_eq!(places, sorted);
    }

    #[test]
    fn every_tool_is_a_command() {
        for tool in &TOOLS {
            assert!(
                command::parse(&[tool.name.to_string(), "--help-me".into()])
                    .unwrap_err()
                    .contains("is not an option here"),
                "{}",
                tool.name
            );
        }
    }

    #[test]
    fn a_tool_says_the_default_its_command_takes() {
        for tool in &TOOLS {
            let Some(how) = tool.machine else { continue };
            let mut given: Vec<String> = tool
                .params
                .iter()
                .filter(|p| p.required)
                .map(|p| match p.kind {
                    Kind::Place | Kind::Whole => format!(r#""{}":1"#, p.name),
                    Kind::Words => format!(r#""{}":["a"]"#, p.name),
                    _ => format!(r#""{}":"a""#, p.name),
                })
                .collect();
            match tool.name {
                "click" => given.push(r#""name":"OK""#.into()),
                "set" => given.push(r#""memory":512"#.into()),
                _ => {}
            }
            let call = run(&format!("{{{}}}", given.join(",")), tool.name);
            let (command, _) = command::parse(&call.words).unwrap().unwrap();
            assert_eq!(command.machine_use(), how, "{}", tool.name);
        }
    }

    #[test]
    fn a_tool_takes_a_machine_where_its_command_does() {
        for tool in &TOOLS {
            let words = [tool.name, "--machine", "m", "--help-me"].map(String::from);
            let refused = command::parse(&words).unwrap_err();
            let wanted = if tool.machine.is_some() {
                "--help-me is not an option here"
            } else {
                "--machine is not an option here"
            };
            assert!(refused.starts_with(wanted), "{}: {refused}", tool.name);
        }
    }
}
