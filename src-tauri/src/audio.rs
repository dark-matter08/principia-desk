//! Listening mode: a lesson as a conversation you can hear.
//!
//! When a class asks for it, the tutor writes the prepared lesson a second
//! time as a two-host dialogue, a teacher and a sharp student, in a spoken
//! register, and the desk voices every line with the engine and the voices
//! the class chose. The transcript stays in the player, the lesson stays
//! readable, and nothing here can hold a session up: the audio is written
//! after the lesson is ready, and a voice that fails leaves the lines for
//! the system voice to read.
//!
//! Four engines, from nothing to install to the best sound. Each one is
//! found where it already is before the desk offers to install it: a
//! `piper` on the PATH, a Python with `kokoro` or `mlx_audio` importable
//! (an activated virtual environment counts), or the desk's own isolated
//! environments under the profile.
//!
//! - `system`: the voices the operating system already has, spoken by the
//!   webview (`speechSynthesis`). Nothing to download.
//! - `piper`: Piper, a neural voice on the processor of any platform; the
//!   `piper-tts` package, voices downloaded one at a time (about 60 MB).
//! - `kokoro`: Kokoro-82M, warm voices on any platform: through mlx-audio
//!   on Apple Silicon, through the `kokoro` package (PyTorch, CPU)
//!   elsewhere. The model is fetched on the first render.
//! - `vibevoice`: Microsoft's VibeVoice, the podcast model, as the
//!   realtime 0.5B converted for mlx-audio: Apple Silicon only, named
//!   voices in English, German and Italian, the model fetched on first use.

use crate::agents::process;
use crate::db;
use crate::execution_log::Feed;
use crate::pyenv::{self, Installer};
use crate::state::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub const SYSTEM: &str = "system";
pub const PIPER: &str = "piper";
pub const KOKORO: &str = "kokoro";
pub const VIBEVOICE: &str = "vibevoice";
pub const ENGINES: [&str; 4] = [SYSTEM, PIPER, KOKORO, VIBEVOICE];

const KOKORO_MLX_MODEL: &str = "mlx-community/Kokoro-82M-bf16";
const KOKORO_TORCH_REPO: &str = "hexgrad/Kokoro-82M";
const VIBEVOICE_MODEL: &str = "mlx-community/VibeVoice-Realtime-0.5B-8bit";
const RENDER_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const SAMPLE: &str = "A cache is a sticky note on the fridge; the database is the filing cabinet in the basement. Deleting the note makes the next reader walk downstairs.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptLine {
    /// `teacher` or `student`.
    pub speaker: String,
    pub text: String,
    /// The voiced segment, when one was rendered.
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GeneratedScript {
    pub lines: Vec<ScriptLine>,
}

