//! Shared durable study lifecycle. Subject adapters select and validate content,
//! interpret practice, and project evidence inside the completion transaction.
//! No provider call or OS focus operation may run while that transaction is open.
use super::{
    classes::PathReference,
    enrollment::{
        self, CourseReference, EnrollmentConfiguration, FocusPolicy, LearningGoal, StudyPace,
        TutorPreference,
    },
};
use crate::db::{DbError, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LessonVersionId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Planned,
    Preparing,
    Ready,
    Active,
    Paused,
    Completed,
    Skipped,
}
impl Status {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Skipped)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Recall,
    Learn,
    Practice,
    Check,
    Feedback,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    Lesson,
    Retrieval,
}

/// A native adapter supplies the selection. The daily routine retains its own
/// owner until the learner assigns it to a class; a date is only provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanOwner {
    Class {
        path: PathReference,
    },
    DailyRoutine {
        service_date: String,
        course: CourseReference,
        tutor: TutorPreference,
        focus_policy: FocusPolicy,
        goal: LearningGoal,
        pace: StudyPace,
    },
}
impl PlanOwner {
    fn parts(&self) -> (&str, &str) {
        match self {
            Self::Class { path } => ("class", &path.class_id),
            Self::DailyRoutine { .. } => ("daily_routine", "daily-routine"),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanSession {
    /// Persisted by the caller before awaiting preparation; retries reuse it.
    pub request_key: String,
    pub owner: PlanOwner,
    pub kind: SessionKind,
    pub stages: Vec<Stage>,
    /// Subject-authored lesson/unit selection, reason and prerequisite advice.
    pub selection: Value,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Context {
    pub owner: PlanOwner,
    pub course: CourseReference,
    pub tutor: TutorPreference,
    pub focus_policy: FocusPolicy,
    pub goal: LearningGoal,
    pub pace: StudyPace,
    pub kind: SessionKind,
    pub stages: Vec<Stage>,
    pub selection: Value,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadingPosition {
    pub anchor: Option<String>,
    pub offset: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointBody {
    pub stage: Stage,
    pub reading: ReadingPosition,
    /// Editor drafts and activity state. This is work, never assessment credit.
    pub work: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub revision: u32,
    pub body: CheckpointBody,
    pub updated_at: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub context: Context,
    pub status: Status,
    pub revision: u32,
    pub lesson_version_id: Option<LessonVersionId>,
    pub checkpoint: Checkpoint,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedLesson {
    pub title: String,
    /// Immutable adapter payload, including displayed assessments and rubrics.
    pub body: Value,
    /// Native generator provenance: actual runner/model, prompt version, sources.
    /// Legacy imports must identify unknown provenance explicitly.
    pub provenance: Value,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LessonVersion {
    pub id: LessonVersionId,
    pub session_id: SessionId,
    pub content: PreparedLesson,
    pub fingerprint: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparationLease {
    pub session_id: SessionId,
    pub token: String,
    pub request_fingerprint: String,
    pub expires_at_millis: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preparation {
    pub status: String,
    pub attempts: u32,
    pub error: Option<String>,
    pub lease_expires_at_millis: Option<i64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Completed,
    Skipped,
}
impl Disposition {
    fn key(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Skipped => "skipped",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionResult {
    pub session_id: SessionId,
    pub disposition: Disposition,
    pub checkpoint_revision: u32,
    pub checkpoint: CheckpointBody,
    pub outcome: Value,
    pub finished_at: String,
}

fn invalid(message: &str) -> DbError {
    DbError::Invalid(message.into())
}
fn stamp(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(SecondsFormat::Millis, true)
}
fn identifier(prefix: &str) -> String {
    format!("{prefix}-{:032x}", rand::random::<u128>())
}
fn fingerprint(value: &impl Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
fn transaction(conn: &Connection) -> Result<Transaction<'_>> {
    Ok(Transaction::new_unchecked(
        conn,
        TransactionBehavior::Immediate,
    )?)
}
fn require_revision(session: &Session, expected: u32) -> Result<()> {
    if session.revision != expected {
        return Err(invalid(
            "The session changed. Reload its current state before continuing.",
        ));
    }
    Ok(())
}

pub fn get(conn: &Connection, id: &SessionId) -> Result<Session> {
    let row = conn.query_row("SELECT s.context_json,s.status,s.revision,s.lesson_version_id,s.created_at,s.updated_at,s.finished_at,c.revision,c.body_json,c.updated_at FROM study_sessions s JOIN study_checkpoints c ON c.session_id=s.id WHERE s.id=?1", [&id.0], |r| {
        Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,u32>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,Option<String>>(6)?,r.get::<_,u32>(7)?,r.get::<_,String>(8)?,r.get::<_,String>(9)?))
    })?;
    Ok(Session {
        id: id.clone(),
        context: serde_json::from_str(&row.0)?,
        status: serde_json::from_value(Value::String(row.1))?,
        revision: row.2,
        lesson_version_id: row.3.map(LessonVersionId),
        created_at: row.4,
        updated_at: row.5,
        finished_at: row.6,
        checkpoint: Checkpoint {
            revision: row.7,
            body: serde_json::from_str(&row.8)?,
            updated_at: row.9,
        },
    })
}
pub fn resumable(conn: &Connection, owner: &PlanOwner) -> Result<Option<Session>> {
    let (kind, key) = owner.parts();
    let id: Option<String> = conn.query_row("SELECT id FROM study_sessions WHERE owner_kind=?1 AND owner_key=?2 AND status NOT IN ('completed','skipped')", params![kind,key], |r| r.get(0)).optional()?;
    id.map(|id| get(conn, &SessionId(id))).transpose()
}
pub fn history(conn: &Connection, class_id: &str) -> Result<Vec<Session>> {
    let ids = conn
        .prepare("SELECT id FROM study_sessions WHERE class_id=?1 ORDER BY rowid DESC")?
        .query_map([class_id], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ids.into_iter()
        .map(|id| get(conn, &SessionId(id)))
        .collect()
}

fn resolve_context(conn: &Connection, input: &PlanSession, now: DateTime<Utc>) -> Result<Context> {
    let (course, tutor, focus_policy, goal, pace) = match &input.owner {
        PlanOwner::Class { path } => {
            let row: (String,String,String,String,String,String) = conn.query_row("SELECT c.course_id,c.course_snapshot_fingerprint,c.active_path_revision_id,c.configuration_json,c.status,s.version FROM classes c JOIN course_snapshots s ON s.fingerprint=c.course_snapshot_fingerprint WHERE c.id=?1", [&path.class_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
            if row.4 != "active" {
                return Err(invalid("Reactivate the class before planning new work."));
            }
            if row.1 != path.course_snapshot_fingerprint || row.2 != path.path_revision_id {
                return Err(invalid(
                    "The personal path changed. Select the next lesson from the current path.",
                ));
            }
            let config: EnrollmentConfiguration = serde_json::from_str(&row.3)?;
            (
                CourseReference {
                    course_id: row.0,
                    fingerprint: row.1,
                    version: row.5,
                },
                config.tutor,
                config.focus_policy,
                config.goal,
                config.pace,
            )
        }
        PlanOwner::DailyRoutine {
            service_date,
            course,
            tutor,
            focus_policy,
            goal,
            pace,
        } => {
            if chrono::NaiveDate::parse_from_str(service_date, "%Y-%m-%d").is_err() {
                return Err(invalid("Invalid daily routine date."));
            }
            (
                course.clone(),
                tutor.clone(),
                focus_policy.clone(),
                goal.clone(),
                pace.clone(),
            )
        }
    };
    let (current, snapshot) = enrollment::course_snapshot(&course.course_id)?;
    if current != course {
        return Err(invalid(
            "The curriculum changed. Review the current course before starting new work.",
        ));
    }
    if crate::agents::RunnerId::parse(&tutor.provider).is_none()
        || !crate::agents::valid_model(&tutor.model)
    {
        return Err(invalid(
            "Choose a valid runner and model before preparing a lesson.",
        ));
    }
    // A session may run as long as a study day does; the lesson itself is
    // capped and a long day becomes a block of lessons.
    if !(10..=480).contains(&pace.session_minutes)
        || pace
            .weekly_minutes
            .is_some_and(|minutes| !(10..=10080).contains(&minutes))
    {
        return Err(invalid("Choose a valid session duration and weekly pace."));
    }
    conn.execute("INSERT OR IGNORE INTO course_snapshots(fingerprint,course_id,version,body_json,created_at) VALUES(?1,?2,?3,?4,?5)", params![course.fingerprint,course.course_id,course.version,serde_json::to_string(&snapshot)?,stamp(now)])?;
    Ok(Context {
        owner: input.owner.clone(),
        course,
        tutor,
        focus_policy,
        goal,
        pace,
        kind: input.kind,
        stages: input.stages.clone(),
        selection: input.selection.clone(),
    })
}

/// Planning is local and atomic. Provider work starts only after this returns.
/// An identical request always returns its original session, even after finish
/// or a later path/default change. A different request never overwrites work.
pub fn plan(conn: &Connection, input: &PlanSession, now: DateTime<Utc>) -> Result<Session> {
    if input.request_key.trim().is_empty()
        || input.request_key.len() > 200
        || !input.selection.is_object()
        || serde_json::to_vec(&input.selection)?.len() > 262_144
    {
        return Err(invalid("Invalid session preparation request."));
    }
    if input.stages.is_empty()
        || input.stages.last() != Some(&Stage::Feedback)
        || input.stages.windows(2).any(|s| s[0] >= s[1])
        || (input.kind == SessionKind::Retrieval && input.stages.contains(&Stage::Learn))
    {
        return Err(invalid(
            "Choose ordered, distinct learning stages ending in feedback.",
        ));
    }
    let tx = transaction(conn)?;
    let existing: Option<(String, String)> = tx
        .query_row(
            "SELECT id,request_json FROM study_sessions WHERE request_key=?1",
            [&input.request_key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((id, raw)) = existing {
        if serde_json::from_str::<PlanSession>(&raw)? != *input {
            return Err(invalid(
                "This request key already belongs to a different lesson selection.",
            ));
        }
        return get(&tx, &SessionId(id));
    }
    if resumable(&tx, &input.owner)?.is_some() {
        return Err(invalid(
            "This owner already has saved work. Resume or explicitly skip that session first.",
        ));
    }
    let context = resolve_context(&tx, input, now)?;
    let id = SessionId(identifier("study"));
    let (kind, key) = input.owner.parts();
    let (class_id, path_id) = match &input.owner {
        PlanOwner::Class { path } => (Some(&path.class_id), Some(&path.path_revision_id)),
        _ => (None, None),
    };
    let now = stamp(now);
    tx.execute("INSERT INTO study_sessions(id,request_key,request_json,owner_kind,owner_key,class_id,course_snapshot_fingerprint,path_revision_id,context_json,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'planned',?10,?10)", params![id.0,input.request_key,serde_json::to_string(input)?,kind,key,class_id,context.course.fingerprint,path_id,serde_json::to_string(&context)?,now])?;
    let body = CheckpointBody {
        stage: input.stages[0],
        reading: ReadingPosition::default(),
        work: BTreeMap::new(),
    };
    tx.execute(
        "INSERT INTO study_checkpoints(session_id,body_json,updated_at) VALUES(?1,?2,?3)",
        params![id.0, serde_json::to_string(&body)?, now],
    )?;
    tx.execute("INSERT INTO study_preparation_jobs(session_id,request_fingerprint,status,updated_at) VALUES(?1,?2,'queued',?3)",params![id.0,fingerprint(&context)?,now])?;
    let result = get(&tx, &id)?;
    tx.commit()?;
    Ok(result)
}

pub fn preparation(conn: &Connection, id: &SessionId) -> Result<Preparation> {
    Ok(conn.query_row("SELECT status,attempts,error,lease_expires_at FROM study_preparation_jobs WHERE session_id=?1",[&id.0],|r|Ok(Preparation{status:r.get(0)?,attempts:r.get(1)?,error:r.get(2)?,lease_expires_at_millis:r.get(3)?}))?)
}
fn lease_expiry(now: DateTime<Utc>, seconds: u32) -> Result<i64> {
    if !(10..=3600).contains(&seconds) {
        return Err(invalid(
            "Preparation leases must last between 10 and 3600 seconds.",
        ));
    }
    now.timestamp_millis()
        .checked_add(i64::from(seconds) * 1000)
        .ok_or_else(|| invalid("Preparation lease time overflow."))
}
/// Concurrent claimants receive None while a live lease exists. Expired leases
/// can be reclaimed after restart; their old workers can no longer publish.
pub fn claim_preparation(
    conn: &Connection,
    id: &SessionId,
    now: DateTime<Utc>,
    lease_seconds: u32,
) -> Result<Option<PreparationLease>> {
    let expires = lease_expiry(now, lease_seconds)?;
    let tx = transaction(conn)?;
    let session = get(&tx, id)?;
    if !matches!(session.status, Status::Planned | Status::Preparing) {
        return Ok(None);
    }
    let job = preparation(&tx, id)?;
    if job.status != "queued"
        && !(job.status == "running"
            && job
                .lease_expires_at_millis
                .is_some_and(|t| t <= now.timestamp_millis()))
    {
        return Ok(None);
    }
    let token = identifier("lease");
    let request_fingerprint = tx.query_row(
        "SELECT request_fingerprint FROM study_preparation_jobs WHERE session_id=?1",
        [&id.0],
        |r| r.get(0),
    )?;
    tx.execute("UPDATE study_preparation_jobs SET status='running',attempts=attempts+1,lease_token=?2,lease_expires_at=?3,finished_token=NULL,error=NULL,updated_at=?4 WHERE session_id=?1",params![id.0,token,expires,stamp(now)])?;
    tx.execute("UPDATE study_sessions SET status='preparing',revision=revision+1,updated_at=?2 WHERE id=?1",params![id.0,stamp(now)])?;
    tx.commit()?;
    Ok(Some(PreparationLease {
        session_id: id.clone(),
        token,
        request_fingerprint,
        expires_at_millis: expires,
    }))
}
fn require_lease(conn: &Connection, lease: &PreparationLease, now: DateTime<Utc>) -> Result<()> {
    let valid: bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM study_preparation_jobs j JOIN study_sessions s ON s.id=j.session_id WHERE j.session_id=?1 AND j.status='running' AND j.lease_token=?2 AND j.request_fingerprint=?3 AND j.lease_expires_at>?4 AND s.status='preparing')",params![lease.session_id.0,lease.token,lease.request_fingerprint,now.timestamp_millis()],|r|r.get(0))?;
    if !valid {
        return Err(invalid(
            "This preparation lease expired or was replaced; its result cannot be published.",
        ));
    }
    Ok(())
}
pub fn renew_preparation(
    conn: &Connection,
    lease: &PreparationLease,
    now: DateTime<Utc>,
    seconds: u32,
) -> Result<PreparationLease> {
    let expires = lease_expiry(now, seconds)?;
    let tx = transaction(conn)?;
    require_lease(&tx, lease, now)?;
    tx.execute("UPDATE study_preparation_jobs SET lease_expires_at=MAX(lease_expires_at,?2),updated_at=?3 WHERE session_id=?1",params![lease.session_id.0,expires,stamp(now)])?;
    let expires_at_millis = preparation(&tx, &lease.session_id)?
        .lease_expires_at_millis
        .unwrap();
    tx.commit()?;
    Ok(PreparationLease {
        expires_at_millis,
        ..lease.clone()
    })
}
pub fn fail_preparation(
    conn: &Connection,
    lease: &PreparationLease,
    error: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    if error.trim().is_empty() || error.len() > 8000 {
        return Err(invalid("Provide a bounded, nonempty preparation error."));
    }
    let tx = transaction(conn)?;
    require_lease(&tx, lease, now)?;
    tx.execute("UPDATE study_preparation_jobs SET status='failed',lease_token=NULL,lease_expires_at=NULL,finished_token=?2,error=?3,updated_at=?4 WHERE session_id=?1",params![lease.session_id.0,lease.token,error,stamp(now)])?;
    tx.commit()?;
    Ok(())
}
/// Fail every preparation that was still running when the app last stopped.
///
/// Preparation workers live inside this process, so a lease still standing at
/// startup belongs to a worker that no longer exists: the app crashed or was
/// killed mid-generation. Left alone, that lease blocks every Start for up to
/// an hour while the study alarm rings, which is a trap. Marking the job
/// failed lets the next Start retry it, and the old worker could not publish
/// anyway because its token is gone with it.
pub fn release_orphaned_preparations(conn: &Connection, now: DateTime<Utc>) -> Result<usize> {
    let released = conn.execute(
        "UPDATE study_preparation_jobs SET status='failed',lease_token=NULL,lease_expires_at=NULL,finished_token=NULL,error='Preparation did not survive an app restart. Start again to retry it.',updated_at=?1 WHERE status='running'",
        [stamp(now)],
    )?;
    Ok(released)
}
pub fn retry_preparation(conn: &Connection, id: &SessionId, now: DateTime<Utc>) -> Result<()> {
    let tx = transaction(conn)?;
    if get(&tx, id)?.status != Status::Preparing {
        return Err(invalid("This session has no failed preparation to retry."));
    }
    let job = preparation(&tx, id)?;
    if job.status == "queued" {
        return Ok(());
    }
    if job.status != "failed" {
        return Err(invalid("Preparation is already running or finished."));
    }
    tx.execute("UPDATE study_preparation_jobs SET status='queued',error=NULL,finished_token=NULL,updated_at=?2 WHERE session_id=?1",params![id.0,stamp(now)])?;
    tx.commit()?;
    Ok(())
}
pub fn lesson(conn: &Connection, id: &SessionId) -> Result<Option<LessonVersion>> {
    let row: Option<(String,String,String)> = conn.query_row("SELECT v.id,v.content_json,v.fingerprint FROM lesson_versions v JOIN study_sessions s ON s.lesson_version_id=v.id AND s.id=v.session_id WHERE s.id=?1",[&id.0],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    row.map(|(key, raw, fingerprint)| {
        Ok(LessonVersion {
            id: LessonVersionId(key),
            session_id: id.clone(),
            content: serde_json::from_str(&raw)?,
            fingerprint,
        })
    })
    .transpose()
}
pub fn publish_preparation(
    conn: &Connection,
    lease: &PreparationLease,
    content: &PreparedLesson,
    now: DateTime<Utc>,
) -> Result<LessonVersion> {
    if content.title.trim().is_empty()
        || content.title.len() > 500
        || !content.body.is_object()
        || !content.provenance.is_object()
        || serde_json::to_vec(content)?.len() > 8_388_608
    {
        return Err(invalid("Invalid prepared lesson payload."));
    }
    let hash = fingerprint(content)?;
    let tx = transaction(conn)?;
    let published: bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM study_preparation_jobs WHERE session_id=?1 AND status='ready' AND finished_token=?2 AND request_fingerprint=?3)",params![lease.session_id.0,lease.token,lease.request_fingerprint],|r|r.get(0))?;
    if published {
        let previous = lesson(&tx, &lease.session_id)?
            .ok_or_else(|| invalid("Prepared lesson is missing."))?;
        if previous.fingerprint != hash {
            return Err(invalid(
                "This lease already published a different immutable lesson.",
            ));
        }
        return Ok(previous);
    }
    require_lease(&tx, lease, now)?;
    let id = LessonVersionId(identifier("lesson"));
    tx.execute("INSERT INTO lesson_versions(id,session_id,content_json,fingerprint,created_at) VALUES(?1,?2,?3,?4,?5)",params![id.0,lease.session_id.0,serde_json::to_string(content)?,hash,stamp(now)])?;
    tx.execute("UPDATE study_sessions SET status='ready',lesson_version_id=?2,revision=revision+1,updated_at=?3 WHERE id=?1",params![lease.session_id.0,id.0,stamp(now)])?;
    tx.execute("UPDATE study_preparation_jobs SET status='ready',lease_token=NULL,lease_expires_at=NULL,finished_token=?2,error=NULL,updated_at=?3 WHERE session_id=?1",params![lease.session_id.0,lease.token,stamp(now)])?;
    let result =
        lesson(&tx, &lease.session_id)?.ok_or_else(|| invalid("Prepared lesson is missing."))?;
    tx.commit()?;
    Ok(result)
}

/// Issued only by the native focus coordinator once it owns enforcement for
/// exactly one session; focused and strict sessions cannot activate without it.
#[derive(Debug)]
pub struct FocusGrant(pub(crate) ());

/// Logical foreground ownership for advisory sessions. Focused and strict
/// sessions must be activated through the coordinator's grant.
pub fn activate(
    conn: &Connection,
    id: &SessionId,
    expected_revision: u32,
    now: DateTime<Utc>,
) -> Result<Session> {
    activate_inner(conn, id, expected_revision, now, None)
}

/// Activation of a focused or strict session under the coordinator's grant.
pub fn activate_focused(
    conn: &Connection,
    id: &SessionId,
    expected_revision: u32,
    now: DateTime<Utc>,
    grant: FocusGrant,
) -> Result<Session> {
    activate_inner(conn, id, expected_revision, now, Some(grant))
}

fn activate_inner(
    conn: &Connection,
    id: &SessionId,
    expected_revision: u32,
    now: DateTime<Utc>,
    grant: Option<FocusGrant>,
) -> Result<Session> {
    let tx = transaction(conn)?;
    let session = get(&tx, id)?;
    if session.status == Status::Active
        && (session.revision == expected_revision
            || session.revision.checked_sub(1) == Some(expected_revision))
    {
        return Ok(session);
    }
    require_revision(&session, expected_revision)?;
    if !matches!(session.status, Status::Ready | Status::Paused) {
        return Err(invalid("Only a ready or paused session can be activated."));
    }
    if session.context.focus_policy != FocusPolicy::Advisory && grant.is_none() {
        return Err(invalid(
            "Focused study needs the native focus coordinator before activation.",
        ));
    }
    tx.execute("UPDATE study_sessions SET status='paused',revision=revision+1,updated_at=?1 WHERE status='active'",[stamp(now)])?;
    tx.execute(
        "UPDATE study_sessions SET status='active',revision=revision+1,updated_at=?2 WHERE id=?1",
        params![id.0, stamp(now)],
    )?;
    let result = get(&tx, id)?;
    tx.commit()?;
    Ok(result)
}
pub fn pause(
    conn: &Connection,
    id: &SessionId,
    expected_revision: u32,
    now: DateTime<Utc>,
) -> Result<Session> {
    let tx = transaction(conn)?;
    let session = get(&tx, id)?;
    if session.status == Status::Paused
        && (session.revision == expected_revision
            || session.revision.checked_sub(1) == Some(expected_revision))
    {
        return Ok(session);
    }
    require_revision(&session, expected_revision)?;
    if session.status != Status::Active {
        return Err(invalid("Only the active session can be paused."));
    }
    tx.execute(
        "UPDATE study_sessions SET status='paused',revision=revision+1,updated_at=?2 WHERE id=?1",
        params![id.0, stamp(now)],
    )?;
    let result = get(&tx, id)?;
    tx.commit()?;
    Ok(result)
}

/// Checkpoint revisions are separate from lifecycle revisions: an autosave does
/// not invalidate a pause, and pausing cannot redirect a captured editor owner.
pub fn save_checkpoint(
    conn: &Connection,
    id: &SessionId,
    expected_revision: u32,
    body: &CheckpointBody,
    now: DateTime<Utc>,
) -> Result<Checkpoint> {
    let tx = transaction(conn)?;
    let session = get(&tx, id)?;
    if !matches!(
        session.status,
        Status::Ready | Status::Active | Status::Paused
    ) {
        return Err(invalid("This session is not editable."));
    }
    if !session.context.stages.contains(&body.stage)
        || body.reading.anchor.as_ref().is_some_and(|s| s.len() > 300)
        || body.reading.offset > 1_000_000
        || body.work.len() > 64
        || body.work.keys().any(|k| k.is_empty() || k.len() > 200)
        || serde_json::to_vec(body)?.len() > 2_097_152
    {
        return Err(invalid("Invalid session checkpoint."));
    }
    if session.checkpoint.revision != expected_revision {
        if session.checkpoint.revision.checked_sub(1) == Some(expected_revision)
            && session.checkpoint.body == *body
        {
            return Ok(session.checkpoint);
        }
        return Err(invalid(
            "Saved work changed. Reload it before retrying this edit.",
        ));
    }
    if session.checkpoint.body == *body {
        return Ok(session.checkpoint);
    }
    tx.execute("UPDATE study_checkpoints SET revision=revision+1,body_json=?2,updated_at=?3 WHERE session_id=?1",params![id.0,serde_json::to_string(body)?,stamp(now)])?;
    let result = get(&tx, id)?.checkpoint;
    tx.commit()?;
    Ok(result)
}
pub fn result(conn: &Connection, id: &SessionId) -> Result<Option<SessionResult>> {
    let row: Option<(String,u32,String,String,String)> = conn.query_row("SELECT disposition,checkpoint_revision,checkpoint_json,outcome_json,finished_at FROM study_results WHERE session_id=?1",[&id.0],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
    row.map(
        |(disposition, checkpoint_revision, checkpoint, outcome, finished_at)| {
            Ok(SessionResult {
                session_id: id.clone(),
                disposition: serde_json::from_value(Value::String(disposition))?,
                checkpoint_revision,
                checkpoint: serde_json::from_str(&checkpoint)?,
                outcome: serde_json::from_str(&outcome)?,
                finished_at,
            })
        },
    )
    .transpose()
}

/// The adapter validates required evidence and writes its progress/occurrence
/// projection in this transaction. Any failure rolls back every write. A retry
/// returns the original result without running the projection a second time.
pub fn finish<F>(
    conn: &Connection,
    id: &SessionId,
    expected_revision: u32,
    checkpoint_revision: u32,
    disposition: Disposition,
    now: DateTime<Utc>,
    project: F,
) -> Result<SessionResult>
where
    F: FnOnce(&Transaction<'_>, &Session) -> Result<Value>,
{
    let tx = transaction(conn)?;
    if let Some(previous) = result(&tx, id)? {
        if previous.disposition != disposition
            || previous.checkpoint_revision != checkpoint_revision
        {
            return Err(invalid(
                "This session already has a different immutable result.",
            ));
        }
        return Ok(previous);
    }
    let session = get(&tx, id)?;
    require_revision(&session, expected_revision)?;
    if session.status.terminal() || session.checkpoint.revision != checkpoint_revision {
        return Err(invalid("The saved session work changed before submission."));
    }
    if disposition == Disposition::Completed
        && (session.status != Status::Active
            || session.context.stages.last() != Some(&session.checkpoint.body.stage))
    {
        return Err(invalid(
            "Reach feedback in the active session before completing it.",
        ));
    }
    let outcome = project(&tx, &session)?;
    if !outcome.is_object() || serde_json::to_vec(&outcome)?.len() > 2_097_152 {
        return Err(invalid("Invalid session result."));
    }
    let now = stamp(now);
    if disposition == Disposition::Completed {
        let pending: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM assessment_attempts WHERE owner_kind='study_session' AND owner_key=?1 AND status='active')", [&id.0], |r| r.get(0))?;
        if pending {
            return Err(invalid(
                "Finish the current assessment before completing the session.",
            ));
        }
    } else {
        tx.execute("UPDATE assessment_attempts SET status='abandoned',finished_at=?2 WHERE owner_kind='study_session' AND owner_key=?1 AND status='active'", params![id.0,now])?;
    }
    tx.execute("INSERT INTO study_results(session_id,disposition,checkpoint_revision,checkpoint_json,outcome_json,finished_at) VALUES(?1,?2,?3,?4,?5,?6)",params![id.0,disposition.key(),checkpoint_revision,serde_json::to_string(&session.checkpoint.body)?,serde_json::to_string(&outcome)?,now])?;
    tx.execute("UPDATE study_sessions SET status=?2,revision=revision+1,updated_at=?3,finished_at=?3 WHERE id=?1",params![id.0,disposition.key(),now])?;
    tx.execute("UPDATE study_preparation_jobs SET status='cancelled',lease_token=NULL,lease_expires_at=NULL,updated_at=?2 WHERE session_id=?1 AND status<>'ready'",params![id.0,now])?;
    let result = result(&tx, id)?.ok_or_else(|| invalid("Session result is missing."))?;
    tx.commit()?;
    Ok(result)
}
