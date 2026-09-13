//! Provider keys stay outside SQLite and are never returned to the webview.
//! Native runner adapters read them; the UI receives configured/not-configured.
//! Each platform keeps them in its own secret store: the login Keychain on
//! macOS, the Credential Manager on Windows, the desktop's Secret Service
//! (GNOME Keyring, KWallet) on Linux. An environment variable always wins over
//! a stored key.
//!
//! An installed desk has to be able to save a key on every machine, so when
//! no store answers (a Linux session without a secret service, a store that
//! refuses) the key goes to a private file in the profile directory instead,
//! readable by this user alone, and the interface says so.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const SERVICE: &str = "principia-desk";
/// Keys saved before the rename live under the old service name. Reads fall
/// back to it so an upgrade never looks like a lost key.
const LEGACY_SERVICE: &str = "system-design-roulette";

fn account_for(name: &str) -> String {
    format!("{name}_api_key")
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{account_for, StoreKind, LEGACY_SERVICE, SERVICE};
    use std::process::Command;

    pub const NATIVE: StoreKind = StoreKind::Keychain;

    /// The login Keychain is always there; `security` answers even when it
    /// is locked (it asks the user).
    pub fn store_answers() -> bool {
        true
    }

    fn read(service: &str, name: &str) -> Option<String> {
        let output = Command::new("security")
            .args([
                "find-generic-password",
                "-a",
                &account_for(name),
                "-s",
                service,
                "-w",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    }

    pub fn get_secret(name: &str) -> Option<String> {
        // A key saved before the rename still belongs to this learner.
        read(SERVICE, name).or_else(|| read(LEGACY_SERVICE, name))
    }

    pub fn set_secret(name: &str, value: &str) -> Result<(), String> {
        let value = value.trim();
        if value.is_empty() {
            return delete_secret(name);
        }
        // `-U` updates the item in place if it already exists, so re-saving
        // a key doesn't create duplicate keychain entries.
        let status = Command::new("security")
            .args([
                "add-generic-password",
                "-a",
                &account_for(name),
                "-s",
                SERVICE,
                "-w",
                value,
                "-U",
            ])
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("keychain write failed".into())
        }
    }

    pub fn delete_secret(name: &str) -> Result<(), String> {
        let status = Command::new("security")
            .args([
                "delete-generic-password",
                "-a",
                &account_for(name),
                "-s",
                SERVICE,
            ])
            .status()
            .map_err(|e| e.to_string())?;
        // Exit code 44 is "item not found" — clearing an already-clear key
        // is not an error.
        if status.success() || status.code() == Some(44) {
            Ok(())
        } else {
            Err("keychain delete failed".into())
        }
    }
}

/// Windows and Linux go through the `keyring` crate: the Credential Manager
/// and the Secret Service respectively. A store that does not answer (a Linux
/// session without a secret service, a locked collection) reads as "not
/// configured" and says so on a save, the same as a locked Keychain would.
#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{account_for, StoreKind, LEGACY_SERVICE, SERVICE};
    use keyring::{Entry, Error};

    #[cfg(target_os = "windows")]
    const STORE: &str = "the Windows Credential Manager";
    #[cfg(not(target_os = "windows"))]
    const STORE: &str = "the desktop's secret service (GNOME Keyring or KWallet)";
    #[cfg(target_os = "windows")]
    pub const NATIVE: StoreKind = StoreKind::CredentialManager;
    #[cfg(not(target_os = "windows"))]
    pub const NATIVE: StoreKind = StoreKind::SecretService;

    /// Whether the store is reachable at all: a lookup that finds nothing is
    /// an answer, a bus or a service that is not there is not.
    pub fn store_answers() -> bool {
        match Entry::new(SERVICE, "probe").and_then(|entry| entry.get_password()) {
            Ok(_) | Err(Error::NoEntry) => true,
            Err(e) => {
                log::debug!("secret store does not answer: {e}");
                false
            }
        }
    }

    fn read(service: &str, name: &str) -> Option<String> {
        let entry = Entry::new(service, &account_for(name)).ok()?;
        match entry.get_password() {
            Ok(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
            Ok(_) => None,
            Err(Error::NoEntry) => None,
            Err(e) => {
                log::debug!("secret store read for {name} failed: {e}");
                None
            }
        }
    }

    pub fn get_secret(name: &str) -> Option<String> {
        // A key saved before the rename still belongs to this learner.
        read(SERVICE, name).or_else(|| read(LEGACY_SERVICE, name))
    }

    pub fn set_secret(name: &str, value: &str) -> Result<(), String> {
        let value = value.trim();
        if value.is_empty() {
            return delete_secret(name);
        }
        let entry = Entry::new(SERVICE, &account_for(name)).map_err(|e| e.to_string())?;
        entry.set_password(value).map_err(|e| {
            format!(
                "{STORE} did not take the key ({e}); export the environment \
                 variable instead (see README)"
            )
        })
    }

    pub fn delete_secret(name: &str) -> Result<(), String> {
        let entry = Entry::new(SERVICE, &account_for(name)).map_err(|e| e.to_string())?;
        // Clearing an already-clear key is not an error.
        match entry.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("{STORE} did not release the key: {e}")),
        }
    }
}

