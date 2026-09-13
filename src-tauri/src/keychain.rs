//! Provider keys stay outside SQLite and are never returned to the webview.
//! Native runner adapters read them; the UI receives configured/not-configured.
//! Each platform keeps them in its own secret store: the login Keychain on
//! macOS, the Credential Manager on Windows, the desktop's Secret Service
//! (GNOME Keyring, KWallet) on Linux. An environment variable always wins over
//! a stored key, and a machine with no store answering keeps working on
//! environment keys alone.

const SERVICE: &str = "principia-desk";
/// Keys saved before the rename live under the old service name. Reads fall
/// back to it so an upgrade never looks like a lost key.
const LEGACY_SERVICE: &str = "system-design-roulette";

fn account_for(name: &str) -> String {
    format!("{name}_api_key")
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{account_for, LEGACY_SERVICE, SERVICE};
    use std::process::Command;

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
    use super::{account_for, LEGACY_SERVICE, SERVICE};
    use keyring::{Entry, Error};

    #[cfg(target_os = "windows")]
    const STORE: &str = "the Windows Credential Manager";
    #[cfg(not(target_os = "windows"))]
    const STORE: &str = "the desktop's secret service (GNOME Keyring or KWallet)";

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

/// Read a stored secret. `None` covers "never set", "keychain locked", and
/// "`security` unavailable" alike — callers treat all three as not configured.
pub fn get_secret(name: &str) -> Option<String> {
    imp::get_secret(name)
}

/// Store (or clear, if `value` is blank) a secret.
pub fn set_secret(name: &str, value: &str) -> Result<(), String> {
    imp::set_secret(name, value)
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
