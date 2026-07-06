//! Scripted driver: agent/e2e verification without screenshots.
//!
//! `renderer.exe app.wasm --script steps.txt` runs commands against the live
//! document via real event synthesis (hit-tested pointer events, IME commits)
//! and can dump the guest DOM as indented text for golden-file assertions.
//!
//! Script grammar (one command per line, `#` comments):
//!   wait <ms>
//!   click <selector>        e.g. click #increment
//!   type <text...>          IME-commit into the focused element
//!   focus <selector>
//!   dump <path>             write the mount subtree as indented pseudo-HTML
//!   quit

use std::sync::Arc;
use std::time::Duration;

use blitz_shell::{BlitzShellEvent, BlitzShellProxy};

pub enum ScriptCmd {
    Click(String),
    Type(String),
    Focus(String),
    Dump(String),
    Quit,
}

/// Default settle time between commands (lets renders/paints land).
const STEP_DELAY_MS: u64 = 250;

pub fn run(path: String, proxy: BlitzShellProxy) {
    std::thread::Builder::new()
        .name("script".into())
        .spawn(move || {
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    crate::logger::log("script", &format!("cannot read {path}: {e}"));
                    return;
                }
            };
            crate::logger::log("script", &format!("running {path}"));
            // Let the window come up before the first command.
            std::thread::sleep(Duration::from_millis(800));

            for (lineno, raw) in text.lines().enumerate() {
                let line = raw.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let (verb, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
                let rest = rest.trim().trim_matches('"').to_string();
                let cmd = match verb {
                    "wait" => {
                        let ms: u64 = rest.parse().unwrap_or(STEP_DELAY_MS);
                        std::thread::sleep(Duration::from_millis(ms));
                        continue;
                    }
                    "click" => ScriptCmd::Click(rest),
                    "type" => ScriptCmd::Type(rest),
                    "focus" => ScriptCmd::Focus(rest),
                    "dump" => ScriptCmd::Dump(rest),
                    "quit" => ScriptCmd::Quit,
                    other => {
                        crate::logger::log(
                            "script",
                            &format!("{path}:{}: unknown command '{other}'", lineno + 1),
                        );
                        continue;
                    }
                };
                crate::logger::log("script", &format!("> {line}"));
                proxy.send_event(BlitzShellEvent::Embedder(Arc::new(cmd)));
                std::thread::sleep(Duration::from_millis(STEP_DELAY_MS));
            }
            crate::logger::log("script", "script finished");
        })
        .expect("spawn script thread");
}
