//! Python on this machine, for the two things the desk runs in an isolated
//! environment: SearXNG and the voices. Interpreters are probed by absolute
//! path and asked their own version, never the `python3` PATH happens to
//! name, and uv is preferred when present because it fetches the pinned
//! version itself. Every step reports to the Logs feed as it runs.

use crate::agents::process;
use crate::execution_log::Feed;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The CPython version uv fetches. SearXNG lags the newest release or two;
/// the voice packages are fine with anything from 3.9.
pub const PY_VERSION: &str = "3.12";
/// The window SearXNG's pinned requirements resolve wheels for. Outside it
/// the install is still offered, with the likely failure named first.
const PY_MIN: u32 = 10;
const PY_MAX: u32 = 13;
/// Preference, not numeric order: the pinned version first, then out from it.
const PY_PREFERRED: [u32; 4] = [12, 11, 13, 10];
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// One step of an install, as the setup page lists them afterwards.
#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub step: String,
    pub ok: bool,
    pub output: String,
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// The interpreter inside a virtual environment.
pub fn venv_python(venv: &Path) -> PathBuf {
    if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python")
    }
}

pub fn python_prefixes() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PRINCIPIA_PYTHON_DIRS")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        // /usr/bin/python3 on macOS is the Xcode stub; asking it its version
        // raises the developer tools dialog.
        dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    } else if cfg!(target_os = "linux") {
        dirs.extend(["/usr/bin", "/usr/local/bin"].map(PathBuf::from));
        if let Some(home) = home_dir() {
            dirs.push(home.join(".local").join("bin"));
        }
    } else if cfg!(windows) {
        // python.org installs, per user and for the machine; each folder
        // holds python.exe rather than a python3.N name.
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            roots.push(PathBuf::from(local).join("Programs").join("Python"));
        }
        roots.push(PathBuf::from("C:\\"));
        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            let mut found: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("Python3"))
                })
                .collect();
            found.sort();
            found.reverse();
            dirs.extend(found);
        }
    }
    dirs
}

/// The minor version an interpreter reports for itself; the name only hints.
async fn python_minor(bin: &Path) -> Option<u32> {
    let mut command = tokio::process::Command::new(bin);
    command.args([
        "-c",
        "import sys; print(sys.version_info[0], sys.version_info[1])",
    ]);
    scrub_python_env(&mut command);
    let output = process::capture(command, None, Duration::from_secs(5))
        .await
        .ok()?;
    let mut parts = output.split_whitespace();
    let major = parts.next()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    (major == "3").then_some(minor)
}

#[derive(Debug, Clone)]
pub struct Python {
    pub path: PathBuf,
    pub minor: u32,
    pub too_new: bool,
}

/// Every `python3.N` on this machine, in the order the desk would rather use
/// them, then whatever else is installed, then the bare `python3`.
pub fn python_candidates() -> Vec<PathBuf> {
    let prefixes = python_prefixes();
    let mut out: Vec<PathBuf> = Vec::new();
    let mut add = |path: PathBuf| {
        if path.exists() && !out.contains(&path) {
            out.push(path);
        }
    };
    for version in PY_PREFERRED {
        for dir in &prefixes {
            add(dir.join(format!("python3.{version}")));
        }
    }
    for dir in &prefixes {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| {
                name.strip_prefix("python3.").is_some_and(|rest| {
                    rest.chars().all(|c| c.is_ascii_digit()) && !rest.is_empty()
                })
            })
            .collect();
        names.sort();
        for name in names {
            add(dir.join(name));
        }
    }
    for dir in &prefixes {
        add(dir.join("python3"));
        if cfg!(windows) {
            add(dir.join("python.exe"));
        }
    }
    out
}

/// An interpreter the desk can trust, never the one PATH happens to name.
pub async fn find_python() -> Option<Python> {
    let mut newest: Option<Python> = None;
    for path in python_candidates() {
        let Some(minor) = python_minor(&path).await else {
            continue;
        };
        if minor < PY_MIN {
            continue;
        }
        if minor <= PY_MAX {
            return Some(Python {
                path,
                minor,
                too_new: false,
            });
        }
        // Too new for the pins: keep the least-new one in case it is all there is.
        match &newest {
            Some(n) if minor >= n.minor => {}
            _ => {
                newest = Some(Python {
                    path,
                    minor,
                    too_new: true,
                })
            }
        }
    }
    newest
}

/// How to get an interpreter SearXNG builds against, on this machine: uv
/// everywhere, since it fetches its own pinned CPython.
pub fn python_install_command() -> String {
    if cfg!(target_os = "macos") {
        "brew install uv".into()
    } else if cfg!(windows) {
        "winget install --id=astral-sh.uv -e".into()
    } else {
        "curl -LsSf https://astral.sh/uv/install.sh | sh".into()
    }
}

/// A clean environment: inheriting VIRTUAL_ENV or a PATH into another
/// project's venv is how an install lands somewhere surprising.
pub fn scrub_python_env(command: &mut tokio::process::Command) {
    command
        .env_remove("VIRTUAL_ENV")
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH");
}

/// Runs commands one at a time, reporting each to the feed and keeping the
/// list of what happened for the setup page.
pub struct Installer<'a> {
    pub feed: &'a Feed,
    pub steps: Vec<Step>,
}

impl<'a> Installer<'a> {
    pub fn new(feed: &'a Feed) -> Self {
        Self {
            feed,
            steps: Vec::new(),
        }
    }

    pub fn push(&mut self, step: &str, ok: bool, output: String) -> bool {
        self.feed.say(if ok {
            format!("{step}: done")
        } else {
            format!("{step}: failed\n{output}")
        });
        self.steps.push(Step {
            step: step.into(),
            ok,
            output,
        });
        ok
    }

