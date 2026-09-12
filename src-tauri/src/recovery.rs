//! The recovery console: a way out that does not depend on the main window.
//!
//! A locked desk once sat behind a blank webview, and the escape hatch lived
//! in that webview. Now a global key combination opens a small console above
//! every other window, including a locked desk, and four commands typed in
//! order end an enforced session the way the escape hatch does: `unlock`
//! says what will happen and issues a challenge, `confirm <code>` proves a
//! person is at the keyboard, `phrase <escape phrase>` is the break-glass
//! phrase, and `release` lets the machine go. If the console itself cannot
//! appear, because the webview is what died, pressing the combination five
//! times inside ten seconds releases the lock anyway. A machine is never held
//! by a window that cannot be seen.

use crate::state::AppState;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// The combination, in the plugin's spelling. The plugin maps it to the
/// platform's own keys: Control, Option, Shift and U on macOS; Ctrl, Alt,
/// Shift and U on Windows and Linux.
pub const SHORTCUT: &str = "Ctrl+Alt+Shift+U";
pub const WINDOW: &str = "recovery";

/// One key of the combination as it reads on a keycap here.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Key {
    pub glyph: &'static str,
    pub name: &'static str,
}

/// The combination as this platform names it, and what this platform
/// makes of a system-wide shortcut. Every piece of text a person reads
/// about the combination comes from here.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Combination {
    pub platform: &'static str,
    /// The combination written out, as this platform's users write it.
    pub label: &'static str,
    pub keys: [Key; 4],
    /// What to know about global shortcuts on this platform.
    pub note: &'static str,
}

#[cfg(target_os = "macos")]
pub const COMBINATION: Combination = Combination {
    platform: "macOS",
    label: "Control + Option + Shift + U",
    keys: [
        Key { glyph: "⌃", name: "control" },
        Key { glyph: "⌥", name: "option" },
        Key { glyph: "⇧", name: "shift" },
        Key { glyph: "U", name: "" },
    ],
    note: "Registered with macOS as a system hot key. It works on every Space and over full-screen apps, and needs no permission.",
};

#[cfg(target_os = "windows")]
pub const COMBINATION: Combination = Combination {
    platform: "Windows",
    label: "Ctrl + Alt + Shift + U",
    keys: [
        Key { glyph: "Ctrl", name: "" },
        Key { glyph: "Alt", name: "" },
        Key { glyph: "Shift", name: "" },
        Key { glyph: "U", name: "" },
    ],
    note: "Registered with Windows as a system hot key. If another program already holds this combination, registration fails and the desk says so here.",
};

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const COMBINATION: Combination = Combination {
    platform: "Linux",
    label: "Ctrl + Alt + Shift + U",
    keys: [
        Key { glyph: "Ctrl", name: "" },
        Key { glyph: "Alt", name: "" },
        Key { glyph: "Shift", name: "" },
        Key { glyph: "U", name: "" },
    ],
    note: "Registered with the X11 server. Wayland desktops do not hand global shortcuts to applications; there the desk says so here, and the release token and the escape hatch remain.",
};
/// Presses of the combination that release the lock without a console.
pub const VALVE_PRESSES: usize = 5;
pub const VALVE_WINDOW: Duration = Duration::from_secs(10);
/// A challenge code lives this long.
const CHALLENGE_LIFETIME: Duration = Duration::from_secs(300);

/// Where the learner is in the unlock sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    Idle,
    Challenged { code: String, issued: Instant },
    Confirmed,
    Phrased,
}

/// The console's memory: the sequence in progress, recent presses of the
/// combination for the valve, and whether the system took the shortcut.
pub struct RecoveryState {
    pub stage: Mutex<Stage>,
    pub presses: Mutex<Vec<Instant>>,
    pub registered: AtomicBool,
}

impl Default for RecoveryState {
    fn default() -> Self {
        Self {
            stage: Mutex::new(Stage::Idle),
            presses: Mutex::new(Vec::new()),
            registered: AtomicBool::new(false),
        }
    }
}

/// One rung of the ladder, for the guide.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Rung {
    pub command: &'static str,
    pub what: &'static str,
}

