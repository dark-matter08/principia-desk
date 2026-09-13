//! A private metasearch engine on this machine, run as a local process: no
//! Docker, no account, no key. SearXNG asks the public engines on the
//! learner's behalf and answers JSON, which is what a tutor that cannot
//! browse is missing.
//!
//! The desk finds an instance that is already answering (Remote Ledger's,
//! on the same port, is the common case), starts one that is installed but
//! stopped, or installs its own under `~/.principia-desk/searxng`: a shallow
//! clone and an isolated Python, the same layout Remote Ledger uses, so one
//! machine can carry either install and both desks share it.
//!
//! Three things about running it natively that are easy to get wrong:
//!
//! 1. Never run a bare `python3`. PATH may point at another project's
//!    virtualenv. Interpreters are probed by absolute path and asked their
//!    own version; uv, when present, fetches a pinned one instead.
//! 2. JSON is off by default. SearXNG ships `formats: [html]`, so the API
//!    answers 403 until settings.yml says otherwise.
//! 3. It needs a `secret_key`; without one it refuses to start.

use crate::agents::process;
use crate::execution_log::Feed;
pub use crate::pyenv::Step;
use crate::pyenv::{self, find_python, home_dir, python_install_command, Installer};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

const REPO: &str = "https://github.com/searxng/searxng.git";
const ARCHIVE: &str = "https://github.com/searxng/searxng/archive/refs/heads/master.zip";

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub installed: bool,
    pub running: bool,
    /// The address the desk is configured to ask.
    pub url: String,
    /// Whether that address is this machine, so installing and starting here
    /// would answer it.
    pub local: bool,
    /// Where an install lives or would go.
    pub home: String,
    /// The install found is Remote Ledger's, shared rather than duplicated.
    pub shared_with_ledger: bool,
    pub pid: Option<u32>,
    /// JSON output enabled in settings.yml; without it the API answers 403.
    pub json_enabled: bool,
    pub has_uv: bool,
    pub has_git: bool,
    pub python: Option<String>,
    pub python_version: Option<String>,
    pub python_too_new: bool,
    /// The command to run here to get an interpreter, when one is needed.
    pub python_install: Option<String>,
    pub can_install: bool,
    pub version: Option<String>,
    pub log_tail: String,
}

/// The desk's own install, then Remote Ledger's: either serves.
fn homes() -> Vec<PathBuf> {
    home_dir()
        .map(|home| {
            vec![
                home.join(".principia-desk").join("searxng"),
                home.join(".remote-ledger").join("searxng"),
            ]
        })
        .unwrap_or_default()
}

fn own_home() -> PathBuf {
    homes()
        .into_iter()
        .next()
        .unwrap_or_else(|| PathBuf::from("searxng"))
}

fn venv_python(home: &Path) -> PathBuf {
    pyenv::venv_python(&home.join("venv"))
}

fn src_dir(home: &Path) -> PathBuf {
    home.join("src")
}

fn settings_path(home: &Path) -> PathBuf {
    home.join("settings.yml")
}

fn pid_path(home: &Path) -> PathBuf {
    home.join("searxng.pid")
}

fn log_path(home: &Path) -> PathBuf {
    home.join("searxng.log")
}

/// Written only after the environment is proven to import: a half-finished
/// install leaves a venv and a checkout behind, and both look like success.
fn marker_path(home: &Path) -> PathBuf {
    home.join("installed.json")
}

fn is_installed(home: &Path) -> bool {
    venv_python(home).exists() && src_dir(home).exists() && marker_path(home).exists()
}

/// The first install on this machine that is complete.
fn installed_home() -> Option<PathBuf> {
    homes().into_iter().find(|home| is_installed(home))
}

