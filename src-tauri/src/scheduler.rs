//! Class appointment wakeups, one backend per OS.
//!
//! All three backends register the installed binary to launch with `--triggered`
//! at every configured time, and self-heal if the registration points at a
//! stale path or interval set. The public API is platform-agnostic:
//!
//! - macOS  → launchd LaunchAgent (`~/Library/LaunchAgents/<LABEL>.plist`)
//! - Linux  → systemd user timer (`~/.config/systemd/user/<UNIT>.{service,timer}`)
//! - Windows→ Task Scheduler entry (`schtasks`, task name = PRODUCT)

pub const LABEL: &str = "com.darkmatter.principia-desk";
/// The launch identity used before the rename, removed on first launch.
pub const LEGACY_LABEL: &str = "com.darkmatter.system-design-roulette";
// Existing Windows task identity: changing the display name must not duplicate schedules.
pub const PRODUCT: &str = "Principia Desk";
/// The scheduled-task name used before the rename.
pub const LEGACY_PRODUCT: &str = "System Design Roulette";

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Task Scheduler supports multiple CalendarTriggers in one task, preserving
/// the existing task identity when its daily appointment times change.
/// Schema: https://learn.microsoft.com/en-us/windows/win32/taskschd/daily-trigger-example--xml-
#[cfg(any(target_os = "windows", test))]
fn windows_task_xml(exe: &str, user: &str, times: &[(u32, u32)]) -> String {
    let triggers = times.iter().map(|(hour, minute)| format!(
        "<CalendarTrigger><StartBoundary>2000-01-01T{hour:02}:{minute:02}:00</StartBoundary><Enabled>true</Enabled><ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay></CalendarTrigger>"
    )).collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers>{triggers}</Triggers>
  <Principals><Principal id="Learner"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings><MultipleInstancesPolicy>Parallel</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><StartWhenAvailable>true</StartWhenAvailable><Enabled>true</Enabled><ExecutionTimeLimit>PT0S</ExecutionTimeLimit></Settings>
  <Actions Context="Learner"><Exec><Command>{exe}</Command><Arguments>--triggered</Arguments></Exec></Actions>
</Task>"#,
        exe = xml_escape(exe),
        user = xml_escape(user)
    )
}

#[cfg(any(target_os = "windows", test))]
fn windows_task_matches(xml: &str, exe: &str, times: &[(u32, u32)]) -> bool {
    use quick_xml::{events::Event, Reader};
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut commands = Vec::new();
    let mut arguments = Vec::new();
    let mut boundaries = Vec::new();
    let mut intervals = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                let name = start.local_name();
                if !matches!(
                    name.as_ref(),
                    b"Command" | b"Arguments" | b"StartBoundary" | b"DaysInterval" | b"Enabled"
                ) {
                    continue;
                }
                let Ok(raw) = reader.read_text(start.name()) else {
                    return false;
                };
                let Ok(value) = quick_xml::escape::unescape(&raw) else {
                    return false;
                };
                match name.as_ref() {
                    b"Command" => commands.push(value.into_owned()),
                    b"Arguments" => arguments.push(value.into_owned()),
                    b"StartBoundary" => boundaries.push(value.into_owned()),
                    b"DaysInterval" => intervals.push(value.into_owned()),
                    b"Enabled" if value == "false" => return false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return false,
            _ => {}
        }
    }
    boundaries.sort();
    let mut expected = times
        .iter()
        .map(|(hour, minute)| format!("2000-01-01T{hour:02}:{minute:02}:00"))
        .collect::<Vec<_>>();
    expected.sort();
    commands == [exe]
        && arguments == ["--triggered"]
        && boundaries == expected
        && intervals.len() == times.len()
        && intervals.iter().all(|value| value == "1")
}

/// Absolute path to the binary the scheduler should launch. Falls back to a
/// sensible per-OS default if `current_exe()` is somehow unavailable.
fn current_exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| imp::default_exe_path())
}