/// Everything the guide shows: the combination as this platform names it,
/// whether the system took it, the valve, and the ladder.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Status {
    pub combination: Combination,
    pub registered: bool,
    pub valve_presses: usize,
    pub valve_seconds: u64,
    pub ladder: Vec<Rung>,
}

pub fn status(recovery: &RecoveryState) -> Status {
    Status {
        combination: COMBINATION,
        registered: recovery.registered.load(Ordering::SeqCst),
        valve_presses: VALVE_PRESSES,
        valve_seconds: VALVE_WINDOW.as_secs(),
        ladder: LADDER
            .iter()
            .map(|(command, what)| Rung { command, what })
            .collect(),
    }
}

/// What a line typed at the console comes back with.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Reply {
    pub lines: Vec<String>,
    /// The sequence finished and the desk is released; the console may close.
    pub released: bool,
    /// The learner asked the console to close.
    pub close: bool,
}

impl Reply {
    fn say(lines: &[&str]) -> Self {
        Self {
            lines: lines.iter().map(|line| line.to_string()).collect(),
            ..Self::default()
        }
    }
}

/// The step-by-step ladder, as shown in Settings, onboarding and `help`.
pub const LADDER: [(&str, &str); 4] = [
    (
        "unlock",
        "Explains what will happen and issues a six-character challenge code.",
    ),
    (
        "confirm <code>",
        "Type the code back. It proves a person is at the keyboard.",
    ),
    (
        "phrase <your escape phrase>",
        "The break-glass phrase you set during setup.",
    ),
    (
        "release",
        "Pauses the session with its work intact, breaks the streak, and frees the machine.",
    ),
];

/// A six-character code from an alphabet with no look-alikes.
pub fn challenge_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    (0..6)
        .map(|_| ALPHABET[rand::random::<usize>() % ALPHABET.len()] as char)
        .collect()
}

fn describe(state: &AppState) -> Vec<String> {
    let locked = state.locked.load(std::sync::atomic::Ordering::SeqCst);
    let holder = state.focus.holder();
    let mut lines = vec![format!(
        "desk: {}",
        if locked { "LOCKED" } else { "not locked" }
    )];
    match holder {
        Some(holder) => lines.push(format!(
            "focused session: {} ({})",
            holder.session_id, holder.course_id
        )),
        None => lines.push("focused session: none".into()),
    }
    if let Some(token) = crate::kiosk::release_token() {
        lines.push(format!("release token present at {}", token.display()));
    }
    lines
}

/// What a line asks for once the sequence has been checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Show the reply; nothing else.
    Say,
    /// The sequence is complete: release the desk, then show the reply.
    Release,
    /// The learner asked the console to close.
    Close,
}

