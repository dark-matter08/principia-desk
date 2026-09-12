use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Db(pub Mutex<Connection>);

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid focus: {0}")]
    InvalidFocus(String),
    #[error("focus is required to start a new session")]
    FocusRequired,
    #[error("focus cannot change mid-session")]
    FocusLocked,
    #[error("invalid curriculum brief for {0}: {1}")]
    InvalidCurriculum(String, String),
    #[error("{0}")]
    Invalid(String),
}

impl From<&str> for DbError {
    fn from(message: &str) -> Self {
        DbError::Invalid(message.into())
    }
}

pub type Result<T> = std::result::Result<T, DbError>;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CurriculumBrief {
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub core: bool,
    #[serde(default)]
    pub learner_outcome: String,
    #[serde(default)]
    pub mechanisms: Vec<String>,
    #[serde(default)]
    pub production_scenario: String,
    #[serde(default)]
    pub misconceptions: Vec<String>,
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub artifact: String,
    #[serde(default)]
    pub primary_sources: Vec<String>,
    #[serde(default)]
    pub related_concepts: Vec<String>,
}

impl CurriculumBrief {
    pub fn validate(&self) -> std::result::Result<(), &'static str> {
        if !matches!(
            self.phase.as_str(),
            "foundations" | "mechanisms" | "production" | "synthesis" | "elective"
        ) {
            return Err(
                "phase must be foundations, mechanisms, production, synthesis, or elective",
            );
        }
        if self.learner_outcome.split_whitespace().count() < 8 {
            return Err("learner outcome is too vague");
        }
        if self.mechanisms.len() < 2 || self.mechanisms.iter().any(|value| value.trim().is_empty())
        {
            return Err("at least two named mechanisms are required");
        }
        if self.production_scenario.split_whitespace().count() < 8 {
            return Err("production scenario is too vague");
        }
        if self.misconceptions.is_empty()
            || self
                .misconceptions
                .iter()
                .any(|value| value.trim().is_empty())
        {
            return Err("at least one misconception is required");
        }
        if self.evidence.split_whitespace().count() < 6 {
            return Err("observable evidence is too vague");
        }
        if self.artifact.split_whitespace().count() < 6 {
            return Err("cumulative artifact is too vague");
        }
        if self.primary_sources.len() < 2
            || self
                .primary_sources
                .iter()
                .any(|source| !(source.starts_with("https://") || source.starts_with("http://")))
        {
            return Err("at least two absolute primary-source URLs are required");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub category: String,
    pub weight: f64,
    pub times_picked: i64,
    pub last_picked_date: Option<String>,
    pub tier: i64,
    pub focus: String,
    pub curriculum: CurriculumBrief,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub date: String,
    pub concept_id: Option<i64>,
    pub status: String,
    pub current_step: String,
    pub quiz_score: Option<f64>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub reading_seconds: i64,
    /// 'lesson' (default) or 'pop_quiz' — chosen by the nightly planner.
    pub session_type: String,
    /// Teacher's one-line reason for the chosen type (shown in UI).
    pub plan_reason: String,
    /// Engineering course ID from the catalog (empty until chosen).
    pub focus: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Course {
    pub id: i64,
    pub session_date: String,
    pub concept_id: i64,
    pub markdown: String,
    pub resources_json: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: i64,
    pub course_id: i64,
    pub prompt: String,
    pub kind: String,
    pub choices_json: Option<String>,
    pub correct_answer: String,
    pub explanation: String,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attempt {
    pub question_id: i64,
    pub session_date: String,
    pub user_answer: String,
    pub correct: bool,
    pub grader_feedback: String,
}

/// Open the learner database through the checksummed migration boundary.
pub fn open(path: &PathBuf) -> Result<Connection> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    crate::storage::migrations::enable_foreign_keys(&conn)?;
    crate::storage::migrations::migrate(&conn, path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    crate::storage::migrations::enable_foreign_keys(&conn)?;
    Ok(conn)
}

pub fn seed_concepts(conn: &Connection, seed_json: &str) -> Result<usize> {
    #[derive(Deserialize)]
    struct SeedConcept {
        slug: String,
        title: String,
        category: String,
        #[serde(default)]
        tier: i64,
        #[serde(default)]
        prereqs: Vec<String>,
        #[serde(default = "default_legacy_focus")]
        focus: String,
        #[serde(default)]
        curriculum: CurriculumBrief,
    }
    fn default_legacy_focus() -> String {
        crate::focus::LEGACY_FOCUS.into()
    }
    let seeds: Vec<SeedConcept> = serde_json::from_str(seed_json)?;
    let mut index = HashMap::new();
    for seed in &seeds {
        if seed.slug.is_empty() || index.insert(seed.slug.as_str(), seed).is_some() {
            return Err(DbError::InvalidCurriculum(
                seed.slug.clone(),
                "duplicate or empty slug".into(),
            ));
        }
    }
    // Validate the entire graph before any metadata is written. A broken
    // release must not partially refresh a learner's installed curriculum.
    for seed in &seeds {
        crate::focus::validate_selectable(&seed.focus)?;
        seed.curriculum
            .validate()
            .map_err(|reason| DbError::InvalidCurriculum(seed.slug.clone(), reason.to_string()))?;
        if !(0..=3).contains(&seed.tier) {
            return Err(DbError::InvalidCurriculum(
                seed.slug.clone(),
                "tier must be between zero and three".into(),
            ));
        }
        for prerequisite in &seed.prereqs {
            let valid = index
                .get(prerequisite.as_str())
                .is_some_and(|previous| previous.focus == seed.focus && previous.tier <= seed.tier);
            if !valid {
                return Err(DbError::InvalidCurriculum(
                    seed.slug.clone(),
                    format!("invalid prerequisite: {prerequisite}"),
                ));
            }
        }
        for related in &seed.curriculum.related_concepts {
            if index
                .get(related.as_str())
                .is_none_or(|other| other.focus == seed.focus)
            {
                return Err(DbError::InvalidCurriculum(
                    seed.slug.clone(),
                    format!("related concept must exist in another track: {related}"),
                ));
            }
        }
    }
    fn visit<'a>(
        slug: &'a str,
        index: &HashMap<&'a str, &'a SeedConcept>,
        visiting: &mut std::collections::HashSet<&'a str>,
        visited: &mut std::collections::HashSet<&'a str>,
    ) -> Result<()> {
        if visited.contains(slug) {
            return Ok(());
        }
        if !visiting.insert(slug) {
            return Err(DbError::InvalidCurriculum(
                slug.to_string(),
                "prerequisite cycle".into(),
            ));
        }
        for prerequisite in &index[slug].prereqs {
            visit(prerequisite, index, visiting, visited)?;
        }
        visiting.remove(slug);
        visited.insert(slug);
        Ok(())
    }
    let mut visited = std::collections::HashSet::new();
    for seed in &seeds {
        visit(
            &seed.slug,
            &index,
            &mut std::collections::HashSet::new(),
            &mut visited,
        )?;
    }
    let tx = conn.unchecked_transaction()?;
    let mut inserted = 0;
    for s in seeds {
        let prereqs_json = serde_json::to_string(&s.prereqs)?;
        let brief_json = serde_json::to_string(&s.curriculum)?;
        inserted += tx.execute(
            "INSERT OR IGNORE INTO concepts
                (slug, title, category, tier, prereqs_json, focus, brief_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                s.slug,
                s.title,
                s.category,
                s.tier,
                prereqs_json,
                s.focus,
                brief_json
            ],
        )?;
        // Curriculum metadata always refreshes from seed (existing installs
        // pick up tier/prereq/focus changes); progress columns are never touched.
        tx.execute(
            "UPDATE concepts
             SET title = ?2, category = ?3, tier = ?4, prereqs_json = ?5,
                 focus = ?6, brief_json = ?7
             WHERE slug = ?1",
            params![
                s.slug,
                s.title,
                s.category,
                s.tier,
                prereqs_json,
                s.focus,
                brief_json
            ],
        )?;
    }
    tx.commit()?;
    Ok(inserted)
}

pub fn get_config(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM config WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    Ok(match rows.next()? {
        Some(row) => Some(row.get(0)?),
        None => None,
    })
}

pub fn set_config(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO config (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_session(conn: &Connection, date: &str) -> Result<Option<Session>> {
    let mut stmt = conn.prepare(
        "SELECT date, concept_id, status, current_step, quiz_score, started_at, completed_at, reading_seconds,
                session_type, plan_reason, focus
         FROM sessions WHERE date = ?1",
    )?;
    let mut rows = stmt.query(params![date])?;
    Ok(match rows.next()? {
        Some(r) => Some(Session {
            date: r.get(0)?,
            concept_id: r.get(1)?,
            status: r.get(2)?,
            current_step: r.get(3)?,
            quiz_score: r.get(4)?,
            started_at: r.get(5)?,
            completed_at: r.get(6)?,
            reading_seconds: r.get(7)?,
            session_type: r.get(8)?,
            plan_reason: r.get(9)?,
            focus: r.get(10)?,
        }),
        None => None,
    })
}

pub fn upsert_session(conn: &Connection, s: &Session) -> Result<()> {
    conn.execute(
        "INSERT INTO sessions (date, concept_id, status, current_step, quiz_score, started_at, completed_at, reading_seconds, session_type, plan_reason, focus)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(date) DO UPDATE SET
            concept_id = excluded.concept_id,
            status = excluded.status,
            current_step = excluded.current_step,
            quiz_score = excluded.quiz_score,
            started_at = excluded.started_at,
            completed_at = excluded.completed_at,
            reading_seconds = excluded.reading_seconds,
            session_type = excluded.session_type,
            plan_reason = excluded.plan_reason,
            focus = CASE
                WHEN sessions.focus != '' AND excluded.focus != '' AND sessions.focus != excluded.focus
                THEN sessions.focus
                WHEN sessions.focus != '' THEN sessions.focus
                ELSE excluded.focus
            END",
        params![
            s.date, s.concept_id, s.status, s.current_step,
            s.quiz_score, s.started_at, s.completed_at, s.reading_seconds,
            s.session_type, s.plan_reason, s.focus
        ],
    )?;
    Ok(())
}

/// Stable identity at the legacy primary boundary. Its eventual shared-runtime
/// import must reuse this ID, including for original pre-upgrade records.
pub fn primary_session_id(conn: &Connection, date: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT session_id FROM primary_session_ids WHERE legacy_date=?1",
            [date],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn primary_session_by_id(conn: &Connection, id: &str) -> Result<Session> {
    let date: String = conn
        .query_row(
            "SELECT legacy_date FROM primary_session_ids WHERE session_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            DbError::Invalid("This study session is unavailable. Reload its saved state.".into())
        })?;
    get_session(conn, &date)?
        .ok_or_else(|| DbError::Invalid("The saved primary session is missing.".into()))
}

/// Resume unfinished work before considering a new calendar day. Older legacy
/// builds could leave multiple days active; retain all rows and resume the most
/// recently started one instead of silently assigning its work to today's row.
pub fn current_primary_session(conn: &Connection, today: &str) -> Result<Option<Session>> {
    let date: Option<String> = conn.query_row("SELECT date FROM sessions WHERE status='in_progress' AND date<=?1 ORDER BY date DESC LIMIT 1",[today],|r|r.get(0)).optional()?;
    get_session(conn, date.as_deref().unwrap_or(today))
}

/// Timers persist only to the captured still-active reading session. A finished
/// or skipped session cannot receive another timer tick after navigation.
pub fn save_primary_reading(conn: &Connection, id: &str, seconds: i64) -> Result<bool> {
    if seconds < 0 {
        return Err(DbError::Invalid("Reading time cannot be negative.".into()));
    }
    Ok(conn.execute("UPDATE sessions SET reading_seconds=MAX(reading_seconds,?2) WHERE date=(SELECT legacy_date FROM primary_session_ids WHERE session_id=?1) AND status='in_progress' AND current_step='course'",params![id,seconds])?==1)
}

/// Explicitly change the track when a finished day starts a new voluntary
/// session. Normal in-progress upserts intentionally cannot change focus.
pub fn set_session_focus(conn: &Connection, date: &str, focus: &str) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET focus = ?2 WHERE date = ?1",
        params![date, focus],
    )?;
    Ok(())
}

pub fn insert_course(
    conn: &Connection,
    session_date: &str,
    concept_id: i64,
    markdown: &str,
    resources_json: &str,
    source: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO courses (session_date, concept_id, markdown, resources_json, source, generated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
        params![session_date, concept_id, markdown, resources_json, source],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_course(conn: &Connection, course_id: i64) -> Result<Option<Course>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_date, concept_id, markdown, resources_json, source
         FROM courses WHERE id = ?1",
    )?;
    let mut rows = stmt.query(params![course_id])?;
    Ok(match rows.next()? {
        Some(r) => Some(Course {
            id: r.get(0)?,
            session_date: r.get(1)?,
            concept_id: r.get(2)?,
            markdown: r.get(3)?,
            resources_json: r.get(4)?,
            source: r.get(5)?,
        }),
        None => None,
    })
}

pub fn course_for_date(conn: &Connection, session_date: &str) -> Result<Option<Course>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_date, concept_id, markdown, resources_json, source
         FROM courses WHERE session_date = ?1 ORDER BY id DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![session_date])?;
    Ok(match rows.next()? {
        Some(r) => Some(Course {
            id: r.get(0)?,
            session_date: r.get(1)?,
            concept_id: r.get(2)?,
            markdown: r.get(3)?,
            resources_json: r.get(4)?,
            source: r.get(5)?,
        }),
        None => None,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourseExercise {
    pub course_id: i64,
    pub title: String,
    pub instructions: String,
    pub starter_code: Option<String>,
    pub deliverable: Option<String>,
    pub hints: Vec<String>,
}

/// One exercise exactly as the workspace renders it, owned by a primary
/// course or a classroom session (exactly one of the two).
#[derive(Debug, Clone, Serialize)]
pub struct ExerciseView {
    pub course_id: Option<i64>,
    pub classroom_session_id: Option<i64>,
    /// Shared-runtime lesson whose checkpoint holds the draft and completion.
    pub study_session_id: Option<String>,
    pub title: String,
    pub instructions: String,
    pub starter_code: Option<String>,
    pub deliverable: Option<String>,
    pub hints: Vec<String>,
    pub draft: Option<String>,
    pub completed: bool,
    pub reflection: String,
}

/// Upserts so re-generating a course (rare, but possible on retry) never
/// leaves two exercises for one course.
pub fn upsert_course_exercise(
    conn: &Connection,
    course_id: i64,
    title: &str,
    instructions: &str,
    starter_code: Option<&str>,
    deliverable: Option<&str>,
    hints: &[String],
) -> Result<()> {
    let hints_json = serde_json::to_string(hints)?;
    conn.execute(
        "INSERT INTO course_exercises (course_id, title, instructions, starter_code, deliverable, hints_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(course_id) DO UPDATE SET
            title = excluded.title,
            instructions = excluded.instructions,
            starter_code = excluded.starter_code,
            deliverable = excluded.deliverable,
            hints_json = excluded.hints_json",
        params![course_id, title, instructions, starter_code, deliverable, hints_json],
    )?;
    Ok(())
}

pub fn get_course_exercise(conn: &Connection, course_id: i64) -> Result<Option<CourseExercise>> {
    let mut stmt = conn.prepare(
        "SELECT course_id, title, instructions, starter_code, deliverable, hints_json
         FROM course_exercises WHERE course_id = ?1",
    )?;
    let mut rows = stmt.query(params![course_id])?;
    Ok(match rows.next()? {
        Some(r) => {
            let hints_json: String = r.get(5)?;
            Some(CourseExercise {
                course_id: r.get(0)?,
                title: r.get(1)?,
                instructions: r.get(2)?,
                starter_code: r.get(3)?,
                deliverable: r.get(4)?,
                hints: serde_json::from_str(&hints_json).unwrap_or_default(),
            })
        }
        None => None,
    })
}

fn exercise_owner(course_id: Option<i64>, classroom_session_id: Option<i64>) -> Result<()> {
    if course_id.is_some() == classroom_session_id.is_some() {
        return Err("exercise owner must be exactly one of course or classroom session".into());
    }
    Ok(())
}

pub fn save_exercise_draft(
    conn: &Connection,
    course_id: Option<i64>,
    classroom_session_id: Option<i64>,
    draft: &str,
) -> Result<()> {
    exercise_owner(course_id, classroom_session_id)?;
    if let Some(id) = classroom_session_id {
        let changed = conn.execute(
            "INSERT INTO exercise_drafts (classroom_session_id, draft, updated_at)
             VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(classroom_session_id) DO UPDATE SET
                draft = excluded.draft,
                updated_at = excluded.updated_at",
            params![id, draft],
        )?;
        if changed == 0 {
            return Err("classroom session not found".into());
        }
        return Ok(());
    }
    conn.execute(
        "INSERT INTO exercise_drafts (course_id, draft, updated_at)
         VALUES (?1, ?2, datetime('now'))
         ON CONFLICT(course_id) DO UPDATE SET
            draft = excluded.draft,
            updated_at = excluded.updated_at",
        params![course_id, draft],
    )?;
    Ok(())
}

pub fn get_exercise_draft(
    conn: &Connection,
    course_id: Option<i64>,
    classroom_session_id: Option<i64>,
) -> Result<Option<String>> {
    exercise_owner(course_id, classroom_session_id)?;
    let row = if let Some(id) = classroom_session_id {
        conn.query_row(
            "SELECT draft FROM exercise_drafts WHERE classroom_session_id = ?1",
            [id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    } else {
        conn.query_row(
            "SELECT draft FROM exercise_drafts WHERE course_id = ?1",
            [course_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    };
    Ok(row.filter(|draft| !draft.is_empty()))
}

pub fn save_exercise_completion(
    conn: &Connection,
    course_id: Option<i64>,
    classroom_session_id: Option<i64>,
    completed: bool,
    reflection: &str,
) -> Result<()> {
    exercise_owner(course_id, classroom_session_id)?;
    if let Some(id) = classroom_session_id {
        let changed = conn.execute(
            "INSERT INTO exercise_drafts
                (classroom_session_id, draft, completed, reflection, updated_at)
             VALUES (?1, '', ?2, ?3, datetime('now'))
             ON CONFLICT(classroom_session_id) DO UPDATE SET
                completed = excluded.completed,
                reflection = excluded.reflection,
                updated_at = excluded.updated_at",
            params![id, completed as i64, reflection.trim()],
        )?;
        if changed == 0 {
            return Err("classroom session not found".into());
        }
        return Ok(());
    }
    conn.execute(
        "INSERT INTO exercise_drafts (course_id, draft, completed, reflection, updated_at)
         VALUES (?1, '', ?2, ?3, datetime('now'))
         ON CONFLICT(course_id) DO UPDATE SET
            completed = excluded.completed,
            reflection = excluded.reflection,
            updated_at = excluded.updated_at",
        params![course_id, completed as i64, reflection.trim()],
    )?;
    Ok(())
}

pub fn get_exercise_completion(
    conn: &Connection,
    course_id: Option<i64>,
    classroom_session_id: Option<i64>,
) -> Result<(bool, String)> {
    exercise_owner(course_id, classroom_session_id)?;
    let row = if let Some(id) = classroom_session_id {
        conn.query_row(
            "SELECT completed, reflection FROM exercise_drafts
             WHERE classroom_session_id = ?1",
            [id],
            |row| Ok((row.get::<_, i64>(0)? != 0, row.get(1)?)),
        )
        .optional()?
    } else {
        conn.query_row(
            "SELECT completed, reflection FROM exercise_drafts WHERE course_id = ?1",
            [course_id],
            |row| Ok((row.get::<_, i64>(0)? != 0, row.get(1)?)),
        )
        .optional()?
    };
    Ok(row.unwrap_or((false, String::new())))
}

pub fn insert_question(
    conn: &Connection,
    course_id: i64,
    prompt: &str,
    kind: &str,
    choices_json: Option<&str>,
    correct_answer: &str,
    explanation: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO questions (course_id, prompt, kind, choices_json, correct_answer, explanation)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            course_id,
            prompt,
            kind,
            choices_json,
            correct_answer,
            explanation
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn questions_for_course(conn: &Connection, course_id: i64) -> Result<Vec<Question>> {
    let mut stmt = conn.prepare(
        "SELECT id, course_id, prompt, kind, choices_json, correct_answer, explanation, origin
         FROM questions WHERE course_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map(params![course_id], row_to_question)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn row_to_question(r: &rusqlite::Row) -> std::result::Result<Question, rusqlite::Error> {
    Ok(Question {
        id: r.get(0)?,
        course_id: r.get(1)?,
        prompt: r.get(2)?,
        kind: r.get(3)?,
        choices_json: r.get(4)?,
        correct_answer: r.get(5)?,
        explanation: r.get(6)?,
        origin: r.get(7)?,
    })
}

/// Today's quiz = carryover due today + fresh questions from the most recent
/// course in this focus, so switching tracks never strands an untested lesson.
pub fn quiz_for_date(
    conn: &Connection,
    date: &str,
    _yesterday: &str,
    focus: &str,
) -> Result<Vec<Question>> {
    let mut out: Vec<Question> = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT q.id, q.course_id, q.prompt, q.kind, q.choices_json, q.correct_answer, q.explanation, 'carryover'
         FROM carryover c JOIN questions q ON q.id = c.question_id
         JOIN courses co ON co.id = q.course_id
         JOIN concepts cpt ON cpt.id = co.concept_id
         WHERE c.scheduled_for <= ?1 AND cpt.focus = ?2
         ORDER BY c.failed_on",
    )?;
    let rows = stmt.query_map(params![date, focus], row_to_question)?;
    for q in rows {
        out.push(q?);
    }
    let mut stmt = conn.prepare(
        "SELECT q.id, q.course_id, q.prompt, q.kind, q.choices_json, q.correct_answer, q.explanation, q.origin
         FROM questions q
         JOIN courses co ON co.id = q.course_id
         JOIN concepts cpt ON cpt.id = co.concept_id
         WHERE co.id = (
             SELECT co2.id
             FROM courses co2
             JOIN concepts cpt2 ON cpt2.id = co2.concept_id
             WHERE co2.session_date < ?1 AND cpt2.focus = ?2
             ORDER BY co2.session_date DESC, co2.id DESC
             LIMIT 1
         )
           AND cpt.focus = ?2
           AND q.id NOT IN (SELECT question_id FROM carryover)
           AND q.id NOT IN (SELECT question_id FROM attempts)
         ORDER BY q.id",
    )?;
    let rows = stmt.query_map(params![date, focus], row_to_question)?;
    for q in rows {
        out.push(q?);
    }
    Ok(out)
}

/// Adaptive exit-check questions, grouped into rounds and deliberately
/// separate from `questions` (tomorrow's quiz).
#[allow(clippy::too_many_arguments)]
pub fn insert_exit_question(
    conn: &Connection,
    course_id: i64,
    round: i64,
    prompt: &str,
    choices_json: &str,
    correct_answer: &str,
    explanation: &str,
    section: &str,
    learning_objective: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO exit_questions (course_id, round, prompt, choices_json, correct_answer, explanation, section, learning_objective)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![course_id, round, prompt, choices_json, correct_answer, explanation, section, learning_objective],
    )?;
    Ok(conn.last_insert_rowid())
}

#[derive(Debug, Clone, Serialize)]
pub struct ExitQuestion {
    pub id: i64,
    pub prompt: String,
    pub choices: Vec<String>,
    pub correct_answer: String,
    pub explanation: String,
    pub section: String,
    pub learning_objective: String,
}

pub fn exit_questions_for_course(
    conn: &Connection,
    course_id: i64,
    round: i64,
) -> Result<Vec<ExitQuestion>> {
    let mut stmt = conn.prepare(
        "SELECT id, prompt, choices_json, correct_answer, explanation, section, learning_objective
         FROM exit_questions WHERE course_id = ?1 AND round = ?2 ORDER BY id",
    )?;
    let rows = stmt.query_map(params![course_id, round], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, prompt, choices_json, correct_answer, explanation, section, learning_objective) =
            row?;
        out.push(ExitQuestion {
            id,
            prompt,
            choices: serde_json::from_str(&choices_json).unwrap_or_default(),
            correct_answer,
            explanation,
            section,
            learning_objective,
        });
    }
    Ok(out)
}

pub fn all_exit_question_prompts(conn: &Connection, course_id: i64) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT prompt FROM exit_questions WHERE course_id = ?1 ORDER BY id")?;
    let rows = stmt.query_map(params![course_id], |row| row.get(0))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Durable audit trail of every graded exit-check answer — which round,
/// which learning area, right or wrong. Not on the read path for
/// generating the next round (that uses the compact `exit_failed_areas`
/// config key for speed) but keeps a full history for future dossier use.
#[allow(clippy::too_many_arguments)]
pub fn insert_exit_attempt(
    conn: &Connection,
    course_id: i64,
    question_id: i64,
    round: i64,
    correct: bool,
    user_answer: &str,
    section: &str,
    learning_objective: &str,
    misconception: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO exit_attempts (course_id, question_id, round, correct, user_answer, section, learning_objective, misconception, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))",
        params![
            course_id,
            question_id,
            round,
            correct as i64,
            user_answer,
            section,
            learning_objective,
            misconception
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Breadth sample for a pop-quiz day: previously-attempted questions from
/// quizzed concepts, prioritizing struggling/decayed then review-due, random
/// within a band. Excludes anything already in today's base set.
pub fn pop_quiz_sample(
    conn: &Connection,
    date: &str,
    focus: &str,
    exclude: &[i64],
    limit: i64,
) -> Result<Vec<Question>> {
    let exclude_csv = exclude
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT q.id, q.course_id, q.prompt, q.kind, q.choices_json, q.correct_answer, q.explanation, q.origin
         FROM questions q
         JOIN courses co ON co.id = q.course_id
         JOIN concepts cpt ON cpt.id = co.concept_id
         JOIN mastery m ON m.concept_id = co.concept_id
         WHERE cpt.focus = ?3
           AND q.id IN (SELECT DISTINCT question_id FROM attempts)
           AND q.id NOT IN (SELECT question_id FROM carryover)
           {}
         ORDER BY CASE
             WHEN m.state IN ('struggling','decayed') THEN 0
             WHEN m.next_review_date IS NOT NULL AND m.next_review_date <= ?1 THEN 1
             ELSE 2 END,
           RANDOM()
         LIMIT ?2",
        if exclude_csv.is_empty() { String::new() } else { format!("AND q.id NOT IN ({exclude_csv})") }
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![date, limit, focus], row_to_question)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// A small daily spaced-retrieval sample. Unlike a pop quiz, this only pulls
/// concepts that are struggling/decayed or whose review date is due; it never
/// adds random breadth just to make the quiz longer.
pub fn spaced_review_sample(
    conn: &Connection,
    date: &str,
    focus: &str,
    exclude: &[i64],
    limit: i64,
) -> Result<Vec<Question>> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let exclude_csv = exclude
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let exclusion = if exclude_csv.is_empty() {
        String::new()
    } else {
        format!("AND q.id NOT IN ({exclude_csv})")
    };
    let sql = format!(
        "SELECT q.id, q.course_id, q.prompt, q.kind, q.choices_json, q.correct_answer, q.explanation, q.origin
         FROM questions q
         JOIN courses co ON co.id = q.course_id
         JOIN concepts cpt ON cpt.id = co.concept_id
         JOIN mastery m ON m.concept_id = co.concept_id
         WHERE cpt.focus = ?2
           AND q.id IN (SELECT DISTINCT question_id FROM attempts)
           AND q.id NOT IN (SELECT question_id FROM carryover)
           AND (m.state IN ('struggling','decayed')
                OR (m.next_review_date IS NOT NULL AND m.next_review_date <= ?1))
           {exclusion}
         ORDER BY CASE WHEN m.state IN ('struggling','decayed') THEN 0 ELSE 1 END,
                  m.next_review_date,
                  RANDOM()
         LIMIT ?3"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![date, focus, limit], row_to_question)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn record_attempt(conn: &Connection, a: &Attempt) -> Result<()> {
    conn.execute(
        "INSERT INTO attempts (question_id, session_date, user_answer, correct, grader_feedback)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            a.question_id,
            a.session_date,
            a.user_answer,
            a.correct as i64,
            a.grader_feedback
        ],
    )?;
    Ok(())
}

pub fn attempts_for_session(conn: &Connection, date: &str) -> Result<Vec<Attempt>> {
    let mut stmt = conn.prepare(
        "SELECT question_id, session_date, user_answer, correct, grader_feedback
         FROM attempts WHERE session_date = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map(params![date], |r| {
        Ok(Attempt {
            question_id: r.get(0)?,
            session_date: r.get(1)?,
            user_answer: r.get(2)?,
            correct: r.get::<_, i64>(3)? != 0,
            grader_feedback: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Question failed today: schedule (or reschedule) it for tomorrow.
pub fn push_carryover(
    conn: &Connection,
    question_id: i64,
    today: &str,
    tomorrow: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO carryover (question_id, failed_on, scheduled_for)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(question_id) DO UPDATE SET
            times_failed = times_failed + 1,
            failed_on = excluded.failed_on,
            scheduled_for = excluded.scheduled_for",
        params![question_id, today, tomorrow],
    )?;
    Ok(())
}

pub fn clear_carryover(conn: &Connection, question_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM carryover WHERE question_id = ?1",
        params![question_id],
    )?;
    Ok(())
}

pub fn carryover_count(conn: &Connection, due_by: &str, focus: &str) -> Result<i64> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM carryover c
         JOIN questions q ON q.id = c.question_id
         JOIN courses co ON co.id = q.course_id
         JOIN concepts cpt ON cpt.id = co.concept_id
         WHERE c.scheduled_for <= ?1 AND cpt.focus = ?2",
        params![due_by, focus],
        |r| r.get(0),
    )?;
    Ok(n)
}

/// Pool restricted to least-picked active concepts in a focus track.
pub fn roulette_pool(conn: &Connection, focus: &str) -> Result<Vec<Concept>> {
    let mut stmt = conn.prepare(
        "SELECT id, slug, title, category, weight, times_picked, last_picked_date,
                tier, focus, brief_json
         FROM concepts
         WHERE active = 1 AND focus = ?1
           AND times_picked = (SELECT MIN(times_picked) FROM concepts WHERE active = 1 AND focus = ?1)",
    )?;
    let rows = stmt.query_map(params![focus], row_to_concept)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn all_concepts(conn: &Connection, focus: &str) -> Result<Vec<Concept>> {
    let mut stmt = conn.prepare(
        "SELECT id, slug, title, category, weight, times_picked, last_picked_date,
                tier, focus, brief_json
         FROM concepts WHERE active = 1 AND focus = ?1 ORDER BY title",
    )?;
    let rows = stmt.query_map(params![focus], row_to_concept)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn row_to_concept(r: &rusqlite::Row) -> std::result::Result<Concept, rusqlite::Error> {
    let brief_json: String = r.get(9)?;
    Ok(Concept {
        id: r.get(0)?,
        slug: r.get(1)?,
        title: r.get(2)?,
        category: r.get(3)?,
        weight: r.get(4)?,
        times_picked: r.get(5)?,
        last_picked_date: r.get(6)?,
        tier: r.get(7)?,
        focus: r.get(8)?,
        curriculum: serde_json::from_str(&brief_json).unwrap_or_default(),
    })
}

pub fn get_concept(conn: &Connection, id: i64) -> Result<Option<Concept>> {
    let mut stmt = conn.prepare(
        "SELECT id, slug, title, category, weight, times_picked, last_picked_date,
                tier, focus, brief_json
         FROM concepts WHERE id = ?1",
    )?;
    let mut rows = stmt.query(params![id])?;
    Ok(match rows.next()? {
        Some(r) => Some(row_to_concept(r)?),
        None => None,
    })
}

pub fn mark_concept_picked(conn: &Connection, id: i64, date: &str) -> Result<()> {
    conn.execute(
        "UPDATE concepts SET times_picked = times_picked + 1, last_picked_date = ?2 WHERE id = ?1",
        params![id, date],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub date: String,
    pub status: String,
    pub quiz_score: Option<f64>,
    pub concept_title: Option<String>,
    pub focus: String,
}

pub fn history(conn: &Connection, limit: i64) -> Result<Vec<HistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT s.date, s.status, s.quiz_score,
                CASE WHEN s.status = 'pending' THEN NULL ELSE c.title END,
                s.focus
         FROM sessions s LEFT JOIN concepts c ON c.id = s.concept_id
         WHERE s.status != 'pending' OR s.date <= date('now', 'localtime')
         ORDER BY s.date DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| {
        Ok(HistoryEntry {
            date: r.get(0)?,
            status: r.get(1)?,
            quiz_score: r.get(2)?,
            concept_title: r.get(3)?,
            focus: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Consecutive completed days ending today or yesterday. A day counts if
/// *any* subject was completed that day — the primary session, or an
/// advisory classroom/language class. This only feeds the cosmetic uptime
/// badge; it never affects the primary session's owed/kiosk-lock state.
pub fn streak(conn: &Connection, today: &str) -> Result<i64> {
    let mut stmt = conn.prepare(
        "SELECT date FROM (
            SELECT date FROM sessions WHERE status = 'completed'
            UNION
            SELECT session_date AS date FROM classroom_sessions WHERE status = 'completed'
            UNION
            SELECT session_date AS date FROM language_sessions WHERE status = 'completed'
         ) ORDER BY date DESC",
    )?;
    let dates: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let today_d = match chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Ok(0),
    };
    let mut streak = 0i64;
    let mut expect = today_d;
    for ds in &dates {
        let d = match chrono::NaiveDate::parse_from_str(ds, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => continue,
        };
        if streak == 0 && d == today_d.pred_opt().unwrap_or(today_d) {
            expect = d;
        }
        if d == expect {
            streak += 1;
            expect = d.pred_opt().unwrap_or(d);
        } else if d < expect {
            break;
        }
    }
    Ok(streak)
}

pub mod jobs {
    use super::*;

    pub fn enqueue(conn: &Connection, kind: &str, target_date: &str) -> Result<()> {
        conn.execute(
            "INSERT INTO generation_jobs (kind, target_date, status, created_at)
             VALUES (?1, ?2, 'queued', datetime('now'))
             ON CONFLICT(kind, target_date) DO UPDATE SET
                status = CASE WHEN generation_jobs.status = 'done' THEN 'done' ELSE 'queued' END",
            params![kind, target_date],
        )?;
        Ok(())
    }

    /// Force a job back to queued even if it already ran (used when an
    /// extended session adds a course after tomorrow's quiz was generated).
    pub fn requeue(conn: &Connection, kind: &str, target_date: &str) -> Result<()> {
        conn.execute(
            "INSERT INTO generation_jobs (kind, target_date, status, created_at)
             VALUES (?1, ?2, 'queued', datetime('now'))
             ON CONFLICT(kind, target_date) DO UPDATE SET status = 'queued', attempts = 0",
            params![kind, target_date],
        )?;
        Ok(())
    }

    pub fn next_queued(conn: &Connection) -> Result<Option<(i64, String, String)>> {
        let mut stmt = conn.prepare(
            "SELECT id, kind, target_date FROM generation_jobs
             WHERE status = 'queued' AND attempts < 3 ORDER BY id LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        Ok(match rows.next()? {
            Some(r) => Some((r.get(0)?, r.get(1)?, r.get(2)?)),
            None => None,
        })
    }

    /// Retiring the daily routine must not run its queued future appointments.
    /// Preserve those rows for import/history while servicing only saved work.
    pub fn next_for_active_legacy_session(
        conn: &Connection,
    ) -> Result<Option<(i64, String, String)>> {
        conn.query_row(
            "SELECT j.id, j.kind, j.target_date FROM generation_jobs j
             JOIN sessions s ON s.date = j.target_date
             WHERE j.status = 'queued' AND j.attempts < 3
               AND s.status = 'in_progress'
             ORDER BY j.id LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn mark(conn: &Connection, id: i64, status: &str, error: Option<&str>) -> Result<()> {
        conn.execute(
            "UPDATE generation_jobs SET status = ?2, error = ?3,
                attempts = attempts + CASE WHEN ?2 IN ('failed','done') THEN 1 ELSE 0 END,
                finished_at = CASE WHEN ?2 IN ('failed','done') THEN datetime('now') ELSE finished_at END
             WHERE id = ?1",
            params![id, status, error],
        )?;
        Ok(())
    }
}