// ── Public, platform-agnostic API ──────────────────────────────────────────

/// Write/refresh the OS schedule and (re)activate it.
pub fn install(hour: u32, minute: u32) -> Result<(), String> {
    install_many(&[(hour, minute)])
}

/// Write/refresh all enabled daily trigger times. Duplicate times are removed.
/// Retire the wake-up installed under the identity used before the rename, so
/// a machine never carries two agents for the same desk.
pub fn retire_legacy() {
    imp::retire_legacy();
}

pub fn install_many(times: &[(u32, u32)]) -> Result<(), String> {
    let times = normalize_times(times)?;
    if times.is_empty() {
        return uninstall();
    }
    imp::install_many(&times)
}

/// Remove the OS schedule. Best-effort; missing entries are not an error.
pub fn uninstall() -> Result<(), String> {
    imp::uninstall()
}

/// Whether a schedule is currently registered.
pub fn is_installed() -> bool {
    imp::is_installed()
}

/// Self-heal: if the registered schedule points at a different binary than the
/// one running (e.g. setup ran from a dev build, then the user installed the
/// app), rewrite it for the current executable. Callers skip this while paused.
pub fn ensure_current(hour: u32, minute: u32) {
    ensure_current_many(&[(hour, minute)])
}

/// Self-heal both the executable path and the full set of trigger times.
pub fn ensure_current_many(times: &[(u32, u32)]) {
    match normalize_times(times) {
        Ok(times) if !times.is_empty() => imp::ensure_current_many(&times),
        Ok(_) => {
            if let Err(error) = uninstall() {
                log::warn!("could not remove obsolete scheduler wakeups: {error}");
            }
        }
        Err(error) => log::warn!("invalid scheduler interval set: {error}"),
    }
}

fn normalize_times(times: &[(u32, u32)]) -> Result<Vec<(u32, u32)>, String> {
    if times
        .iter()
        .any(|(hour, minute)| *hour > 23 || *minute > 59)
    {
        return Err("schedule time is outside 00:00–23:59".into());
    }
    let mut normalized = times.to_vec();
    normalized.sort_unstable();
    normalized.dedup();
    Ok(normalized)
}

// ── macOS: launchd ─────────────────────────────────────────────────────────
#[cfg(target_os = "macos")]
mod imp {
    use super::{current_exe_path, LABEL};
    use std::path::PathBuf;
    use std::process::Command;

    pub fn default_exe_path() -> String {
        "/Applications/Principia Desk.app/Contents/MacOS/principia-desk".into()
    }