/// The sequence itself, apart from the desk. `verify` checks the escape
/// phrase; `status` describes the desk. Pure over the stage it is given,
/// so the order of the four steps can be held to in tests.
pub fn step(
    stage: &Stage,
    line: &str,
    status: &dyn Fn() -> Vec<String>,
    verify: &dyn Fn(&str) -> Result<bool, String>,
) -> (Stage, Reply, Outcome) {
    let line = line.trim();
    let (verb, argument) = match line.split_once(char::is_whitespace) {
        Some((verb, rest)) => (verb.to_ascii_lowercase(), rest.trim().to_string()),
        None => (line.to_ascii_lowercase(), String::new()),
    };
    match verb.as_str() {
        "" => (stage.clone(), Reply::default(), Outcome::Say),
        "help" | "?" => {
            let mut lines = vec![
                "Principia recovery console. Four commands, in order, end an enforced session:"
                    .to_string(),
            ];
            for (index, (command, what)) in LADDER.iter().enumerate() {
                lines.push(format!("  {}. {command}", index + 1));
                lines.push(format!("     {what}"));
            }
            lines.push(String::new());
            lines.push("Also: status · cancel · close · help".into());
            lines.push(format!(
                "If this console cannot open, press {} {VALVE_PRESSES} times within {} seconds: the lock releases on its own.",
                COMBINATION.label,
                VALVE_WINDOW.as_secs()
            ));
            (
                stage.clone(),
                Reply {
                    lines,
                    ..Reply::default()
                },
                Outcome::Say,
            )
        }
        "status" => {
            let mut lines = status();
            lines.push(format!(
                "sequence: {}",
                match stage {
                    Stage::Idle => "not started (type: unlock)".to_string(),
                    Stage::Challenged { .. } => "waiting for: confirm <code>".to_string(),
                    Stage::Confirmed => "waiting for: phrase <your escape phrase>".to_string(),
                    Stage::Phrased => "ready: type release".to_string(),
                }
            ));
            (
                stage.clone(),
                Reply {
                    lines,
                    ..Reply::default()
                },
                Outcome::Say,
            )
        }
        "unlock" => {
            let code = challenge_code();
            let mut lines = status();
            lines.push(String::new());
            lines.push(
                "Releasing pauses the session with its work intact and breaks your streak.".into(),
            );
            lines.push(format!("Challenge code: {code}"));
            lines.push(format!("To continue, type:  confirm {code}"));
            (
                Stage::Challenged {
                    code,
                    issued: Instant::now(),
                },
                Reply {
                    lines,
                    ..Reply::default()
                },
                Outcome::Say,
            )
        }
        "confirm" => {
            match stage {
                Stage::Challenged { code, issued } if issued.elapsed() <= CHALLENGE_LIFETIME => {
                    if argument.trim().eq_ignore_ascii_case(code) {
                        (
                            Stage::Confirmed,
                            Reply::say(&["Confirmed.", "Now type:  phrase <your escape phrase>"]),
                            Outcome::Say,
                        )
                    } else {
                        (
                        stage.clone(),
                        Reply::say(&["That is not the code. Read it from the line above and type it again."]),
                        Outcome::Say,
                    )
                    }
                }
                Stage::Challenged { .. } => (
                    Stage::Idle,
                    Reply::say(&["That code has expired. Type: unlock"]),
                    Outcome::Say,
                ),
                _ => (
                    stage.clone(),
                    Reply::say(&["Start with: unlock"]),
                    Outcome::Say,
                ),
            }
        }
        "phrase" => {
            if *stage != Stage::Confirmed {
                return (
                    stage.clone(),
                    Reply::say(&["Confirm the challenge code first: unlock, then confirm <code>."]),
                    Outcome::Say,
                );
            }
            match verify(&argument) {
                Ok(true) => (
                    Stage::Phrased,
                    Reply::say(&["Phrase accepted.", "Type:  release"]),
                    Outcome::Say,
                ),
                Ok(false) => (
                    stage.clone(),
                    Reply::say(&[
                        "That is not your escape phrase. Type it exactly, spaces and all.",
                    ]),
                    Outcome::Say,
                ),
                Err(reason) => (stage.clone(), Reply::say(&[&reason]), Outcome::Say),
            }
        }
        "release" => {
            if *stage != Stage::Phrased {
                return (
                    stage.clone(),
                    Reply::say(&["The phrase has to be accepted first: unlock, confirm <code>, phrase <your escape phrase>, then release."]),
                    Outcome::Say,
                );
            }
            (
                Stage::Idle,
                Reply {
                    lines: vec!["Released. The lock is down.".into()],
                    released: true,
                    close: false,
                },
                Outcome::Release,
            )
        }
        "cancel" => (
            Stage::Idle,
            Reply::say(&["Sequence cancelled."]),
            Outcome::Say,
        ),
        "close" | "exit" | "quit" => (
            stage.clone(),
            Reply {
                close: true,
                ..Reply::default()
            },
            Outcome::Close,
        ),
        other => (
            stage.clone(),
            Reply::say(&[&format!("Unknown command: {other}. Type help.")]),
            Outcome::Say,
        ),
    }
}

/// Run one line of the console against the desk. The release goes through
/// the same path as the escape hatch.
pub fn command(app: &AppHandle, state: &AppState, recovery: &RecoveryState, line: &str) -> Reply {
    let current = recovery.stage.lock().unwrap().clone();
    let status = || describe(state);
    let verify = |typed: &str| crate::kiosk::verify_escape(state, typed);
    let (next, mut reply, outcome) = step(&current, line, &status, &verify);
    *recovery.stage.lock().unwrap() = next;
    if outcome == Outcome::Release {
        if let Some(session) = release(app, state, "recovery console") {
            reply.lines.push(format!(
                "Session {session} is paused with its work intact; resume it from the desk when you are ready."
            ));
        }
        reply.lines.push("This console closes in a moment.".into());
    }
    reply
}

