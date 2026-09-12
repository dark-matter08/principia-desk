//! Provider keys stay outside SQLite and are never returned to the webview.
//! Native runner adapters read them; the UI receives configured/not-configured.
//! macOS uses the login Keychain. Other platforms accept environment keys and
//! report in-app storage as unavailable instead of discarding a saved key.

#[cfg(target_os = "macos")]
const SERVICE: &str = "principia-desk";
/// Keys saved before the rename live under the old service name. Reads fall
/// back to it so an upgrade never looks like a lost key.
#[cfg(target_os = "macos")]
const LEGACY_SERVICE: &str = "system-design-roulette";

#[cfg(any(target_os = "macos", test))]
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

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn get_secret(_name: &str) -> Option<String> {
        None
    }

    pub fn set_secret(_name: &str, _value: &str) -> Result<(), String> {
        Err("in-app key storage is only available on macOS; export the \
             environment variable instead (see README)"
            .into())
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
