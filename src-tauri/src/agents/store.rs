use super::types::*;
use crate::generator::{GenError, Result};
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use std::{path::Path, time::Duration};

fn connection(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(storage_error)?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(storage_error)?;
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(storage_error)?;
    Ok(conn)
}
fn storage_error(error: impl std::fmt::Display) -> GenError {
    GenError::Api(format!("agent activity could not be saved: {error}"))
}
pub fn policy(conn: &Connection) -> Result<AgentPolicy> {
    Ok(AgentPolicy {
        fallback_agent: crate::db::get_config(conn, "agent_fallback")
            .map_err(storage_error)?
            .filter(|s| !s.is_empty()),
        fallback_model: crate::db::get_config(conn, "agent_fallback_model")
            .map_err(storage_error)?
            .unwrap_or_else(|| "default".into()),
        monthly_budget_usd: crate::db::get_config(conn, "agent_budget_monthly_usd")
            .map_err(storage_error)?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0),
    })
}
pub fn fallback(path: &Path, custom: &str) -> Result<Option<Route>> {
    let policy = policy(&connection(path)?)?;
    policy
        .fallback_agent
        .map(|name| {
            let runner = RunnerId::parse(&name)
                .ok_or_else(|| GenError::Api("unknown configured fallback runner".into()))?;
            Ok(Route {
                runner,
                model: super::effective_model(runner, &policy.fallback_model),
                custom_command: custom.into(),
            })
        })
        .transpose()
}
pub fn begin(path: &Path, id: &str, req: &RunRequest, fallback_of: Option<&str>) -> Result<()> {
    let mut conn = connection(path)?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let metered = req.route.runner.metered()
        && !(req.route.runner == RunnerId::OpenrouterApi
            && super::models::free_model_id(&req.route.model));
    if metered {
        let cap = crate::db::get_config(&tx, "agent_budget_monthly_usd")
            .map_err(storage_error)?
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|n| n.is_finite() && *n > 0.0)
            .unwrap_or(0.0);
        if cap > 0.0 {
            let (spent, unknown, running): (f64, i64, i64) = tx.query_row("SELECT COALESCE(SUM(cost_usd),0), COALESCE(SUM(status!='running' AND cost_usd IS NULL),0), COALESCE(SUM(status='running'),0) FROM agent_calls WHERE metered=1 AND substr(started_at,1,7)=strftime('%Y-%m','now')", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(storage_error)?;
            if spent >= cap {
                return Err(GenError::Api(format!(
                    "Monthly API budget reached (${spent:.2} / ${cap:.2}). Adjust it in Settings."
                )));
            }
            if unknown > 0 || running > 0 {
                return Err(GenError::Api("API budget cannot be checked while another call is running or its cost is unknown. Review agent activity before retrying.".into()));
            }
        }
    }
    tx.execute("INSERT INTO agent_calls(id,started_at,runner,model,purpose,owner_key,fallback_of,status,metered) VALUES (?1,strftime('%Y-%m-%dT%H:%M:%fZ','now'),?2,?3,?4,?5,(SELECT id FROM agent_calls WHERE id=?6),'running',?7)", params![id,req.route.runner.id(),req.route.model,req.purpose,req.owner,fallback_of,metered]).map_err(storage_error)?;
    tx.commit().map_err(storage_error)
}
pub fn finish(
    path: &Path,
    id: &str,
    model: &str,
    duration: u64,
    usage: Option<&Usage>,
    error: Option<&str>,
) -> Result<()> {
    let conn = connection(path)?;
    let count = conn.execute("UPDATE agent_calls SET finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),model=?2,status=?3,input_tokens=?4,output_tokens=?5,cached_tokens=?6,tokens_estimated=?7,cost_usd=?8,duration_ms=?9,error_kind=?10,metered=COALESCE(?11,metered) WHERE id=?1 AND status='running'", params![id,model,if error.is_some(){"failed"}else{"succeeded"},usage.map(|u|u.input_tokens),usage.map(|u|u.output_tokens),usage.map(|u|u.cached_tokens),usage.is_some_and(|u|u.tokens_estimated),usage.and_then(|u|u.cost_usd),duration,error,usage.map(|u|u.metered)]).map_err(storage_error)?;
    if count != 1 {
        return Err(storage_error("call record was changed or missing"));
    }
    Ok(())
}
#[derive(Debug, Serialize)]
pub struct CallSummary {
    pub id: String,
    pub started_at: String,
    pub runner: String,
    pub model: String,
    pub purpose: String,
    pub status: String,
    pub duration_ms: Option<u64>,
    pub cost_usd: Option<f64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub tokens_estimated: bool,
    pub error_kind: Option<String>,
    pub fallback_of: Option<String>,
}
pub fn recent(path: &Path) -> Result<Vec<CallSummary>> {
    let conn = connection(path)?;
    let mut statement = conn.prepare("SELECT id,started_at,runner,model,purpose,status,duration_ms,cost_usd,input_tokens,output_tokens,tokens_estimated,error_kind,fallback_of FROM agent_calls ORDER BY started_at DESC,rowid DESC LIMIT 40").map_err(storage_error)?;
    let rows = statement
        .query_map([], |r| {
            Ok(CallSummary {
                id: r.get(0)?,
                started_at: r.get(1)?,
                runner: r.get(2)?,
                model: r.get(3)?,
                purpose: r.get(4)?,
                status: r.get(5)?,
                duration_ms: r.get(6)?,
                cost_usd: r.get(7)?,
                input_tokens: r.get(8)?,
                output_tokens: r.get(9)?,
                tokens_estimated: r.get(10)?,
                error_kind: r.get(11)?,
                fallback_of: r.get(12)?,
            })
        })
        .map_err(storage_error)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage_error)
}
