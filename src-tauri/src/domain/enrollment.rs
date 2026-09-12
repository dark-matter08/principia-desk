//! Durable enrollment choices. A draft never starts a class, schedules work,
//! acquires focus, or manufactures prior-learning evidence.
use crate::{
    catalog,
    db::{DbError, Result},
    language,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EnrollmentDraftId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CourseReference {
    pub course_id: String,
    pub version: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "route", rename_all = "snake_case", deny_unknown_fields)]
pub enum EntryChoice {
    Foundations,
    Diagnostic,
    Manual {
        entry_point: String,
        familiar_competencies: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LearningGoal {
    CourseOutcome { note: String },
    LanguageLevel { target_level: String, note: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusPolicy {
    Advisory,
    Focused,
    Strict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudyPace {
    pub session_minutes: u32,
    pub weekly_minutes: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TutorPreference {
    pub provider: String,
    /// Provider-specific model ID, not the legacy Claude model enum.
    pub model: String,
    pub custom_agent_bin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentConfiguration {
    pub goal: LearningGoal,
    pub entry: EntryChoice,
    pub pace: StudyPace,
    pub tutor: TutorPreference,
    pub focus_policy: FocusPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FamiliarityOption {
    pub id: String,
    pub label: String,
    pub group: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryPointOption {
    pub id: String,
    pub label: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentOptions {
    pub course: CourseReference,
    pub entry_points: Vec<EntryPointOption>,
    pub familiarity_options: Vec<FamiliarityOption>,
    pub default_configuration: EnrollmentConfiguration,
    /// Whether a placement check exists for this course. A learner's own
    /// class has none until its question bank is written.
    #[serde(default = "yes")]
    pub diagnostic_available: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftStatus {
    Draft,
    Accepted,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrollmentDraft {
    pub id: EnrollmentDraftId,
    pub course: CourseReference,
    pub configuration: EnrollmentConfiguration,
    pub status: DraftStatus,
    pub revision: u32,
    pub accepted_class_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveEnrollmentDraft {
    pub id: Option<EnrollmentDraftId>,
    pub expected_revision: Option<u32>,
    pub course: CourseReference,
    pub configuration: EnrollmentConfiguration,
}

fn definition(course_id: &str) -> Result<&'static catalog::CourseDefinition> {
    catalog::course(course_id).ok_or_else(|| DbError::InvalidFocus(course_id.into()))
}

/// Canonical snapshot: serde_json sorts object keys and retains authored array
/// order. The frontend generator hashes the same content for preview requests.
pub fn course_snapshot(course_id: &str) -> Result<(CourseReference, Value)> {
    let course = definition(course_id)?;
    // A learner's own course: its definition and topics come from the
    // registry, filled from the database at startup and on publish.
    if catalog::is_custom(course_id) {
        let curriculum = catalog::custom_curriculum(course_id)
            .ok_or_else(|| DbError::InvalidFocus(course_id.into()))?;
        let definition = serde_json::to_value(course)?;
        let snapshot = serde_json::json!({"course": definition, "curriculum": curriculum, "prompt": course.prompt, "reference_lessons": []});
        let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&snapshot)?));
        return Ok((
            CourseReference {
                course_id: course.course_id.into(),
                version: course.version.into(),
                fingerprint,
            },
            snapshot,
        ));
    }
    let manifest: Value = serde_json::from_str(include_str!("../../seed/catalog.json"))?;
    let definition = manifest["courses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["course_id"] == course_id)
        .unwrap()
        .clone();
    let curriculum = if course.kind == catalog::SubjectKind::Engineering {
        let concepts: Vec<Value> = serde_json::from_str(include_str!("../../seed/concepts.json"))?;
        Value::Array(
            concepts
                .into_iter()
                .filter(|concept| concept["focus"] == course.id)
                .collect(),
        )
    } else {
        serde_json::from_str(match course.id {
            "german" => include_str!("../../seed/languages/german.json"),
            "italian" => include_str!("../../seed/languages/italian.json"),
            _ => return Err(DbError::InvalidFocus(course.id.into())),
        })?
    };
    let references = course
        .bundled_lessons
        .iter()
        .map(|content| serde_json::from_str::<Value>(content))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let snapshot = serde_json::json!({"course": definition, "curriculum": curriculum, "prompt": course.prompt, "reference_lessons": references});
    let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&snapshot)?));
    Ok((
        CourseReference {
            course_id: course.course_id.into(),
            version: course.version.into(),
            fingerprint,
        },
        snapshot,
    ))
}

pub fn options(course_id: &str) -> Result<EnrollmentOptions> {
    let (reference, snapshot) = course_snapshot(course_id)?;
    let course = definition(course_id)?;
    let familiarity_options = if course.kind == catalog::SubjectKind::Engineering {
        snapshot["curriculum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|concept| FamiliarityOption {
                id: concept["slug"].as_str().unwrap().into(),
                label: concept["title"].as_str().unwrap().into(),
                group: concept["curriculum"]["phase"].as_str().map(String::from),
            })
            .collect()
    } else {
        language::STRANDS
            .iter()
            .map(|strand| FamiliarityOption {
                id: (*strand).into(),
                label: strand.replace('_', " "),
                group: None,
            })
            .collect()
    };
    Ok(EnrollmentOptions {
        diagnostic_available: crate::domain::placement::has_bank(course_id),
        course: reference,
        entry_points: course
            .entry_points
            .iter()
            .map(|point| EntryPointOption {
                id: point.id.into(),
                label: point.label.into(),
            })
            .collect(),
        familiarity_options,
        default_configuration: EnrollmentConfiguration {
            goal: if course.kind == catalog::SubjectKind::Language {
                LearningGoal::LanguageLevel {
                    target_level: "A2".into(),
                    note: String::new(),
                }
            } else {
                LearningGoal::CourseOutcome {
                    note: String::new(),
                }
            },
            entry: EntryChoice::Foundations,
            pace: StudyPace {
                session_minutes: 30,
                weekly_minutes: None,
            },
            tutor: TutorPreference {
                provider: "claude".into(),
                model: "sonnet".into(),
                custom_agent_bin: None,
            },
            focus_policy: FocusPolicy::Advisory,
        },
    })
}

fn validate(input: &SaveEnrollmentDraft, options: &EnrollmentOptions) -> Result<()> {
    if input.course != options.course {
        return Err(DbError::Invalid(
            "curriculum changed; reload course options before saving this draft".into(),
        ));
    }
    let config = &input.configuration;
    let course = definition(&input.course.course_id)?;
    let note = match (&config.goal, course.kind) {
        (LearningGoal::CourseOutcome { note }, catalog::SubjectKind::Engineering) => note,
        (LearningGoal::LanguageLevel { target_level, note }, catalog::SubjectKind::Language) => {
            let target = language::LEVELS
                .iter()
                .position(|level| *level == target_level)
                .ok_or_else(|| DbError::Invalid("unknown target level".into()))?;
            if let EntryChoice::Manual { entry_point, .. } = &config.entry {
                let start = language::LEVELS
                    .iter()
                    .position(|level| *level == entry_point)
                    .ok_or_else(|| DbError::Invalid("unknown starting level".into()))?;
                if start > target {
                    return Err(DbError::Invalid(
                        "the target level must include the chosen starting level".into(),
                    ));
                }
            }
            note
        }
        _ => {
            return Err(DbError::Invalid(
                "goal does not match the selected course".into(),
            ))
        }
    };
    if note.len() > 4000 {
        return Err(DbError::Invalid("goal note is too long".into()));
    }
    if let EntryChoice::Manual {
        entry_point,
        familiar_competencies,
    } = &config.entry
    {
        if !options
            .entry_points
            .iter()
            .any(|point| point.id == *entry_point)
        {
            return Err(DbError::Invalid("unknown course starting point".into()));
        }
        let mut seen = std::collections::HashSet::new();
        for competency in familiar_competencies {
            if !seen.insert(competency)
                || !options
                    .familiarity_options
                    .iter()
                    .any(|option| option.id == *competency)
            {
                return Err(DbError::Invalid(
                    "familiarity must reference distinct competencies in this course".into(),
                ));
            }
        }
    }
    if !(10..=480).contains(&config.pace.session_minutes)
        || config
            .pace
            .weekly_minutes
            .is_some_and(|minutes| minutes < config.pace.session_minutes || minutes > 10080)
    {
        return Err(DbError::Invalid(
            "choose sessions of 10–480 minutes and a weekly pace that can contain a session".into(),
        ));
    }
    if crate::agents::RunnerId::parse(&config.tutor.provider).is_none()
        || !crate::agents::valid_model(&config.tutor.model)
    {
        return Err(DbError::Invalid(
            "choose a supported provider and a nonempty provider-specific model ID".into(),
        ));
    }
    if config
        .tutor
        .custom_agent_bin
        .as_ref()
        .is_some_and(|path| path.len() > 4096 || path.contains('\0'))
        || (config.tutor.provider == "custom"
            && config
                .tutor
                .custom_agent_bin
                .as_ref()
                .is_none_or(|path| path.trim().is_empty()))
    {
        return Err(DbError::Invalid(
            "the custom provider needs a valid executable path".into(),
        ));
    }
    Ok(())
}

fn read_draft(conn: &Connection, id: &str) -> Result<Option<EnrollmentDraft>> {
    let record = conn.query_row("SELECT d.id, d.course_id, s.version, d.course_snapshot_fingerprint, d.configuration_json, d.status, d.revision, d.accepted_class_id, d.created_at, d.updated_at FROM enrollment_drafts d JOIN course_snapshots s ON s.fingerprint = d.course_snapshot_fingerprint WHERE d.id = ?1", [id], |row| {
        Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,String>(2)?, row.get::<_,String>(3)?, row.get::<_,String>(4)?, row.get::<_,String>(5)?, row.get::<_,u32>(6)?, row.get::<_,Option<String>>(7)?, row.get::<_,String>(8)?, row.get::<_,String>(9)?))
    }).optional()?;
    record
        .map(
            |(
                id,
                course_id,
                version,
                fingerprint,
                configuration,
                status,
                revision,
                accepted_class_id,
                created_at,
                updated_at,
            )| {
                Ok(EnrollmentDraft {
                    id: EnrollmentDraftId(id),
                    course: CourseReference {
                        course_id,
                        version,
                        fingerprint,
                    },
                    configuration: serde_json::from_str(&configuration)?,
                    status: serde_json::from_value(Value::String(status))?,
                    revision,
                    accepted_class_id,
                    created_at,
                    updated_at,
                })
            },
        )
        .transpose()
}

pub fn draft(conn: &Connection, id: &EnrollmentDraftId) -> Result<EnrollmentDraft> {
    read_draft(conn, &id.0)?.ok_or_else(|| DbError::Invalid("enrollment draft not found".into()))
}

pub fn draft_for_course(conn: &Connection, course_id: &str) -> Result<Option<EnrollmentDraft>> {
    definition(course_id)?;
    let id: Option<String> = conn
        .query_row(
            "SELECT id FROM enrollment_drafts WHERE course_id = ?1 AND status = 'draft'",
            [course_id],
            |row| row.get(0),
        )
        .optional()?;
    id.map(|id| read_draft(conn, &id))
        .transpose()
        .map(Option::flatten)
}

pub fn save_draft(conn: &Connection, input: &SaveEnrollmentDraft) -> Result<EnrollmentDraft> {
    let options = options(&input.course.course_id)?;
    validate(input, &options)?;
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let existing = if let Some(id) = &input.id {
        Some(
            read_draft(&tx, &id.0)?
                .ok_or_else(|| DbError::Invalid("enrollment draft not found".into()))?,
        )
    } else {
        draft_for_course(&tx, &input.course.course_id)?
    };
    if let Some(existing) = &existing {
        if existing.course.course_id != input.course.course_id
            || existing.status != DraftStatus::Draft
        {
            return Err(DbError::Invalid(
                "this draft cannot be changed for that course".into(),
            ));
        }
        // Retrying a save whose desired state is present makes no new revision.
        if existing.course == input.course && existing.configuration == input.configuration {
            return Ok(existing.clone());
        }
        if input.id.as_ref() != Some(&existing.id)
            || input.expected_revision != Some(existing.revision)
        {
            return Err(DbError::Invalid(
                "enrollment draft changed; reload it before saving".into(),
            ));
        }
    } else if input.expected_revision.is_some() {
        return Err(DbError::Invalid("a new draft has no prior revision".into()));
    }
    let now = chrono::Utc::now().to_rfc3339();
    let configuration = serde_json::to_string(&input.configuration)?;
    let (_, snapshot) = course_snapshot(&input.course.course_id)?;
    tx.execute("INSERT OR IGNORE INTO course_snapshots (fingerprint, course_id, version, body_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)", params![input.course.fingerprint, input.course.course_id, input.course.version, serde_json::to_string(&snapshot)?, now])?;
    let id = if let Some(existing) = existing {
        tx.execute("UPDATE enrollment_drafts SET course_snapshot_fingerprint = ?2, configuration_json = ?3, revision = revision + 1, updated_at = ?4 WHERE id = ?1", params![existing.id.0, input.course.fingerprint, configuration, now])?;
        existing.id
    } else {
        let id = EnrollmentDraftId(format!("draft-{:032x}", rand::random::<u128>()));
        tx.execute("INSERT INTO enrollment_drafts (id, course_id, course_snapshot_fingerprint, configuration_json, revision, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)", params![id.0, input.course.course_id, input.course.fingerprint, configuration, now])?;
        id
    };
    let saved =
        read_draft(&tx, &id.0)?.ok_or_else(|| DbError::Invalid("saved draft missing".into()))?;
    tx.commit()?;
    Ok(saved)
}