/// A voice the desk knows how to fetch or find.
#[derive(Debug, Clone, Serialize)]
pub struct VoiceStatus {
    pub id: String,
    pub engine: &'static str,
    pub label: &'static str,
    pub language: &'static str,
    /// A word on the voice, so a pair can be picked without hearing them.
    pub hint: &'static str,
    pub size_mb: u32,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineStatus {
    pub id: &'static str,
    pub label: &'static str,
    pub ready: bool,
    /// `desk` (the desk's own environment), `found` (already on this
    /// machine), or empty.
    pub source: &'static str,
    /// The binary or interpreter that answers for it.
    pub via: String,
    /// Whether this machine could set it up.
    pub installable: bool,
    pub why: String,
    /// One line on what it is, for the tile.
    pub blurb: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioStatus {
    pub engines: Vec<EngineStatus>,
    pub voices: Vec<VoiceStatus>,
    pub has_uv: bool,
    pub python: Option<String>,
    pub python_install: Option<String>,
    pub apple_silicon: bool,
}

struct VoiceSpec {
    id: &'static str,
    engine: &'static str,
    label: &'static str,
    language: &'static str,
    hint: &'static str,
    size_mb: u32,
}

/// Piper voices from rhasspy/piper-voices, fetched one at a time; Kokoro's
/// and VibeVoice's come with their models.
const VOICES: &[VoiceSpec] = &[
    VoiceSpec {
        id: "en_US-lessac-medium",
        engine: PIPER,
        label: "Lessac",
        language: "English (US)",
        hint: "clear, even, a natural teacher",
        size_mb: 63,
    },
    VoiceSpec {
        id: "en_US-amy-medium",
        engine: PIPER,
        label: "Amy",
        language: "English (US)",
        hint: "bright, quick",
        size_mb: 63,
    },
    VoiceSpec {
        id: "en_US-hfc_female-medium",
        engine: PIPER,
        label: "HFC female",
        language: "English (US)",
        hint: "warm, low",
        size_mb: 63,
    },
    VoiceSpec {
        id: "en_US-hfc_male-medium",
        engine: PIPER,
        label: "HFC male",
        language: "English (US)",
        hint: "calm, low",
        size_mb: 63,
    },
    VoiceSpec {
        id: "en_US-joe-medium",
        engine: PIPER,
        label: "Joe",
        language: "English (US)",
        hint: "plain, direct",
        size_mb: 63,
    },
    VoiceSpec {
        id: "en_US-ryan-high",
        engine: PIPER,
        label: "Ryan",
        language: "English (US)",
        hint: "the fullest of the set, larger download",
        size_mb: 121,
    },
    VoiceSpec {
        id: "en_GB-alan-medium",
        engine: PIPER,
        label: "Alan",
        language: "English (UK)",
        hint: "measured, Scottish",
        size_mb: 63,
    },
    VoiceSpec {
        id: "en_GB-alba-medium",
        engine: PIPER,
        label: "Alba",
        language: "English (UK)",
        hint: "soft, Scottish",
        size_mb: 63,
    },
    VoiceSpec {
        id: "en_GB-jenny_dioco-medium",
        engine: PIPER,
        label: "Jenny",
        language: "English (UK)",
        hint: "light, southern",
        size_mb: 63,
    },
    VoiceSpec {
        id: "de_DE-thorsten-medium",
        engine: PIPER,
        label: "Thorsten",
        language: "German",
        hint: "for the German class",
        size_mb: 63,
    },
    VoiceSpec {
        id: "it_IT-riccardo-x_low",
        engine: PIPER,
        label: "Riccardo",
        language: "Italian",
        hint: "for the Italian class, small",
        size_mb: 28,
    },
    VoiceSpec {
        id: "af_heart",
        engine: KOKORO,
        label: "Heart",
        language: "English (US)",
        hint: "warm, the best of the set",
        size_mb: 0,
    },
    VoiceSpec {
        id: "af_bella",
        engine: KOKORO,
        label: "Bella",
        language: "English (US)",
        hint: "bright",
        size_mb: 0,
    },
    VoiceSpec {
        id: "af_nicole",
        engine: KOKORO,
        label: "Nicole",
        language: "English (US)",
        hint: "quiet, close",
        size_mb: 0,
    },
    VoiceSpec {
        id: "af_sarah",
        engine: KOKORO,
        label: "Sarah",
        language: "English (US)",
        hint: "even",
        size_mb: 0,
    },
    VoiceSpec {
        id: "am_michael",
        engine: KOKORO,
        label: "Michael",
        language: "English (US)",
        hint: "even, low",
        size_mb: 0,
    },
    VoiceSpec {
        id: "am_fenrir",
        engine: KOKORO,
        label: "Fenrir",
        language: "English (US)",
        hint: "deep",
        size_mb: 0,
    },
    VoiceSpec {
        id: "am_adam",
        engine: KOKORO,
        label: "Adam",
        language: "English (US)",
        hint: "plain",
        size_mb: 0,
    },
    VoiceSpec {
        id: "bf_emma",
        engine: KOKORO,
        label: "Emma",
        language: "English (UK)",
        hint: "clear",
        size_mb: 0,
    },
    VoiceSpec {
        id: "bm_george",
        engine: KOKORO,
        label: "George",
        language: "English (UK)",
        hint: "measured",
        size_mb: 0,
    },
    VoiceSpec {
        id: "bm_lewis",
        engine: KOKORO,
        label: "Lewis",
        language: "English (UK)",
        hint: "light",
        size_mb: 0,
    },
    VoiceSpec {
        id: "en-Carter_man",
        engine: VIBEVOICE,
        label: "Carter",
        language: "English",
        hint: "steady, a natural teacher",
        size_mb: 0,
    },
    VoiceSpec {
        id: "en-Davis_man",
        engine: VIBEVOICE,
        label: "Davis",
        language: "English",
        hint: "warm",
        size_mb: 0,
    },
    VoiceSpec {
        id: "en-Frank_man",
        engine: VIBEVOICE,
        label: "Frank",
        language: "English",
        hint: "deep",
        size_mb: 0,
    },
    VoiceSpec {
        id: "en-Mike_man",
        engine: VIBEVOICE,
        label: "Mike",
        language: "English",
        hint: "quick, a natural student",
        size_mb: 0,
    },
    VoiceSpec {
        id: "en-Emma_woman",
        engine: VIBEVOICE,
        label: "Emma",
        language: "English",
        hint: "clear",
        size_mb: 0,
    },
    VoiceSpec {
        id: "en-Grace_woman",
        engine: VIBEVOICE,
        label: "Grace",
        language: "English",
        hint: "bright",
        size_mb: 0,
    },
    VoiceSpec {
        id: "de-Spk0_man",
        engine: VIBEVOICE,
        label: "German man",
        language: "German",
        hint: "for the German class",
        size_mb: 0,
    },
    VoiceSpec {
        id: "de-Spk1_woman",
        engine: VIBEVOICE,
        label: "German woman",
        language: "German",
        hint: "for the German class",
        size_mb: 0,
    },
    VoiceSpec {
        id: "it-Spk0_woman",
        engine: VIBEVOICE,
        label: "Italian woman",
        language: "Italian",
        hint: "for the Italian class",
        size_mb: 0,
    },
    VoiceSpec {
        id: "it-Spk1_man",
        engine: VIBEVOICE,
        label: "Italian man",
        language: "Italian",
        hint: "for the Italian class",
        size_mb: 0,
    },
];

fn voices_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("voices")
}
fn piper_venv(data_dir: &Path) -> PathBuf {
    data_dir.join("piper-venv")
}
fn kokoro_venv(data_dir: &Path) -> PathBuf {
    data_dir.join("kokoro-venv")
}
fn mlx_venv(data_dir: &Path) -> PathBuf {
    data_dir.join("mlx-venv")
}
fn vibevoice_venv(data_dir: &Path) -> PathBuf {
    data_dir.join("vibevoice-venv")
}
/// The realtime model's voice presets (`.pt`, a few MB each), fetched from
/// the official repository with the engine; the mlx model carries its own.
fn vibevoice_voices_dir(data_dir: &Path) -> PathBuf {
    voices_dir(data_dir).join("vibevoice")
}
fn vibevoice_preset(data_dir: &Path, id: &str) -> PathBuf {
    vibevoice_voices_dir(data_dir).join(format!("{id}.pt"))
}
const VIBEVOICE_TORCH_MODEL: &str = "microsoft/VibeVoice-Realtime-0.5B";
const VIBEVOICE_PACKAGE: &str =
    "https://github.com/microsoft/VibeVoice/archive/refs/heads/main.zip";
const VIBEVOICE_PRESETS: &str =
    "https://raw.githubusercontent.com/microsoft/VibeVoice/main/demo/voices/streaming_model";
fn audio_dir(data_dir: &Path, session_id: &str) -> PathBuf {
    data_dir.join("audio").join(session_id)
}
fn apple_silicon() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}
/// An NVIDIA driver on the PATH is the cheap sign of a CUDA GPU to build
/// PyTorch for; without it the processor build is the honest choice.
fn nvidia_gpu() -> bool {
    !cfg!(target_os = "macos") && process::resolve("nvidia-smi").is_some()
}
fn voice_file(data_dir: &Path, id: &str) -> PathBuf {
    voices_dir(data_dir).join(format!("{id}.onnx"))
}
fn spec(id: &str) -> Option<&'static VoiceSpec> {
    VOICES.iter().find(|v| v.id == id)
}

/// How an engine runs on this machine, found in this order: the desk's own
/// environment, then whatever the machine already has.
#[derive(Debug, Clone)]
pub enum Backend {
    /// `piper` as a command (the package's console script or the binary).
    PiperCli(PathBuf),
    /// `python -m piper` in an interpreter that imports it.
    PiperPython(PathBuf),
    /// The `kokoro` package (PyTorch) in an interpreter.
    KokoroTorch(PathBuf),
    /// mlx-audio in an interpreter: Kokoro and VibeVoice on Apple Silicon.
    Mlx(PathBuf),
    /// Microsoft's `vibevoice` package (PyTorch) in an interpreter: VibeVoice
    /// on any processor, a CUDA GPU when there is one.
    VibeVoiceTorch(PathBuf),
}