/// End the enforced session the way the escape hatch does, and say so in
/// the execution feed, so a release is never silent.
pub fn release(app: &AppHandle, state: &AppState, by: &str) -> Option<String> {
    let paused = crate::enforcement::escape(app, state);
    state.clear_chat_threads();
    crate::kiosk::release(app, state);
    let _ = app.emit("classroom:state", serde_json::json!({ "refresh": true }));
    let _ = app.emit("classroom:state", serde_json::json!({ "escaped": true }));
    state
        .generator
        .feed
        .say(format!("recovery: the desk was released by the {by}"));
    log::warn!("recovery: desk released by the {by}");
    paused
}

/// The combination was pressed: open the console, and count the press for
/// the valve.
pub fn summon(app: &AppHandle) {
    let state = app.state::<AppState>();
    let recovery = app.state::<RecoveryState>();
    let now = Instant::now();
    let tripped = {
        let mut presses = recovery.presses.lock().unwrap();
        presses.retain(|at| now.duration_since(*at) <= VALVE_WINDOW);
        presses.push(now);
        presses.len() >= VALVE_PRESSES
    };
    if tripped {
        recovery.presses.lock().unwrap().clear();
        if state.locked.load(std::sync::atomic::Ordering::SeqCst) || state.focus.holder().is_some()
        {
            release(
                app,
                &state,
                "recovery valve: the combination was pressed five times",
            );
        }
    }
    if let Err(error) = open_console(app) {
        log::error!("recovery console could not open: {error}");
    }
}

