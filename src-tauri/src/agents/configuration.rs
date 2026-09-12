//! Saved runner setup is independent of the tutor currently selected for study.
use super::{default_model, process, valid_model, RunnerId};
use crate::db;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunnerConfiguration {
    pub runner: RunnerId,
    pub models: Vec<String>,
    pub custom_command: String,
}

pub fn read(conn: &Connection, runner: RunnerId) -> Result<RunnerConfiguration, String> {
    let key = format!("runner_setup_{}", runner.id());
    if let Some(value) = db::get_config(conn, &key).map_err(|e| e.to_string())? {
        return serde_json::from_str(&value)
            .map_err(|e| format!("could not read runner setup: {e}"));
    }
    let model = db::get_config(conn, &format!("model_{}", runner.id()))
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| default_model(runner));
    let custom_command = if runner == RunnerId::CustomCli {
        db::get_config(conn, "custom_agent_bin")
            .map_err(|e| e.to_string())?
            .unwrap_or_default()
    } else {
        String::new()
    };
    Ok(RunnerConfiguration {
        runner,
        models: vec![super::effective_model(runner, &model)],
        custom_command,
    })
}

pub fn save(
    conn: &Connection,
    mut configuration: RunnerConfiguration,
) -> Result<RunnerConfiguration, String> {
    if configuration.models.len() > 100 || configuration.models.iter().any(|m| !valid_model(m)) {
        return Err("Save at most 100 valid model IDs per runner.".into());
    }
    let mut unique = std::collections::HashSet::new();
    configuration.models.retain(|m| unique.insert(m.clone()));
    if configuration.runner == RunnerId::CustomCli {
        if configuration.custom_command.len() > 4096 {
            return Err("Custom command is too long.".into());
        }
        if !configuration.custom_command.trim().is_empty() {
            process::command_words(&configuration.custom_command).map_err(|e| e.to_string())?;
        }
    } else {
        configuration.custom_command.clear();
    }
    let value = serde_json::to_string(&configuration).map_err(|e| e.to_string())?;
    db::set_config(
        conn,
        &format!("runner_setup_{}", configuration.runner.id()),
        &value,
    )
    .map_err(|e| e.to_string())?;
    Ok(configuration)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuring_other_runners_never_changes_the_active_tutor() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE config(key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .unwrap();
        for (key, value) in [
            ("agent", "claude"),
            ("model", "opus"),
            ("custom_agent_bin", "old-command"),
        ] {
            db::set_config(&conn, key, value).unwrap();
        }
        let custom = save(
            &conn,
            RunnerConfiguration {
                runner: RunnerId::CustomCli,
                models: vec!["default".into()],
                custom_command: "'my new command' --print".into(),
            },
        )
        .unwrap();
        let api = save(
            &conn,
            RunnerConfiguration {
                runner: RunnerId::OpenaiApi,
                models: vec!["model-a".into(), "model-b".into(), "model-a".into()],
                custom_command: "unused".into(),
            },
        )
        .unwrap();
        assert_eq!(read(&conn, RunnerId::CustomCli).unwrap(), custom);
        assert_eq!(api.models, vec!["model-a", "model-b"]);
        assert!(api.custom_command.is_empty());
        assert_eq!(
            db::get_config(&conn, "agent").unwrap().as_deref(),
            Some("claude")
        );
        assert_eq!(
            db::get_config(&conn, "model").unwrap().as_deref(),
            Some("opus")
        );
        assert_eq!(
            db::get_config(&conn, "custom_agent_bin")
                .unwrap()
                .as_deref(),
            Some("old-command")
        );
        assert!(save(
            &conn,
            RunnerConfiguration {
                runner: RunnerId::OpenaiApi,
                models: vec!["not a model ID".into()],
                custom_command: String::new()
            }
        )
        .is_err());
        assert_eq!(read(&conn, RunnerId::OpenaiApi).unwrap(), api);
    }
}