fn read_pid(home: &Path) -> Option<u32> {
    std::fs::read_to_string(pid_path(home))
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|pid| *pid > 0)
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    // Signal 0 checks for existence; EPERM means it exists but is not ours.
    let result = unsafe { libc::kill(pid as i32, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn pid_alive(pid: u32) -> bool {
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

#[cfg(not(any(unix, windows)))]
fn pid_alive(_pid: u32) -> bool {
    false
}

/// The port the configured address names, when it is this machine.
pub fn local_port(url: &str) -> Option<u16> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let local = matches!(
        host,
        "127.0.0.1" | "localhost" | "[::1]" | "::1" | "0.0.0.0"
    );
    local.then(|| parsed.port_or_known_default().unwrap_or(8899))
}

/// Is an instance answering at the address?
pub async fn running(client: &reqwest::Client, url: &str) -> bool {
    let probe = |path: &str| {
        client
            .get(format!("{}{path}", url.trim_end_matches('/')))
            .timeout(Duration::from_millis(1500))
            .send()
    };
    if let Ok(response) = probe("/healthz").await {
        if response.status().is_success() {
            return true;
        }
    }
    // Older builds have no /healthz; the root page is proof enough.
    probe("").await.is_ok_and(|r| r.status().is_success())
}

fn json_enabled(home: &Path) -> bool {
    std::fs::read_to_string(settings_path(home))
        .map(|text| {
            text.lines()
                .any(|line| line.trim_start().starts_with("- json") && line.trim() == "- json")
        })
        .unwrap_or(false)
}

fn read_version(home: &Path) -> Option<String> {
    let text =
        std::fs::read_to_string(src_dir(home).join("searx").join("version_frozen.py")).ok()?;
    let rest = text.split("VERSION_STRING").nth(1)?;
    let start = rest.find('"')? + 1;
    let end = start + rest[start..].find('"')?;
    Some(rest[start..end].to_string())
}

/// The last lines it wrote, for when it will not start.
pub fn log_tail(home: &Path, lines: usize) -> String {
    let text = std::fs::read_to_string(log_path(home)).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    let tail = all[all.len().saturating_sub(lines)..].join("\n");
    tail.chars()
        .rev()
        .take(3000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Everything the setup page needs to say what to offer.
pub async fn status(client: &reqwest::Client, url: &str) -> Status {
    let home = installed_home().unwrap_or_else(own_home);
    let installed = is_installed(&home);
    let shared_with_ledger = installed && home.to_string_lossy().contains(".remote-ledger");
    let has_uv = process::resolve("uv").is_some();
    let has_git = process::resolve("git").is_some();
    let python = find_python().await;
    let pid = read_pid(&home).filter(|pid| pid_alive(*pid));
    let python_install = if has_uv || python.as_ref().is_some_and(|p| !p.too_new) {
        None
    } else {
        Some(python_install_command())
    };
    Status {
        installed,
        running: running(client, url).await,
        url: url.to_string(),
        local: local_port(url).is_some(),
        home: home.to_string_lossy().into_owned(),
        shared_with_ledger,
        pid,
        json_enabled: json_enabled(&home),
        has_uv,
        has_git,
        python: python
            .as_ref()
            .map(|p| p.path.to_string_lossy().into_owned()),
        python_version: python.as_ref().map(|p| format!("3.{}", p.minor)),
        python_too_new: python.as_ref().is_some_and(|p| p.too_new),
        python_install,
        can_install: has_uv || python.is_some(),
        version: installed.then(|| read_version(&home)).flatten(),
        log_tail: log_tail(&home, 20),
    }
}

/// settings.yml, written by the desk and overwritten on every install:
/// loopback only, no public-instance settings, JSON on.
fn settings_yaml(port: u16) -> String {
    use rand::Rng;
    let secret: String = (0..32)
        .map(|_| format!("{:02x}", rand::thread_rng().gen::<u8>()))
        .collect();
    // A raw string, not a continued one: a `\` continuation would strip the
    // indentation YAML lives on.
    format!(
        r#"# Generated by Principia Desk. Edits here are overwritten on reinstall.
use_default_settings: true

general:
  instance_name: "Principia Desk search"
  donation_url: false
  contact_url: false

server:
  port: {port}
  bind_address: "127.0.0.1"
  secret_key: "{secret}"
  limiter: false
  public_instance: false
  image_proxy: false

search:
  # SearXNG ships [html] only; the JSON API answers 403 until json is listed.
  formats:
    - html
    - json
  safe_search: 0
  autocomplete: ""

ui:
  static_use_hash: true
"#
    )
}

/// SearXNG's source as an archive, for a machine without git: fetched into
/// the home folder and unpacked beside it; an existing `src` is replaced.
async fn fetch_archive(installer: &mut Installer<'_>, home: &Path, src: &Path) -> bool {
    let archive = home.join("searxng-src.zip");
    installer.feed.say(format!("fetching {ARCHIVE}"));
    let client = reqwest::Client::new();
    let bytes = match client
        .get(ARCHIVE)
        .timeout(Duration::from_secs(1200))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => match r.bytes().await {
            Ok(b) => b,
            Err(e) => {
                installer.push("Fetching SearXNG", false, e.to_string());
                return false;
            }
        },
        Ok(r) => {
            installer.push(
                "Fetching SearXNG",
                false,
                format!("{ARCHIVE} answered {}", r.status()),
            );
            return false;
        }
        Err(e) => {
            installer.push("Fetching SearXNG", false, e.to_string());
            return false;
        }
    };
    if let Err(e) = std::fs::write(&archive, &bytes) {
        installer.push("Fetching SearXNG", false, e.to_string());
        return false;
    }
    installer.push(
        "Fetching SearXNG",
        true,
        format!("{} MB", bytes.len() / 1_000_000),
    );
    let unpacked = home.join("searxng-master");
    let _ = std::fs::remove_dir_all(&unpacked);
    let home_text = home.to_string_lossy().into_owned();
    let archive_text = archive.to_string_lossy().into_owned();
    if !installer
        .run(
            Path::new("tar"),
            &["-xf", &archive_text, "-C", &home_text],
            "Unpacking SearXNG",
            None,
        )
        .await
    {
        return false;
    }
    let _ = std::fs::remove_file(&archive);
    let _ = std::fs::remove_dir_all(src);
    if let Err(e) = std::fs::rename(&unpacked, src) {
        installer.push("Unpacking SearXNG", false, e.to_string());
        return false;
    }
    true
}

/// Clone, build an isolated environment, write settings. uv when present
/// (it fetches the pinned interpreter itself), plain venv and pip otherwise.
pub async fn install(feed: &Feed, port: u16) -> Result<Vec<Step>, String> {
    let home = own_home();
    std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    let mut installer = Installer::new(feed);
    let src = src_dir(&home);
    match process::resolve("git") {
        Some(git) => {
            let git = PathBuf::from(git);
            if !src.join(".git").exists() {
                let target = src.to_string_lossy().into_owned();
                if !installer
                    .run(
                        &git,
                        &["clone", "--depth", "1", REPO, &target],
                        "Cloning SearXNG",
                        None,
                    )
                    .await
                {
                    return Ok(installer.steps);
                }
            } else {
                installer
                    .run(
                        &git,
                        &["pull", "--ff-only"],
                        "Updating the checkout",
                        Some(&src),
                    )
                    .await;
                // A clone cut short (the desk closed mid-way) leaves `.git`
                // and no files; a pull is happy with that. Put the tree back,
                // and if that fails, clone again from nothing.
                if !src.join("requirements.txt").exists() {
                    let restored = installer
                        .run(
                            &git,
                            &["checkout", "--", "."],
                            "Restoring the checkout",
                            Some(&src),
                        )
                        .await;
                    if !restored || !src.join("requirements.txt").exists() {
                        let _ = std::fs::remove_dir_all(&src);
                        let target = src.to_string_lossy().into_owned();
                        if !installer
                            .run(
                                &git,
                                &["clone", "--depth", "1", REPO, &target],
                                "Cloning SearXNG again",
                                None,
                            )
                            .await
                        {
                            return Ok(installer.steps);
                        }
                    }
                }
            }
        }
        // No git (a fresh Windows): the same tree as an archive, unpacked
        // with the tar every platform ships.
        None => {
            if !fetch_archive(&mut installer, &home, &src).await {
                return Ok(installer.steps);
            }
        }
    }

    // Not `pip install -e .`: SearXNG's setup.py imports the package to read
    // its version, and the package imports msgspec, so an editable install
    // fails on a clean environment before it can install anything. The
    // pinned requirements and a run from the checkout are what SearXNG's own
    // `make run` does.
    let requirements = src.join("requirements.txt");
    if !requirements.exists() {
        installer.push(
            "requirements.txt",
            false,
            format!(
                "not found in {}; the source there is incomplete. Delete that folder and install again.",
                src.display()
            ),
        );
        return Ok(installer.steps);
    }
    let venv = home.join("venv");
    let requirements_text = requirements.to_string_lossy().into_owned();
    let python = match pyenv::create_venv(&mut installer, &venv).await {
        Ok(python) => python,
        Err(_) => return Ok(installer.steps),
    };
    if !pyenv::pip_install(
        &mut installer,
        &python,
        &["-r", &requirements_text],
        "Installing dependencies",
        Some(&src),
    )
    .await
    {
        return Ok(installer.steps);
    }

    // Prove the environment loads it before calling the install a success.
    if !installer
        .run(
            &python,
            &["-c", "import searx, msgspec, flask; print(searx.__file__)"],
            "Checking it imports",
            Some(&src),
        )
        .await
    {
        return Ok(installer.steps);
    }
    std::fs::write(settings_path(&home), settings_yaml(port)).map_err(|e| e.to_string())?;
    installer.push(
        "Writing settings.yml (JSON API enabled)",
        true,
        String::new(),
    );
    std::fs::write(
        marker_path(&home),
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    )
    .map_err(|e| e.to_string())?;
    installer.push("Installed", true, home.to_string_lossy().into_owned());
    Ok(installer.steps)
}

/// Start the installed instance in the background and wait until it answers.
pub async fn start(client: &reqwest::Client, url: &str, port: u16) -> Result<bool, String> {
    if running(client, url).await {
        return Ok(true);
    }
    let Some(home) = installed_home() else {
        return Err("SearXNG is not installed on this machine.".into());
    };
    let settings = settings_path(&home);
    // settings.yml can go missing or predate a port change; cheap to rewrite.
    if !settings.exists() {
        std::fs::write(&settings, settings_yaml(port)).map_err(|e| e.to_string())?;
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path(&home))
        .map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let src = src_dir(&home);
    let mut command = std::process::Command::new(venv_python(&home));
    command
        .args(["-m", "searx.webapp"])
        .current_dir(&src)
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(err)
        .env("SEARXNG_SETTINGS_PATH", &settings)
        .env("SEARXNG_PORT", port.to_string())
        .env("SEARXNG_BIND_ADDRESS", "127.0.0.1")
        .env("VIRTUAL_ENV", home.join("venv"))
        .env_remove("PYTHONHOME")
        // The package is not installed into site-packages; it runs from the checkout.
        .env("PYTHONPATH", &src);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group, so it outlives the desk and a signal to the
        // desk does not reach it.
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }
    let child = command
        .spawn()
        .map_err(|e| format!("could not start SearXNG: {e}"))?;
    let _ = std::fs::write(pid_path(&home), child.id().to_string());
    let deadline = std::time::Instant::now() + Duration::from_secs(40);
    while std::time::Instant::now() < deadline {
        if running(client, url).await {
            return Ok(true);
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
    }
    Ok(false)
}

/// Stop the instance this machine started, if it is still there.
pub fn stop() -> bool {
    let Some(home) = installed_home() else {
        return false;
    };
    let Some(pid) = read_pid(&home).filter(|pid| pid_alive(*pid)) else {
        let _ = std::fs::remove_file(pid_path(&home));
        return false;
    };
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
    }
    let _ = std::fs::remove_file(pid_path(&home));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_port_reads_loopback_addresses_only() {
        assert_eq!(local_port("http://127.0.0.1:8899"), Some(8899));
        assert_eq!(local_port("http://localhost:8080/"), Some(8080));
        assert_eq!(local_port("http://localhost"), Some(80));
        assert_eq!(local_port("https://search.example.org"), None);
        assert_eq!(local_port("not a url"), None);
    }

    #[test]
    fn settings_enable_json_and_bind_loopback() {
        let text = settings_yaml(8899);
        assert!(text.contains("    - json\n"));
        assert!(text.contains("bind_address: \"127.0.0.1\""));
        assert!(text.contains("port: 8899"));
        assert!(!text.contains("secret_key: \"\""));
    }

    #[test]
    fn json_flag_is_read_from_settings() {
        let dir = std::env::temp_dir().join(format!("principia-searxng-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(settings_path(&dir), settings_yaml(8899)).unwrap();
        assert!(json_enabled(&dir));
        std::fs::write(settings_path(&dir), "search:\n  formats:\n    - html\n").unwrap();
        assert!(!json_enabled(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
