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
//!   hover <selector>        synthetic PointerEnter at the element
//!   unhover <selector>      synthetic PointerLeave
//!   rclick <selector>       synthetic ContextMenu
//!   key <name>              KeyDown to the focused element (Enter/Escape/Tab/End/Home)
//!   wheel <selector> <dy>   Wheel at the element (negative dy = scroll up)
//!   dump <path>             write the mount subtree as indented pseudo-HTML
//!   navigate <target>       address-bar navigate the active tab (web://name or path.wasm)
//!   back                    active tab's history back
//!   forward                 active tab's history forward
//!   reload                  reload the active tab's current target
//!   newtab [target]         open a new tab, optionally navigating it immediately
//!   closetab                close the active tab
//!   switchtab <idx>         activate tab by index (0-based, creation order)
//!   quit

use std::sync::Arc;
use std::time::Duration;

use blitz_shell::{BlitzShellEvent, BlitzShellProxy};

pub enum ScriptCmd {
    Click(String),
    Type(String),
    Focus(String),
    Hover(String),
    Unhover(String),
    RClick(String),
    Key(String),
    Wheel(String, f64),
    Dump(String),
    DumpApp(String),
    Navigate(String),
    Back,
    Forward,
    Reload,
    NewTab(String),
    CloseTab,
    SwitchTab(usize),
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
                    "hover" => ScriptCmd::Hover(rest),
                    "unhover" => ScriptCmd::Unhover(rest),
                    "rclick" => ScriptCmd::RClick(rest),
                    "key" => ScriptCmd::Key(rest),
                    "wheel" => {
                        let (sel, dy) = rest.rsplit_once(char::is_whitespace).unwrap_or((rest.as_str(), "-120"));
                        ScriptCmd::Wheel(sel.trim().to_string(), dy.trim().parse().unwrap_or(-120.0))
                    }
                    "dump" => ScriptCmd::Dump(rest),
                    "dumpapp" => ScriptCmd::DumpApp(rest),
                    "navigate" => ScriptCmd::Navigate(rest),
                    "back" => ScriptCmd::Back,
                    "forward" => ScriptCmd::Forward,
                    "reload" => ScriptCmd::Reload,
                    "newtab" => ScriptCmd::NewTab(rest),
                    "closetab" => ScriptCmd::CloseTab,
                    "switchtab" => ScriptCmd::SwitchTab(rest.parse().unwrap_or(0)),
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
