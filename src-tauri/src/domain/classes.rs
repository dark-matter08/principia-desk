//! Accepted personal paths. Preference changes never manufacture learning credit.
//! The classroom adapters remain responsible for their existing session records.
use super::{
    enrollment::{self, DraftStatus, EnrollmentConfiguration, EnrollmentDraftId, LearningGoal},
    placement::{self, Recommendation},
};
use crate::{
    catalog,
    db::{self, DbError, Result},
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathReference {
    pub class_id: String,
    pub path_revision_id: String,
    pub course_snapshot_fingerprint: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedPath {
    pub reference: PathReference,
    pub revision: u32,
    pub configuration: EnrollmentConfiguration,
    pub recommendation: Recommendation,
    pub accepted_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathSummary {
    pub class_id: String,
    pub path_revision_id: String,
    pub revision: u32,
    pub entry_point: String,
    pub entry_label: String,
    pub route: String,
    pub earlier_topics: usize,
    pub refreshers: usize,
}
impl AcceptedPath {
    pub fn summary(&self) -> PathSummary {
        PathSummary {
            class_id: self.reference.class_id.clone(),
            path_revision_id: self.reference.path_revision_id.clone(),
            revision: self.revision,
            entry_point: self.recommendation.entry_point.clone(),
            entry_label: self.recommendation.entry_label.clone(),
            route: self.recommendation.route.clone(),
            earlier_topics: self.recommendation.earlier_topics.len(),
            refreshers: self.recommendation.refreshers.len(),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptPath {
    pub draft_id: EnrollmentDraftId,
    pub expected_revision: u32,
    pub recommendation_id: String,
}
fn read_path(conn: &Connection, path_id: &str) -> Result<AcceptedPath> {
    let row = conn.query_row("SELECT id, class_id, revision, course_snapshot_fingerprint, entry_profile_json, plan_json, accepted_at FROM path_revisions WHERE id = ?1", [path_id], |r| {
        Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,u32>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?))
    })?;
    Ok(AcceptedPath {
        reference: PathReference {
            path_revision_id: row.0,
            class_id: row.1,
            course_snapshot_fingerprint: row.3,
        },
        revision: row.2,
        configuration: serde_json::from_str(&row.4)?,
        recommendation: serde_json::from_str(&row.5)?,
        accepted_at: row.6,
    })
}
/// Change how future sessions of a class are enforced. Existing sessions keep
/// the policy snapshotted when they were planned. A class without a path yet
/// begins at the foundations so the policy has a class to belong to.
pub fn set_focus_policy(
    conn: &Connection,
    course_id: &str,
    policy: enrollment::FocusPolicy,
    today: &str,
) -> Result<EnrollmentConfiguration> {
    if current_path(conn, course_id)?.is_none() {
        ensure_default_path(conn, course_id, today)?;
    }
    let mut config = current_configuration(conn, course_id)?
        .ok_or_else(|| DbError::Invalid("This class has no configuration yet.".into()))?;
    config.focus_policy = policy;
    conn.execute(
        "UPDATE classes SET configuration_json=?2,updated_at=?3 WHERE course_id=?1",
        params![
            course_id,
            serde_json::to_string(&config)?,
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    Ok(config)
}

pub fn path_by_id(conn: &Connection, path_id: &str) -> Result<AcceptedPath> {
    read_path(conn, path_id)
}

/// A deliberate change to an accepted route. Manual choices never create
/// grades, mastery or completion evidence; they only change what is selected next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PathChange {
    /// Check out of familiar topics: they leave the route uncredited.
    Bypass { topics: Vec<String> },
    /// Put earlier, bypassed or checked topics back on the route.
    Include { topics: Vec<String> },
    /// Prior knowledge demonstrated by a unit challenge attempt; no completion credit.
    CheckOut {
        topics: Vec<String>,
        attempt_id: String,
    },
    /// Take a short bridge lesson on `topic` before more work on `before`.
    AcceptBridge { topic: String, before: String },
    /// Decline a proposed bridge; the pair is not proposed again.
    DeclineBridge { topic: String, before: String },
}

/// A gap seen in practice: a failed check on a topic whose prerequisite was
/// set aside (earlier, bypassed or checked) and is not completed here.
#[derive(Debug, Clone, Serialize)]
pub struct BridgeProposal {
    pub topic: super::placement::PathTopic,
    pub before: super::placement::PathTopic,
    pub session_id: String,
    pub score: f64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisePath {
    pub course_id: String,
    pub expected_revision: u32,
    pub change: PathChange,
}

fn revision_by_number(
    conn: &Connection,
    class_id: &str,
    revision: u32,
) -> Result<Option<AcceptedPath>> {
    let id: Option<String> = conn
        .query_row(
            "SELECT id FROM path_revisions WHERE class_id = ?1 AND revision = ?2",
            params![class_id, revision],
            |r| r.get(0),
        )
        .optional()?;
    id.map(|id| read_path(conn, &id)).transpose()
}

/// Apply a change to a plan; `Ok(false)` when it would leave the route as it is.
fn apply_change(
    conn: &Connection,
    course_id: &str,
    plan: &mut Recommendation,
    change: &PathChange,
) -> Result<bool> {
    let concepts = db::all_concepts(conn, course_id)?;
    let states: std::collections::HashMap<i64, String> = crate::mastery::overview(conn, course_id)?
        .into_iter()
        .map(|entry| (entry.concept_id, entry.state))
        .collect();
    let concept = |slug: &str| {
        concepts
            .iter()
            .find(|c| c.slug == slug)
            .ok_or_else(|| DbError::Invalid(format!("{slug} is not a topic of this course.")))
    };
    let listed =
        |list: &[super::placement::PathTopic], slug: &str| list.iter().any(|t| t.id == slug);
    let mut changed = false;
    match change {
        PathChange::Bypass { topics } => {
            for slug in topics {
                let concept = concept(slug)?;
                if states
                    .get(&concept.id)
                    .is_some_and(|state| crate::selection::is_completed(state))
                {
                    return Err(DbError::Invalid(format!(
                        "{} is already completed here; there is nothing to bypass.",
                        concept.title
                    )));
                }
                if listed(&plan.bypassed, slug) {
                    continue;
                }
                plan.bypassed.push(super::placement::PathTopic {
                    id: slug.clone(),
                    label: concept.title.clone(),
                    reason: "Bypassed by choice; not assessed and not counted as coverage.".into(),
                });
                if !listed(&plan.earlier_topics, slug) {
                    plan.earlier_topics.push(super::placement::PathTopic {
                        id: slug.clone(),
                        label: concept.title.clone(),
                        reason: "Bypassed by choice; available for voluntary study.".into(),
                    });
                }
                changed = true;
            }
        }
        PathChange::CheckOut { topics, attempt_id } => {
            for slug in topics {
                let concept = concept(slug)?;
                if states
                    .get(&concept.id)
                    .is_some_and(|state| crate::selection::is_completed(state))
                    || listed(&plan.checked, slug)
                {
                    continue;
                }
                plan.checked.push(super::placement::PathTopic {
                    id: slug.clone(),
                    label: concept.title.clone(),
                    reason: format!(
                        "Prior knowledge checked by unit challenge {attempt_id}; not completed here."
                    ),
                });
                plan.bypassed.retain(|t| &t.id != slug);
                if !listed(&plan.earlier_topics, slug) {
                    plan.earlier_topics.push(super::placement::PathTopic {
                        id: slug.clone(),
                        label: concept.title.clone(),
                        reason: "Prior knowledge checked; available for voluntary study.".into(),
                    });
                }
                changed = true;
            }
        }
        PathChange::AcceptBridge { topic, before } => {
            let concept_topic = concept(topic)?;
            let dependent = concept(before)?;
            if !listed(&plan.bridges, topic) {
                plan.bridges.push(super::placement::PathTopic {
                    id: topic.clone(),
                    label: concept_topic.title.clone(),
                    reason: format!("Bridge before {}.", dependent.title),
                });
                changed = true;
            }
            let before_len = plan.declined_bridges.len();
            plan.declined_bridges
                .retain(|d| !(&d.topic == topic && &d.before == before));
            if plan.declined_bridges.len() != before_len {
                changed = true;
            }
        }
        PathChange::DeclineBridge { topic, before } => {
            concept(topic)?;
            concept(before)?;
            if !plan
                .declined_bridges
                .iter()
                .any(|d| &d.topic == topic && &d.before == before)
            {
                plan.declined_bridges
                    .push(super::placement::BridgeDecision {
                        topic: topic.clone(),
                        before: before.clone(),
                    });
                changed = true;
            }
        }
        PathChange::Include { topics } => {
            for slug in topics {
                concept(slug)?;
                let before = plan.earlier_topics.len() + plan.bypassed.len() + plan.checked.len();
                plan.earlier_topics.retain(|t| &t.id != slug);
                plan.bypassed.retain(|t| &t.id != slug);
                plan.checked.retain(|t| &t.id != slug);
                if plan.earlier_topics.len() + plan.bypassed.len() + plan.checked.len() != before {
                    changed = true;
                }
            }
        }
    }
    Ok(changed)
}

/// Record a new path revision for an accepted engineering class. Existing
/// sessions keep the revision they were planned with; a lost response can be
/// replayed with the same base revision and receives the same result.
pub fn revise(conn: &Connection, input: &RevisePath, _today: &str) -> Result<AcceptedPath> {
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let course = catalog::course(&input.course_id)
        .ok_or_else(|| DbError::InvalidFocus(input.course_id.clone()))?;
    if course.kind != catalog::SubjectKind::Engineering {
        return Err(DbError::Invalid(
            "Language paths change through their starting band and target level.".into(),
        ));
    }
    let current = current_path(&tx, &input.course_id)?
        .ok_or_else(|| DbError::Invalid("Accept a learning path before revising it.".into()))?;
    if enrollment::course_snapshot(&input.course_id)?.0.fingerprint
        != current.reference.course_snapshot_fingerprint
    {
        return Err(DbError::Invalid(
            "The curriculum changed. Review a new path before revising this one.".into(),
        ));
    }
    if current.revision == input.expected_revision + 1 {
        if let Some(base) =
            revision_by_number(&tx, &current.reference.class_id, input.expected_revision)?
        {
            let mut plan = base.recommendation.clone();
            if matches!(
                apply_change(&tx, &input.course_id, &mut plan, &input.change),
                Ok(true)
            ) && serde_json::to_value(&plan)? == serde_json::to_value(&current.recommendation)?
            {
                return Ok(current);
            }
        }
    }
    if current.revision != input.expected_revision {
        return Err(DbError::Invalid(
            "The path changed. Review the current revision before revising it.".into(),
        ));
    }
    let mut plan = current.recommendation.clone();
    if !apply_change(&tx, &input.course_id, &mut plan, &input.change)? {
        return Err(DbError::Invalid(
            "That change would leave the route as it is.".into(),
        ));
    }
    let path_id = format!("path-{:032x}", rand::random::<u128>());
    let now = chrono::Utc::now().to_rfc3339();
    tx.execute(
        "INSERT INTO path_revisions(id,class_id,revision,course_snapshot_fingerprint,entry_profile_json,plan_json,accepted_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            path_id,
            current.reference.class_id,
            current.revision + 1,
            current.reference.course_snapshot_fingerprint,
            serde_json::to_string(&current.configuration)?,
            serde_json::to_string(&plan)?,
            now
        ],
    )?;
    tx.execute(
        "UPDATE classes SET active_path_revision_id=?2, updated_at=?3 WHERE id=?1",
        params![current.reference.class_id, path_id, now],
    )?;
    let result = read_path(&tx, &path_id)?;
    tx.commit()?;
    Ok(result)
}

/// A class that was activated from its settings without choosing a starting
/// point begins at the foundations. This never touches a learner's pending
/// setup draft: an unfinished starting-point choice must be completed first.
pub fn ensure_default_path(
    conn: &Connection,
    course_id: &str,
    today: &str,
) -> Result<AcceptedPath> {
    use super::enrollment::{SaveEnrollmentDraft, TutorPreference};
    if let Some(path) = current_path(conn, course_id)? {
        return Ok(path);
    }
    if enrollment::draft_for_course(conn, course_id)?.is_some() {
        return Err(DbError::Invalid(
            "Finish choosing this class's starting point, or accept its suggested path, before starting a lesson.".into(),
        ));
    }
    let options = enrollment::options(course_id)?;
    let program = crate::classroom::program_row(conn, course_id).map_err(DbError::Invalid)?;
    let mut configuration = options.default_configuration.clone();
    configuration.tutor = TutorPreference {
        provider: program.agent,
        model: program.model,
        custom_agent_bin: (!program.custom_agent_bin.is_empty())
            .then_some(program.custom_agent_bin),
    };
    configuration.pace.session_minutes = program.session_minutes as u32;
    configuration.pace.weekly_minutes =
        (program.target_weekly_minutes > 0).then_some(program.target_weekly_minutes as u32);
    match &mut configuration.goal {
        LearningGoal::CourseOutcome { note } => *note = program.learning_goal,
        LearningGoal::LanguageLevel { target_level, note } => {
            let language =
                crate::language::program_view(conn, course_id, today).map_err(DbError::Invalid)?;
            *target_level = language.target_level;
            *note = program.learning_goal;
            configuration.pace.weekly_minutes = Some(language.weekly_minutes as u32);
        }
    }
    let draft = enrollment::save_draft(
        conn,
        &SaveEnrollmentDraft {
            id: None,
            expected_revision: None,
            course: options.course,
            configuration,
        },
    )?;
    let recommendation = placement::recommend(conn, &draft.id, draft.revision)?;
    accept(
        conn,
        &AcceptPath {
            draft_id: draft.id,
            expected_revision: draft.revision,
            recommendation_id: recommendation.id,
        },
        today,
    )
}

pub fn current_path(conn: &Connection, course_id: &str) -> Result<Option<AcceptedPath>> {
    let id: Option<String> = conn
        .query_row(
            "SELECT active_path_revision_id FROM classes WHERE course_id = ?1",
            [course_id],
            |r| r.get(0),
        )
        .optional()?;
    id.map(|id| read_path(conn, &id)).transpose()
}

pub fn current_configuration(
    conn: &Connection,
    course_id: &str,
) -> Result<Option<EnrollmentConfiguration>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT configuration_json FROM classes WHERE course_id=?1",
            [course_id],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(DbError::from))
        .transpose()
}

/// Keep mutable class settings aligned while the subject adapters own sessions.
/// The accepted entry profile and past path revisions stay immutable.
pub fn sync_configuration(conn: &Connection, course_id: &str, today: &str) -> Result<()> {
    let Some(mut config) = current_configuration(conn, course_id)? else {
        return Ok(());
    };
    let program = crate::classroom::program_row(conn, course_id).map_err(DbError::Invalid)?;
    config.tutor.provider = program.agent;
    config.tutor.model = program.model;
    config.tutor.custom_agent_bin =
        (!program.custom_agent_bin.is_empty()).then_some(program.custom_agent_bin);
    config.pace.session_minutes = program.session_minutes as u32;
    config.pace.weekly_minutes =
        (program.target_weekly_minutes > 0).then_some(program.target_weekly_minutes as u32);
    match &mut config.goal {
        LearningGoal::CourseOutcome { note } => *note = program.learning_goal,
        LearningGoal::LanguageLevel { target_level, note } => {
            let language =
                crate::language::program_view(conn, course_id, today).map_err(DbError::Invalid)?;
            *target_level = language.target_level;
            *note = program.learning_goal;
            config.pace.weekly_minutes = Some(language.weekly_minutes as u32);
        }
    }
    conn.execute(
        "UPDATE classes SET configuration_json=?2,status=?3,updated_at=?4 WHERE course_id=?1",
        params![
            course_id,
            serde_json::to_string(&config)?,
            if program.enabled
                && crate::classroom::has_enabled_schedule(conn, course_id)
                    .map_err(DbError::Invalid)?
            {
                "active"
            } else {
                "paused"
            },
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

/// Validate the exact recommendation again inside the same write transaction.
/// Duplicate acceptance returns that immutable revision, even after a later edit.
pub fn accept(conn: &Connection, input: &AcceptPath, today: &str) -> Result<AcceptedPath> {
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let draft = enrollment::draft(&tx, &input.draft_id)?;
    if draft.status == DraftStatus::Accepted {
        let id: Option<String> = tx.query_row("SELECT id FROM path_revisions WHERE class_id = ?1 AND json_extract(plan_json,'$.id') = ?2 AND json_extract(plan_json,'$.draft_revision') = ?3",
            params![draft.accepted_class_id,input.recommendation_id,input.expected_revision], |r| r.get(0)).optional()?;
        return id.map(|id| read_path(&tx, &id)).unwrap_or_else(|| {
            Err(DbError::Invalid(
                "This draft was accepted with a different recommendation.".into(),
            ))
        });
    }
    let recommendation =
        placement::recommend_in_transaction(&tx, &input.draft_id, input.expected_revision)?;
    if recommendation.id != input.recommendation_id {
        return Err(DbError::Invalid(
            "The recommendation changed. Review the current path before accepting it.".into(),
        ));
    }
    let course = catalog::course(&draft.course.course_id)
        .ok_or_else(|| DbError::InvalidFocus(draft.course.course_id.clone()))?;
    if !crate::agents::valid_model(&draft.configuration.tutor.model) {
        return Err(DbError::Invalid(
            "Choose a valid model ID before accepting the path.".into(),
        ));
    }
    if crate::agents::RunnerId::parse(&draft.configuration.tutor.provider)
        == Some(crate::agents::RunnerId::CustomCli)
    {
        crate::agents::process::command_words(
            draft
                .configuration
                .tutor
                .custom_agent_bin
                .as_deref()
                .unwrap_or(""),
        )
        .map_err(|e| DbError::Invalid(e.to_string()))?;
    }
    let previous = current_path(&tx, course.course_id)?;
    let class_id = previous
        .as_ref()
        .map(|p| p.reference.class_id.clone())
        .unwrap_or_else(|| format!("class-{:032x}", rand::random::<u128>()));
    let revision = previous.as_ref().map_or(1, |p| p.revision + 1);
    let path_id = format!("path-{:032x}", rand::random::<u128>());
    let now = chrono::Utc::now().to_rfc3339();
    let configuration_json = serde_json::to_string(&draft.configuration)?;
    // Save an accepted path without activating an unscheduled class, or one
    // whose study times overlap another active class.
    let scheduled = crate::classroom::has_enabled_schedule(&tx, course.id)
        .map_err(DbError::Invalid)?
        && crate::classroom::activation_conflicts(
            &tx,
            course.id,
            i64::from(draft.configuration.pace.session_minutes),
        )
        .map_err(DbError::Invalid)?
        .is_empty();
    let status = if scheduled { "active" } else { "paused" };
    let changed = tx.execute("UPDATE classroom_programs SET enabled=?9, agent=?2, model=?3, custom_agent_bin=?4, session_minutes=?5, learning_goal=?6, target_weekly_minutes=COALESCE(?7,target_weekly_minutes), updated_at=?8 WHERE subject_id=?1",
        params![course.id,draft.configuration.tutor.provider,draft.configuration.tutor.model,draft.configuration.tutor.custom_agent_bin.as_deref().unwrap_or(""),draft.configuration.pace.session_minutes,
            match &draft.configuration.goal { LearningGoal::CourseOutcome{note} | LearningGoal::LanguageLevel{note,..} => note },draft.configuration.pace.weekly_minutes,now,scheduled])?;
    if changed != 1 {
        return Err(DbError::Invalid(
            "Classroom data is not initialized. Restart the app and retry.".into(),
        ));
    }
    if let LearningGoal::LanguageLevel { target_level, .. } = &draft.configuration.goal {
        // The draft's session length is a placeholder; the study times already
        // saved for this class decide it below.
        // Existing lesson/assessment rows and the original progress baseline are
        // retained. An explicit new path changes the cursor for subsequent work.
        let changed = tx.execute("UPDATE language_programs SET enabled=?8, current_level=?2, target_level=?3, start_level=CASE WHEN EXISTS(SELECT 1 FROM language_sessions WHERE language=?1) THEN start_level ELSE ?2 END, start_date=CASE WHEN EXISTS(SELECT 1 FROM language_sessions WHERE language=?1) THEN start_date ELSE ?7 END, weekly_minutes=COALESCE(?4,weekly_minutes), session_minutes=?5, updated_at=?6 WHERE language=?1",
            params![course.id,recommendation.entry_point,target_level,draft.configuration.pace.weekly_minutes,draft.configuration.pace.session_minutes,now,today,scheduled])?;
        if changed != 1 {
            return Err(DbError::Invalid(
                "Language data is not initialized. Restart the app and retry.".into(),
            ));
        }
    }
    // The session length is not asked for: it follows the study times.
    crate::classroom::follow_schedule_length(&tx, course.id).map_err(DbError::Invalid)?;
    tx.execute("INSERT INTO classes(id,course_id,course_snapshot_fingerprint,status,configuration_json,active_path_revision_id,created_at,updated_at) VALUES(?1,?2,?3,?7,?4,?5,?6,?6) ON CONFLICT(course_id) DO UPDATE SET course_snapshot_fingerprint=excluded.course_snapshot_fingerprint, configuration_json=excluded.configuration_json,active_path_revision_id=excluded.active_path_revision_id,status=excluded.status,updated_at=excluded.updated_at",
        params![class_id,course.course_id,draft.course.fingerprint,configuration_json,path_id,now,status])?;
    tx.execute("INSERT INTO path_revisions(id,class_id,revision,course_snapshot_fingerprint,entry_profile_json,plan_json,accepted_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![path_id,class_id,revision,draft.course.fingerprint,configuration_json,serde_json::to_string(&recommendation)?,now])?;
    tx.execute("UPDATE enrollment_drafts SET status='accepted',accepted_class_id=?2,updated_at=?3 WHERE id=?1",params![draft.id.0,class_id,now])?;
    tx.execute("INSERT OR IGNORE INTO legacy_crosswalk(legacy_table,legacy_key,entity_kind,entity_id,imported_at) VALUES('classroom_programs',?1,'class',?2,?3)",params![course.id,class_id,now])?;
    let result = read_path(&tx, &path_id)?;
    tx.commit()?;
    Ok(result)
}

/// Completed class sessions of `class_id` with their selection slug and outcome.
fn completed_results(
    conn: &Connection,
    class_id: &str,
) -> Result<Vec<(String, String, serde_json::Value, String)>> {
    let mut statement = conn.prepare(
        "SELECT s.id, json_extract(s.context_json,'$.selection.slug'), r.outcome_json, r.finished_at FROM study_sessions s JOIN study_results r ON r.session_id = s.id WHERE s.class_id = ?1 AND r.disposition = 'completed' ORDER BY r.finished_at DESC",
    )?;
    let rows = statement.query_map([class_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, slug, outcome, finished) = row?;
        out.push((
            id,
            slug,
            serde_json::from_str(&outcome).unwrap_or(serde_json::Value::Null),
            finished,
        ));
    }
    Ok(out)
}

/// Bridge lessons proposed from failed checks: the failed topic's prerequisites
/// that were set aside and are neither completed, accepted nor declined.
pub fn bridge_proposals(conn: &Connection, course_id: &str) -> Result<Vec<BridgeProposal>> {
    let Some(path) = current_path(conn, course_id)? else {
        return Ok(Vec::new());
    };
    let plan = &path.recommendation;
    let concepts = db::all_concepts(conn, course_id)?;
    let states: std::collections::HashMap<i64, String> = crate::mastery::overview(conn, course_id)?
        .into_iter()
        .map(|entry| (entry.concept_id, entry.state))
        .collect();
    let mut prerequisites = std::collections::HashMap::new();
    let mut statement =
        conn.prepare("SELECT slug, prereqs_json FROM concepts WHERE active = 1 AND focus = ?1")?;
    for row in statement.query_map([course_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (slug, raw) = row?;
        prerequisites.insert(
            slug,
            serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default(),
        );
    }
    let listed =
        |list: &[super::placement::PathTopic], slug: &str| list.iter().any(|t| t.id == slug);
    let topic = |slug: &str, reason: String| {
        concepts
            .iter()
            .find(|c| c.slug == slug)
            .map(|c| super::placement::PathTopic {
                id: c.slug.clone(),
                label: c.title.clone(),
                reason,
            })
    };
    let mut proposals: Vec<BridgeProposal> = Vec::new();
    for (session_id, slug, outcome, _) in completed_results(conn, &path.reference.class_id)? {
        if outcome["passed"].as_bool() != Some(false) {
            continue;
        }
        let Some(dependent) = concepts.iter().find(|c| c.slug == slug) else {
            continue;
        };
        for prerequisite in prerequisites.get(&slug).cloned().unwrap_or_default() {
            let set_aside = listed(&plan.earlier_topics, &prerequisite)
                || listed(&plan.bypassed, &prerequisite)
                || listed(&plan.checked, &prerequisite);
            let completed = concepts
                .iter()
                .find(|c| c.slug == prerequisite)
                .and_then(|c| states.get(&c.id))
                .is_some_and(|state| crate::selection::is_completed(state));
            let decided = listed(&plan.bridges, &prerequisite)
                || plan
                    .declined_bridges
                    .iter()
                    .any(|d| d.topic == prerequisite && d.before == slug);
            if !set_aside
                || completed
                || decided
                || proposals.iter().any(|p| p.topic.id == prerequisite)
            {
                continue;
            }
            let (Some(topic), Some(before)) = (
                topic(
                    &prerequisite,
                    format!(
                        "Set aside on your route; {} depends on it.",
                        dependent.title
                    ),
                ),
                topic(&slug, "The check on this topic did not pass.".into()),
            ) else {
                continue;
            };
            proposals.push(BridgeProposal {
                topic,
                before,
                session_id: session_id.clone(),
                score: outcome["score"].as_f64().unwrap_or(0.0),
            });
        }
    }
    Ok(proposals)
}

/// An accepted bridge is pending until a session on that topic completes after
/// the revision that accepted it.
fn pending_bridge(conn: &Connection, path: &AcceptedPath, slug: &str) -> Result<bool> {
    let accepted_at: Option<String> = {
        let mut statement = conn.prepare(
            "SELECT accepted_at, plan_json FROM path_revisions WHERE class_id = ?1 ORDER BY revision",
        )?;
        let rows = statement.query_map([&path.reference.class_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut found = None;
        for row in rows {
            let (at, plan) = row?;
            let plan: Recommendation = serde_json::from_str(&plan)?;
            if plan.bridges.iter().any(|t| t.id == slug) {
                found = Some(at);
                break;
            }
        }
        found
    };
    let Some(accepted_at) = accepted_at else {
        return Ok(false);
    };
    Ok(!completed_results(conn, &path.reference.class_id)?
        .iter()
        .any(|(_, taken, _, finished)| taken == slug && *finished >= accepted_at))
}

/// Why a topic is next: an accepted bridge, or the accepted route itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NextReason {
    Bridge,
    Route,
}

/// A manual/diagnostic bypass changes the route, not the mastery ledger. Earlier
/// prerequisites are carried into the lesson as visible checks and refreshers.
/// Accepted bridge lessons come first, once each, before regular selection.
pub fn next_concept(conn: &Connection, course_id: &str, date: &str) -> Result<Option<db::Concept>> {
    if current_path(conn, course_id)?.is_none() {
        return crate::selection::draw(conn, date, course_id);
    }
    let next = peek_next_concept(conn, course_id)?.map(|(concept, _)| concept);
    if let Some(concept) = &next {
        db::mark_concept_picked(conn, concept.id, date)?;
    }
    Ok(next)
}

/// The topic selection would serve next on an accepted route, without
/// recording a pick. `None` when the route has nothing left.
pub fn peek_next_concept(
    conn: &Connection,
    course_id: &str,
) -> Result<Option<(db::Concept, NextReason)>> {
    let Some(path) = current_path(conn, course_id)? else {
        return Ok(None);
    };
    for bridge in &path.recommendation.bridges {
        if pending_bridge(conn, &path, &bridge.id)? {
            if let Some(concept) = db::all_concepts(conn, course_id)?
                .into_iter()
                .find(|c| c.slug == bridge.id)
            {
                return Ok(Some((concept, NextReason::Bridge)));
            }
        }
    }
    if enrollment::course_snapshot(course_id)?.0.fingerprint
        != path.reference.course_snapshot_fingerprint
    {
        return Err(DbError::Invalid(
            "The curriculum changed. Review a new path before starting another lesson.".into(),
        ));
    }
    let earlier: std::collections::HashSet<_> = path
        .recommendation
        .earlier_topics
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    let mut states = std::collections::HashMap::new();
    let mut statement = conn.prepare("SELECT c.slug,COALESCE(m.state,'unseen'),c.prereqs_json FROM concepts c LEFT JOIN mastery m ON m.concept_id=c.id WHERE c.active=1 AND c.focus=?1")?;
    let rows = statement.query_map([course_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut prerequisites = std::collections::HashMap::new();
    for row in rows {
        let (slug, state, raw) = row?;
        prerequisites.insert(slug.clone(), serde_json::from_str::<Vec<String>>(&raw)?);
        states.insert(slug, state);
    }
    let mut candidates: Vec<_> = db::all_concepts(conn, course_id)?
        .into_iter()
        .filter(|c| {
            !earlier.contains(c.slug.as_str())
                && !states
                    .get(&c.slug)
                    .is_some_and(|state| crate::selection::is_completed(state))
        })
        .filter(|c| {
            prerequisites.get(&c.slug).is_none_or(|reqs| {
                reqs.iter().all(|slug| {
                    earlier.contains(slug.as_str())
                        || states.get(slug).is_some_and(|s| {
                            matches!(
                                s.as_str(),
                                "practicing"
                                    | "struggling"
                                    | "mastered"
                                    | "maintenance"
                                    | "decayed"
                            )
                        })
                })
            })
        })
        .collect();
    let order = |phase: &str| {
        [
            "foundations",
            "mechanisms",
            "production",
            "synthesis",
            "elective",
        ]
        .iter()
        .position(|p| *p == phase)
        .unwrap_or(5)
    };
    candidates.sort_by_key(|c| {
        (
            !c.curriculum.core,
            order(&c.curriculum.phase),
            c.times_picked,
            c.tier,
            c.id,
        )
    });
    Ok(candidates
        .into_iter()
        .next()
        .map(|concept| (concept, NextReason::Route)))
}