impl Backend {
    fn via(&self) -> String {
        self.python().to_string_lossy().into_owned()
    }
    fn python(&self) -> &Path {
        match self {
            Backend::PiperCli(p)
            | Backend::PiperPython(p)
            | Backend::KokoroTorch(p)
            | Backend::Mlx(p)
            | Backend::VibeVoiceTorch(p) => p,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Found {
    pub piper: Option<(Backend, &'static str)>,
    pub kokoro: Option<(Backend, &'static str)>,
    pub vibevoice: Option<(Backend, &'static str)>,
}

fn existing(python: PathBuf) -> Option<PathBuf> {
    python.exists().then_some(python)
}

/// Where each engine is, if anywhere. Desk environments first, so a desk
/// install wins over a stray package; then the machine's own.
pub async fn discover(data_dir: &Path) -> Found {
    let mut found = Found::default();
    if let Some(python) = existing(pyenv::venv_python(&piper_venv(data_dir))) {
        found.piper = Some((Backend::PiperPython(python), "desk"));
    }
    if let Some(python) = existing(pyenv::venv_python(&mlx_venv(data_dir))) {
        found.kokoro = Some((Backend::Mlx(python.clone()), "desk"));
        found.vibevoice = Some((Backend::Mlx(python), "desk"));
    }
    if found.kokoro.is_none() {
        if let Some(python) = existing(pyenv::venv_python(&kokoro_venv(data_dir))) {
            found.kokoro = Some((Backend::KokoroTorch(python), "desk"));
        }
    }
    if found.vibevoice.is_none() {
        if let Some(python) = existing(pyenv::venv_python(&vibevoice_venv(data_dir))) {
            found.vibevoice = Some((Backend::VibeVoiceTorch(python), "desk"));
        }
    }
    if found.piper.is_none() {
        if let Some(cli) = process::resolve("piper") {
            found.piper = Some((Backend::PiperCli(PathBuf::from(cli)), "found"));
        }
    }
    if found.piper.is_some() && found.kokoro.is_some() && found.vibevoice.is_some() {
        return found;
    }
    for python in pyenv::interpreters() {
        if found.piper.is_none() && pyenv::imports(&python, "piper").await {
            found.piper = Some((Backend::PiperPython(python.clone()), "found"));
        }
        if (found.kokoro.is_none() || found.vibevoice.is_none())
            && apple_silicon()
            && pyenv::imports(&python, "mlx_audio").await
        {
            if found.kokoro.is_none() {
                found.kokoro = Some((Backend::Mlx(python.clone()), "found"));
            }
            if found.vibevoice.is_none() {
                found.vibevoice = Some((Backend::Mlx(python.clone()), "found"));
            }
        }
        if found.kokoro.is_none() && pyenv::imports(&python, "kokoro").await {
            found.kokoro = Some((Backend::KokoroTorch(python.clone()), "found"));
        }
        if found.vibevoice.is_none() && pyenv::imports(&python, "vibevoice").await {
            found.vibevoice = Some((Backend::VibeVoiceTorch(python.clone()), "found"));
        }
        if found.piper.is_some() && found.kokoro.is_some() && found.vibevoice.is_some() {
            break;
        }
    }
    found
}

/// Where Piper voices may already be: the desk's folder, then the folders
/// the Piper tools and people commonly use. `PRINCIPIA_PIPER_VOICES` adds one.
fn piper_voice_dirs(data_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![voices_dir(data_dir)];
    if let Some(extra) = std::env::var_os("PRINCIPIA_PIPER_VOICES") {
        dirs.push(PathBuf::from(extra));
    }
    if let Some(home) = pyenv::home_dir() {
        dirs.push(home.join(".local").join("share").join("piper-voices"));
        dirs.push(home.join(".local").join("share").join("piper"));
        dirs.push(home.join("piper-voices"));
        dirs.push(home.join(".cache").join("piper"));
    }
    dirs
}

/// The `.onnx` of a Piper voice, wherever it is, with its `.json` beside it.
fn piper_voice_path(data_dir: &Path, id: &str) -> Option<PathBuf> {
    for dir in piper_voice_dirs(data_dir) {
        let onnx = dir.join(format!("{id}.onnx"));
        if onnx.exists() && onnx.with_extension("onnx.json").exists() {
            return Some(onnx);
        }
    }
    None
}

fn voice_installed(data_dir: &Path, found: &Found, spec: &VoiceSpec) -> bool {
    match spec.engine {
        PIPER => piper_voice_path(data_dir, spec.id).is_some(),
        KOKORO => found.kokoro.is_some(),
        VIBEVOICE => match &found.vibevoice {
            Some((Backend::VibeVoiceTorch(_), _)) => vibevoice_preset(data_dir, spec.id).exists(),
            Some(_) => true,
            None => false,
        },
        _ => true,
    }
}

fn engine_status(
    id: &'static str,
    label: &'static str,
    blurb: &'static str,
    found: Option<&(Backend, &'static str)>,
    installable: bool,
    why_missing: String,
) -> EngineStatus {
    EngineStatus {
        id,
        label,
        ready: found.is_some(),
        source: found.map(|(_, source)| *source).unwrap_or(""),
        via: found.map(|(backend, _)| backend.via()).unwrap_or_default(),
        installable,
        why: match found {
            Some((backend, "desk")) => format!("Installed by the desk at {}.", backend.via()),
            Some((backend, _)) => format!("Found on this machine: {}.", backend.via()),
            None => why_missing,
        },
        blurb,
    }
}

/// Everything the voices page needs: which engines are ready, where, which
/// could be, and every voice with whether it is here.
pub async fn status(data_dir: &Path) -> AudioStatus {
    let has_uv = process::resolve("uv").is_some();
    let python = pyenv::find_python().await;
    let can_build = has_uv || python.as_ref().is_some_and(|p| !p.too_new);
    let found = discover(data_dir).await;
    let need_python = format!(
        "Needs uv or Python 3.10 to 3.13. Install uv and it fetches its own Python:\n{}",
        pyenv::python_install_command()
    );
    let engines = vec![
        EngineStatus {
            id: SYSTEM,
            label: "System voice",
            ready: true,
            source: "found",
            via: String::new(),
            installable: false,
            why:
                "The voices this computer already has, read by the desk itself. Nothing to install."
                    .into(),
            blurb: "nothing to install",
        },
        engine_status(
            PIPER,
            "Piper",
            "neural, any processor",
            found.piper.as_ref(),
            can_build,
            if can_build {
                "A neural voice that runs on this processor, on macOS, Windows and Linux: the piper-tts package in an isolated Python under the profile, about 120 MB, then a voice of about 60 MB each.".into()
            } else {
                need_python.clone()
            },
        ),
        engine_status(
            KOKORO,
            "Kokoro",
            "warm, 82M, any platform",
            found.kokoro.as_ref(),
            can_build,
            if can_build && apple_silicon() {
                "Warm, natural voices from an 82M model, on the GPU through mlx-audio: about 1 GB in an isolated Python under the profile, the model (about 330 MB) fetched on the first render.".into()
            } else if can_build {
                "Warm, natural voices from an 82M model, on the processor through the kokoro package (PyTorch): about 400 MB in an isolated Python under the profile, the model fetched on the first render.".into()
            } else {
                need_python.clone()
            },
        ),
        engine_status(
            VIBEVOICE,
            "VibeVoice",
            "the podcast model, any platform",
            found.vibevoice.as_ref(),
            can_build,
            if !can_build {
                need_python.clone()
            } else if apple_silicon() {
                "Microsoft's VibeVoice, the model built for long two-host audio, as the realtime 0.5B converted for mlx-audio on the GPU: about 1 GB in an isolated Python under the profile, the model (about 700 MB) fetched on the first render. Named voices in English, German and Italian.".into()
            } else if nvidia_gpu() {
                "Microsoft's VibeVoice, the model built for long two-host audio: the official package with PyTorch for your NVIDIA GPU, about 3 GB in an isolated Python under the profile, the realtime 0.5B model (about 2 GB) fetched on the first render. Named voices in English, German and Italian.".into()
            } else {
                "Microsoft's VibeVoice, the model built for long two-host audio: the official package with PyTorch on the processor, about 1 GB in an isolated Python under the profile, the realtime 0.5B model (about 2 GB) fetched on the first render. On a processor it renders slower than it plays, which the desk absorbs by writing the audio ahead of the study time; a lesson takes a while, and the Logs page counts the lines. Named voices in English, German and Italian.".into()
            },
        ),
    ];
    let voices = VOICES
        .iter()
        .map(|v| VoiceStatus {
            id: v.id.into(),
            engine: v.engine,
            label: v.label,
            language: v.language,
            hint: v.hint,
            size_mb: v.size_mb,
            installed: voice_installed(data_dir, &found, v),
        })
        .collect();
    AudioStatus {
        engines,
        voices,
        has_uv,
        python: python
            .as_ref()
            .map(|p| p.path.to_string_lossy().into_owned()),
        python_install: if can_build {
            None
        } else {
            Some(pyenv::python_install_command())
        },
        apple_silicon: apple_silicon(),
    }
}

/// Build the isolated environment an engine runs in.
pub async fn install_engine(
    feed: &Feed,
    data_dir: &Path,
    engine: &str,
) -> Result<Vec<pyenv::Step>, String> {
    let mut installer = Installer::new(feed);
    let (venv, packages, check): (PathBuf, Vec<&str>, &str) = match engine {
        PIPER => (
            piper_venv(data_dir),
            vec!["piper-tts"],
            "import piper; print('piper ok')",
        ),
        KOKORO if apple_silicon() => (
            mlx_venv(data_dir),
            vec!["mlx-audio"],
            "import mlx_audio; print('mlx-audio ok')",
        ),
        KOKORO => (
            kokoro_venv(data_dir),
            vec!["kokoro", "soundfile"],
            "import kokoro, soundfile; print('kokoro ok')",
        ),
        VIBEVOICE if apple_silicon() => (
            mlx_venv(data_dir),
            vec!["mlx-audio"],
            "import mlx_audio; print('mlx-audio ok')",
        ),
        VIBEVOICE => return install_vibevoice_torch(feed, data_dir).await,
        SYSTEM => return Err("The system voice needs nothing installed.".into()),
        other => return Err(format!("unknown engine {other}")),
    };
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let python = match pyenv::create_venv(&mut installer, &venv).await {
        Ok(python) => python,
        Err(_) => return Ok(installer.steps),
    };
    if !pyenv::pip_install(
        &mut installer,
        &python,
        &packages,
        &format!("Installing {}", packages.join(" and ")),
        None,
    )
    .await
    {
        return Ok(installer.steps);
    }
    if !installer
        .run(&python, &["-c", check], "Checking it imports", None)
        .await
    {
        return Ok(installer.steps);
    }
    installer.push("Installed", true, venv.to_string_lossy().into_owned());
    Ok(installer.steps)
}

/// Remove an engine's desk environment; the voices downloaded for it stay,
/// and an install found elsewhere on the machine is not touched.
pub fn remove_engine(data_dir: &Path, engine: &str) -> Result<(), String> {
    let venvs: Vec<PathBuf> = match engine {
        PIPER => vec![piper_venv(data_dir)],
        KOKORO => vec![kokoro_venv(data_dir), mlx_venv(data_dir)],
        VIBEVOICE => vec![mlx_venv(data_dir), vibevoice_venv(data_dir)],
        _ => return Err("nothing to remove for that engine".into()),
    };
    for venv in venvs {
        if venv.exists() {
            std::fs::remove_dir_all(&venv).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Fetch a Piper voice into the profile.
pub async fn install_voice(
    feed: &Feed,
    data_dir: &Path,
    id: &str,
) -> Result<Vec<pyenv::Step>, String> {
    let spec = spec(id).ok_or_else(|| format!("unknown voice {id}"))?;
    if spec.engine != PIPER {
        return Err(
            "Only Piper voices are downloaded one at a time; the others come with their model."
                .into(),
        );
    }
    let found = discover(data_dir).await;
    let dir = voices_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut installer = Installer::new(feed);
    let dir_text = dir.to_string_lossy().into_owned();
    let step = format!("Downloading the voice {} ({} MB)", spec.label, spec.size_mb);
    let ok = match found.piper {
        Some((Backend::PiperPython(python), _)) => {
            installer
                .run(
                    &python,
                    &[
                        "-m",
                        "piper.download_voices",
                        "--download-dir",
                        &dir_text,
                        id,
                    ],
                    &step,
                    None,
                )
                .await
        }
        Some((Backend::PiperCli(cli), _)) => {
            // The console script sits beside the interpreter that owns it.
            let python = cli.parent().map(|bin| {
                bin.join(if cfg!(windows) {
                    "python.exe"
                } else {
                    "python"
                })
            });
            match python.filter(|p| p.exists()) {
                Some(python) => {
                    installer
                        .run(
                            &python,
                            &[
                                "-m",
                                "piper.download_voices",
                                "--download-dir",
                                &dir_text,
                                id,
                            ],
                            &step,
                            None,
                        )
                        .await
                }
                None => download_piper_voice(&mut installer, &dir, id).await,
            }
        }
        _ => download_piper_voice(&mut installer, &dir, id).await,
    };
    if !ok {
        return Ok(installer.steps);
    }
    if piper_voice_path(data_dir, id).is_none() {
        installer.push(
            "Checking the files",
            false,
            "the voice did not arrive whole".into(),
        );
        return Ok(installer.steps);
    }
    installer.push("Voice ready", true, id.into());
    Ok(installer.steps)
}

/// The voice files straight from rhasspy/piper-voices, for a machine with
/// the `piper` binary and no Python beside it.
async fn download_piper_voice(installer: &mut Installer<'_>, dir: &Path, id: &str) -> bool {
    // en_US-lessac-medium → en/en_US/lessac/medium/en_US-lessac-medium
    let Some((locale, rest)) = id.split_once('-') else {
        installer.push(
            "Voice address",
            false,
            format!("{id} is not a Piper voice name"),
        );
        return false;
    };
    let Some((name, quality)) = rest.rsplit_once('-') else {
        installer.push(
            "Voice address",
            false,
            format!("{id} is not a Piper voice name"),
        );
        return false;
    };
    let language = locale.split('_').next().unwrap_or(locale);
    let base = format!("https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/{language}/{locale}/{name}/{quality}/{id}");
    let client = reqwest::Client::new();
    for suffix in [".onnx.json", ".onnx"] {
        let url = format!("{base}{suffix}");
        installer.feed.say(format!("fetching {url}"));
        let response = match client
            .get(&url)
            .timeout(Duration::from_secs(1200))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                installer.push(
                    "Downloading the voice",
                    false,
                    format!("{url} answered {}", r.status()),
                );
                return false;
            }
            Err(e) => {
                installer.push("Downloading the voice", false, e.to_string());
                return false;
            }
        };
        let bytes = match response.bytes().await {
            Ok(b) => b,
            Err(e) => {
                installer.push("Downloading the voice", false, e.to_string());
                return false;
            }
        };
        if let Err(e) = std::fs::write(dir.join(format!("{id}{suffix}")), &bytes) {
            installer.push("Writing the voice", false, e.to_string());
            return false;
        }
    }
    installer.push(&format!("Downloading the voice {id}"), true, String::new());
    true
}

pub fn remove_voice(data_dir: &Path, id: &str) -> Result<(), String> {
    let onnx = voice_file(data_dir, id);
    let _ = std::fs::remove_file(onnx.with_extension("onnx.json"));
    if onnx.exists() {
        std::fs::remove_file(&onnx).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Two voices of an engine that differ when they can, for a class that
/// has not chosen: the first two ready ones.
pub async fn default_voices(data_dir: &Path, engine: &str) -> (String, String) {
    let found = discover(data_dir).await;
    let ready: Vec<&VoiceSpec> = VOICES
        .iter()
        .filter(|v| v.engine == engine && voice_installed(data_dir, &found, v))
        .collect();
    match ready.as_slice() {
        [] => (String::new(), String::new()),
        [one] => (one.id.into(), one.id.into()),
        [first, second, ..] => (first.id.into(), second.id.into()),
    }
}

/// Voice every line into `out`, `seg_000.wav` onward.
pub async fn render(
    feed: &Feed,
    data_dir: &Path,
    engine: &str,
    teacher: &str,
    student: &str,
    lines: &[ScriptLine],
    out: &Path,
) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let found = discover(data_dir).await;
    match engine {
        PIPER => match found.piper {
            Some((backend, _)) => {
                render_piper(feed, data_dir, &backend, teacher, student, lines, out).await
            }
            None => Err("Piper is not on this machine.".into()),
        },
        KOKORO => match found.kokoro {
            Some((Backend::Mlx(python), _)) => {
                render_mlx(
                    feed,
                    &python,
                    KOKORO_MLX_MODEL,
                    teacher,
                    student,
                    lines,
                    out,
                )
                .await
            }
            Some((Backend::KokoroTorch(python), _)) => {
                render_kokoro_torch(feed, &python, teacher, student, lines, out).await
            }
            _ => Err("Kokoro is not on this machine.".into()),
        },
        VIBEVOICE => match found.vibevoice {
            Some((Backend::Mlx(python), _)) => {
                render_mlx(feed, &python, VIBEVOICE_MODEL, teacher, student, lines, out).await
            }
            Some((Backend::VibeVoiceTorch(python), _)) => {
                render_vibevoice_torch(feed, data_dir, &python, teacher, student, lines, out).await
            }
            _ => Err("VibeVoice is not on this machine.".into()),
        },
        SYSTEM => Err("the system voice is read by the interface, not rendered".into()),
        other => Err(format!("unknown engine {other}")),
    }
}

fn voice_for<'a>(speaker: &str, teacher: &'a str, student: &'a str) -> &'a str {
    if speaker == "student" && !student.is_empty() {
        student
    } else if !teacher.is_empty() {
        teacher
    } else {
        student
    }
}

async fn render_piper(
    feed: &Feed,
    data_dir: &Path,
    backend: &Backend,
    teacher: &str,
    student: &str,
    lines: &[ScriptLine],
    out: &Path,
) -> Result<(), String> {
    let total = lines.len();
    for (index, line) in lines.iter().enumerate() {
        let voice = voice_for(&line.speaker, teacher, student);
        if voice.is_empty() {
            return Err("no voice chosen".into());
        }
        let model = piper_voice_path(data_dir, voice)
            .ok_or_else(|| format!("the voice {voice} is not downloaded"))?;
        let target = out.join(format!("seg_{index:03}.wav"));
        let model_text = model.to_string_lossy().into_owned();
        let target_text = target.to_string_lossy().into_owned();
        let mut command = match backend {
            Backend::PiperCli(cli) => {
                let mut c = tokio::process::Command::new(cli);
                c.args([
                    "-m",
                    &model_text,
                    "-f",
                    &target_text,
                    "--sentence-silence",
                    "0.25",
                ]);
                c
            }
            _ => {
                let mut c = tokio::process::Command::new(backend.python());
                c.args([
                    "-m",
                    "piper",
                    "-m",
                    &model_text,
                    "-f",
                    &target_text,
                    "--sentence-silence",
                    "0.25",
                ]);
                c
            }
        };
        command.env_remove("PYTHONHOME");
        process::capture(command, Some(&line.text), Duration::from_secs(300))
            .await
            .map_err(|e| format!("line {} did not render: {e}", index + 1))?;
        if !target.exists() {
            return Err(format!("line {} did not render", index + 1));
        }
        if index % 10 == 9 || index + 1 == total {
            feed.say(format!("voiced {} of {total} lines", index + 1));
        }
    }
    Ok(())
}

fn plan_json(lines: &[ScriptLine], teacher: &str, student: &str) -> Result<String, String> {
    let plan: Vec<serde_json::Value> = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            serde_json::json!({ "index": index, "voice": voice_for(&line.speaker, teacher, student), "text": line.text })
        })
        .collect();
    serde_json::to_string(&plan).map_err(|e| e.to_string())
}

async fn run_render_script(feed: &Feed, python: &Path, script: &Path) -> Result<(), String> {
    let mut command = tokio::process::Command::new(python);
    command.arg(script);
    command.env_remove("PYTHONHOME");
    let sink = |line: &str| {
        if !line.trim().is_empty() {
            feed.say(line.trim_end().to_string());
        }
    };
    process::capture_events(&mut command, None, RENDER_TIMEOUT, Some(&sink))
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn check_segments(lines: &[ScriptLine], out: &Path) -> Result<(), String> {
    let missing = lines
        .iter()
        .enumerate()
        .filter(|(index, _)| !out.join(format!("seg_{index:03}.wav")).exists())
        .count();
    if missing > 0 {
        return Err(format!("{missing} of {} lines did not render", lines.len()));
    }
    Ok(())
}

/// Kokoro and VibeVoice through mlx-audio: one script, a segment per line.
async fn render_mlx(
    feed: &Feed,
    python: &Path,
    model: &str,
    teacher: &str,
    student: &str,
    lines: &[ScriptLine],
    out: &Path,
) -> Result<(), String> {
    let script = out.join("render.py");
    let plan = plan_json(lines, teacher, student)?;
    std::fs::write(
        &script,
        format!(
            r#"import json, os
from mlx_audio.tts.generate import generate_audio

plan = json.loads({plan:?})
out = {out:?}
for line in plan:
    prefix = os.path.join(out, "seg_%03d" % line["index"])
    generate_audio(
        text=line["text"],
        model={model:?},
        voice=line["voice"],
        file_prefix=prefix,
        audio_format="wav",
        join_audio=True,
        verbose=False,
    )
    print("voiced", line["index"] + 1, "of", len(plan), flush=True)
"#,
            plan = plan,
            out = out.to_string_lossy(),
            model = model,
        ),
    )
    .map_err(|e| e.to_string())?;
    run_render_script(feed, python, &script).await?;
    check_segments(lines, out)
}

/// Kokoro through the PyTorch package: one script, a segment per line, the
/// language taken from the voice's first letter (a American, b British).
async fn render_kokoro_torch(
    feed: &Feed,
    python: &Path,
    teacher: &str,
    student: &str,
    lines: &[ScriptLine],
    out: &Path,
) -> Result<(), String> {
    let script = out.join("render.py");
    let plan = plan_json(lines, teacher, student)?;
    std::fs::write(
        &script,
        format!(
            r#"import json, os
import numpy as np
import soundfile as sf
from kokoro import KPipeline

plan = json.loads({plan:?})
out = {out:?}
pipelines = {{}}
for line in plan:
    voice = line["voice"] or "af_heart"
    lang = voice[0] if voice[0] in "ab" else "a"
    if lang not in pipelines:
        pipelines[lang] = KPipeline(lang_code=lang, repo_id={repo:?})
    chunks = [audio for _, _, audio in pipelines[lang](line["text"], voice=voice)]
    audio = np.concatenate([np.asarray(c) for c in chunks]) if chunks else np.zeros(2400, dtype="float32")
    sf.write(os.path.join(out, "seg_%03d.wav" % line["index"]), audio, 24000)
    print("voiced", line["index"] + 1, "of", len(plan), flush=True)
"#,
            plan = plan,
            out = out.to_string_lossy(),
            repo = KOKORO_TORCH_REPO,
        ),
    )
    .map_err(|e| e.to_string())?;
    run_render_script(feed, python, &script).await?;
    check_segments(lines, out)
}

/// The official package with PyTorch: CUDA wheels when an NVIDIA driver is
/// there, processor wheels otherwise, then the ten voice presets the desk
/// lists, fetched from the repository beside the model code.
async fn install_vibevoice_torch(feed: &Feed, data_dir: &Path) -> Result<Vec<pyenv::Step>, String> {
    let mut installer = Installer::new(feed);
    let venv = vibevoice_venv(data_dir);
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let python = match pyenv::create_venv(&mut installer, &venv).await {
        Ok(python) => python,
        Err(_) => return Ok(installer.steps),
    };
    // PyPI's torch is CUDA-built on Linux (large) and processor-only on
    // Windows; the index picks the right one for what the machine has.
    let torch: Vec<&str> = if nvidia_gpu() {
        if cfg!(windows) {
            vec![
                "torch",
                "--index-url",
                "https://download.pytorch.org/whl/cu124",
            ]
        } else {
            vec!["torch"]
        }
    } else if cfg!(target_os = "linux") {
        vec![
            "torch",
            "--index-url",
            "https://download.pytorch.org/whl/cpu",
        ]
    } else {
        vec!["torch"]
    };
    let step = if nvidia_gpu() {
        "Installing PyTorch for the NVIDIA GPU"
    } else {
        "Installing PyTorch for the processor"
    };
    if !pyenv::pip_install(&mut installer, &python, &torch, step, None).await {
        return Ok(installer.steps);
    }
    if !pyenv::pip_install(
        &mut installer,
        &python,
        &[VIBEVOICE_PACKAGE, "soundfile"],
        "Installing VibeVoice",
        None,
    )
    .await
    {
        return Ok(installer.steps);
    }
    if !installer
        .run(
            &python,
            &["-c", "import vibevoice, transformers, torch, soundfile; print('vibevoice ok', torch.__version__)"],
            "Checking it imports",
            None,
        )
        .await
    {
        return Ok(installer.steps);
    }
    let dir = vibevoice_voices_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let client = reqwest::Client::new();
    for voice in VOICES.iter().filter(|v| v.engine == VIBEVOICE) {
        let target = vibevoice_preset(data_dir, voice.id);
        if target.exists() {
            continue;
        }
        let url = format!("{VIBEVOICE_PRESETS}/{}.pt", voice.id);
        feed.say(format!("fetching the voice {}", voice.label));
        let bytes = match client
            .get(&url)
            .timeout(Duration::from_secs(600))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r.bytes().await.map_err(|e| e.to_string())?,
            Ok(r) => {
                installer.push(
                    "Fetching the voices",
                    false,
                    format!("{url} answered {}", r.status()),
                );
                return Ok(installer.steps);
            }
            Err(e) => {
                installer.push("Fetching the voices", false, e.to_string());
                return Ok(installer.steps);
            }
        };
        std::fs::write(&target, &bytes).map_err(|e| e.to_string())?;
    }
    installer.push(
        "Fetching the voices",
        true,
        dir.to_string_lossy().into_owned(),
    );
    installer.push("Installed", true, venv.to_string_lossy().into_owned());
    Ok(installer.steps)
}

/// VibeVoice through the official package: the realtime 0.5B model with a
/// cached voice prompt per host, a segment per line; the model is fetched
/// from Hugging Face on the first run and loaded once per render.
async fn render_vibevoice_torch(
    feed: &Feed,
    data_dir: &Path,
    python: &Path,
    teacher: &str,
    student: &str,
    lines: &[ScriptLine],
    out: &Path,
) -> Result<(), String> {
    for voice in [teacher, student] {
        if !voice.is_empty() && !vibevoice_preset(data_dir, voice).exists() {
            return Err(format!("the VibeVoice voice {voice} is not on this machine; install VibeVoice again to fetch the presets"));
        }
    }
    let script = out.join("render.py");
    let plan = plan_json(lines, teacher, student)?;
    std::fs::write(
        &script,
        format!(
            r#"import copy, json, os, time
import torch
from vibevoice.modular.modeling_vibevoice_streaming_inference import VibeVoiceStreamingForConditionalGenerationInference
from vibevoice.processor.vibevoice_streaming_processor import VibeVoiceStreamingProcessor
from transformers.cache_utils import DynamicCache
from transformers.modeling_outputs import BaseModelOutputWithPast

plan = json.loads({plan:?})
out = {out:?}
presets = {presets:?}
model_id = {model:?}
device = "cuda" if torch.cuda.is_available() else "cpu"
dtype = torch.bfloat16 if device == "cuda" else torch.float32
print("loading", model_id, "on", device, flush=True)
processor = VibeVoiceStreamingProcessor.from_pretrained(model_id)
model = VibeVoiceStreamingForConditionalGenerationInference.from_pretrained(
    model_id, torch_dtype=dtype, device_map=device, attn_implementation="sdpa")
model.eval()
model.set_ddpm_inference_steps(num_steps=5)
prompts = {{}}
def prompt_for(voice):
    if voice not in prompts:
        with torch.serialization.safe_globals([BaseModelOutputWithPast, DynamicCache]):
            prompts[voice] = torch.load(os.path.join(presets, voice + ".pt"), map_location=device, weights_only=True)
    return prompts[voice]
for line in plan:
    started = time.time()
    voice = line["voice"] or "en-Carter_man"
    cached = prompt_for(voice)
    inputs = processor.process_input_with_cached_prompt(
        text=line["text"].replace("’", "'").replace("“", '"').replace("”", '"'),
        cached_prompt=cached, padding=True, return_tensors="pt", return_attention_mask=True)
    for k, v in inputs.items():
        if torch.is_tensor(v):
            inputs[k] = v.to(device)
    outputs = model.generate(**inputs, max_new_tokens=None, cfg_scale=1.5, tokenizer=processor.tokenizer,
        generation_config={{"do_sample": False}}, verbose=False, all_prefilled_outputs=copy.deepcopy(cached))
    if not outputs.speech_outputs or outputs.speech_outputs[0] is None:
        raise SystemExit("no audio for line %d" % (line["index"] + 1))
    processor.save_audio(outputs.speech_outputs[0], output_path=os.path.join(out, "seg_%03d.wav" % line["index"]))
    print("voiced", line["index"] + 1, "of", len(plan), "in %.0fs" % (time.time() - started), flush=True)
"#,
            plan = plan,
            out = out.to_string_lossy(),
            presets = vibevoice_voices_dir(data_dir).to_string_lossy(),
            model = VIBEVOICE_TORCH_MODEL,
        ),
    )
    .map_err(|e| e.to_string())?;
    run_render_script(feed, python, &script).await?;
    check_segments(lines, out)
}

/// A sentence in one voice, so a pair can be chosen by ear. Kept under
/// `audio/preview/` and reused.
pub async fn preview(
    feed: &Feed,
    data_dir: &Path,
    engine: &str,
    voice: &str,
) -> Result<PathBuf, String> {
    let out = data_dir
        .join("audio")
        .join("preview")
        .join(engine)
        .join(voice);
    let target = out.join("seg_000.wav");
    if target.exists() {
        return Ok(target);
    }
    let lines = vec![ScriptLine {
        speaker: "teacher".into(),
        text: SAMPLE.into(),
        file: None,
    }];
    render(feed, data_dir, engine, voice, voice, &lines, &out).await?;
    Ok(target)
}

// ── The audio of one lesson ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct LessonAudio {
    pub session_id: String,
    /// `writing`, `rendering`, `ready` or `failed`.
    pub status: String,
    pub engine: String,
    pub teacher_voice: String,
    pub student_voice: String,
    pub lines: Vec<ScriptLine>,
    pub error: Option<String>,
    pub updated_at: String,
    /// Whether the interface should read the lines itself: the system voice
    /// was chosen, or the rendering failed and the lines are all there is.
    pub speak: bool,
}

fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn lesson_audio(conn: &Connection, session_id: &str) -> db::Result<Option<LessonAudio>> {
    let row = conn
        .query_row(
            "SELECT engine, teacher_voice, student_voice, lines_json, audio_dir, status, error, updated_at FROM lesson_audio WHERE session_id = ?1",
            [session_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((
        engine,
        teacher_voice,
        student_voice,
        lines_json,
        audio_dir,
        status,
        error,
        updated_at,
    )) = row
    else {
        return Ok(None);
    };
    let mut lines: Vec<ScriptLine> = serde_json::from_str(&lines_json).unwrap_or_default();
    let mut rendered = 0;
    if let Some(dir) = &audio_dir {
        for (index, line) in lines.iter_mut().enumerate() {
            let file = Path::new(dir).join(format!("seg_{index:03}.wav"));
            if file.exists() {
                line.file = Some(file.to_string_lossy().into_owned());
                rendered += 1;
            }
        }
    }
    let speak = engine == SYSTEM
        || (status == "failed" && !lines.is_empty())
        || (status == "ready" && rendered < lines.len());
    Ok(Some(LessonAudio {
        session_id: session_id.into(),
        status,
        engine,
        teacher_voice,
        student_voice,
        lines,
        error,
        updated_at,
        speak,
    }))
}

fn begin(
    conn: &Connection,
    session_id: &str,
    engine: &str,
    teacher: &str,
    student: &str,
) -> db::Result<()> {
    let at = now();
    conn.execute(
        "INSERT INTO lesson_audio (session_id, engine, teacher_voice, student_voice, lines_json, audio_dir, status, error, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, '[]', NULL, 'writing', NULL, ?5, ?5)
         ON CONFLICT(session_id) DO UPDATE SET engine = excluded.engine, teacher_voice = excluded.teacher_voice, student_voice = excluded.student_voice, lines_json = '[]', audio_dir = NULL, status = 'writing', error = NULL, updated_at = excluded.updated_at",
        params![session_id, engine, teacher, student, at],
    )?;
    Ok(())
}

fn set_lines(
    conn: &Connection,
    session_id: &str,
    lines: &[ScriptLine],
    status: &str,
) -> db::Result<()> {
    conn.execute(
        "UPDATE lesson_audio SET lines_json = ?2, status = ?3, updated_at = ?4 WHERE session_id = ?1",
        params![session_id, serde_json::to_string(lines)?, status, now()],
    )?;
    Ok(())
}

fn finish(
    conn: &Connection,
    session_id: &str,
    dir: Option<&Path>,
    error: Option<&str>,
) -> db::Result<()> {
    conn.execute(
        "UPDATE lesson_audio SET audio_dir = ?2, status = ?3, error = ?4, updated_at = ?5 WHERE session_id = ?1",
        params![
            session_id,
            dir.map(|d| d.to_string_lossy().into_owned()),
            if error.is_some() { "failed" } else { "ready" },
            error,
            now()
        ],
    )?;
    Ok(())
}

/// What a class asked for, if it asked. `force` writes the audio for a
/// lesson whose class has it off, when a learner presses Listen anyway.
pub fn preference_for(
    conn: &Connection,
    course_id: &str,
) -> crate::domain::enrollment::AudioPreference {
    crate::domain::classes::current_configuration(conn, course_id)
        .ok()
        .flatten()
        .map(|config| config.audio)
        .unwrap_or_default()
}

/// Write the dialogue for a prepared lesson and voice it. Runs after the
/// lesson is ready and never in its way: a failure is recorded on the row
/// and the lesson is unchanged.
pub async fn prepare_for_session(
    app: &AppHandle,
    session_id: &str,
    force: bool,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let (course_id, label, minutes, title, markdown, preference) = {
        let conn = state.db.0.lock().unwrap();
        let id = crate::domain::sessions::SessionId(session_id.to_string());
        let session = crate::domain::sessions::get(&conn, &id).map_err(|e| e.to_string())?;
        let course_id = session.context.course.course_id.clone();
        let preference = preference_for(&conn, &course_id);
        if !preference.enabled && !force {
            return Ok(());
        }
        if let Some(existing) = lesson_audio(&conn, session_id).map_err(|e| e.to_string())? {
            if existing.status == "ready"
                || existing.status == "writing"
                || existing.status == "rendering"
            {
                return Ok(());
            }
        }
        let Some(lesson) =
            crate::domain::sessions::lesson(&conn, &id).map_err(|e| e.to_string())?
        else {
            return Err("the lesson is not prepared yet".into());
        };
        let markdown = lesson
            .content
            .body
            .get("markdown")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or("this lesson has no reading to voice")?;
        let label = crate::classroom::program_row(&conn, &course_id)
            .map(|p| p.label)
            .unwrap_or_else(|_| course_id.clone());
        (
            course_id,
            label,
            session.context.pace.session_minutes,
            lesson.content.title.clone(),
            markdown,
            preference,
        )
    };
    let engine = if preference.engine.is_empty() {
        SYSTEM.to_string()
    } else {
        preference.engine.clone()
    };
    let (teacher, student) =
        if preference.teacher_voice.is_empty() && preference.student_voice.is_empty() {
            default_voices(&state.data_dir, &engine).await
        } else {
            (
                preference.teacher_voice.clone(),
                preference.student_voice.clone(),
            )
        };
    {
        let conn = state.db.0.lock().unwrap();
        begin(&conn, session_id, &engine, &teacher, &student).map_err(|e| e.to_string())?;
    }
    let _ = app.emit(
        "audio:state",
        serde_json::json!({ "session_id": session_id, "status": "writing" }),
    );
    let feed = &state.generator.feed;
    let _run = feed.begin(&format!("audio:{session_id}"), &course_id);
    feed.say(format!("writing the dialogue for {title} ({label})"));
    let lines = match state
        .generator
        .generate_audio_script(&title, &label, minutes, &markdown)
        .await
    {
        Ok(lines) if !lines.is_empty() => lines,
        Ok(_) => {
            let conn = state.db.0.lock().unwrap();
            let _ = finish(&conn, session_id, None, Some("the tutor returned no lines"));
            let _ = app.emit(
                "audio:state",
                serde_json::json!({ "session_id": session_id, "status": "failed" }),
            );
            return Err("the tutor returned no lines".into());
        }
        Err(error) => {
            let message = format!("the dialogue was not written: {error}");
            let conn = state.db.0.lock().unwrap();
            let _ = finish(&conn, session_id, None, Some(&message));
            let _ = app.emit(
                "audio:state",
                serde_json::json!({ "session_id": session_id, "status": "failed" }),
            );
            return Err(message);
        }
    };
    feed.say(format!("{} lines written", lines.len()));
    if engine == SYSTEM {
        let conn = state.db.0.lock().unwrap();
        set_lines(&conn, session_id, &lines, "ready").map_err(|e| e.to_string())?;
        finish(&conn, session_id, None, None).map_err(|e| e.to_string())?;
        let _ = app.emit(
            "audio:state",
            serde_json::json!({ "session_id": session_id, "status": "ready" }),
        );
        return Ok(());
    }
    {
        let conn = state.db.0.lock().unwrap();
        set_lines(&conn, session_id, &lines, "rendering").map_err(|e| e.to_string())?;
    }
    let _ = app.emit(
        "audio:state",
        serde_json::json!({ "session_id": session_id, "status": "rendering" }),
    );
    let out = audio_dir(&state.data_dir, session_id);
    let outcome = render(
        feed,
        &state.data_dir,
        &engine,
        &teacher,
        &student,
        &lines,
        &out,
    )
    .await;
    let conn = state.db.0.lock().unwrap();
    match outcome {
        Ok(()) => {
            finish(&conn, session_id, Some(&out), None).map_err(|e| e.to_string())?;
            feed.say("audio ready");
            let _ = app.emit(
                "audio:state",
                serde_json::json!({ "session_id": session_id, "status": "ready" }),
            );
            Ok(())
        }
        Err(error) => {
            let message = format!("the voice failed, the desk reads the lines itself: {error}");
            finish(&conn, session_id, None, Some(&message)).map_err(|e| e.to_string())?;
            feed.say(message.clone());
            let _ = app.emit(
                "audio:state",
                serde_json::json!({ "session_id": session_id, "status": "failed" }),
            );
            Err(message)
        }
    }
}

/// Start the audio for a session in the background, when its class asks
/// for it. Called once the lesson is ready.
pub fn spawn_for_session(app: &AppHandle, session_id: &str) {
    let handle = app.clone();
    let id = session_id.to_string();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = prepare_for_session(&handle, &id, false).await {
            log::warn!("audio for {id}: {error}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_voice_falls_back_to_the_other_host() {
        assert_eq!(voice_for("student", "t", "s"), "s");
        assert_eq!(voice_for("teacher", "t", "s"), "t");
        assert_eq!(voice_for("student", "t", ""), "t");
        assert_eq!(voice_for("teacher", "", "s"), "s");
    }

    #[test]
    fn every_voice_belongs_to_a_known_engine() {
        for voice in VOICES {
            assert!(
                matches!(voice.engine, PIPER | KOKORO | VIBEVOICE),
                "{}",
                voice.id
            );
        }
        assert!(spec("en_US-lessac-medium").is_some());
        assert!(spec("en-Carter_man").is_some());
        assert!(spec("nope").is_none());
    }

    /// `cargo test --lib audio::tests::print_discovery -- --ignored --nocapture`
    /// says what this machine has, the way the voices page will see it.
    #[tokio::test]
    #[ignore]
    async fn print_discovery() {
        let dir = std::env::temp_dir().join("principia-discovery");
        let found = discover(&dir).await;
        println!("piper: {:?}", found.piper);
        println!("kokoro: {:?}", found.kokoro);
        println!("vibevoice: {:?}", found.vibevoice);
    }

    /// `cargo test --lib audio::tests::render_one_line -- --ignored --nocapture`
    /// voices one sentence with each engine this machine has, under the
    /// temporary directory, and says how long each took.
    #[tokio::test]
    #[ignore]
    async fn render_one_line() {
        let dir = std::env::temp_dir().join("principia-render-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, _rx) = tokio::sync::broadcast::channel(64);
        let feed = Feed::new(tx);
        let found = discover(&dir).await;
        let line = vec![ScriptLine {
            speaker: "teacher".into(),
            text: SAMPLE.into(),
            file: None,
        }];
        if found.kokoro.is_some() {
            let started = std::time::Instant::now();
            let out = dir.join("kokoro");
            let result = render(&feed, &dir, KOKORO, "af_heart", "am_michael", &line, &out).await;
            println!(
                "kokoro: {result:?} in {:?} → {}",
                started.elapsed(),
                out.join("seg_000.wav").display()
            );
        }
        if found.piper.is_some() {
            let voice = "en_US-lessac-medium";
            if piper_voice_path(&dir, voice).is_none() {
                let steps = install_voice(&feed, &dir, voice).await;
                println!(
                    "piper voice: {:?}",
                    steps.map(|s| s
                        .iter()
                        .map(|x| format!("{} {}", x.step, x.ok))
                        .collect::<Vec<_>>())
                );
            }
            let started = std::time::Instant::now();
            let out = dir.join("piper");
            let result = render(&feed, &dir, PIPER, voice, voice, &line, &out).await;
            println!(
                "piper: {result:?} in {:?} → {}",
                started.elapsed(),
                out.join("seg_000.wav").display()
            );
        }
    }

    #[test]
    fn a_piper_voice_found_beside_the_desk_counts_as_installed() {
        let dir = std::env::temp_dir().join(format!("principia-voices-{}", std::process::id()));
        let extra = dir.join("elsewhere");
        std::fs::create_dir_all(&extra).unwrap();
        std::fs::write(extra.join("en_US-amy-medium.onnx"), b"x").unwrap();
        std::fs::write(extra.join("en_US-amy-medium.onnx.json"), b"{}").unwrap();
        std::env::set_var("PRINCIPIA_PIPER_VOICES", &extra);
        assert!(piper_voice_path(&dir, "en_US-amy-medium").is_some());
        assert!(piper_voice_path(&dir, "en_US-joe-medium").is_none());
        std::env::remove_var("PRINCIPIA_PIPER_VOICES");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
