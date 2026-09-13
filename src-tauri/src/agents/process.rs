//! Bounded, cancellation-safe desktop process transport, shared by generation and ping.
use crate::generator::{GenError, Result};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt as _;

const MAX_OUTPUT: usize = 16 * 1024 * 1024;

fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    // The PATH a running desk carries is the one it started with; a tool
    // installed since (uv, Python, piper) lands in one of these or in the
    // PATH the system now holds, which Windows keeps in the registry.
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    if let Some(home) = &home {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".cargo").join("bin"));
        if let Ok(entries) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            let mut versions: Vec<_> = entries.flatten().map(|e| e.path().join("bin")).collect();
            versions.sort();
            versions.reverse();
            dirs.extend(versions);
        }
    }
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            // winget's shims, and python.org's per-user installs.
            dirs.push(local.join("Microsoft").join("WinGet").join("Links"));
            if let Ok(entries) = std::fs::read_dir(local.join("Programs").join("Python")) {
                let mut versions: Vec<_> = entries.flatten().map(|e| e.path()).collect();
                versions.sort();
                versions.reverse();
                for dir in versions {
                    dirs.push(dir.join("Scripts"));
                    dirs.push(dir);
                }
            }
        }
        dirs.extend(registry_path());
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    dirs
}

/// The PATH as Windows holds it now (user, then machine), read from the
/// registry so an install made after the desk started is still found.
#[cfg(windows)]
fn registry_path() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for (hive, key) in [
        ("HKCU", "Environment"),
        (
            "HKLM",
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        ),
    ] {
        let Ok(output) = std::process::Command::new("reg")
            .args(["query", &format!("{hive}\\{key}"), "/v", "Path"])
            .creation_flags(0x08000000)
            .output()
        else {
            continue;
        };
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("Path") {
                // "Path    REG_EXPAND_SZ    C:\a;C:\b"
                let mut parts = rest.split_whitespace();
                let _kind = parts.next();
                let value = parts.collect::<Vec<_>>().join(" ");
                let expanded = expand_windows_vars(&value);
                dirs.extend(std::env::split_paths(&expanded));
            }
        }
    }
    dirs
}

/// `%VAR%` references in a registry PATH, replaced from this process's environment.
#[cfg(windows)]
fn expand_windows_vars(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn resolve(name: &str) -> Option<String> {
    let path = Path::new(name);
    if path.components().count() > 1 {
        return executable(path)
            .then(|| path.canonicalize().ok())
            .flatten()
            .map(|p| p.to_string_lossy().into_owned());
    }
    for dir in search_dirs() {
        let candidate = dir.join(name);
        if executable(&candidate) {
            return Some(candidate.to_string_lossy().into_owned());
        }
        #[cfg(windows)]
        for ext in ["exe", "cmd", "bat"] {
            let candidate = candidate.with_extension(ext);
            if executable(&candidate) {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    None
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Shell-style quoting without shell expansion or execution. Keeps paths with spaces usable.
pub fn command_words(spec: &str) -> Result<Vec<String>> {
    let mut words = vec![];
    let mut current = String::new();
    let mut quote = None;
    let mut started = false;
    let mut chars = spec.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (_, '\\')
                if quote != Some('\'')
                    && chars.peek().is_some_and(|n| {
                        *n == '\\' || *n == '"' || *n == '\'' || n.is_whitespace()
                    }) =>
            {
                current.push(chars.next().unwrap());
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            _ => {
                current.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err(GenError::Api("custom command has an unclosed quote".into()));
    }
    if started {
        words.push(current);
    }
    if words.first().is_none_or(String::is_empty) {
        return Err(GenError::NoBinary);
    }
    Ok(words)
}

pub async fn read_bounded(mut reader: impl AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let n = reader.read(&mut chunk).await?;
        if n == 0 {
            return Ok(bytes);
        }
        if bytes.len() + n > MAX_OUTPUT {
            return Err(std::io::Error::other("agent output exceeded 16 MiB"));
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
}

pub async fn capture(
    mut command: Command,
    input: Option<&str>,
    timeout: Duration,
) -> Result<String> {
    capture_events(&mut command, input, timeout, None).await
}

pub type EventSink<'a> = Option<&'a (dyn Fn(&str) + Send + Sync)>;
async fn read_events(
    mut reader: impl AsyncRead + Unpin,
    sink: EventSink<'_>,
) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    let mut line_start = 0;
    loop {
        let n = reader.read(&mut chunk).await?;
        if n == 0 {
            return Ok(bytes);
        }
        if bytes.len() + n > MAX_OUTPUT {
            return Err(std::io::Error::other("agent output exceeded 16 MiB"));
        }
        let before = bytes.len();
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(sink) = sink {
            for index in before..bytes.len() {
                if bytes[index] == b'\n' {
                    sink(&String::from_utf8_lossy(&bytes[line_start..index]));
                    line_start = index + 1;
                }
            }
        }
    }
}

pub async fn capture_events(
    command: &mut Command,
    input: Option<&str>,
    timeout: Duration,
    sink: EventSink<'_>,
) -> Result<String> {
    // A GUI-launched app may find an nvm CLI whose /usr/bin/env node interpreter
    // is absent from launchd's PATH. Give the child the same discovery paths.
    let mut dirs = search_dirs();
    let program = Path::new(command.as_std().get_program());
    if program.is_absolute() {
        if let Some(parent) = program.parent() {
            dirs.insert(0, parent.to_owned());
        }
    }
    if let Ok(path) = std::env::join_paths(dirs) {
        command.env("PATH", path);
    }
    command
        .kill_on_drop(true)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW for a packaged desktop parent.
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            GenError::NoBinary
        } else {
            GenError::Io(e)
        }
    })?;
    #[cfg(unix)]
    let _group = ProcessGroup(child.id().unwrap());
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let stdin = child.stdin.take();
    let work = async {
        let write = async {
            if let Some(mut stdin) = stdin {
                // An early failure may close stdin before reading the entire prompt.
                if let Err(e) = stdin.write_all(input.unwrap_or_default().as_bytes()).await {
                    if e.kind() != std::io::ErrorKind::BrokenPipe {
                        return Err(e);
                    }
                }
                let _ = stdin.shutdown().await;
            }
            Ok::<(), std::io::Error>(())
        };
        tokio::try_join!(
            read_events(stdout, sink),
            read_bounded(stderr),
            child.wait(),
            write
        )
    };
    match tokio::time::timeout(timeout, work).await {
        Ok(Ok((out, _, status, ()))) if status.success() => String::from_utf8(out)
            .map_err(|e| GenError::Parse(format!("agent output was not UTF-8: {e}"))),
        Ok(Ok((out, err, status, ()))) => Err(GenError::BadExit(
            status.code().unwrap_or(-1),
            String::from_utf8_lossy(if err.iter().all(u8::is_ascii_whitespace) {
                &out
            } else {
                &err
            })
            .chars()
            .take(1200)
            .collect(),
        )),
        Ok(Err(e)) => {
            let _ = child.kill().await;
            Err(GenError::Io(e))
        }
        Err(_) => {
            let _ = child.kill().await;
            Err(GenError::Timeout(timeout))
        }
    }
}

#[cfg(unix)]
struct ProcessGroup(u32);
#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        // This child created a new process group. Kill only that group, including
        // workers that inherited its pipes, on timeout/cancellation/completion.
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
    }
}

pub struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new(root: &Path, call_id: &str) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let path = root.join(call_id);
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
