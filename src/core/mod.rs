//! What VMA decides without reaching any system: which command was asked,
//! which VirtualBox it works with, which machine is meant and how its state
//! reads, what an install takes from its ISO, what goes to a machine's mouse
//! and keyboard, what an agent's client asks over MCP, and how a path given
//! from WSL is found by Windows and shown back.
//! It names no platform, touches no file and reads no clock, so it is tested
//! on any machine; the Windows shell (`src/win/`) does what it decides.

pub mod agent;
pub mod command;
pub mod input;
pub mod install;
pub mod json;
pub mod machine;
pub mod mcp;
pub mod paths;
pub mod version;