/// The private file that stands in for a secret store: `secrets.json` in
/// the profile directory, mode 0600 where modes exist.
mod file_store {
    use super::{account_for, Path};
    use std::collections::BTreeMap;

    const FILE: &str = "secrets.json";

    fn load(dir: &Path) -> BTreeMap<String, String> {
        std::fs::read_to_string(dir.join(FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(dir: &Path, map: &BTreeMap<String, String>) -> Result<(), String> {
        let path = dir.join(FILE);
        let text = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn get(dir: &Path, name: &str) -> Option<String> {
        load(dir)
            .get(&account_for(name))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    pub fn set(dir: &Path, name: &str, value: &str) -> Result<(), String> {
        let mut map = load(dir);
        if value.trim().is_empty() {
            map.remove(&account_for(name));
        } else {
            map.insert(account_for(name), value.trim().to_string());
        }
        if map.is_empty() {
            let _ = std::fs::remove_file(dir.join(FILE));
            return Ok(());
        }
        save(dir, &map)
    }

    pub fn has_any(dir: &Path) -> bool {
        !load(dir).is_empty()
    }
}

static PROFILE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Where the fallback file lives. Called once at startup with the profile
/// directory; before that, or in tests, there is no fallback.
pub fn init(profile_dir: &Path) {
    let _ = PROFILE_DIR.set(profile_dir.to_path_buf());
}

/// Which store holds the keys on this machine, for the interface to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StoreKind {
    Keychain,
    CredentialManager,
    SecretService,
    /// `secrets.json` in the profile directory.
    ProfileFile,
}

impl StoreKind {
    pub fn label(self) -> &'static str {
        match self {
            StoreKind::Keychain => "the macOS Keychain",
            StoreKind::CredentialManager => "the Windows Credential Manager",
            StoreKind::SecretService => "the desktop's secret service",
            StoreKind::ProfileFile => "a private file in your profile",
        }
    }
}

/// The store a save would go to right now. A key already kept in the file
/// keeps the file as the answer, so the interface does not promise a store
/// the key is not in.
pub fn store_kind() -> StoreKind {
    if let Some(dir) = PROFILE_DIR.get() {
        if file_store::has_any(dir) {
            return StoreKind::ProfileFile;
        }
    }
    if imp::store_answers() {
        imp::NATIVE
    } else {
        StoreKind::ProfileFile
    }
}

/// Read a stored secret. `None` covers "never set", "store locked", and
/// "store unavailable" alike — callers treat all three as not configured.
pub fn get_secret(name: &str) -> Option<String> {
    imp::get_secret(name).or_else(|| PROFILE_DIR.get().and_then(|dir| file_store::get(dir, name)))
}

/// Store (or clear, if `value` is blank) a secret: in the platform's store
/// when it answers, in the profile file when it does not.
pub fn set_secret(name: &str, value: &str) -> Result<(), String> {
    match imp::set_secret(name, value) {
        Ok(()) => {
            // A key that once fell back to the file must not shadow the new one.
            if let Some(dir) = PROFILE_DIR.get() {
                let _ = file_store::set(dir, name, "");
            }
            Ok(())
        }
        Err(native) => {
            let Some(dir) = PROFILE_DIR.get() else {
                return Err(native);
            };
            log::warn!("{}; keeping the key in the profile file instead", native);
            file_store::set(dir, name, value)
        }
    }
}

pub fn has_secret(name: &str) -> bool {
    get_secret(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_naming_is_namespaced_per_provider() {
        assert_eq!(account_for("deepseek"), "deepseek_api_key");
        assert_ne!(account_for("deepseek"), account_for("openai"));
    }
}