    pub async fn run(&mut self, bin: &Path, args: &[&str], step: &str, cwd: Option<&Path>) -> bool {
        self.feed.say(format!("{step}…"));
        let mut command = tokio::process::Command::new(bin);
        command.args(args);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        scrub_python_env(&mut command);
        let feed = self.feed;
        let sink = |line: &str| {
            if !line.trim().is_empty() {
                feed.say(format!("  {}", line.trim_end()));
            }
        };
        match process::capture_events(&mut command, None, INSTALL_TIMEOUT, Some(&sink)).await {
            Ok(output) => {
                let tail: String = output
                    .chars()
                    .rev()
                    .take(1200)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                self.push(step, true, tail.trim().to_string())
            }
            Err(error) => {
                let text = error.to_string();
                let tail: String = text
                    .chars()
                    .rev()
                    .take(1500)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                self.push(step, false, tail)
            }
        }
    }
}

/// Create a virtual environment at `venv`: uv with its own pinned Python
/// when present, otherwise a found interpreter. The interpreter inside it
/// comes back.
pub async fn create_venv(installer: &mut Installer<'_>, venv: &Path) -> Result<PathBuf, String> {
    let venv_text = venv.to_string_lossy().into_owned();
    let python = venv_python(venv);
    if let Some(uv) = process::resolve("uv") {
        let uv = PathBuf::from(uv);
        if !installer
            .run(
                &uv,
                &["venv", "--python", PY_VERSION, &venv_text],
                &format!("Creating an isolated Python {PY_VERSION}"),
                None,
            )
            .await
        {
            return Err("the environment could not be created".into());
        }
        return Ok(python);
    }
    let Some(found) = find_python().await else {
        let message = format!(
            "No Python 3.{PY_MIN}+ found in {}.\nInstall uv and it fetches its own Python {PY_VERSION}:\n\n  {}",
            python_prefixes()
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(", "),
            python_install_command()
        );
        installer.push("python", false, message.clone());
        return Err(message);
    };
    if found.too_new {
        installer.push(
            &format!(
                "Python 3.{} is newer than pinned for; trying it anyway",
                found.minor
            ),
            true,
            format!(
                "If the install fails, install uv and it fetches Python {PY_VERSION}:\n\n  {}",
                python_install_command()
            ),
        );
    }
    if !installer
        .run(
            &found.path,
            &["-m", "venv", &venv_text],
            &format!("Creating a virtual environment (Python 3.{})", found.minor),
            None,
        )
        .await
    {
        return Err("the environment could not be created".into());
    }
    if !installer
        .run(
            &python,
            &["-m", "pip", "install", "--upgrade", "pip"],
            "Updating pip",
            None,
        )
        .await
    {
        return Err("pip could not be updated".into());
    }
    Ok(python)
}

/// `pip install` into the environment, through uv when it is there.
pub async fn pip_install(
    installer: &mut Installer<'_>,
    python: &Path,
    args: &[&str],
    step: &str,
    cwd: Option<&Path>,
) -> bool {
    let python_text = python.to_string_lossy().into_owned();
    if let Some(uv) = process::resolve("uv") {
        let mut full = vec!["pip", "install", "--python", python_text.as_str()];
        full.extend_from_slice(args);
        return installer.run(&PathBuf::from(uv), &full, step, cwd).await;
    }
    let mut full = vec!["-m", "pip", "install"];
    full.extend_from_slice(args);
    installer.run(python, &full, step, cwd).await
}

/// Every interpreter worth asking whether a package is importable: the
/// `python3` PATH names (an activated virtual environment lands here), then
/// the ones found by absolute path.
pub fn interpreters() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for name in ["python3", "python"] {
        if let Some(found) = process::resolve(name) {
            let path = PathBuf::from(found);
            if !out.contains(&path) {
                out.push(path);
            }
        }
    }
    for path in python_candidates() {
        if !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// Whether `import <module>` succeeds in an interpreter. Ten seconds, so a
/// heavy package that takes a while to load still answers.
pub async fn imports(python: &Path, module: &str) -> bool {
    let mut command = tokio::process::Command::new(python);
    command.args([
        "-c",
        &format!("import importlib.util as u, sys; sys.exit(0 if u.find_spec({module:?}) else 1)"),
    ]);
    // Not scrubbed: an activated environment is exactly what is being asked about.
    command.env_remove("PYTHONHOME");
    process::capture(command, None, Duration::from_secs(10))
        .await
        .is_ok()
}

/// Install uv with its official installer, into `~/.local/bin`, where the
/// desk looks; then anything that needs a Python can fetch its own.
pub async fn install_uv(installer: &mut Installer<'_>) -> bool {
    if process::resolve("uv").is_some() {
        installer.push(
            "uv is already here",
            true,
            process::resolve("uv").unwrap_or_default(),
        );
        return true;
    }
    let ok = if cfg!(windows) {
        installer
            .run(
                Path::new("powershell"),
                &[
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    "irm https://astral.sh/uv/install.ps1 | iex",
                ],
                "Installing uv",
                None,
            )
            .await
    } else {
        installer
            .run(
                Path::new("sh"),
                &["-c", "curl -LsSf https://astral.sh/uv/install.sh | sh"],
                "Installing uv",
                None,
            )
            .await
    };
    if !ok {
        return false;
    }
    match process::resolve("uv") {
        Some(found) => {
            installer.push("uv ready", true, found);
            true
        }
        None => {
            installer.push(
                "Finding uv",
                false,
                "the installer finished but uv is not in a folder the desk looks in; open a new terminal, run `uv --version`, and press Refresh detection".into(),
            );
            false
        }
    }
}