    fn plist_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/LaunchAgents")
                .join(format!("{LABEL}.plist")),
        )
    }

    fn interval_contents(times: &[(u32, u32)]) -> String {
        times
            .iter()
            .map(|(hour, minute)| {
                format!(
                    "    <dict>\n      <key>Hour</key><integer>{hour}</integer>\n      <key>Minute</key><integer>{minute}</integer>\n    </dict>"
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn plist_contents(times: &[(u32, u32)]) -> String {
        let exe = super::xml_escape(&current_exe_path());
        let intervals = interval_contents(times);
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>--triggered</string>
  </array>
  <key>StartCalendarInterval</key>
  <array>
{intervals}
  </array>
  <key>RunAtLoad</key><true/>
  <key>ProcessType</key><string>Interactive</string>
  <key>StandardOutPath</key><string>/tmp/principia-desk.launchd.log</string>
  <key>StandardErrorPath</key><string>/tmp/principia-desk.launchd.log</string>
</dict>
</plist>
"#
        )
    }

    /// Unload and delete the agent installed under the pre-rename identity so
    /// one machine never holds two agents for the same desk.
    pub fn retire_legacy() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let path = PathBuf::from(home)
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", super::LEGACY_LABEL));
        if !path.exists() {
            return;
        }
        let _ = Command::new("launchctl")
            .args([
                "bootout",
                &format!("gui/{}/{}", get_uid(), super::LEGACY_LABEL),
            ])
            .output();
        if std::fs::remove_file(&path).is_ok() {
            log::info!("removed the launch agent from before the rename");
        }
    }

    pub fn install_many(times: &[(u32, u32)]) -> Result<(), String> {
        let path = plist_path().ok_or("no HOME")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, plist_contents(times)).map_err(|e| e.to_string())?;
        let uid = get_uid();
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{uid}/{LABEL}")])
            .output();
        let out = Command::new("launchctl")
            .args(["bootstrap", &format!("gui/{uid}"), &path.to_string_lossy()])
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            // "already bootstrapped" style errors are fine after bootout race
            if !err.contains("Bootstrap failed: 5") {
                return Err(format!("launchctl bootstrap failed: {err}"));
            }
        }
        Ok(())
    }

    pub fn uninstall() -> Result<(), String> {
        let uid = get_uid();
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{uid}/{LABEL}")])
            .output();
        if let Some(path) = plist_path() {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    pub fn is_installed() -> bool {
        plist_path().map(|p| p.exists()).unwrap_or(false)
    }

    pub fn ensure_current_many(times: &[(u32, u32)]) {
        let Some(path) = plist_path() else { return };
        let exe = current_exe_path();
        let expected = plist_contents(times);
        let needs_install = match std::fs::read_to_string(&path) {
            Ok(existing) => existing != expected,
            Err(_) => true,
        };
        if needs_install {
            log::info!("launchd plist missing/stale; reinstalling for {exe}");
            if let Err(e) = install_many(times) {
                log::warn!("launchd self-heal failed: {e}");
            }
        }
    }

    fn get_uid() -> u32 {
        // SAFETY: getuid is always safe to call. Declared here (not at module
        // scope) so the symbol is only linked on macOS, where it exists.
        unsafe { libc_getuid() }
    }

    extern "C" {
        #[link_name = "getuid"]
        fn libc_getuid() -> u32;
    }
}

// ── Linux: systemd user timer ──────────────────────────────────────────────
#[cfg(target_os = "linux")]
mod imp {
    use super::current_exe_path;
    use std::path::PathBuf;
    use std::process::Command;

    const UNIT: &str = "principia-desk";
    const LEGACY_UNIT: &str = "system-design-roulette";

    /// Stop and delete the timer installed under the name used before the
    /// rename, so one account never carries two timers for the same desk.
    pub fn retire_legacy() {
        let Some(dir) = unit_dir() else {
            return;
        };
        let service = dir.join(format!("{LEGACY_UNIT}.service"));
        let timer = dir.join(format!("{LEGACY_UNIT}.timer"));
        if !service.exists() && !timer.exists() {
            return;
        }
        let _ = Command::new("systemctl")
            .args([
                "--user",
                "disable",
                "--now",
                &format!("{LEGACY_UNIT}.timer"),
            ])
            .output();
        let _ = std::fs::remove_file(&service);
        let _ = std::fs::remove_file(&timer);
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output();
        log::info!("removed the systemd timer from before the rename");
    }

    pub fn default_exe_path() -> String {
        "/usr/bin/principia-desk".into()
    }

    fn unit_dir() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("systemd/user"))
    }

    fn service_path() -> Option<PathBuf> {
        Some(unit_dir()?.join(format!("{UNIT}.service")))
    }

    fn timer_path() -> Option<PathBuf> {
        Some(unit_dir()?.join(format!("{UNIT}.timer")))
    }

    fn service_contents() -> String {
        let exe = current_exe_path();
        format!(
            "[Unit]\n\
             Description=System Design Roulette daily session\n\n\
             [Service]\n\
             Type=simple\n\
             ExecStart={exe} --triggered\n"
        )
    }

    fn timer_contents(times: &[(u32, u32)]) -> String {
        let intervals = times
            .iter()
            .map(|(hour, minute)| format!("OnCalendar=*-*-* {hour:02}:{minute:02}:00"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "[Unit]\nDescription=Daily System Design Roulette triggers\n\n\
             [Timer]\n{intervals}\nPersistent=true\n\n\
             [Install]\nWantedBy=timers.target\n"
        )
    }

    fn systemctl(args: &[&str]) -> Result<(), String> {
        let out = Command::new("systemctl")
            .arg("--user")
            .args(args)
            .output()
            .map_err(|e| format!("systemctl not available: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    pub fn install_many(times: &[(u32, u32)]) -> Result<(), String> {
        let dir = unit_dir().ok_or("no HOME/XDG_CONFIG_HOME")?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(service_path().unwrap(), service_contents()).map_err(|e| e.to_string())?;
        std::fs::write(timer_path().unwrap(), timer_contents(times)).map_err(|e| e.to_string())?;
        let _ = systemctl(&["daemon-reload"]);
        systemctl(&["enable", "--now", &format!("{UNIT}.timer")])
    }

    pub fn uninstall() -> Result<(), String> {
        let _ = systemctl(&["disable", "--now", &format!("{UNIT}.timer")]);
        if let Some(p) = timer_path() {
            let _ = std::fs::remove_file(p);
        }
        if let Some(p) = service_path() {
            let _ = std::fs::remove_file(p);
        }
        let _ = systemctl(&["daemon-reload"]);
        Ok(())
    }

    pub fn is_installed() -> bool {
        timer_path().map(|p| p.exists()).unwrap_or(false)
    }

    pub fn ensure_current_many(times: &[(u32, u32)]) {
        let Some(svc) = service_path() else { return };
        let exe = current_exe_path();
        let timer = timer_path();
        let expected_timer = timer_contents(times);
        let needs_install = match (
            std::fs::read_to_string(&svc),
            timer.and_then(|path| std::fs::read_to_string(path).ok()),
        ) {
            (Ok(existing), Some(existing_timer)) => {
                !existing.contains(&exe) || existing_timer != expected_timer
            }
            _ => true,
        };
        if needs_install {
            log::info!("systemd unit missing/stale; reinstalling for {exe}");
            if let Err(e) = install_many(times) {
                log::warn!("systemd self-heal failed: {e}");
            }
        }
    }
}

// ── Windows: Task Scheduler (schtasks) ─────────────────────────────────────
#[cfg(target_os = "windows")]
mod imp {
    use super::{current_exe_path, PRODUCT};
    use std::process::Command;

    pub fn default_exe_path() -> String {
        "principia-desk.exe".into()
    }

    /// Delete the scheduled task registered under the name used before the
    /// rename, so one account never carries two tasks for the same desk.
    pub fn retire_legacy() {
        let present = Command::new("schtasks")
            .args(["/Query", "/TN", super::LEGACY_PRODUCT])
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false);
        if !present {
            return;
        }
        let _ = Command::new("schtasks")
            .args(["/Delete", "/TN", super::LEGACY_PRODUCT, "/F"])
            .output();
        log::info!("removed the scheduled task from before the rename");
    }

    pub fn install_many(times: &[(u32, u32)]) -> Result<(), String> {
        use std::io::Write;
        use std::sync::atomic::{AtomicU64, Ordering};
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let user = std::env::var("USERNAME").map_err(|_| "Windows account name unavailable")?;
        let user = match std::env::var("USERDOMAIN") {
            Ok(domain) if !domain.is_empty() => format!("{domain}\\{user}"),
            _ => user,
        };
        let path = std::env::temp_dir().join(format!(
            "principia-schedule-{}-{}.xml",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let xml = super::windows_task_xml(&current_exe_path(), &user, times);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            file.write_all(xml.as_bytes()).map_err(|e| e.to_string())?;
            drop(file);
            Command::new("schtasks")
                .args(["/Create", "/TN", PRODUCT, "/XML"])
                .arg(&path)
                .arg("/F")
                .output()
                .map_err(|e| format!("schtasks not available: {e}"))
        })();
        let _ = std::fs::remove_file(&path);
        let out = result?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    pub fn uninstall() -> Result<(), String> {
        let _ = Command::new("schtasks")
            .args(["/Delete", "/TN", PRODUCT, "/F"])
            .output();
        Ok(())
    }

    pub fn is_installed() -> bool {
        Command::new("schtasks")
            .args(["/Query", "/TN", PRODUCT])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn ensure_current_many(times: &[(u32, u32)]) {
        let exe = current_exe_path();
        let needs_install = match Command::new("schtasks")
            .args(["/Query", "/TN", PRODUCT, "/XML"])
            .output()
        {
            Ok(out) if out.status.success() => {
                let bytes = &out.stdout;
                let xml = if bytes.starts_with(&[0xff, 0xfe]) || bytes.get(1) == Some(&0) {
                    let bytes = bytes.strip_prefix(&[0xff, 0xfe]).unwrap_or(bytes);
                    let (pairs, _) = bytes.as_chunks::<2>();
                    String::from_utf16_lossy(
                        &pairs
                            .iter()
                            .copied()
                            .map(u16::from_le_bytes)
                            .collect::<Vec<_>>(),
                    )
                } else {
                    String::from_utf8_lossy(bytes).into_owned()
                };
                !super::windows_task_matches(&xml, &exe, times)
            }
            _ => true,
        };
        if needs_install {
            log::info!("scheduled task missing/stale; reinstalling for {exe}");
            if let Err(error) = install_many(times) {
                log::warn!("schtasks self-heal failed: {error}");
            }
        }
    }
}

// ── Other targets: no-op (compiles, returns a clear error) ─────────────────
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod imp {
    pub fn default_exe_path() -> String {
        "principia-desk".into()
    }
    pub fn install_many(_times: &[(u32, u32)]) -> Result<(), String> {
        Err("scheduling is not supported on this platform".into())
    }
    pub fn uninstall() -> Result<(), String> {
        Ok(())
    }
    pub fn is_installed() -> bool {
        false
    }
    pub fn ensure_current_many(_times: &[(u32, u32)]) {}
}

#[cfg(test)]
mod tests {
    use super::normalize_times;

    #[test]
    fn scheduler_times_are_validated_sorted_and_deduplicated() {
        assert_eq!(
            normalize_times(&[(19, 0), (7, 30), (19, 0)]).unwrap(),
            vec![(7, 30), (19, 0)]
        );
        assert!(normalize_times(&[(24, 0)]).is_err());
        assert!(normalize_times(&[(9, 60)]).is_err());
    }

    #[test]
    fn windows_task_covers_all_times_and_detects_stale_or_disabled_triggers() {
        let exe = r"C:\Users\A & B\Principia Desk.exe";
        let times = [(7, 30), (19, 0)];
        let xml = super::windows_task_xml(exe, r"DESKTOP\learner", &times);
        assert!(super::windows_task_matches(&xml, exe, &times));
        assert!(!super::windows_task_matches(&xml, exe, &[(7, 30)]));
        assert!(!super::windows_task_matches(&xml, exe, &[(7, 30), (20, 0)]));
        assert!(!super::windows_task_matches(&xml, "old.exe", &times));
        assert!(!super::windows_task_matches(
            &xml.replace("<Enabled>true", "<Enabled>false"),
            exe,
            &times
        ));
        assert!(!super::windows_task_matches(
            &xml.replace("<DaysInterval>1", "<DaysInterval>2"),
            exe,
            &times
        ));
        assert!(!super::windows_task_matches("<broken", exe, &times));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launchd_plist_contains_every_calendar_interval() {
        let plist = super::imp::plist_contents(&[(7, 30), (19, 0)]);
        assert!(plist.contains("<key>StartCalendarInterval</key>"));
        assert_eq!(plist.matches("<key>Hour</key>").count(), 2);
        assert!(plist.contains("<integer>7</integer>"));
        assert!(plist.contains("<integer>19</integer>"));
        assert!(plist.contains("<string>--triggered</string>"));
    }
}
