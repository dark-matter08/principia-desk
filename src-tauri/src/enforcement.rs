//! One coordinator owns foreground enforcement for class sessions. Advisory
//! sessions never lock. A focused or strict session may take the lock only
//! when no other session holds it, only once its content is ready, and the
//! lock is released or transferred explicitly: on completion, skip, escape or
//! restart recovery. The retired daily routine never acquires it.
use crate::{
    domain::{
        enrollment::FocusPolicy,
        sessions::{self, FocusGrant, Session, SessionId},
    },
    kiosk::KioskLevel,
    state::AppState,
};
use serde::Serialize;
use std::sync::Mutex;
use tauri::AppHandle;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FocusOwner {
    pub session_id: String,
    pub course_id: String,
    pub policy: FocusPolicy,
    pub occurrence_id: Option<String>,
}

#[derive(Default)]
pub struct Coordinator {
    owner: Mutex<Option<FocusOwner>>,
}

pub fn level_for(policy: &FocusPolicy) -> Option<KioskLevel> {
    match policy {
        FocusPolicy::Advisory => None,
        FocusPolicy::Focused => Some(KioskLevel::Firm),
        FocusPolicy::Strict => Some(KioskLevel::Hard),
    }
}

impl Coordinator {
    pub fn holder(&self) -> Option<FocusOwner> {
        self.owner.lock().unwrap().clone()
    }

    /// Whether `session` may take the foreground now. Advisory sessions are
    /// always admitted without a grant; a focused/strict session receives a
    /// grant only when the lock is free or already its own.
    pub fn admit(&self, session: &Session) -> Result<Option<FocusGrant>, String> {
        if session.context.focus_policy == FocusPolicy::Advisory {
            if let Some(holder) = self.holder() {
                if holder.session_id != session.id.0 {
                    return Err(format!(
                        "A focused session for {} holds the desk. Finish it or use the escape hatch first.",
                        holder.course_id
                    ));
                }
            }
            return Ok(None);
        }
        if session.lesson_version_id.is_none() {
            return Err("This lesson is not prepared yet, so it cannot take focus.".into());
        }
        match self.holder() {
            Some(holder) if holder.session_id != session.id.0 => Err(format!(
                "A focused session for {} holds the desk. Finish it or use the escape hatch first.",
                holder.course_id
            )),
            _ => Ok(Some(FocusGrant(()))),
        }
    }

    /// Record `session` as the holder (the app-level activation does this
    /// after enforcement engages; tests use it to model that step).
    pub fn own(&self, session: &Session) -> Result<(), String> {
        self.take(owner_of(session))
    }

    /// Drop ownership if `session_id` holds it. Idempotent.
    pub fn release_owner(&self, session_id: &str) -> bool {
        self.drop_owner(session_id)
    }

    fn take(&self, owner: FocusOwner) -> Result<(), String> {
        let mut current = self.owner.lock().unwrap();
        match &*current {
            Some(existing) if existing.session_id != owner.session_id => Err(format!(
                "A focused session for {} already holds the desk.",
                existing.course_id
            )),
            _ => {
                *current = Some(owner);
                Ok(())
            }
        }
    }

    fn drop_owner(&self, session_id: &str) -> bool {
        let mut current = self.owner.lock().unwrap();
        if current
            .as_ref()
            .is_some_and(|owner| owner.session_id == session_id)
        {
            *current = None;
            true
        } else {
            false
        }
    }
}

fn owner_of(session: &Session) -> FocusOwner {
    FocusOwner {
        session_id: session.id.0.clone(),
        course_id: session.context.course.course_id.clone(),
        policy: session.context.focus_policy.clone(),
        occurrence_id: session.context.selection["occurrence_id"]
            .as_str()
            .map(String::from),
    }
}

/// Activate a session through the coordinator: advisory sessions activate
/// plainly; focused/strict ones take the lock and engage enforcement.
pub fn activate(app: &AppHandle, state: &AppState, id: &SessionId) -> Result<Session, String> {
    let conn = state.db.0.lock().unwrap();
    let session = sessions::get(&conn, id).map_err(|e| e.to_string())?;
    let grant = state.focus.admit(&session)?;
    let activated = match grant {
        None => crate::subjects::engineering::activate(&conn, id)?,
        Some(grant) => {
            if session.status == sessions::Status::Active {
                session.clone()
            } else {
                sessions::activate_focused(&conn, id, session.revision, chrono::Utc::now(), grant)
                    .map_err(|e| e.to_string())?
            }
        }
    };
    drop(conn);
    if let Some(level) = level_for(&activated.context.focus_policy) {
        state.focus.take(owner_of(&activated))?;
        crate::kiosk::engage_at(app, state, level);
    }
    Ok(activated)
}

/// Release enforcement if `session_id` holds it. Idempotent.
pub fn release(app: &AppHandle, state: &AppState, session_id: &str) {
    if state.focus.release_owner(session_id) {
        crate::kiosk::release(app, state);
    }
}

/// Emergency release: pause the focused session so its work survives, then
/// drop the lock. Returns the paused session ID when one was held.
pub fn escape(app: &AppHandle, state: &AppState) -> Option<String> {
    let holder = state.focus.holder()?;
    {
        let conn = state.db.0.lock().unwrap();
        let _ = crate::subjects::engineering::pause(&conn, &SessionId(holder.session_id.clone()));
    }
    release(app, state, &holder.session_id);
    Some(holder.session_id)
}

/// After a restart, an active focused session recovers its enforcement once
/// the webview is ready; nothing is locked behind an unready frontend.
pub fn restore(app: &AppHandle, state: &AppState) {
    let candidate = {
        let conn = state.db.0.lock().unwrap();
        let id: Option<String> = conn
            .query_row(
                "SELECT id FROM study_sessions WHERE status='active' AND owner_kind='class'",
                [],
                |r| r.get(0),
            )
            .ok();
        id.and_then(|id| sessions::get(&conn, &SessionId(id)).ok())
    };
    let Some(session) = candidate else {
        return;
    };
    if level_for(&session.context.focus_policy).is_none() || session.lesson_version_id.is_none() {
        return;
    }
    if let Err(error) = activate(app, state, &session.id) {
        log::warn!(
            "focused session {} could not recover enforcement: {error}",
            session.id.0
        );
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FocusView {
    pub session_id: String,
    pub course_id: String,
    pub policy: FocusPolicy,
    pub locked: bool,
}

pub fn view(state: &AppState) -> Option<FocusView> {
    let holder = state.focus.holder()?;
    Some(FocusView {
        session_id: holder.session_id,
        course_id: holder.course_id,
        policy: holder.policy,
        locked: state.locked.load(std::sync::atomic::Ordering::SeqCst),
    })
}