/// Show the console above everything, creating it the first time.
pub fn open_console(app: &AppHandle) -> Result<(), String> {
    let window = match app.get_webview_window(WINDOW) {
        Some(window) => window,
        None => {
            tauri::WebviewWindowBuilder::new(app, WINDOW, tauri::WebviewUrl::App("recovery".into()))
                .title("Principia recovery")
                .inner_size(720.0, 460.0)
                .min_inner_size(520.0, 320.0)
                .resizable(true)
                .always_on_top(true)
                .visible_on_all_workspaces(true)
                .center()
                .build()
                .map_err(|error| error.to_string())?
        }
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    #[cfg(target_os = "macos")]
    {
        let win = window.clone();
        let _ = window.run_on_main_thread(move || {
            let _ = objc2::exception::catch(std::panic::AssertUnwindSafe(|| unsafe {
                crate::kiosk::mac::activate_self();
                if let Ok(ptr) = win.ns_window() {
                    crate::kiosk::mac::raise_console(ptr as *mut objc2::runtime::AnyObject);
                }
            }));
        });
    }
    Ok(())
}

/// Register the combination system-wide and remember whether the system
/// took it. A failure is logged and shown in Settings, never fatal: the
/// release token and the dead man's switch still stand.
pub fn install(app: &AppHandle) {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    let shortcut: tauri_plugin_global_shortcut::Shortcut = match SHORTCUT.parse() {
        Ok(shortcut) => shortcut,
        Err(error) => {
            log::error!("recovery shortcut {SHORTCUT} did not parse: {error}");
            return;
        }
    };
    let outcome = app
        .global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                summon(app);
            }
        });
    let label = COMBINATION.label;
    match outcome {
        Ok(()) => {
            app.state::<RecoveryState>()
                .registered
                .store(true, Ordering::SeqCst);
            log::info!("recovery console on {label} ({})", COMBINATION.platform);
        }
        Err(error) => log::error!(
            "recovery shortcut {label} could not be registered on {}: {error}",
            COMBINATION.platform
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_codes_are_six_unambiguous_characters() {
        for _ in 0..50 {
            let code = challenge_code();
            assert_eq!(code.len(), 6);
            assert!(code
                .chars()
                .all(|c| c.is_ascii_uppercase() || ('2'..='9').contains(&c)));
            assert!(!code.contains(['I', 'O', '0', '1']));
        }
    }

    #[test]
    fn the_ladder_is_four_steps_in_order() {
        assert_eq!(LADDER.len(), 4);
        assert_eq!(LADDER[0].0, "unlock");
        assert_eq!(LADDER[3].0, "release");
    }

    #[test]
    fn the_combination_is_named_for_this_platform() {
        let shortcut: tauri_plugin_global_shortcut::Shortcut =
            SHORTCUT.parse().expect("the plugin spelling parses");
        assert!(shortcut
            .mods
            .contains(tauri_plugin_global_shortcut::Modifiers::CONTROL));
        assert!(shortcut
            .mods
            .contains(tauri_plugin_global_shortcut::Modifiers::ALT));
        assert!(shortcut
            .mods
            .contains(tauri_plugin_global_shortcut::Modifiers::SHIFT));
        assert_eq!(COMBINATION.keys[3].glyph, "U");
        assert_eq!(COMBINATION.keys.len(), 4);
        if cfg!(target_os = "macos") {
            assert_eq!(COMBINATION.platform, "macOS");
            assert_eq!(COMBINATION.label, "Control + Option + Shift + U");
            assert_eq!(COMBINATION.keys[1].name, "option");
        } else {
            assert_eq!(COMBINATION.label, "Ctrl + Alt + Shift + U");
            assert_eq!(COMBINATION.keys[1].glyph, "Alt");
        }
        let status = status(&RecoveryState::default());
        assert!(!status.registered, "nothing is registered before install");
        assert_eq!(status.ladder.len(), 4);
        assert_eq!(status.valve_presses, VALVE_PRESSES);
    }

    fn run(stage: &Stage, line: &str, phrase_ok: bool) -> (Stage, Reply, Outcome) {
        step(stage, line, &|| vec!["desk: LOCKED".into()], &|typed| {
            Ok(phrase_ok && typed == "the phrase")
        })
    }

    #[test]
    fn the_four_steps_must_be_taken_in_order() {
        let (stage, reply, outcome) = run(&Stage::Idle, "release", true);
        assert_eq!(stage, Stage::Idle);
        assert_eq!(outcome, Outcome::Say);
        assert!(reply.lines[0].contains("has to be accepted first"));

        let (stage, _, _) = run(&Stage::Idle, "phrase the phrase", true);
        assert_eq!(
            stage,
            Stage::Idle,
            "the phrase is not taken before the code"
        );

        let (stage, reply, _) = run(&Stage::Idle, "unlock", true);
        let Stage::Challenged { code, .. } = stage.clone() else {
            panic!("unlock issues a challenge");
        };
        assert!(reply
            .lines
            .iter()
            .any(|line| line.contains(&format!("confirm {code}"))));

        let (wrong, _, _) = run(&stage, "confirm NOPE", true);
        assert_eq!(wrong, stage, "a wrong code keeps the challenge");
        let (confirmed, _, _) = run(&stage, &format!("confirm {}", code.to_lowercase()), true);
        assert_eq!(
            confirmed,
            Stage::Confirmed,
            "the code is not case-sensitive"
        );

        let (still, reply, _) = run(&confirmed, "phrase wrong", true);
        assert_eq!(still, Stage::Confirmed);
        assert!(reply.lines[0].contains("not your escape phrase"));
        let (phrased, _, _) = run(&confirmed, "phrase the phrase", true);
        assert_eq!(phrased, Stage::Phrased);

        let (done, reply, outcome) = run(&phrased, "release", true);
        assert_eq!(done, Stage::Idle);
        assert_eq!(outcome, Outcome::Release);
        assert!(reply.released);
    }

    #[test]
    fn an_expired_challenge_starts_over_and_cancel_resets() {
        let stale = Stage::Challenged {
            code: "ABCDEF".into(),
            issued: Instant::now() - CHALLENGE_LIFETIME - Duration::from_secs(1),
        };
        let (stage, reply, _) = run(&stale, "confirm ABCDEF", true);
        assert_eq!(stage, Stage::Idle);
        assert!(reply.lines[0].contains("expired"));
        let (stage, _, _) = run(&Stage::Confirmed, "cancel", true);
        assert_eq!(stage, Stage::Idle);
        let (_, reply, outcome) = run(&Stage::Idle, "help", true);
        assert_eq!(outcome, Outcome::Say);
        assert!(reply
            .lines
            .iter()
            .any(|line| line.contains(COMBINATION.label)));
        assert_eq!(run(&Stage::Idle, "close", true).2, Outcome::Close);
    }
}
