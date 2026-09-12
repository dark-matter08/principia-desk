//! Ollama setup and model downloads, adapted from Remote Ledger's local-model route.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    process::Stdio,
    sync::{Mutex, OnceLock},
};

pub fn base() -> Result<String> {
    let value = std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into());
    let value = value.trim_end_matches('/').trim_end_matches("/v1");
    let url = reqwest::Url::parse(value).map_err(|_| GenError::Api("invalid OLLAMA_URL".into()))?;
    // The local route must remain local; a remote server belongs to an API route.
    if !matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    ) || !matches!(url.scheme(), "http" | "https")
        || url.password().is_some()
        || !url.username().is_empty()
    {
        return Err(GenError::Api(
            "On this machine requires a loopback Ollama endpoint".into(),
        ));
    }
    Ok(value.into())
}
fn client() -> reqwest::Client {
    reqwest::Client::new()
}
pub fn binary() -> Option<String> {
    if let Some(bin) = process::resolve("ollama")
        .or_else(|| process::resolve("/Applications/Ollama.app/Contents/Resources/ollama"))
    {
        return Some(bin);
    }
    #[cfg(windows)]
    for root in ["LOCALAPPDATA", "ProgramFiles"] {
        if let Ok(root) = std::env::var(root) {
            for path in [
                format!("{root}/Programs/Ollama/ollama.exe"),
                format!("{root}/Ollama/ollama.exe"),
            ] {
                if let Some(bin) = process::resolve(&path) {
                    return Some(bin);
                }
            }
        }
    }
    None
}
pub async fn reachable() -> bool {
    let Ok(base) = base() else {
        return false;
    };
    client()
        .get(format!("{base}/api/version"))
        .timeout(Duration::from_millis(1500))
        .send()
        .await
        .is_ok_and(|r| r.status().is_success())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModel {
    pub name: String,
    pub size_bytes: u64,
    pub modified: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct LocalStatus {
    pub installed: bool,
    pub running: bool,
    pub version: Option<String>,
    pub models: Vec<LocalModel>,
    pub memory_gb: Option<u64>,
    pub install_command: String,
    pub can_install: bool,
}
fn install_command() -> (String, bool) {
    if cfg!(target_os = "macos") {
        if process::resolve("brew").is_some() {
            ("brew install ollama".into(), true)
        } else {
            (
                "Download the macOS app from ollama.com/download".into(),
                false,
            )
        }
    } else if cfg!(target_os = "windows") {
        (
            "winget install --id Ollama.Ollama --exact".into(),
            process::resolve("winget").is_some(),
        )
    } else {
        ("curl -fsSL https://ollama.com/install.sh | sh".into(), true)
    }
}
fn memory_gb() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("/usr/sbin/sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse::<u64>()
            .ok()
            .map(|bytes| bytes / (1024 * 1024 * 1024))
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/meminfo")
            .ok()?
            .lines()
            .find_map(|line| {
                line.strip_prefix("MemTotal:")?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
            .map(|kb| kb / (1024 * 1024))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}
pub async fn status() -> Result<LocalStatus> {
    let base = base()?;
    let bin = binary();
    let mut version = None;
    let mut models = vec![];
    let mut running = false;
    if let Ok(response) = client()
        .get(format!("{base}/api/version"))
        .timeout(Duration::from_millis(1500))
        .send()
        .await
    {
        if response.status().is_success() {
            running = true;
            version = response
                .json::<Value>()
                .await
                .ok()
                .and_then(|v| v["version"].as_str().map(str::to_owned));
        }
    }
    if running {
        let response = client()
            .get(format!("{base}/api/tags"))
            .timeout(Duration::from_secs(4))
            .send()
            .await
            .map_err(|e| GenError::Api(e.without_url().to_string()))?;
        if !response.status().is_success() {
            return Err(GenError::Api(format!(
                "Could not list installed Ollama models ({})",
                response.status()
            )));
        }
        let value = response
            .json::<Value>()
            .await
            .map_err(|_| GenError::Parse("could not read installed Ollama models".into()))?;
        models = value["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                Some(LocalModel {
                    name: m["name"].as_str()?.into(),
                    size_bytes: m["size"].as_u64().unwrap_or(0),
                    modified: m["modified_at"].as_str().unwrap_or_default().into(),
                })
            })
            .collect();
    }
    let (install_command, can_install) = install_command();
    Ok(LocalStatus {
        installed: bin.is_some(),
        running,
        version,
        models,
        memory_gb: memory_gb(),
        install_command,
        can_install,
    })
}
pub async fn install() -> Result<String> {
    if binary().is_some() {
        return Ok("Ollama is already installed.".into());
    }
    let mut command = if cfg!(target_os = "macos") {
        let bin = process::resolve("brew").ok_or_else(|| {
            GenError::Api("Download Ollama from ollama.com/download, then refresh.".into())
        })?;
        let mut c = tokio::process::Command::new(bin);
        c.args(["install", "ollama"]);
        c
    } else if cfg!(target_os = "windows") {
        let mut c =
            tokio::process::Command::new(process::resolve("winget").ok_or(GenError::NoBinary)?);
        c.args(["install", "--id", "Ollama.Ollama", "--exact"]);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.args(["-c", "curl -fsSL https://ollama.com/install.sh | sh"]);
        c
    };
    command.stdin(Stdio::null());
    let output = process::capture(command, None, Duration::from_secs(900)).await?;
    if binary().is_none() {
        return Err(GenError::Api("Installer finished, but Ollama was not found. Check the installer output or install from ollama.com/download.".into()));
    }
    Ok(format!(
        "Ollama installed. {}",
        output
            .chars()
            .rev()
            .take(600)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
    ))
}
pub async fn start() -> Result<()> {
    if reachable().await {
        return Ok(());
    }
    let bin = binary().ok_or(GenError::NoBinary)?;
    let mut command = tokio::process::Command::new(bin);
    command
        .env("OLLAMA_HOST", base()?)
        .arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn()?;
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if reachable().await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    Err(GenError::Api(
        "Ollama did not start. Run ollama serve to inspect its error.".into(),
    ))
}
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 160
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_:./".contains(c))
    {
        return Err(GenError::Api("invalid Ollama model name".into()));
    }
    if name.ends_with(":cloud") || name.ends_with("-cloud") {
        return Err(GenError::Api(
            "Choose downloaded weights for On this machine; cloud models use a remote provider."
                .into(),
        ));
    }
    Ok(())
}
pub async fn validate_chat_model(name: &str) -> Result<()> {
    validate_name(name)?;
    let response = client()
        .post(format!("{}/api/show", base()?))
        .timeout(Duration::from_secs(8))
        .json(&json!({"model":name}))
        .send()
        .await
        .map_err(|_| {
            GenError::Api("Ollama is not running. Start it under On this machine.".into())
        })?;
    if !response.status().is_success() {
        return Err(GenError::Api(format!(
            "Ollama does not have {name}. Download it first."
        )));
    }
    let value = response
        .json::<Value>()
        .await
        .map_err(|_| GenError::Parse("could not inspect the local model".into()))?;
    validate_chat_capabilities(value)
}
pub(super) fn validate_chat_capabilities(value: Value) -> Result<()> {
    if value["remote_host"].as_str().is_some_and(|s| !s.is_empty()) {
        return Err(GenError::Api(
            "This model runs in the cloud. Choose downloaded weights for On this machine.".into(),
        ));
    }
    if value["capabilities"]
        .as_array()
        .is_some_and(|caps| !caps.iter().any(|c| c == "completion"))
    {
        return Err(GenError::Api(
            "This model cannot answer prompts. Choose a chat model, not an embedding-only model."
                .into(),
        ));
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullState {
    pub model: String,
    pub status: String,
    pub percent: Option<u64>,
    pub done: bool,
    pub error: Option<String>,
}
static PULLS: OnceLock<Mutex<HashMap<String, PullState>>> = OnceLock::new();
pub fn pulls() -> Vec<PullState> {
    PULLS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect()
}
fn update(state: PullState) {
    PULLS
        .get()
        .unwrap()
        .lock()
        .unwrap()
        .insert(state.model.clone(), state);
}
pub(super) fn decode_progress(line: &[u8], state: &mut PullState) -> Result<bool> {
    let value: Value = serde_json::from_slice(line)
        .map_err(|_| GenError::Parse("invalid download progress".into()))?;
    if let Some(error) = value["error"].as_str() {
        return Err(GenError::Api(error.into()));
    }
    state.status = value["status"].as_str().unwrap_or("Downloading").into();
    state.percent = value["total"].as_u64().filter(|n| *n > 0).map(|total| {
        (value["completed"].as_u64().unwrap_or(0).saturating_mul(100) / total).min(100)
    });
    Ok(state.status == "success")
}
pub async fn pull(name: String) -> Result<PullState> {
    validate_name(&name)?;
    if !reachable().await {
        start().await?;
    }
    let initial = PullState {
        model: name.clone(),
        status: "Starting download".into(),
        percent: None,
        done: false,
        error: None,
    };
    {
        let mut pulls = PULLS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap();
        if let Some(existing) = pulls.get(&name).filter(|p| !p.done) {
            return Ok(existing.clone());
        }
        pulls.insert(name.clone(), initial.clone());
    }
    let mut state = initial.clone();
    tokio::spawn(async move {
        let result=async {
            let mut response=client().post(format!("{}/api/pull",base()?)).timeout(Duration::from_secs(3600)).json(&json!({"model":name,"stream":true})).send().await.map_err(|e|GenError::Api(e.without_url().to_string()))?;
            if !response.status().is_success(){return Err(GenError::Api(format!("Download returned {}",response.status())));}
            let mut buffer=Vec::new();let mut success=false;
            while let Some(chunk)=response.chunk().await.map_err(|e|GenError::Api(e.to_string()))? {
                buffer.extend_from_slice(&chunk);
                if buffer.len()>1024*1024{return Err(GenError::Parse("unreadable download progress".into()));}
                while let Some(end)=buffer.iter().position(|b|*b==b'\n') {
                    let line:Vec<_>=buffer.drain(..=end).collect();
                    if line.iter().all(u8::is_ascii_whitespace){continue;}
                    success |= decode_progress(&line, &mut state)?;
                    update(state.clone());
                }
            }
            if !buffer.iter().all(u8::is_ascii_whitespace) { success |= decode_progress(&buffer, &mut state)?; }
            if !success{return Err(GenError::Api("Download ended before Ollama confirmed success. Refresh installed models or retry.".into()));}Ok(())
        }.await;
        state.done = true;
        match result {
            Ok(()) => {
                state.status = "Downloaded".into();
                state.percent = Some(100);
            }
            Err(error) => {
                state.status = "Download failed".into();
                state.error = Some(error.to_string());
            }
        }
        update(state);
    });
    Ok(initial)
}
pub async fn remove(name: &str) -> Result<()> {
    validate_name(name)?;
    let response = client()
        .delete(format!("{}/api/delete", base()?))
        .timeout(Duration::from_secs(15))
        .json(&json!({"model":name}))
        .send()
        .await
        .map_err(|e| GenError::Api(e.without_url().to_string()))?;
    if !response.status().is_success() {
        return Err(GenError::Api(format!(
            "Ollama could not remove the model ({})",
            response.status()
        )));
    }
    PULLS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .remove(name);
    Ok(())
}
