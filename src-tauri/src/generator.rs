use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error("agent binary not found")]
    NoBinary,
    #[error("agent timed out after {0:?}")]
    Timeout(Duration),
    #[error("agent exited with status {0}: {1}")]
    BadExit(i32, String),
    #[error("could not parse agent output: {0}")]
    Parse(String),
    #[error("lesson did not pass review: {0}")]
    Quality(String),
    #[error("provider returned an unusable response ({model}): {reason}")]
    ProviderResponse {
        model: String,
        reason: String,
        usage: Box<crate::agents::Usage>,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("provider API error: {0}")]
    Api(String),
}

pub type Result<T> = std::result::Result<T, GenError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub title: String,
    pub url: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub why: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedCourse {
    pub title: String,
    pub markdown: String,
    #[serde(default)]
    pub resources: Vec<Resource>,
    #[serde(default)]
    pub key_takeaways: Vec<String>,
    /// Round-one MCQs that gate early exit from the reader (optional in the
    /// payload; generated on demand if the model omits them).
    #[serde(default)]
    pub exit_questions: Vec<ExitCheck>,
    /// Structured, non-blocking exercise shown alongside the reader.
    #[serde(default)]
    pub exercise: Option<Exercise>,
    /// What the editorial audit still wanted changed when the corrections
    /// ran out. The lesson is published with these shown to the learner rather
    /// than discarded, because a lesson with caveats beats no lesson. The
    /// writer never emits this field, so it stays out of the metadata contract.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review_notes: Vec<String>,
    /// Set when the lesson was written without retrieved documentation, so
    /// the reader is told its claims were not checked against the sources.
    /// The writer never emits this field either.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub research_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CourseQualityScores {
    coverage_depth: u8,
    mechanism_depth: u8,
    specificity: u8,
    production_transfer: u8,
    dossier_adherence: u8,
    exercise_alignment: u8,
    source_discipline: u8,
}

impl CourseQualityScores {
    /// Below this on any axis the editor's complaint is about substance, not
    /// polish, and the lesson is not published even with notes.
    const PUBLISHABLE_FLOOR: u8 = 3;

    fn publishable_with_notes(&self) -> bool {
        [
            self.coverage_depth,
            self.mechanism_depth,
            self.specificity,
            self.production_transfer,
            self.dossier_adherence,
            self.exercise_alignment,
            self.source_discipline,
        ]
        .into_iter()
        .all(|score| score >= Self::PUBLISHABLE_FLOOR)
    }

    fn validate(&self) -> std::result::Result<(), String> {
        let scores = [
            ("coverage depth", self.coverage_depth),
            ("mechanism depth", self.mechanism_depth),
            ("specificity", self.specificity),
            ("production transfer", self.production_transfer),
            ("dossier adherence", self.dossier_adherence),
            ("exercise alignment", self.exercise_alignment),
            ("source discipline", self.source_discipline),
        ];
        let weak = scores
            .into_iter()
            .filter(|(_, score)| *score < 4 || *score > 5)
            .map(|(name, score)| format!("{name}={score}"))
            .collect::<Vec<_>>();
        if weak.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "editor rated the submitted course below the 4/5 quality floor: {}",
                weak.join(", ")
            ))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CourseEditorialReview {
    scores: CourseQualityScores,
    /// Blocking issues in the submitted draft, not a re-emission of its prose.
    issues: Vec<CourseEditorialIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CourseEditorialIssue {
    section: String,
    reason: String,
}

/// Keep long Markdown out of model-authored JSON. Quotes, shell escapes and
/// fenced examples remain prose; only the small machine-consumed part is JSON.
#[derive(Debug, Serialize, Deserialize)]
struct CourseMetadata {
    title: String,
    #[serde(default)]
    resources: Vec<Resource>,
    #[serde(default)]
    key_takeaways: Vec<String>,
    exit_questions: Vec<ExitCheck>,
    exercise: Exercise,
}

impl CourseMetadata {
    fn with_markdown(self, markdown: String) -> GeneratedCourse {
        GeneratedCourse {
            title: self.title,
            markdown,
            resources: self.resources,
            key_takeaways: self.key_takeaways,
            exit_questions: self.exit_questions,
            exercise: Some(self.exercise),
            review_notes: Vec::new(),
            research_note: None,
        }
    }
}

fn object_schema(properties: serde_json::Value) -> serde_json::Value {
    let required: Vec<_> = properties
        .as_object()
        .expect("schema properties")
        .keys()
        .cloned()
        .collect();
    serde_json::json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

fn course_metadata_schema() -> serde_json::Value {
    use serde_json::json;
    let text = json!({"type":"string"});
    let strings = json!({"type":"array","items":text});
    let resource = object_schema(json!({"title":text,"url":text,"type":text,"why":text}));
    let question = object_schema(
        json!({"prompt":text,"choices":{"type":"array","items":text,"minItems":4,"maxItems":4},"correct_answer":text,"explanation":text,"section":text,"learning_objective":text}),
    );
    let exercise = object_schema(
        json!({"title":text,"instructions":text,"starter_code":{"type":["string","null"]},"deliverable":text,"hints":{"type":"array","items":text,"minItems":1,"maxItems":3}}),
    );
    object_schema(
        json!({"title":text,"resources":{"type":"array","items":resource},"key_takeaways":strings,"exit_questions":{"type":"array","items":question,"minItems":5,"maxItems":5},"exercise":exercise}),
    )
}

fn course_audit_schema() -> serde_json::Value {
    let score = serde_json::json!({"type":"integer","minimum":1,"maximum":5});
    let scores = object_schema(
        serde_json::json!({"coverage_depth":score,"mechanism_depth":score,"specificity":score,"production_transfer":score,"dossier_adherence":score,"exercise_alignment":score,"source_discipline":score}),
    );
    let mut targets = REQUIRED_COURSE_SECTION_TITLES.to_vec();
    targets.push("Assessment");
    let issue = object_schema(
        serde_json::json!({"section":{"type":"string","enum":targets},"reason":{"type":"string"}}),
    );
    object_schema(serde_json::json!({"scores":scores,"issues":{"type":"array","items":issue}}))
}

/// One string per listed section title, nothing else.
fn section_hints_schema(titles: &[&str]) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    for title in titles {
        properties.insert((*title).to_string(), serde_json::json!({"type":"string"}));
    }
    object_schema(serde_json::Value::Object(properties))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exercise {
    pub title: String,
    pub instructions: String,
    #[serde(default)]
    pub starter_code: Option<String>,
    #[serde(default)]
    pub deliverable: Option<String>,
    #[serde(default)]
    pub hints: Vec<String>,
}

/// Courses may carry either the current `## Practical exercise` heading or
/// the legacy `## Tool-building exercise` heading. Extract either one so the
/// workspace still works for old generated content without constraining new
/// architecture exercises to be CLI/tool-shaped.
pub fn extract_tool_building_exercise(markdown: &str) -> Option<Exercise> {
    let (start, marker, title) = [
        ("## Practical exercise", "Practical exercise"),
        ("## Tool-building exercise", "Tool-building exercise"),
    ]
    .into_iter()
    .filter_map(|(marker, title)| markdown.find(marker).map(|start| (start, marker, title)))
    .min_by_key(|(start, _, _)| *start)?;
    let after_heading = &markdown[start + marker.len()..];
    let end = after_heading.find("\n## ").unwrap_or(after_heading.len());
    let body = after_heading[..end].trim();
    if body.is_empty() {
        return None;
    }
    Some(Exercise {
        title: title.to_string(),
        instructions: body.to_string(),
        starter_code: None,
        deliverable: None,
        hints: Vec::new(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitCheck {
    pub prompt: String,
    pub choices: Vec<String>,
    pub correct_answer: String,
    #[serde(default)]
    pub explanation: String,
    /// The course's ## heading this question is drawn from — powers
    /// targeted remediation on a miss (empty for older/legacy content).
    #[serde(default)]
    pub section: String,
    /// Short phrase naming the specific mechanism this question checks.
    #[serde(default)]
    pub learning_objective: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitChecks {
    pub questions: Vec<ExitCheck>,
}

/// One learning objective the student just missed, carried into the next
/// exit-check round so remediation stays targeted instead of merely bigger.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FailedArea {
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub learning_objective: String,
}

impl FailedArea {
    /// Human-readable label for the UI ("next round revisits: …").
    pub fn label(&self) -> String {
        match (self.section.trim(), self.learning_objective.trim()) {
            ("", "") => "the missed question".to_string(),
            (section, "") => section.to_string(),
            ("", objective) => objective.to_string(),
            (section, objective) => format!("{section}: {objective}"),
        }
    }
}

fn format_targeting(failed_areas: &[FailedArea]) -> String {
    if failed_areas.is_empty() {
        return "- Cover distinct parts of the course; distribute questions across its major sections.".to_string();
    }
    let list = failed_areas
        .iter()
        .enumerate()
        .map(|(index, area)| format!("{}. {}", index + 1, area.label()))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "- The student just missed these exact learning objectives from the previous round. EVERY question this round must test one of them, using a fresh scenario or example, never a reworded repeat of the missed question. Do not write questions about any other part of the course this round.\nMissed objectives to target:\n{list}"
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedQuestion {
    pub prompt: String,
    pub kind: String,
    #[serde(default)]
    pub choices: Option<Vec<String>>,
    pub correct_answer: String,
    pub explanation: String,
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub learning_objective: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedQuiz {
    pub questions: Vec<GeneratedQuestion>,
}

fn usable_mcq(
    prompt: &str,
    choices: Option<&[String]>,
    correct_answer: &str,
    explanation: &str,
) -> bool {
    validate_mcq(prompt, choices, correct_answer, explanation).is_ok()
}

fn validate_mcq(
    prompt: &str,
    choices: Option<&[String]>,
    correct_answer: &str,
    explanation: &str,
) -> std::result::Result<(), String> {
    if prompt.trim().is_empty() {
        return Err("prompt must be nonempty".into());
    }
    let choices = choices.ok_or("choices must be an array of four strings")?;
    if choices.len() != 4 || choices.iter().any(|choice| choice.trim().is_empty()) {
        return Err("choices must contain exactly four nonempty strings".into());
    }
    let distinct: std::collections::HashSet<_> = choices.iter().map(|c| c.trim()).collect();
    if distinct.len() != 4 {
        return Err("the four choices must be distinct".into());
    }
    if !distinct.contains(correct_answer.trim()) {
        return Err("correct_answer must equal the full text of exactly one choice, not its letter or index".into());
    }
    if explanation.trim().is_empty() {
        return Err("explanation must identify the misconception and correct mental model".into());
    }
    Ok(())
}

/// Deterministic quality floor for model-authored quizzes. The prompt is not
/// an enforcement mechanism; machine-consumed content is checked before it
/// can become a mastery signal.
pub fn validate_generated_quiz(questions: &[GeneratedQuestion]) -> std::result::Result<(), String> {
    if questions.len() != 5 {
        return Err(format!("expected 5 questions, got {}", questions.len()));
    }
    let mcq_count = questions
        .iter()
        .filter(|question| question.kind == "mcq")
        .count();
    let free_count = questions
        .iter()
        .filter(|question| question.kind == "free")
        .count();
    if (mcq_count, free_count) != (3, 2) {
        return Err(format!(
            "expected 3 mcq and 2 free questions, got {mcq_count} and {free_count}"
        ));
    }
    for (index, question) in questions.iter().enumerate() {
        if question.kind == "mcq" {
            if !usable_mcq(
                &question.prompt,
                question.choices.as_deref(),
                &question.correct_answer,
                &question.explanation,
            ) {
                return Err(format!(
                    "question {} is not a valid four-choice MCQ",
                    index + 1
                ));
            }
        } else if question.prompt.trim().is_empty()
            || question.correct_answer.trim().is_empty()
            || question.explanation.trim().is_empty()
            || question
                .choices
                .as_ref()
                .is_some_and(|choices| !choices.is_empty())
        {
            return Err(format!(
                "question {} is not a valid free response",
                index + 1
            ));
        }
    }
    let prose: String = questions
        .iter()
        .flat_map(|question| {
            [question.prompt.as_str(), question.explanation.as_str()]
                .into_iter()
                .chain(question.choices.iter().flatten().map(String::as_str))
        })
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(reason) = crate::prose::objection(&prose) {
        return Err(format!("quiz {reason}"));
    }
    Ok(())
}

/// Deterministic length window for a base lesson. The prompt targets
/// 1,500-2,100 words of point-form Markdown, read in about fourteen minutes
/// of a thirty-minute session; the floor allows natural variation without
/// accepting a lesson that skips its mechanism, and the ceiling rejects the
/// prose-heavy drafts that a learner could not finish inside the session.
const MIN_COURSE_WORDS: usize = 1_200;
const MAX_COURSE_WORDS: usize = 2_600;
/// A picture of the mental model and one of the production scenario, at
/// least: a lesson that is all prose is harder to take in than one that shows
/// its shape.
const MIN_COURSE_DIAGRAMS: usize = 2;

/// How much lesson a session's minutes deserve.
///
/// Thirty minutes is the base lesson: fourteen to read, ten to practise, six
/// to check. Up to an hour the lesson goes deeper: a larger word budget, so
/// the tutor adds a second worked trace and a fuller practical rather than
/// compressing. Past an hour more words would only be padding, so the scale
/// stops at one and a half times the base and the remaining time is spent on
/// further topics instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LessonBudget {
    /// The session's real length, which the lesson reports as its estimate.
    pub minutes: i64,
    /// Percent of the base word figures, 100 to 150.
    pub scale_percent: usize,
}

impl LessonBudget {
    pub const BASE_MINUTES: i64 = 30;
    pub const DEEPEST_MINUTES: i64 = 60;

    pub fn for_minutes(minutes: i64) -> Self {
        let depth = minutes.clamp(Self::BASE_MINUTES, Self::DEEPEST_MINUTES);
        let scale_percent = 100
            + ((depth - Self::BASE_MINUTES) * 50 / (Self::DEEPEST_MINUTES - Self::BASE_MINUTES))
                as usize;
        Self {
            minutes: minutes.max(1),
            scale_percent,
        }
    }
    fn scaled(&self, words: usize) -> usize {
        words * self.scale_percent / 100
    }
    pub fn min_words(&self) -> usize {
        self.scaled(MIN_COURSE_WORDS)
    }
    pub fn section_minimum(&self, index: usize) -> usize {
        self.scaled(MIN_COURSE_SECTION_WORDS[index])
    }
    /// The most words the gate accepts: past this a learner cannot finish
    /// the reading inside the session.
    pub fn max_words(&self) -> usize {
        self.scaled(MAX_COURSE_WORDS)
    }
    /// The range the prompt asks for, matching the base prompt's 1,500-2,100.
    pub fn target_words(&self) -> (usize, usize) {
        (self.scaled(1_500), self.scaled(2_100))
    }
    /// The budgeted depth in minutes: the session's length, held to the
    /// thirty-to-sixty window the word figures are scaled over.
    pub fn depth_minutes(&self) -> i64 {
        self.minutes
            .clamp(Self::BASE_MINUTES, Self::DEEPEST_MINUTES)
    }
    /// How the session's minutes divide between reading, practice and check.
    pub fn plan(&self) -> crate::lesson_shape::SessionPlan {
        crate::lesson_shape::SessionPlan::for_depth(self.depth_minutes())
    }
    /// Stated last in the prompt, so it overrides the base figures above it.
    pub fn prompt_line(&self) -> String {
        let (low, high) = self.target_words();
        let depth = self.depth_minutes();
        let plan = self.plan();
        format!(
            "LENGTH AND PACE FOR THIS SESSION (this overrides the general figures above): the learner has {depth} minutes in total, about {} to read, {} to practise and {} to answer the check. Write {low}-{high} words of point-form Markdown; a deterministic gate rejects anything below {} words or above {} words and scales every section minimum by {}%. The reading hints under the headings must add up to about {} minutes, and the practical exercise must fit inside {} minutes. Reach the length through substance, a further worked trace, a fuller practical, a production decision with its evidence, never through restatement, and never past the ceiling.",
            plan.learn_minutes,
            plan.practice_minutes,
            plan.check_minutes,
            self.min_words(),
            self.max_words(),
            self.scale_percent,
            plan.learn_minutes,
            plan.practice_minutes,
        )
    }
}

impl Default for LessonBudget {
    fn default() -> Self {
        Self::for_minutes(Self::BASE_MINUTES)
    }
}

const REQUIRED_COURSE_SECTION_TITLES: [&str; 10] = [
    "Why this matters",
    "The simple version",
    "Core mechanics",
    "Mental model",
    "Runnable experiment",
    "Production architecture lens",
    "Trade-offs and failure modes",
    "Migration and observability",
    "Practical exercise",
    "Key takeaways",
];

/// Same-provider correction rounds allowed before a course is rejected.
const MAX_QUALITY_CORRECTIONS: usize = 2;

/// How many primary documents to retrieve and quote per lesson.
const RESEARCH_SOURCE_TARGET: usize = 5;
/// Below this, retrieval is considered to have failed (offline, host outage)
/// rather than the course being ungrounded, so the citation gate is skipped.
const MIN_GROUNDED_SOURCES: usize = 2;
/// Distinct retrieved URLs a grounded course must cite inline. Kept at two on
/// purpose: retrieval sometimes includes a documentation index page, and a
/// course that ignores the vague source while attributing the specific ones is
/// behaving correctly, not cutting corners.
const MIN_INLINE_CITATIONS: usize = 2;

/// A total word count alone can be padded. These floors ensure the material
/// is distributed across explanation, mechanism, production transfer, failure
/// analysis and deliberate practice. They are floors for point form: a
/// section that meets one in bullets has said what it must.
const MIN_COURSE_SECTION_WORDS: [usize; 10] = [50, 90, 240, 90, 100, 160, 130, 100, 80, 40];

fn course_section_word_counts(markdown: &str) -> std::result::Result<[usize; 10], String> {
    let mut counts = [0; 10];
    let mut next_required = 0;
    let mut current: Option<usize> = None;
    let mut active_fence: Option<&str> = None;

    for line in markdown.lines() {
        let trimmed = line.trim();
        let fence = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(marker) = fence {
            if active_fence == Some(marker) {
                active_fence = None;
            } else if active_fence.is_none() {
                active_fence = Some(marker);
            }
        }

        if active_fence.is_none() {
            if let Some(index) = trimmed.strip_prefix("## ").and_then(|heading| {
                REQUIRED_COURSE_SECTION_TITLES
                    .iter()
                    .position(|title| heading == *title)
            }) {
                if index != next_required {
                    return Err(format!(
                        "course section ## {} is out of order; expected {}",
                        REQUIRED_COURSE_SECTION_TITLES[index],
                        REQUIRED_COURSE_SECTION_TITLES
                            .get(next_required)
                            .map(|title| format!("## {title}"))
                            .unwrap_or_else(|| "the end of the lesson".into())
                    ));
                }
                current = Some(index);
                next_required += 1;
                continue;
            }
        }

        if let Some(index) = current {
            counts[index] += line.split_whitespace().count();
        }
    }

    if next_required != REQUIRED_COURSE_SECTION_TITLES.len() {
        return Err(format!(
            "course missing required section ## {}",
            REQUIRED_COURSE_SECTION_TITLES[next_required]
        ));
    }
    Ok(counts)
}

fn normalize_course_headings(markdown: &str) -> String {
    let mut active_fence: Option<&str> = None;
    let mut normalized = Vec::new();
    let mut seen_sections = std::collections::BTreeSet::new();

    for line in markdown.lines() {
        let trimmed = line.trim_start();
        let fence = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(marker) = fence {
            if active_fence == Some(marker) {
                active_fence = None;
            } else if active_fence.is_none() {
                active_fence = Some(marker);
            }
            normalized.push(line.to_string());
            continue;
        }
        if active_fence.is_some() {
            normalized.push(line.to_string());
            continue;
        }

        let hash_count = trimmed.bytes().take_while(|byte| *byte == b'#').count();
        let title = if (1..=6).contains(&hash_count)
            && trimmed
                .as_bytes()
                .get(hash_count)
                .is_some_and(u8::is_ascii_whitespace)
        {
            trimmed[hash_count..].trim().trim_end_matches('#').trim()
        } else {
            ""
        };
        match REQUIRED_COURSE_SECTION_TITLES
            .iter()
            .find(|required| title.eq_ignore_ascii_case(required))
        {
            // A canonical title may only open its section once. A later repeat
            // is a subheading inside the prose; promoting it would duplicate a
            // section boundary and fail the ordering gate.
            Some(canonical) if seen_sections.insert(*canonical) => {
                normalized.push(format!("## {canonical}"));
            }
            Some(canonical) => normalized.push(format!("### {canonical}")),
            None => normalized.push(line.to_string()),
        }
    }

    let mut output = normalized.join("\n");
    if markdown.ends_with('\n') {
        output.push('\n');
    }
    output
}

/// Routing and wire format for one provider call.
struct ProviderCall<'a> {
    agent: &'a str,
    custom_bin: &'a str,
    model: &'a str,
    web_tools: bool,
    timeout: Duration,
    wire: Wire,
}

/// What shape of answer a provider call expects back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wire {
    /// A structured payload, parsed as JSON.
    Json,
    /// Markdown prose. Chosen for long writing: constrained JSON output makes
    /// DeepSeek answer at roughly a third of the length it otherwise writes.
    Prose,
}

/// Models sometimes wrap a whole markdown answer in one fence despite being
/// asked not to. Unwrap that, but never touch fences inside the content.
fn strip_markdown_fence(raw: &str) -> String {
    let trimmed = raw.trim();
    for marker in ["```", "~~~"] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            if let Some(body) = rest
                .split_once('\n')
                .map(|(_language, body)| body)
                .and_then(|body| body.trim_end().strip_suffix(marker))
            {
                return body.trim().to_string();
            }
        }
    }
    trimmed.to_string()
}

/// Split a course body into its required sections, keeping anything before the
/// first heading. Fenced code is opaque so a `##` inside it is not a heading.
fn split_course_sections(markdown: &str) -> (String, [String; 10]) {
    let mut preamble = String::new();
    let mut sections: [String; 10] = std::array::from_fn(|_| String::new());
    let mut current: Option<usize> = None;
    let mut next_required = 0;
    let mut active_fence: Option<&str> = None;

    for line in markdown.lines() {
        let trimmed = line.trim();
        let fence = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(marker) = fence {
            if active_fence == Some(marker) {
                active_fence = None;
            } else if active_fence.is_none() {
                active_fence = Some(marker);
            }
        }
        if active_fence.is_none() && next_required < REQUIRED_COURSE_SECTION_TITLES.len() {
            if let Some(heading) = trimmed.strip_prefix("## ") {
                if heading == REQUIRED_COURSE_SECTION_TITLES[next_required] {
                    current = Some(next_required);
                    next_required += 1;
                    continue;
                }
            }
        }
        let target = match current {
            Some(index) => &mut sections[index],
            None => &mut preamble,
        };
        target.push_str(line);
        target.push('\n');
    }
    (preamble, sections)
}

/// The canonical section a heading line opens, at any `#` level.
fn canonical_heading_title(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    let hashes = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&hashes)
        || !trimmed
            .as_bytes()
            .get(hashes)
            .is_some_and(u8::is_ascii_whitespace)
    {
        return None;
    }
    let title = trimmed[hashes..].trim().trim_end_matches('#').trim();
    REQUIRED_COURSE_SECTION_TITLES
        .iter()
        .find(|required| title.eq_ignore_ascii_case(required))
        .copied()
}

/// Keep only the prose of a single rewritten section. Models re-emit the
/// heading they were given and sometimes continue into the next section; both
/// would duplicate headings and break the required section order.
fn extract_single_section(raw: &str) -> String {
    let mut body = Vec::new();
    let mut active_fence: Option<&str> = None;
    let mut started = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        let fence = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(marker) = fence {
            if active_fence == Some(marker) {
                active_fence = None;
            } else if active_fence.is_none() {
                active_fence = Some(marker);
            }
        }
        // Any heading level counts: normalization promotes a canonical title to
        // `##` whatever level it arrives at.
        if active_fence.is_none() && canonical_heading_title(trimmed).is_some() {
            if started {
                break;
            }
            // The section's own heading is supplied on rebuild.
            continue;
        }
        if !started && trimmed.is_empty() {
            continue;
        }
        started = true;
        body.push(line);
    }
    body.join("\n").trim().to_string()
}

fn rebuild_course_body(preamble: &str, sections: &[String; 10]) -> String {
    let mut body = preamble.trim_end().to_string();
    for (index, section) in sections.iter().enumerate() {
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str(&format!(
            "## {}\n\n{}",
            REQUIRED_COURSE_SECTION_TITLES[index],
            section.trim()
        ));
    }
    body.push('\n');
    body
}

/// Whether a gate failure is about missing depth rather than a broken contract.
fn is_depth_failure(reason: &str) -> bool {
    reason.contains("too thin")
}

/// A per-section measurement of the draft against the gate it must clear, so a
/// correction pass knows exactly where to add substance instead of guessing.
fn course_depth_report(markdown: &str, budget: LessonBudget) -> String {
    let total = markdown.split_whitespace().count();
    let mut lines = vec![format!(
        "- whole course: {total} words, needs at least {} and at most {}",
        budget.min_words(),
        budget.max_words()
    )];
    match course_section_word_counts(markdown) {
        Ok(counts) => {
            for (index, count) in counts.into_iter().enumerate() {
                let minimum = budget.section_minimum(index);
                let verdict = match minimum.checked_sub(count) {
                    Some(0) | None => "meets its minimum".to_string(),
                    Some(shortfall) => format!("SHORT by about {shortfall} words"),
                };
                lines.push(format!(
                    "- `## {}`: {count} words, needs {minimum}, {verdict}",
                    REQUIRED_COURSE_SECTION_TITLES[index]
                ));
            }
        }
        Err(reason) => lines.push(format!("- sections could not be measured: {reason}")),
    }
    lines.join("\n")
}

/// The base gate: a thirty-minute lesson.
pub fn validate_generated_course(course: &GeneratedCourse) -> std::result::Result<(), String> {
    validate_generated_course_for(course, LessonBudget::default())
}

pub fn validate_generated_course_for(
    course: &GeneratedCourse,
    budget: LessonBudget,
) -> std::result::Result<(), String> {
    validate_course_body(course, budget)?;
    validate_course_metadata(course)
}

fn validate_course_body(
    course: &GeneratedCourse,
    budget: LessonBudget,
) -> std::result::Result<(), String> {
    let word_count = course.markdown.split_whitespace().count();
    let min_words = budget.min_words();
    if word_count < min_words {
        return Err(format!(
            "course is too thin: only {word_count} words; at least {min_words} are required for a {}-minute lesson",
            budget.minutes.clamp(LessonBudget::BASE_MINUTES, LessonBudget::DEEPEST_MINUTES)
        ));
    }
    let section_counts = course_section_word_counts(&course.markdown)?;
    for (index, count) in section_counts.into_iter().enumerate() {
        let minimum = budget.section_minimum(index);
        if count < minimum {
            return Err(format!(
                "course section ## {} is too thin: {count} words; at least {minimum} are required",
                REQUIRED_COURSE_SECTION_TITLES[index]
            ));
        }
    }
    let simple_index = course
        .markdown
        .find("## The simple version")
        .ok_or_else(|| "course is missing its simple first-principles explanation".to_string())?;
    let mechanics_index = course
        .markdown
        .find("## Core mechanics")
        .ok_or_else(|| "course is missing its derived core mechanics".to_string())?;
    if simple_index >= mechanics_index {
        return Err("course introduces mechanics before the simple foundation".into());
    }
    let simple_section = course.markdown[simple_index..mechanics_index].to_lowercase();
    if !simple_section.contains("analogy breaks") {
        return Err("simple explanation does not state where its analogy breaks".into());
    }
    let max_words = budget.max_words();
    if word_count > max_words {
        return Err(format!(
            "course is too long: {word_count} words; at most {max_words} fit the reading time of a {}-minute session, condense to point form and cut restatement",
            budget.depth_minutes()
        ));
    }
    let (_, sections) = split_course_sections(&course.markdown);
    let missing =
        crate::lesson_shape::sections_without_hints(&REQUIRED_COURSE_SECTION_TITLES, &sections);
    if !missing.is_empty() {
        return Err(format!(
            "course sections without a reading hint line: {}; each section must open with a line like `*~2 min · what to look for and whether to skim*`",
            missing.join(", ")
        ));
    }
    let diagrams = crate::lesson_shape::diagram_count(&course.markdown);
    if diagrams < MIN_COURSE_DIAGRAMS {
        return Err(format!(
            "course has {diagrams} mermaid diagram(s); at least {MIN_COURSE_DIAGRAMS} are required: picture the mental model and the production scenario"
        ));
    }
    if let Some(reason) = crate::prose::objection(&course.markdown) {
        return Err(reason);
    }
    Ok(())
}

/// Whether a gate failure is about a lesson too long for its session.
fn is_length_failure(reason: &str) -> bool {
    reason.contains("too long")
}

/// Whether a gate failure is only about missing reading hints.
fn is_hint_failure(reason: &str) -> bool {
    reason.contains("without a reading hint")
}

/// The gate-relevant conventions, restated for every correction prompt so a
/// rewrite aimed at one defect does not undo another.
const LESSON_SHAPE_CONTRACT: &str = "SHAPE CONTRACT (a deterministic gate checks all of it):\n- Point form: every section opens with a one-line lede, then bullets of one idea each (at most about 25 words a bullet, at most six bullets a list); no paragraph over three sentences.\n- Reading hint: the first line under every `##` heading is an italic hint like `*~2 min · What to look for, and whether to skim.*`, the time a careful reader needs and what the section is for.\n- Callouts: use `> **Key idea:**`, `> **Watch out:**`, `> **Try it:**`, `> **Example:**` and `> **Decision:**` blockquotes for the points that matter most; at least one Key idea in each of Core mechanics, Mental model and Production architecture lens.\n- Diagrams: at least two fenced ```mermaid blocks (flowchart, sequenceDiagram or stateDiagram-v2 only; quoted labels; under twelve nodes), each followed by a one-sentence caption.\n- The lesson stays inside its word window; the analogy sentence beginning `Where the analogy breaks:` stays in The simple version.";

fn validate_course_metadata(course: &GeneratedCourse) -> std::result::Result<(), String> {
    if course.title.trim().is_empty() {
        return Err("course title is empty".into());
    }
    if course.exit_questions.len() != 5 {
        return Err(format!(
            "expected 5 exit questions, got {}",
            course.exit_questions.len()
        ));
    }
    for (index, check) in course.exit_questions.iter().enumerate() {
        validate_mcq(
            &check.prompt,
            Some(&check.choices),
            &check.correct_answer,
            &check.explanation,
        )
        .map_err(|reason| format!("exit question {}: {reason}", index + 1))?;
        if check.section.trim().is_empty() || check.learning_objective.trim().is_empty() {
            return Err(format!(
                "exit question {}: section and learning_objective must be nonempty",
                index + 1
            ));
        }
    }
    let exercise = course
        .exercise
        .as_ref()
        .ok_or_else(|| "course is missing structured exercise".to_string())?;
    if exercise.title.trim().is_empty()
        || exercise.instructions.split_whitespace().count() < 60
        || exercise
            .deliverable
            .as_deref()
            .is_none_or(|deliverable| deliverable.trim().is_empty())
        || exercise.hints.is_empty()
        || exercise.hints.iter().any(|hint| hint.trim().is_empty())
    {
        return Err("structured exercise needs a title, instructions of at least 60 words, a nonempty deliverable and at least one hint".into());
    }
    if course.resources.iter().any(|resource| {
        let url = resource.url.trim();
        !(url.starts_with("https://") || url.starts_with("http://"))
    }) {
        return Err("resource URL must be absolute HTTP(S)".into());
    }
    Ok(())
}

/// A lesson body in the shape the gate expects: every section opens with a
/// reading hint, the simple version names its analogy's limit, and the
/// mental model and production sections each draw a diagram.
#[cfg(test)]
pub(crate) fn fixture_course_markdown(analogy_limit: &str) -> String {
    REQUIRED_COURSE_SECTION_TITLES
        .iter()
        .enumerate()
        .map(|(index, title)| {
            let foundation = if *title == "The simple version" {
                format!("Where the analogy breaks: {analogy_limit} ")
            } else {
                String::new()
            };
            let diagram = match *title {
                "Mental model" => "```mermaid\nflowchart LR\n  A[\"call\"] --> B[\"queue\"]\n```\n\nThe call waits in the queue.\n\n",
                "Production architecture lens" => "```mermaid\nsequenceDiagram\n  Client->>Service: request\n```\n\nOne request crosses the boundary.\n\n",
                _ => "",
            };
            let extra_depth = if *title == "Core mechanics" { 200 } else { 20 };
            format!(
                "## {title}\n\n*~1 min · Fixture orientation for {title}.*\n\n{diagram}{foundation}{}",
                "mechanism evidence decision experiment ".repeat(
                    (MIN_COURSE_SECTION_WORDS[index] + extra_depth) / 4 + 1
                )
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[derive(Debug, Clone, Serialize)]
pub struct GradeItem {
    pub id: i64,
    pub concept: String,
    pub question: String,
    pub model_answer: String,
    pub user_answer: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Verdict {
    pub id: i64,
    pub correct: bool,
    #[serde(default)]
    pub feedback: String,
    /// Teacher's private observation about the student on this concept.
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Verdicts {
    pub verdicts: Vec<Verdict>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionPlan {
    pub session_type: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Clone)]
pub struct Generator {
    pub claude_bin: String,
    pub codex_bin: Option<String>,
    pub scratch_dir: PathBuf,
    /// Primary model for course generation (the expensive, quality-bound call).
    /// All primary tasks use the selected runner model.
    /// Shared + hot-swappable: settings changes apply without a restart.
    pub model: std::sync::Arc<std::sync::Mutex<String>>,
    /// Which generation provider is primary.
    pub agent: std::sync::Arc<std::sync::Mutex<String>>,
    /// Binary path used when agent == 'custom'. Contract: accepts the prompt
    /// as its final argument and prints the answer to stdout.
    pub custom_bin: std::sync::Arc<std::sync::Mutex<String>>,
    /// Live agent-activity lines for the interface and the execution log.
    /// Silent in tests.
    pub feed: crate::execution_log::Feed,
    /// Fetches the primary documentation a lesson is taught from. Providers are
    /// never trusted to supply URLs from memory.
    pub researcher: crate::research::Researcher,
    pub runner: crate::agents::Runner,
    purpose: String,
    owner: Option<String>,
    output_schema: Option<serde_json::Value>,
}

/// Immutable generation routing for one classroom subject. Subject profiles
/// are resolved from SQLite before a request, so concurrent classes can use
/// different providers and models without mutating the global Generator.
#[derive(Debug, Clone)]
pub struct GenerationProfile {
    pub subject_id: String,
    pub agent: String,
    pub model: String,
    pub custom_bin: String,
    pub prompt_version: String,
}

pub struct CourseRequest<'a> {
    pub title: &'a str,
    pub category: &'a str,
    pub dossier: &'a str,
    pub focus: &'a str,
    pub curriculum: &'a crate::db::CurriculumBrief,
    /// How deep the lesson should go for the session's minutes.
    pub budget: LessonBudget,
}

struct CourseEditContext<'a> {
    curriculum: &'a crate::db::CurriculumBrief,
    dossier: &'a str,
    agent: &'a str,
    custom_bin: &'a str,
    model: &'a str,
    label: &'a str,
    budget: LessonBudget,
}

pub const COURSE_PROMPT: &str = include_str!("../prompts/course.txt");
pub const QUIZ_PROMPT: &str = include_str!("../prompts/quiz.txt");
pub const GRADE_PROMPT: &str = include_str!("../prompts/grade.txt");
pub const TEACHER_PROMPT: &str = include_str!("../prompts/teacher.txt");
pub const PLAN_PROMPT: &str = include_str!("../prompts/plan.txt");
pub const EXIT_PROMPT: &str = include_str!("../prompts/exit.txt");
pub const AUDIO_PROMPT: &str = include_str!("../prompts/audio.txt");
pub const CHAT_PROMPT: &str = include_str!("../prompts/chat.txt");
pub const FIRST_PRINCIPLES_PROMPT: &str = include_str!("../prompts/first-principles.txt");

/// Complete grounding supplied to the bounded course tutor. Keeping this
/// separate from the prompt makes primary and classroom readers use the same
/// quality contract without dropping their curriculum or exercise context.
#[derive(Debug, Clone)]
pub struct CourseChatContext {
    pub title: String,
    pub focus: String,
    pub markdown: String,
    pub learner_outcome: String,
    pub cumulative_artifact: String,
    pub exercise: String,
}

/// One turn of the session-only course chat. Held in memory on `AppState`,
/// never persisted — the whole point is a bounded, throwaway Q&A surface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurn {
    pub role: String, // "user" | "assistant"
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub follow_ups: Vec<String>,
}

impl ChatTurn {
    pub fn user(content: String) -> Self {
        Self {
            role: "user".into(),
            content,
            section: None,
            follow_ups: Vec::new(),
        }
    }

    pub fn assistant(reply: ChatReply) -> Self {
        Self {
            role: "assistant".into(),
            content: reply.answer,
            section: Some(reply.section),
            follow_ups: reply.follow_ups,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatReply {
    pub answer: String,
    pub section: String,
    pub follow_ups: Vec<String>,
}

/// Most recent turns to fold into the prompt — enough for follow-up
/// questions to make sense without letting the prompt grow unbounded.
const CHAT_HISTORY_TURNS: usize = 8;

fn format_chat_history(history: &[ChatTurn]) -> String {
    if history.is_empty() {
        return "(none yet, this is the first question)".to_string();
    }
    history
        .iter()
        .rev()
        .take(CHAT_HISTORY_TURNS)
        .rev()
        .map(|t| {
            let speaker = if t.role == "assistant" {
                "Tutor"
            } else {
                "Student"
            };
            format!("{speaker}: {}", t.content.trim())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn course_headings(markdown: &str) -> Vec<String> {
    markdown
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let heading = line.strip_prefix('#')?.trim_start_matches('#').trim();
            (!heading.is_empty()).then(|| heading.to_string())
        })
        .collect()
}

fn validate_chat_reply(
    reply: &mut ChatReply,
    headings: &[String],
) -> std::result::Result<(), String> {
    let answer_words = reply.answer.split_whitespace().count();
    if !(12..=500).contains(&answer_words) {
        return Err(format!(
            "answer must contain 12-500 words, found {answer_words}"
        ));
    }

    let canonical_section = headings
        .iter()
        .find(|heading| heading.eq_ignore_ascii_case(reply.section.trim()))
        .cloned()
        .ok_or_else(|| "section must exactly match one supplied course heading".to_string())?;
    reply.section = canonical_section;

    if reply.follow_ups.len() != 3 {
        return Err(format!(
            "exactly three follow-up questions are required, found {}",
            reply.follow_ups.len()
        ));
    }
    let mut unique = std::collections::HashSet::new();
    for follow_up in &mut reply.follow_ups {
        *follow_up = follow_up.trim().to_string();
        let words = follow_up.split_whitespace().count();
        if !(3..=22).contains(&words) || follow_up.len() > 180 {
            return Err("each follow-up must be a focused 3-22 word question".into());
        }
        if !unique.insert(follow_up.to_lowercase()) {
            return Err("follow-up questions must be distinct".into());
        }
    }
    if let Some(reason) = crate::prose::objection(&reply.answer) {
        return Err(reason);
    }
    Ok(())
}

/// Model for quiz/grade/repair calls regardless of the configured primary.

#[derive(Debug, Clone, Copy)]
enum PedagogyDomain {
    Engineering,
    Language,
}

impl PedagogyDomain {
    fn sequence(self) -> &'static str {
        match self {
            Self::Engineering => {
                "For software and systems: primitive runtime facts and constraints → mechanism → \
                 composition and boundaries → API, framework, or architecture abstraction → \
                 runnable evidence → production decision."
            }
            Self::Language => {
                "For language: symbol and sound → word pattern → sentence frame → meaningful \
                 exchange → changed real-world situation. For numbers: spoken sequence → \
                 construction rule → practical use."
            }
        }
    }
}

fn prepend_first_principles(task_prompt: &str, domain: PedagogyDomain) -> String {
    format!(
        "{}\n\n{}",
        FIRST_PRINCIPLES_PROMPT.replace("{{DOMAIN_SEQUENCE}}", domain.sequence()),
        task_prompt
    )
}

fn with_pedagogy(task_prompt: &str, focus: &str, domain: PedagogyDomain) -> String {
    with_focus(&prepend_first_principles(task_prompt, domain), focus)
}

/// Prepend the shared pedagogy contract and, when available, the personalized
/// Teacher role + dossier. A fresh install still receives the full
/// first-principles contract on day one.
fn with_teacher(dossier: &str, task_prompt: &str, focus: &str) -> String {
    let prompt = if dossier.trim().is_empty() {
        task_prompt.to_string()
    } else {
        format!(
            "{}\n\n{}",
            TEACHER_PROMPT.replace("{{DOSSIER}}", dossier.trim()),
            task_prompt
        )
    };
    with_pedagogy(&prompt, focus, PedagogyDomain::Engineering)
}

fn with_focus(prompt: &str, focus: &str) -> String {
    prompt
        .replace("{{FOCUS}}", focus)
        .replace("{{FOCUS_LABEL}}", crate::focus::label(focus))
        .replace("{{FOCUS_CONTEXT}}", crate::focus::context(focus))
        .replace("{{MONTH_OUTCOME}}", crate::focus::month_outcome(focus))
}

#[derive(Debug, Clone, Deserialize)]
pub struct FallbackCourse {
    pub slug: String,
    /// Representative role of this bundled lesson: beginner, advanced, remediation,
    /// retrieval or capstone. Empty for older fixtures.
    #[serde(default)]
    pub kind: String,
    pub title: String,
    pub markdown: String,
    #[serde(default)]
    pub resources: Vec<Resource>,
    #[serde(default)]
    pub key_takeaways: Vec<String>,
    pub questions: Vec<GeneratedQuestion>,
    #[serde(default)]
    pub exercise: Option<Exercise>,
}

impl Generator {
    pub fn new(
        claude_bin: String,
        codex_bin: Option<String>,
        scratch_dir: PathBuf,
        model: std::sync::Arc<std::sync::Mutex<String>>,
        agent: std::sync::Arc<std::sync::Mutex<String>>,
        custom_bin: std::sync::Arc<std::sync::Mutex<String>>,
        feed: crate::execution_log::Feed,
    ) -> Self {
        let _ = std::fs::create_dir_all(&scratch_dir);
        let runner = crate::agents::Runner {
            claude_bin: claude_bin.clone(),
            codex_bin: codex_bin.clone(),
            scratch_dir: scratch_dir.clone(),
            database: None,
            feed: feed.clone(),
            #[cfg(test)]
            test_deepseek: None,
        };
        Self {
            runner,
            purpose: "generation".into(),
            owner: None,
            output_schema: None,
            claude_bin,
            codex_bin,
            scratch_dir,
            model,
            agent,
            custom_bin,
            feed,
            researcher: crate::research::Researcher::new(),
        }
    }

    /// The primary CLI agent right now (PRINCIPIA_AGENT env wins for tests).
    pub fn current_agent(&self) -> String {
        std::env::var("PRINCIPIA_AGENT").unwrap_or_else(|_| self.agent.lock().unwrap().clone())
    }

    pub fn current_custom_bin(&self) -> String {
        self.custom_bin.lock().unwrap().clone()
    }

    /// The course-generation model right now (PRINCIPIA_MODEL env wins for tests).
    pub fn current_model(&self) -> String {
        std::env::var("PRINCIPIA_MODEL").unwrap_or_else(|_| self.model.lock().unwrap().clone())
    }

    pub(crate) fn log(&self, msg: impl Into<String>) {
        self.feed.say(msg);
    }

    async fn course_metadata(
        &self,
        markdown: &str,
        agent: &str,
        custom_bin: &str,
        model: &str,
        context: &str,
        failure: &str,
    ) -> Result<GeneratedCourse> {
        let prompt = format!(
            "{}\n\nCOURSE_CONTEXT: {context}\nVALIDATION_FEEDBACK: {failure}\n\nLESSON_BODY:\n{markdown}",
            include_str!("../prompts/course-metadata.txt")
        );
        let mut structured = self.clone();
        structured.output_schema = Some(course_metadata_schema());
        let (metadata, _) = structured
            .run_exact_for::<CourseMetadata>(
                agent,
                custom_bin,
                &prompt,
                false,
                Duration::from_secs(300),
                model,
            )
            .await?;
        Ok(metadata.with_markdown(markdown.to_owned()))
    }

    async fn write_course(
        &self,
        prompt: &str,
        agent: &str,
        custom_bin: &str,
        model: &str,
        context: &str,
    ) -> Result<GeneratedCourse> {
        let markdown = self
            .run_prose_for(agent, custom_bin, prompt, Duration::from_secs(720), model)
            .await?;
        self.course_metadata(
            &markdown,
            agent,
            custom_bin,
            model,
            context,
            "Initial assessment; follow the exact schema.",
        )
        .await
    }

    async fn ensure_course_quality(
        &self,
        mut course: GeneratedCourse,
        agent: &str,
        custom_bin: &str,
        model: &str,
        context: &str,
        budget: LessonBudget,
    ) -> Result<GeneratedCourse> {
        course.markdown = normalize_course_headings(&course.markdown);
        // Body corrections cannot replace already valid questions or exercises.
        for attempt in 0..=MAX_QUALITY_CORRECTIONS {
            let Err(reason) = validate_course_body(&course, budget) else {
                break;
            };
            if attempt == MAX_QUALITY_CORRECTIONS {
                return Err(GenError::Quality(format!("{context} failed body validation after {attempt} same-provider corrections: {reason}")));
            }
            self.log(format!("{agent} is correcting lesson prose: {reason}"));
            let markdown = if is_depth_failure(&reason) {
                self.expand_course_body(&course, agent, custom_bin, model, context, budget)
                    .await?
            } else if is_length_failure(&reason) {
                self.condense_course_body(&course, agent, custom_bin, model, context, budget)
                    .await?
            } else if is_hint_failure(&reason) {
                self.write_section_hints(&course, agent, custom_bin, model, context, budget)
                    .await?
            } else if crate::prose::is_objection(&reason) {
                self.clean_course_prose(&course, agent, custom_bin, model, context)
                    .await?
            } else {
                let prompt = format!(
                    "Correct only this lesson's Markdown. Return plain Markdown, never JSON.\n\nCOURSE_CONTEXT: {context}\nQUALITY_GATE_FAILURE: {reason}\nREQUIRED_SECTION_ORDER (exact headings, no numbering or subtitles):\n{}\nDEPTH REQUIREMENTS:\n{}\n{LESSON_SHAPE_CONTRACT}\n\nPreserve the topic, worked examples, code, citations and section depth. Use the canonical section order. In The simple version include an explicit sentence beginning `Where the analogy breaks:`. Do not add assessment metadata.\n\nLESSON_BODY:\n{}",
                    REQUIRED_COURSE_SECTION_TITLES.map(|title| format!("## {title}")).join("\n"), course_depth_report(&course.markdown, budget), course.markdown
                );
                self.run_prose_for(agent, custom_bin, &prompt, Duration::from_secs(720), model)
                    .await?
            };
            course.markdown = normalize_course_headings(&markdown);
        }
        // Report the exact field contract and repair the small assessment only.
        // Never guess which choice a letter/index was supposed to identify.
        for attempt in 0..=MAX_QUALITY_CORRECTIONS {
            let Err(reason) = validate_course_metadata(&course) else {
                return Ok(course);
            };
            if attempt == MAX_QUALITY_CORRECTIONS {
                return Err(GenError::Quality(format!("{context} failed assessment validation after {attempt} same-provider corrections: {reason}")));
            }
            self.log(format!("{agent} is correcting lesson assessment: {reason}"));
            course = self
                .course_metadata(&course.markdown, agent, custom_bin, model, context, &reason)
                .await?;
        }
        unreachable!("bounded validation returns a course or an error")
    }

    /// Deepen only the sections that fall short, one prose call each. A request
    /// scoped to a single section reliably produces real depth, where a request
    /// to rewrite the whole course comes back compressed.
    async fn expand_course_body(
        &self,
        course: &GeneratedCourse,
        agent: &str,
        custom_bin: &str,
        model: &str,
        context: &str,
        budget: LessonBudget,
    ) -> Result<String> {
        let (preamble, mut sections) = split_course_sections(&course.markdown);
        let mut deepened = 0;
        let mut total = course.markdown.split_whitespace().count();
        let session_minutes = budget
            .minutes
            .clamp(LessonBudget::BASE_MINUTES, LessonBudget::DEEPEST_MINUTES);
        for index in 0..sections.len() {
            let words = sections[index].split_whitespace().count();
            let target = budget.section_minimum(index);
            // Sections at their floor still need the course to clear its total,
            // so ask for a margin rather than the bare minimum.
            let goal = target + target / 4;
            if words >= target && (total >= budget.min_words() || words >= goal) {
                continue;
            }
            // A one-section request tends to overshoot badly, and a course the
            // learner cannot finish inside the session is its own failure.
            let ceiling = goal + goal / 2;
            let title = REQUIRED_COURSE_SECTION_TITLES[index];
            let prompt = format!(
                "You are deepening one section of a course you already wrote. Rewrite only this \
                 section.\n\nCOURSE_CONTEXT: {context}\nCOURSE_TITLE: {}\nSECTION: ## {title}\n\
                 CURRENT LENGTH: {words} words\nREQUIRED LENGTH: between {goal} and {ceiling} \
                 words, the whole course is read in one {session_minutes}-minute session, so staying inside \
                 that range matters as much as reaching it.\n\n\
                 Rewrite this section so it reaches the required length through substance a reader \
                 could act on: the next step of the mechanism, a worked trace with the observation \
                 it produces, a production decision with its evidence and cost, a failure mode with \
                 how it is detected, or a measurement with the numbers to expect. Keep the point-form \
                 shape: the italic reading-hint line first, then a lede and bullets of one idea each, \
                 with `> **Key idea:**`-style callouts and any mermaid diagram kept. Keep every inline \
                 source citation link that is already there and keep the code that earns its place. \
                 Do not restate other sections, do not add a summary, and do not pad, padding is \
                 measured and rejected.{}\n\nDo not output the `## {title}` heading itself and do \
                 not write any other section.\n\nCURRENT_SECTION:\n{}\n\nCOURSE_BODY_FOR_CONTEXT:\n{}",
                course.title,
                if title == "The simple version" {
                    " Keep the sentence beginning exactly `Where the analogy breaks:`."
                } else {
                    ""
                },
                sections[index].trim(),
                course.markdown
            );
            let rewritten = crate::lesson_shape::keep_hint(
                &sections[index],
                &extract_single_section(
                    &self
                        .run_prose_for(agent, custom_bin, &prompt, Duration::from_secs(420), model)
                        .await?,
                ),
            );
            // A shorter answer than we started with is a regression, not a fix.
            let rewritten_words = rewritten.split_whitespace().count();
            if rewritten_words > words {
                total += rewritten_words - words;
                sections[index] = rewritten;
                deepened += 1;
            }
        }
        if deepened == 0 {
            return Err(GenError::Quality(format!(
                "{context} stayed too thin: the provider returned no longer text for any short \
                 section"
            )));
        }
        self.log(format!("{agent} deepened {deepened} thin section(s)"));
        Ok(rebuild_course_body(&preamble, &sections))
    }

    /// Rewrite only the sections that read like a machine wrote them, one
    /// prose call each, so the rest of the lesson stays word for word. The
    /// call is told exactly which expressions to replace.
    async fn clean_course_prose(
        &self,
        course: &GeneratedCourse,
        agent: &str,
        custom_bin: &str,
        model: &str,
        context: &str,
    ) -> Result<String> {
        let (preamble, mut sections) = split_course_sections(&course.markdown);
        let mut cleaned = 0;
        for index in 0..sections.len() {
            let Some(reason) = crate::prose::objection(&sections[index]).or_else(|| {
                // A soft tell or two in every section adds up to a failing
                // lesson; each section with any tell is rewritten.
                (!crate::prose::hits(&sections[index]).is_empty())
                    .then(|| "stock words are spread through the section".to_string())
            }) else {
                continue;
            };
            let title = REQUIRED_COURSE_SECTION_TITLES[index];
            let names = crate::prose::hits(&sections[index])
                .iter()
                .map(|hit| format!("\"{}\"", hit.name.trim_start_matches('^')))
                .collect::<Vec<_>>()
                .join(", ");
            let prompt = format!(
                "You are editing one section of a lesson you already wrote so that it reads as a \
                 person wrote it. Rewrite only this section.\n\nCOURSE_CONTEXT: {context}\n\
                 SECTION: ## {title}\nWHAT THE CHECK FOUND: {reason}\nEXPRESSIONS TO REPLACE: {names}\n\n\
                 {}\n\nReplace each listed expression with plain, specific words, or delete the \
                 sentence when it says nothing. Change nothing else: keep every fact, number, \
                 command, code block, citation link, callout, diagram and the italic reading-hint \
                 line exactly as they are. Do not output the `## {title}` heading and do not write \
                 any other section.\n\nCURRENT_SECTION:\n{}",
                crate::prose::CONTRACT,
                sections[index].trim()
            );
            let rewritten = crate::lesson_shape::keep_hint(
                &sections[index],
                &extract_single_section(
                    &self
                        .run_prose_for(agent, custom_bin, &prompt, Duration::from_secs(300), model)
                        .await?,
                ),
            );
            // A rewrite that lost most of the section is a regression, not a fix.
            let before = sections[index].split_whitespace().count();
            let after = rewritten.split_whitespace().count();
            if after * 10 >= before * 7 {
                sections[index] = rewritten;
                cleaned += 1;
            }
        }
        if cleaned == 0 {
            return Err(GenError::Quality(format!(
                "{context} still reads like a machine wrote it: no section came back rewritten"
            )));
        }
        self.log(format!(
            "{agent} rewrote {cleaned} section(s) in plain words"
        ));
        Ok(rebuild_course_body(&preamble, &sections))
    }

    /// Bring a lesson that overran its session back inside its window, whole,
    /// so the cuts fall where the restatement is rather than section by
    /// section. Substance stays; the words it is said in go.
    async fn condense_course_body(
        &self,
        course: &GeneratedCourse,
        agent: &str,
        custom_bin: &str,
        model: &str,
        context: &str,
        budget: LessonBudget,
    ) -> Result<String> {
        let words = course.markdown.split_whitespace().count();
        let (low, high) = budget.target_words();
        let plan = budget.plan();
        let prompt = format!(
            "Condense this lesson without losing substance. Return plain Markdown, never JSON.\n\n\
             COURSE_CONTEXT: {context}\nCURRENT LENGTH: {words} words\nREQUIRED LENGTH: between \
             {low} and {high} words, the learner reads it in about {} minutes of a {}-minute \
             session, so the ceiling matters as much as the floor.\nSECTION FLOORS:\n{}\n\
             {LESSON_SHAPE_CONTRACT}\n\nKeep every `##` heading in its order, every section's \
             opening hint line, every mermaid diagram with its caption, the code that earns its \
             place and every inline citation link. Turn paragraphs into bullets of one idea each, \
             delete transitions, restatements and summaries that repeat another section, and cut \
             worked examples to the step that carries the point. Do not drop a mechanism, a \
             decision, a failure mode or its evidence, say it in fewer words. Keep the sentence \
             beginning exactly `Where the analogy breaks:` in The simple version. Do not add \
             assessment metadata.\n\nLESSON_BODY:\n{}",
            plan.learn_minutes,
            budget.depth_minutes(),
            course_depth_report(&course.markdown, budget),
            course.markdown
        );
        let condensed = self
            .run_prose_for(agent, custom_bin, &prompt, Duration::from_secs(720), model)
            .await?;
        self.log(format!(
            "{agent} condensed the lesson from {words} to {} words",
            condensed.split_whitespace().count()
        ));
        Ok(condensed)
    }

    /// Write the reading hints the sections are missing, one structured call,
    /// and put each under its heading. Nothing else in the lesson changes.
    async fn write_section_hints(
        &self,
        course: &GeneratedCourse,
        agent: &str,
        custom_bin: &str,
        model: &str,
        context: &str,
        budget: LessonBudget,
    ) -> Result<String> {
        let (preamble, mut sections) = split_course_sections(&course.markdown);
        let missing =
            crate::lesson_shape::sections_without_hints(&REQUIRED_COURSE_SECTION_TITLES, &sections);
        let plan = budget.plan();
        let mut scoped = self.scoped("section-hints");
        scoped.output_schema = Some(section_hints_schema(&missing));
        let prompt = format!(
            "Write the reading hint for each listed section of this lesson. Return only one JSON \
             object whose keys are exactly the section titles listed and whose values are the hint \
             lines.\n\nCOURSE_CONTEXT: {context}\nSECTIONS NEEDING A HINT: {}\n\nA hint line has the \
             form `*~N min · orientation*`: N is the minutes a careful reader needs for that \
             section (1 to 4), and the orientation is one sentence saying what to look for and \
             whether a reader who already knows the idea may skim. The hints of the whole lesson add \
             up to about {} minutes. Example value: `*~2 min · Read closely: the exercise is built \
             on this ordering rule.*`\n\nLESSON_BODY:\n{}",
            missing.join("; "),
            plan.learn_minutes,
            course.markdown
        );
        let (hints, _) = scoped
            .run_exact_for::<std::collections::HashMap<String, String>>(
                agent,
                custom_bin,
                &prompt,
                false,
                Duration::from_secs(300),
                model,
            )
            .await?;
        for (index, title) in REQUIRED_COURSE_SECTION_TITLES.iter().enumerate() {
            if !missing.contains(title) {
                continue;
            }
            let line = hints
                .get(*title)
                .map(|hint| hint.trim())
                .filter(|hint| crate::lesson_shape::parse_hint_line(hint).is_some())
                .ok_or_else(|| {
                    GenError::Quality(format!(
                        "{context}: the provider returned no usable reading hint for {title}"
                    ))
                })?;
            sections[index] = format!("{line}\n\n{}", sections[index].trim_start());
        }
        self.log(format!(
            "{agent} wrote reading hints for {} section(s)",
            missing.len()
        ));
        Ok(rebuild_course_body(&preamble, &sections))
    }

    async fn edit_course_quality(
        &self,
        mut course: GeneratedCourse,
        context: CourseEditContext<'_>,
        sources: &[crate::research::ResearchSource],
    ) -> Result<GeneratedCourse> {
        let mut scoped = self.scoped("quality-review");
        scoped.output_schema = Some(course_audit_schema());
        let brief = serde_json::to_string(context.curriculum)
            .map_err(|error| GenError::Parse(format!("could not serialize curriculum: {error}")))?;
        for attempt in 0..=MAX_QUALITY_CORRECTIONS {
            let draft = serde_json::to_string(&course)
                .map_err(|error| GenError::Parse(format!("could not serialize course: {error}")))?;
            let prompt = format!(
                "{}\n\nCOURSE_CONTEXT: {}\nCURRICULUM_BRIEF: {brief}\nLEARNER_DOSSIER:\n{}\nDEPTH MEASUREMENT:\n{}\nRETRIEVED SOURCE MATERIAL:\n{}\nDRAFT_COURSE:\n{draft}",
                include_str!("../prompts/course-audit.txt"), context.label, context.dossier, course_depth_report(&course.markdown, context.budget),
                crate::research::format_source_material(sources),
            );
            scoped.log(format!(
                "{} is auditing the prepared lesson for {}",
                context.agent, context.label
            ));
            let (review, _) = scoped
                .run_exact_for::<CourseEditorialReview>(
                    context.agent,
                    context.custom_bin,
                    &prompt,
                    false,
                    Duration::from_secs(420),
                    context.model,
                )
                .await?;
            let score_result = review.scores.validate();
            if score_result.is_ok() && review.issues.is_empty() {
                validate_generated_course_for(&course, context.budget)
                    .map_err(GenError::Quality)?;
                return Ok(course);
            }
            let reason = format!(
                "{}; {}",
                score_result.err().unwrap_or_default(),
                review
                    .issues
                    .iter()
                    .map(|issue| format!("{}: {}", issue.section, issue.reason))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            if attempt == MAX_QUALITY_CORRECTIONS {
                // The corrections are spent. If the deterministic gate holds
                // and no score is below the publishable floor, the editor's
                // remaining points are refinements: publish, and show them.
                let refinements = review.scores.publishable_with_notes()
                    && validate_generated_course_for(&course, context.budget).is_ok();
                if refinements {
                    scoped.log(format!(
                        "{} editor's remaining notes are attached to the lesson rather than blocking it: {reason}",
                        context.agent
                    ));
                    course.review_notes = review
                        .issues
                        .iter()
                        .map(|issue| format!("{}: {}", issue.section, issue.reason))
                        .collect();
                    if let Err(weak) = review.scores.validate() {
                        course.review_notes.push(weak);
                    }
                    return Ok(course);
                }
                return Err(GenError::Quality(format!(
                    "{} failed its final editorial audit: {reason}",
                    context.label
                )));
            }
            scoped.log(format!(
                "{} editor requested a correction: {reason}",
                context.agent
            ));
            if review.issues.is_empty()
                || review.issues.iter().any(|issue| {
                    issue.reason.trim().is_empty()
                        || (issue.section != "Assessment"
                            && !REQUIRED_COURSE_SECTION_TITLES.contains(&issue.section.as_str()))
                })
            {
                return Err(GenError::Quality("The editor did not identify a valid section and concrete correction for its failing scores.".into()));
            }
            let (preamble, mut sections) = split_course_sections(&course.markdown);
            let mut changed_body = false;
            for (index, title) in REQUIRED_COURSE_SECTION_TITLES.iter().enumerate() {
                let issues = review
                    .issues
                    .iter()
                    .filter(|issue| issue.section == *title)
                    .map(|issue| issue.reason.as_str())
                    .collect::<Vec<_>>();
                if issues.is_empty() {
                    continue;
                }
                let minimum = context.budget.section_minimum(index);
                let correction = format!(
                    "Correct only the `{title}` section of this lesson. Return that section's Markdown body only, without its heading, any other section, or JSON. Resolve each listed defect while preserving useful mechanisms, code and citations. Keep at least {minimum} substantive words and stay in point form: the italic reading-hint line first, then a lede and bullets of one idea each, callouts and any diagram kept. Do not rewrite or summarize other sections.\n\nEDITOR_FEEDBACK:\n{}\nCURRICULUM_BRIEF: {brief}\nLEARNER_DOSSIER: {}\nRETRIEVED SOURCE MATERIAL:\n{}\nCURRENT_SECTION:\n{}\nFULL_LESSON_FOR_CONTEXT:\n{}",
                    issues.join("\n"), context.dossier, crate::research::format_source_material(sources), sections[index], course.markdown,
                );
                let replacement = self
                    .run_prose_for(
                        context.agent,
                        context.custom_bin,
                        &correction,
                        Duration::from_secs(420),
                        context.model,
                    )
                    .await?;
                let replacement = extract_single_section(&replacement);
                if replacement.is_empty() {
                    return Err(GenError::Quality(format!(
                        "The editor returned no corrected text for {title}."
                    )));
                }
                sections[index] = crate::lesson_shape::keep_hint(&sections[index], &replacement);
                changed_body = true;
            }
            if changed_body {
                course.markdown = rebuild_course_body(&preamble, &sections);
            }
            course = self
                .course_metadata(
                    &course.markdown,
                    context.agent,
                    context.custom_bin,
                    context.model,
                    context.label,
                    &reason,
                )
                .await?;
            course = self
                .ensure_course_quality(
                    course,
                    context.agent,
                    context.custom_bin,
                    context.model,
                    context.label,
                    context.budget,
                )
                .await?;
            course = self
                .ensure_source_grounding(
                    course,
                    sources,
                    &context.curriculum.primary_sources,
                    context.agent,
                    context.custom_bin,
                    context.model,
                    context.label,
                    context.budget,
                )
                .await?;
            // The corrected result must pass another audit; a score for an older
            // draft never certifies newly generated prose or assessments.
        }
        unreachable!("bounded editorial audit returns a course or an error")
    }

    /// Fetch the primary documentation for one lesson before any provider
    /// call, saying in the log what was skipped and why.
    async fn research_for(&self, request: &CourseRequest<'_>) -> crate::research::Gathered {
        let topic = crate::research::course_topic(request.title, request.category);
        let gathered = self
            .researcher
            .gather(
                request.focus,
                &topic,
                &request.curriculum.primary_sources,
                RESEARCH_SOURCE_TARGET,
            )
            .await;
        if !gathered.discovered.is_empty() {
            self.log(format!(
                "research: web search suggested {} page(s) on the subject's hosts",
                gathered.discovered.len()
            ));
        }
        if let Some(reason) = &gathered.search_error {
            self.log(format!("research: web search failed, {reason}"));
        }
        for (url, reason) in &gathered.skipped {
            self.log(format!("research: skipped {url}, {reason}"));
        }
        for (seed, mirror) in &gathered.substituted {
            self.log(format!(
                "research: {seed} could not be reached; reading the same material at {mirror}"
            ));
        }
        if gathered.sources.is_empty() {
            self.log(
                "research: no primary sources could be retrieved; teaching without a verified \
                 reading list, the lesson will carry a note saying so"
                    .to_string(),
            );
        } else {
            self.log(format!(
                "research: retrieved {} primary source(s), {}",
                gathered.sources.len(),
                gathered
                    .sources
                    .iter()
                    .map(|source| source.host.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        gathered
    }

    /// What the reader is told when a lesson was written without retrieved
    /// documentation: which hosts could not be reached, and what that means.
    fn research_note(gathered: &crate::research::Gathered) -> String {
        let hosts = gathered.unreachable_hosts();
        if hosts.is_empty() {
            "No primary documentation was available for this topic when the lesson was written, so its claims were not checked against the sources. Read it with care and verify anything you will rely on.".to_string()
        } else {
            format!(
                "No primary documentation could be retrieved when this lesson was written: {} could not be reached. Its claims were not checked against the sources. Read it with care and verify anything you will rely on.",
                hosts.join(", ")
            )
        }
    }

    /// Drop every unreachable URL and add the retrieved documents, so the
    /// reading list only ever contains links the learner can actually open.
    async fn verified_reading_list(
        &self,
        course_resources: &[Resource],
        sources: &[crate::research::ResearchSource],
        seeds: &[String],
    ) -> Vec<Resource> {
        let mut reading_list: Vec<Resource> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let resources: Vec<_> = course_resources
            .iter()
            .filter(|resource| seen.insert(resource.url.trim().to_owned()))
            .collect();
        // A documentation outage must not multiply its timeout by every link.
        // Bound parallel checks and retain the author's deterministic order.
        for chunk in resources.chunks(4) {
            let handles: Vec<_> = chunk
                .iter()
                .map(|resource| {
                    let researcher = self.researcher.clone();
                    let url = resource.url.trim().to_owned();
                    tokio::spawn(async move { researcher.url_resolves(&url).await })
                })
                .collect();
            for (resource, handle) in chunk.iter().zip(handles) {
                let url = resource.url.trim();
                if handle.await.unwrap_or(false) {
                    reading_list.push(Resource {
                        title: resource.title.clone(),
                        url: url.to_owned(),
                        kind: resource.kind.clone(),
                        why: resource.why.clone(),
                    });
                } else if seeds.iter().any(|seed| seed.trim() == url) {
                    // The curriculum itself names this page. An outage does
                    // not make it wrong; it makes it unreachable today.
                    self.log(format!(
                        "research: kept curriculum link {url} although it could not be reached"
                    ));
                    reading_list.push(Resource {
                        title: resource.title.clone(),
                        url: url.to_owned(),
                        kind: resource.kind.clone(),
                        why: format!(
                            "{} (Named by the curriculum; it could not be reached when this lesson was written.)",
                            resource.why.trim()
                        )
                        .trim()
                        .to_string(),
                    });
                } else {
                    self.log(format!("research: dropped unverifiable link {url}"));
                }
            }
        }
        for source in sources {
            if reading_list.iter().any(|kept| kept.url == source.url) {
                continue;
            }
            reading_list.push(Resource {
                title: source.title.clone(),
                url: source.url.clone(),
                kind: "docs".into(),
                why: format!(
                    "Primary source from {} used to write this lesson.",
                    source.host
                ),
            });
        }
        reading_list
    }

    /// Replace the model's reading list with a verified one and require the
    /// course to actually cite the retrieved documentation. A course that cites
    /// nothing gets one same-provider correction before failing.
    #[allow(clippy::too_many_arguments)]
    async fn ensure_source_grounding(
        &self,
        mut course: GeneratedCourse,
        sources: &[crate::research::ResearchSource],
        seeds: &[String],
        agent: &str,
        custom_bin: &str,
        model: &str,
        context: &str,
        budget: LessonBudget,
    ) -> Result<GeneratedCourse> {
        course.resources = self
            .verified_reading_list(&course.resources, sources, seeds)
            .await;
        if sources.len() < MIN_GROUNDED_SOURCES {
            // Retrieval itself failed (offline, host down). Unverifiable links
            // are already gone; failing the lesson too would punish the learner
            // for a network problem rather than a quality problem.
            return Ok(course);
        }
        // Never demand more citations than there are sources to cite.
        let required = MIN_INLINE_CITATIONS.min(sources.len());
        let cited = crate::research::cited_source_count(&course.markdown, sources);
        if cited >= required {
            return Ok(course);
        }

        self.log(format!(
            "{agent} cited only {cited} of {required} required retrieved source(s); requesting \
             one same-provider citation correction"
        ));
        let url_list = sources
            .iter()
            .map(|source| format!("- {} ({})", source.url, source.title))
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = format!(
            "Correct attribution in this lesson. Return the complete Markdown body only, never JSON.\n\nCOURSE_CONTEXT: {context}\nRETRIEVED SOURCES (use exact URLs):\n{url_list}\n{}\n\nAdd inline markdown links to at least {required} different retrieved URLs at the claims they support. Do not invent sources or change the topic. Preserve examples, section order and depth. The questions and structured exercise are held separately by the application.\n\nLESSON_BODY:\n{}",
            crate::research::format_source_material(sources), course.markdown,
        );
        course.markdown = self
            .run_prose_for(agent, custom_bin, &prompt, Duration::from_secs(720), model)
            .await?;
        let mut corrected = self
            .ensure_course_quality(course, agent, custom_bin, model, context, budget)
            .await?;
        let cited = crate::research::cited_source_count(&corrected.markdown, sources);
        if cited < required {
            return Err(GenError::Quality(format!(
                "{context} still cites only {cited} of the {required} retrieved sources it must \
                 attribute, after one same-provider correction"
            )));
        }
        corrected.resources = self
            .verified_reading_list(&corrected.resources, sources, seeds)
            .await;
        Ok(corrected)
    }

    pub async fn generate_course(
        &self,
        request: CourseRequest<'_>,
    ) -> Result<(GeneratedCourse, String)> {
        let scoped = self.scoped("lesson");
        let curriculum_json = serde_json::to_string_pretty(request.curriculum)
            .map_err(|error| GenError::Parse(format!("could not serialize curriculum: {error}")))?;
        let gathered = scoped.research_for(&request).await;
        let sources = gathered.sources.clone();
        let prompt = with_teacher(
            request.dossier,
            &format!(
                "AUTHORITATIVE CURRICULUM BRIEF:\n{curriculum_json}\n\n{}\n\n{}",
                crate::research::format_source_material(&sources),
                COURSE_PROMPT
                    .replace("{{TITLE}}", request.title)
                    .replace("{{CATEGORY}}", request.category)
            ),
            request.focus,
        );
        let agent = scoped.current_agent();
        let custom_bin = scoped.current_custom_bin();
        let model = scoped.current_model();
        let context = format!("primary {} course", request.focus);
        let course = scoped
            .write_course(&prompt, &agent, &custom_bin, &model, &context)
            .await?;
        let course = scoped
            .ensure_course_quality(
                course,
                &agent,
                &custom_bin,
                &model,
                &context,
                request.budget,
            )
            .await?;
        let course = scoped
            .ensure_source_grounding(
                course,
                &sources,
                &request.curriculum.primary_sources,
                &agent,
                &custom_bin,
                &model,
                &context,
                request.budget,
            )
            .await?;
        let course = scoped
            .edit_course_quality(
                course,
                CourseEditContext {
                    curriculum: request.curriculum,
                    dossier: request.dossier,
                    agent: &agent,
                    custom_bin: &custom_bin,
                    model: &model,
                    label: &context,
                    budget: request.budget,
                },
                &sources,
            )
            .await?;
        let mut course = course;
        if sources.is_empty() {
            course.research_note = Some(Self::research_note(&gathered));
        }
        Ok((course, agent))
    }

    /// Generate an advisory classroom lesson with a subject-owned provider,
    /// model, and versioned prompt contract. The shared course schema and
    /// deterministic validator remain the quality boundary.
    pub async fn generate_classroom_course(
        &self,
        request: CourseRequest<'_>,
        subject_contract: &str,
        profile: &GenerationProfile,
    ) -> Result<(GeneratedCourse, String)> {
        let mut scoped = self.scoped("lesson");
        scoped.owner = Some(format!("catalog:{}", profile.subject_id));
        let curriculum_json = serde_json::to_string_pretty(request.curriculum)
            .map_err(|error| GenError::Parse(format!("could not serialize curriculum: {error}")))?;
        let gathered = scoped.research_for(&request).await;
        let sources = gathered.sources.clone();
        let task = format!(
            "{subject_contract}\n\nPROMPT_PROFILE_VERSION: {}\n\n\
             AUTHORITATIVE CURRICULUM BRIEF:\n{curriculum_json}\n\n{}\n\n{}",
            profile.prompt_version,
            crate::research::format_source_material(&sources),
            COURSE_PROMPT
                .replace("{{TITLE}}", request.title)
                .replace("{{CATEGORY}}", request.category)
        );
        let task = format!("{task}\n\n{}", request.budget.prompt_line());
        let prompt = with_teacher(request.dossier, &task, request.focus);
        // The editor reads the context, not the contract, so the level rides
        // along in it: a beginner lesson must not be corrected back into depth.
        let context = if subject_contract.contains("LEARNER LEVEL: ABSOLUTE BEGINNER") {
            format!(
                "classroom course for {} for an absolute beginner",
                profile.subject_id
            )
        } else {
            format!("classroom course for {}", profile.subject_id)
        };
        let course = scoped
            .write_course(
                &prompt,
                &profile.agent,
                &profile.custom_bin,
                &profile.model,
                &context,
            )
            .await?;
        let course = scoped
            .ensure_course_quality(
                course,
                &profile.agent,
                &profile.custom_bin,
                &profile.model,
                &context,
                request.budget,
            )
            .await?;
        let course = scoped
            .ensure_source_grounding(
                course,
                &sources,
                &request.curriculum.primary_sources,
                &profile.agent,
                &profile.custom_bin,
                &profile.model,
                &context,
                request.budget,
            )
            .await?;
        let course = scoped
            .edit_course_quality(
                course,
                CourseEditContext {
                    curriculum: request.curriculum,
                    dossier: request.dossier,
                    agent: &profile.agent,
                    custom_bin: &profile.custom_bin,
                    model: &profile.model,
                    label: &context,
                    budget: request.budget,
                },
                &sources,
            )
            .await?;
        let mut course = course;
        if sources.is_empty() {
            course.research_note = Some(Self::research_note(&gathered));
        }
        Ok((course, profile.agent.clone()))
    }

    pub(crate) async fn enrich_classroom_language_lesson(
        &self,
        subject_contract: &str,
        profile: &GenerationProfile,
        seed: &crate::language::StoredLesson,
    ) -> (crate::language::StoredLesson, String) {
        let mut scoped = self.scoped("language-lesson");
        scoped.owner = Some(format!("catalog:{}", profile.subject_id));
        let seed_json = serde_json::to_string_pretty(seed).unwrap_or_default();
        let task = format!(
            r#"{subject_contract}

PROMPT_PROFILE_VERSION: {version}

Enrich the curated lesson JSON below. Return ONLY one valid JSON object with
the exact same keys and value types. Keep `scenario`, `can_do`, `phase_label`,
and all `questions` semantically unchanged. The questions are curated
assessment evidence; do not rewrite their answers. Expand `markdown` into a
clear 450+ word lesson with examples and simple analogies. Keep at least six
phrases and six dialogue turns. Make the speaking, writing, and listening tasks
specific enough to perform and self-check. If the curated markdown contains a
`## First principles foundation` section, preserve and expand it in the same
position before the model dialogue.

CURATED_LESSON:
{seed_json}"#,
            version = profile.prompt_version,
        );
        let prompt = prepend_first_principles(&task, PedagogyDomain::Language);
        match scoped
            .run_with_fallback_for::<crate::language::StoredLesson>(
                &profile.agent,
                &profile.custom_bin,
                &prompt,
                false,
                Duration::from_secs(360),
                &profile.model,
            )
            .await
        {
            Ok((mut generated, source)) => {
                // Curated objectives and assessment truth are immutable. The
                // subject agent may enrich teaching material, not move gates.
                generated.scenario = seed.scenario.clone();
                generated.can_do = seed.can_do.clone();
                generated.phase_label = seed.phase_label.clone();
                generated.questions = seed.questions.clone();
                match crate::language::validate_generated_lesson(seed, &generated) {
                    Ok(()) => (generated, source),
                    Err(reason) => {
                        log::warn!(
                            "language class {} failed quality gate ({reason}); using curated lesson",
                            profile.subject_id
                        );
                        (seed.clone(), "curated".into())
                    }
                }
            }
            Err(error) => {
                log::warn!(
                    "language class {} generation failed ({error}); using curated lesson",
                    profile.subject_id
                );
                (seed.clone(), "curated".into())
            }
        }
    }

    pub async fn generate_quiz(
        &self,
        course_markdown: &str,
        dossier: &str,
        focus: &str,
        _preferred_title: &str,
    ) -> Result<(Vec<GeneratedQuestion>, String)> {
        let scoped = self.scoped("retrieval-quiz");
        let prompt = with_teacher(
            dossier,
            &QUIZ_PROMPT.replace("{{COURSE}}", course_markdown),
            focus,
        );
        let agent = scoped.current_agent();
        let custom_bin = scoped.current_custom_bin();
        let model = scoped.current_model();
        let (quiz, source) = scoped
            .run_exact_for::<GeneratedQuiz>(
                &agent,
                &custom_bin,
                &prompt,
                false,
                Duration::from_secs(180),
                &model,
            )
            .await?;
        let Err(reason) = validate_generated_quiz(&quiz.questions) else {
            return Ok((quiz.questions, source));
        };
        let draft = serde_json::to_string(&quiz)
            .map_err(|error| GenError::Parse(format!("could not serialize quiz: {error}")))?;
        let correction = format!(
            "Correct this quiz from the same configured course and provider. It failed a \
             deterministic gate: {reason}. Return one bare JSON object with exactly five \
             `questions`: three valid four-choice MCQs and two valid free-response questions. \
             Keep every question grounded in the supplied course and learner dossier; do not \
             substitute unrelated bundled material.\n\nCOURSE:\n{course_markdown}\n\n\
             LEARNER_DOSSIER:\n{dossier}\n\nQUIZ_TO_CORRECT:{draft}"
        );
        let (corrected, _) = scoped
            .run_exact_for::<GeneratedQuiz>(
                &agent,
                &custom_bin,
                &correction,
                false,
                Duration::from_secs(180),
                &model,
            )
            .await?;
        validate_generated_quiz(&corrected.questions).map_err(|remaining| {
            GenError::Parse(format!(
                "quiz failed validation after one same-provider correction: {remaining}"
            ))
        })?;
        Ok((corrected.questions, source))
    }

    /// Two-host dialogue script for audio-lesson mode. Spoken-register
    /// rewrite of the course; rendered or speech-synthesized by the caller.
    pub async fn generate_audio_script(
        &self,
        course_markdown: &str,
        focus: &str,
    ) -> Result<Vec<crate::audio::ScriptLine>> {
        let scoped = self.scoped("narration");
        let prompt = with_pedagogy(
            &AUDIO_PROMPT.replace("{{COURSE}}", course_markdown),
            focus,
            PedagogyDomain::Engineering,
        );
        let (script, _) = scoped
            .run_with_fallback::<crate::audio::GeneratedScript>(
                &prompt,
                false,
                Duration::from_secs(300),
                &scoped.current_model(),
            )
            .await?;
        Ok(script.lines)
    }

    /// Generate one adaptive exit-check round, excluding every prior prompt.
    /// `failed_areas` — non-empty only after a miss — forces every new
    /// question in the round to target those exact learning objectives
    /// with a fresh scenario, instead of broadly resampling the course.
    pub async fn generate_exit_quiz(
        &self,
        course_markdown: &str,
        focus: &str,
        count: usize,
        exclusions: &[String],
        failed_areas: &[FailedArea],
    ) -> Result<Vec<ExitCheck>> {
        let scoped = self.scoped("exit-check");
        let excluded = if exclusions.is_empty() {
            "(none)".to_string()
        } else {
            exclusions
                .iter()
                .enumerate()
                .map(|(index, prompt)| format!("{}. {}", index + 1, prompt))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let targeting = format_targeting(failed_areas);
        let prompt = with_pedagogy(
            &EXIT_PROMPT
                .replace("{{COURSE}}", course_markdown)
                .replace("{{COUNT}}", &count.to_string())
                .replace("{{EXCLUSIONS}}", &excluded)
                .replace("{{TARGETING}}", &targeting),
            focus,
            PedagogyDomain::Engineering,
        );
        let agent = scoped.current_agent();
        let custom_bin = scoped.current_custom_bin();
        let model = scoped.current_model();
        let (checks, _) = scoped
            .run_exact_for::<ExitChecks>(
                &agent,
                &custom_bin,
                &prompt,
                false,
                Duration::from_secs(120),
                &model,
            )
            .await?;
        let validate = |questions: &[ExitCheck]| -> std::result::Result<(), String> {
            if questions.len() != count {
                return Err(format!(
                    "expected {count} exit checks, got {}",
                    questions.len()
                ));
            }
            for (index, question) in questions.iter().enumerate() {
                if !usable_mcq(
                    &question.prompt,
                    Some(&question.choices),
                    &question.correct_answer,
                    &question.explanation,
                ) || question.section.trim().is_empty()
                    || question.learning_objective.trim().is_empty()
                {
                    return Err(format!("exit check {} is incomplete", index + 1));
                }
                if exclusions
                    .iter()
                    .any(|excluded| excluded.trim() == question.prompt.trim())
                {
                    return Err(format!(
                        "exit check {} repeats an excluded prompt",
                        index + 1
                    ));
                }
            }
            Ok(())
        };
        let Err(reason) = validate(&checks.questions) else {
            return Ok(checks.questions);
        };
        let draft = serde_json::to_string(&checks).map_err(|error| {
            GenError::Parse(format!("could not serialize exit checks: {error}"))
        })?;
        let correction = format!(
            "Correct these exit checks using only the same course and failed objectives. The \
             deterministic gate failed: {reason}. Return one bare JSON object with a `questions` \
             array containing exactly {count} complete four-choice checks. Preserve targeted \
             `section` and `learning_objective` metadata, use fresh prompts, and do not substitute \
             unrelated fallback questions.\n\nCOURSE:\n{course_markdown}\n\n\
             TARGETING:\n{targeting}\n\nEXCLUSIONS:\n{excluded}\n\nCHECKS_TO_CORRECT:{draft}"
        );
        let (corrected, _) = scoped
            .run_exact_for::<ExitChecks>(
                &agent,
                &custom_bin,
                &correction,
                false,
                Duration::from_secs(120),
                &model,
            )
            .await?;
        validate(&corrected.questions).map_err(|remaining| {
            GenError::Parse(format!(
                "exit checks failed validation after one same-provider correction: {remaining}"
            ))
        })?;
        Ok(corrected.questions)
    }

    /// Ask the Teacher to choose tomorrow's session type. `eligible` describes
    /// whether pop_quiz is currently allowed (guardrails re-checked by caller).
    /// Failure falls back to a lesson day — planning can never block.
    pub async fn plan_day(&self, dossier: &str, eligible: bool, focus: &str) -> SessionPlan {
        let scoped = self.scoped("session-plan");
        let prompt = with_teacher(
            dossier,
            &PLAN_PROMPT.replace("{{ELIGIBLE}}", if eligible { "yes" } else { "no" }),
            focus,
        );
        match scoped
            .run_with_fallback::<SessionPlan>(
                &prompt,
                false,
                Duration::from_secs(90),
                &scoped.current_model(),
            )
            .await
        {
            Ok((plan, _)) => plan,
            Err(e) => {
                log::warn!("day planning failed, defaulting to lesson: {e}");
                SessionPlan {
                    session_type: "lesson".into(),
                    reason: String::new(),
                }
            }
        }
    }

    /// Grade free-text answers. On total failure returns None — caller shows self-assess mode.
    pub async fn grade(
        &self,
        items: &[GradeItem],
        dossier: &str,
        focus: &str,
    ) -> Option<Vec<Verdict>> {
        let scoped = self.scoped("grading");
        if items.is_empty() {
            return Some(vec![]);
        }
        let items_json = serde_json::to_string_pretty(items).ok()?;
        let prompt = with_teacher(
            dossier,
            &GRADE_PROMPT.replace("{{ITEMS}}", &items_json),
            focus,
        );
        match scoped
            .run_with_fallback::<Verdicts>(
                &prompt,
                false,
                Duration::from_secs(120),
                &scoped.current_model(),
            )
            .await
        {
            Ok((v, _)) => {
                let expected: std::collections::HashSet<i64> =
                    items.iter().map(|item| item.id).collect();
                let actual: std::collections::HashSet<i64> =
                    v.verdicts.iter().map(|verdict| verdict.id).collect();
                let valid = v.verdicts.len() == items.len()
                    && actual == expected
                    && v.verdicts.iter().all(|verdict| {
                        !verdict.feedback.trim().is_empty()
                            && (verdict.correct || !verdict.note.trim().is_empty())
                    });
                if valid {
                    Some(v.verdicts)
                } else {
                    log::warn!("grading response failed completeness quality gate");
                    None
                }
            }
            Err(e) => {
                log::warn!("grading failed, falling back to self-assess: {e}");
                None
            }
        }
    }

    /// Answer one question in the session-only course chat. Grounded strictly
    /// in the supplied course markdown plus the bounded recent conversation —
    /// no web/tools, no facts outside the course. Errors propagate so the
    /// caller can show a retry affordance instead of a fabricated answer.
    pub async fn answer_course_question(
        &self,
        context: &CourseChatContext,
        question: &str,
        history: &[ChatTurn],
    ) -> Result<ChatReply> {
        let profile = GenerationProfile {
            subject_id: context.focus.clone(),
            agent: self.current_agent(),
            model: self.current_model(),
            custom_bin: self.current_custom_bin(),
            prompt_version: "primary.chat.v2".into(),
        };
        self.answer_course_question_for(context, question, history, &profile)
            .await
    }

    /// Classroom chat must stay with that classroom's immutable teacher
    /// profile instead of silently using the global primary provider.
    pub async fn answer_course_question_for(
        &self,
        context: &CourseChatContext,
        question: &str,
        history: &[ChatTurn],
        profile: &GenerationProfile,
    ) -> Result<ChatReply> {
        let mut scoped = self.scoped("course-tutor");
        scoped.owner = Some(format!("catalog:{}", profile.subject_id));
        let headings = course_headings(&context.markdown);
        if headings.is_empty() {
            return Err(GenError::Parse(
                "course chat requires at least one markdown heading".into(),
            ));
        }
        let heading_list = headings
            .iter()
            .map(|heading| format!("- {heading}"))
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = with_pedagogy(
            &CHAT_PROMPT
                .replace("{{COURSE_TITLE}}", &context.title)
                .replace("{{LEARNER_OUTCOME}}", &context.learner_outcome)
                .replace("{{CUMULATIVE_ARTIFACT}}", &context.cumulative_artifact)
                .replace("{{COURSE}}", &context.markdown)
                .replace("{{EXERCISE}}", &context.exercise)
                .replace("{{HEADINGS}}", &heading_list)
                .replace("{{HISTORY}}", &format_chat_history(history))
                .replace("{{QUESTION}}", question),
            &context.focus,
            PedagogyDomain::Engineering,
        );
        scoped.log(format!(
            "{} is answering a {} course-chat question with {} ({})",
            profile.agent, profile.subject_id, profile.model, profile.prompt_version
        ));
        let (mut reply, _) = scoped
            .run_exact_for::<ChatReply>(
                &profile.agent,
                &profile.custom_bin,
                &prompt,
                false,
                Duration::from_secs(90),
                &profile.model,
            )
            .await?;
        let Err(reason) = validate_chat_reply(&mut reply, &headings) else {
            return Ok(reply);
        };

        scoped.log(format!(
            "{} chat answer missed a quality gate; requesting one same-provider correction: {reason}",
            profile.agent
        ));
        let draft = serde_json::to_string(&reply)
            .map_err(|error| GenError::Parse(format!("could not serialize chat reply: {error}")))?;
        let correction = format!(
            "Correct this course-tutor reply using the same provider and model. It failed this \
             deterministic gate: {reason}. Return one bare JSON object with `answer`, `section`, \
             and exactly three `follow_ups`. Preserve the useful explanation, keep it grounded in \
             the supplied course, and choose `section` exactly from this list:\n{heading_list}\n\n\
             ORIGINAL_COURSE_TUTOR_REQUEST:\n{prompt}\n\nREPLY_TO_CORRECT:\n{draft}"
        );
        let (mut corrected, _) = scoped
            .run_exact_for::<ChatReply>(
                &profile.agent,
                &profile.custom_bin,
                &correction,
                false,
                Duration::from_secs(90),
                &profile.model,
            )
            .await?;
        validate_chat_reply(&mut corrected, &headings).map_err(|remaining| {
            GenError::Parse(format!(
                "course chat failed validation after one same-provider correction: {remaining}"
            ))
        })?;
        Ok(corrected)
    }

    pub(crate) async fn run_exact_for<T: serde::de::DeserializeOwned>(
        &self,
        agent: &str,
        custom_bin: &str,
        prompt: &str,
        web_tools: bool,
        timeout: Duration,
        model: &str,
    ) -> Result<(T, String)> {
        let raw = self
            .run_primary_for(agent, custom_bin, prompt, web_tools, timeout, model)
            .await?;
        let parsed = match parse_json_payload::<T>(&raw) {
            Ok(parsed) => parsed,
            Err(parse_error) => {
                self.log(format!(
                    "{agent} returned JSON that did not match the requested structure; requesting one same-provider repair"
                ));
                self.repair_json_for::<T>(agent, custom_bin, &raw, model, prompt)
                    .await
                    .map_err(|repair_error| {
                        GenError::Parse(format!(
                            "configured provider returned unusable JSON ({parse_error}); \
                             its same-provider repair pass also failed ({repair_error})"
                        ))
                    })?
            }
        };
        Ok((parsed, agent.to_string()))
    }

    async fn run_with_fallback<T: serde::de::DeserializeOwned>(
        &self,
        prompt: &str,
        web_tools: bool,
        timeout: Duration,
        model: &str,
    ) -> Result<(T, String)> {
        let agent = self.current_agent();
        self.run_with_fallback_for(
            &agent,
            &self.current_custom_bin(),
            prompt,
            web_tools,
            timeout,
            model,
        )
        .await
    }

    async fn run_with_fallback_for<T: serde::de::DeserializeOwned>(
        &self,
        agent: &str,
        custom_bin: &str,
        prompt: &str,
        web_tools: bool,
        timeout: Duration,
        model: &str,
    ) -> Result<(T, String)> {
        let request = self.provider_request(
            prompt,
            ProviderCall {
                agent,
                custom_bin,
                model,
                web_tools,
                timeout,
                wire: Wire::Json,
            },
        )?;
        let fallback = self.runner.fallback(custom_bin)?;
        let result = self.runner.run(&request, fallback.as_ref()).await?;
        let actual = result.runner.legacy_id();
        let text = crate::prose::scrub(&result.text);
        let parsed = match parse_json_payload::<T>(&text) {
            Ok(parsed) => parsed,
            Err(_) => {
                self.repair_json_for(actual, custom_bin, &text, &result.model, prompt)
                    .await?
            }
        };
        Ok((parsed, actual.into()))
    }

    async fn run_primary_for(
        &self,
        agent: &str,
        custom_bin: &str,
        prompt: &str,
        web_tools: bool,
        timeout: Duration,
        model: &str,
    ) -> Result<String> {
        self.run_wire_for(
            prompt,
            ProviderCall {
                agent,
                custom_bin,
                model,
                web_tools,
                timeout,
                wire: Wire::Json,
            },
        )
        .await
    }

    /// Long lesson bodies are plain Markdown; only the smaller structured
    /// payloads use JSON. This also preserves code without JSON string escaping.
    async fn run_prose_for(
        &self,
        agent: &str,
        custom_bin: &str,
        prompt: &str,
        timeout: Duration,
        model: &str,
    ) -> Result<String> {
        let raw = self
            .run_wire_for(
                prompt,
                ProviderCall {
                    agent,
                    custom_bin,
                    model,
                    web_tools: false,
                    timeout,
                    wire: Wire::Prose,
                },
            )
            .await?;
        Ok(strip_markdown_fence(&raw))
    }

    /// Every answer is scrubbed of the dashes a model reaches for before
    /// anything reads it, outside code; the rest of the writing rules are
    /// held by the gates that check each kind of text.
    async fn run_wire_for(&self, prompt: &str, call: ProviderCall<'_>) -> Result<String> {
        let request = self.provider_request(prompt, call)?;
        self.runner
            .run(&request, None)
            .await
            .map(|result| crate::prose::scrub(&result.text))
    }

    fn provider_request(
        &self,
        prompt: &str,
        call: ProviderCall<'_>,
    ) -> Result<crate::agents::RunRequest> {
        let runner = crate::agents::RunnerId::parse(call.agent)
            .ok_or_else(|| GenError::Api(format!("unknown runner: {}", call.agent)))?;
        let route = crate::agents::Route {
            runner,
            model: crate::agents::effective_model(runner, call.model),
            custom_command: call.custom_bin.into(),
        };
        let mut request = crate::agents::RunRequest::new(route, prompt);
        // The writing rules ride on every call the desk makes, after the
        // purpose's own instructions where it has them.
        let mut system = String::new();
        if matches!(
            self.purpose.as_str(),
            "lesson" | "quality-review" | "json-repair"
        ) {
            system.push_str("You are writing teaching material for an application. Return only the requested text or JSON. Use the source excerpts supplied in the request; qualify claims they do not support. Describe experiments and exercises for the learner to perform, but do not execute them, create files, delegate tasks, or start another research workflow.\n\n");
        }
        system.push_str(crate::prose::CONTRACT);
        request.system = Some(system);
        request.json = matches!(call.wire, Wire::Json);
        if request.json {
            request.output_schema = self.output_schema.clone();
        }
        request.allow_web = call.web_tools;
        request.timeout = call.timeout;
        request.purpose = self.purpose.clone();
        request.owner = self.owner.clone();
        Ok(request)
    }

    pub(crate) fn scoped(&self, purpose: &str) -> Self {
        let agent = self.agent.lock().unwrap();
        let model = self.model.lock().unwrap();
        let custom = self.custom_bin.lock().unwrap();
        let mut scoped = self.clone();
        scoped.purpose = purpose.into();
        scoped.agent = std::sync::Arc::new(std::sync::Mutex::new(agent.clone()));
        scoped.model = std::sync::Arc::new(std::sync::Mutex::new(model.clone()));
        scoped.custom_bin = std::sync::Arc::new(std::sync::Mutex::new(custom.clone()));
        scoped
    }

    async fn repair_json_for<T: serde::de::DeserializeOwned>(
        &self,
        agent: &str,
        custom_bin: &str,
        raw: &str,
        model: &str,
        original_request: &str,
    ) -> Result<T> {
        let scoped = self.scoped("json-repair");
        if raw.chars().count() > 240_000 {
            return Err(GenError::Parse("The response is too large to repair without losing content. Choose a model that follows the requested output structure.".into()));
        }
        let failure = parse_json_payload::<T>(raw)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        let prompt = format!(
            "Repair the response so it is valid JSON and matches the field names, types and \
             nesting required by the original request. The validation error is: {failure}. \
             Preserve valid lesson content, source citations, answers and the requested subject. \
             Correct malformed fields using the original contract and the supplied response; \
             do not summarize the lesson or replace it with another topic. Return ONLY the \
             complete corrected JSON, without markdown fences or commentary.\n\n\
             ORIGINAL_REQUEST:\n{original_request}\n\nMALFORMED_RESPONSE:\n{raw}"
        );
        let out = scoped
            .run_primary_for(
                agent,
                custom_bin,
                &prompt,
                false,
                Duration::from_secs(300),
                model,
            )
            .await?;
        parse_json_payload::<T>(&out)
    }
}

/// Recover JSON using the runner's balanced-value parser, including arrays and
/// escaped braces in lesson markdown. Payload validation remains typed here.
pub fn parse_json_payload<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    crate::agents::parse_json(raw)
        .ok_or_else(|| GenError::Parse("no complete JSON value in the agent response".into()))
        .and_then(|value| serde_json::from_value(value).map_err(|e| GenError::Parse(e.to_string())))
}

pub fn resolve_on_path(name: &str) -> Option<String> {
    crate::agents::process::resolve(name)
}

fn fallback_sources(focus: &str) -> &'static [&'static str] {
    crate::catalog::course(focus).map_or(&[], |course| course.bundled_lessons)
}

/// Every MCQ across the bundled assessment material for `focus`, pooled
/// together. These questions may keep an auxiliary retrieval check usable,
/// but the bundled lessons are never substituted for a generated course.
pub fn fallback_mcq_pool(focus: &str) -> Vec<GeneratedQuestion> {
    fallback_sources(focus)
        .iter()
        .filter_map(|s| serde_json::from_str::<FallbackCourse>(s).ok())
        .flat_map(|c| c.questions)
        .filter(|q| q.kind == "mcq" && q.choices.is_some())
        .collect()
}

/// The bundled reference lesson authored for exactly this topic, if any.
pub fn fallback_for_slug(focus: &str, slug: &str) -> Option<FallbackCourse> {
    fallback_sources(focus)
        .iter()
        .filter_map(|s| serde_json::from_str::<FallbackCourse>(s).ok())
        .find(|course| course.slug == slug)
}

pub fn pick_fallback(focus: &str, preferred_title: &str) -> FallbackCourse {
    let sources = fallback_sources(focus);
    // Retrieval sets are recall material, never a substitute for a lesson.
    let all: Vec<FallbackCourse> = sources
        .iter()
        .filter_map(|s| serde_json::from_str(s).ok())
        .filter(|course: &FallbackCourse| course.kind != "retrieval")
        .collect();
    let lower = preferred_title.to_lowercase();
    let scored = all
        .iter()
        .map(|course| {
            // Slugs carry a course prefix ("js-event-loop"); a single-word slug
            // ("idempotency") is its own token and must never match through an
            // empty phrase.
            let parts: Vec<&str> = course.slug.split('-').collect();
            let tokens: Vec<&str> = if parts.len() > 1 {
                parts[1..].to_vec()
            } else {
                parts
            };
            let phrase = tokens.join(" ");
            let token_score = tokens
                .iter()
                .filter(|token| token.len() >= 3 && lower.contains(*token))
                .count();
            let score = if lower.contains(&course.slug)
                || (!phrase.is_empty() && lower.contains(&phrase))
            {
                100
            } else {
                token_score
            };
            (score, course)
        })
        .max_by_key(|(score, _)| *score);
    if let Some((score, course)) = scored {
        if score > 0 {
            return course.clone();
        }
    }
    let idx = (chrono::Utc::now().timestamp() as usize / 86_400) % all.len().max(1);
    all.into_iter()
        .nth(idx)
        .expect("bundled fallback courses present")
}

#[cfg(test)]
mod targeting_tests {
    use super::{format_targeting, FailedArea};

    #[test]
    fn label_combines_section_and_objective_when_both_present() {
        let area = FailedArea {
            section: "Core mechanics".into(),
            learning_objective: "microtasks drain before the next macrotask".into(),
        };
        assert_eq!(
            area.label(),
            "Core mechanics: microtasks drain before the next macrotask"
        );
    }

    #[test]
    fn label_falls_back_when_fields_are_blank() {
        let empty = FailedArea {
            section: String::new(),
            learning_objective: String::new(),
        };
        assert_eq!(empty.label(), "the missed question");

        let section_only = FailedArea {
            section: "Trade-offs".into(),
            learning_objective: String::new(),
        };
        assert_eq!(section_only.label(), "Trade-offs");
    }

    #[test]
    fn format_targeting_is_generic_without_failed_areas() {
        let out = format_targeting(&[]);
        assert!(out.contains("distribute questions"));
        assert!(!out.to_lowercase().contains("missed"));
    }

    #[test]
    fn format_targeting_names_every_failed_area_and_forbids_other_sections() {
        let areas = vec![
            FailedArea {
                section: "Core mechanics".into(),
                learning_objective: "microtasks drain before the next macrotask".into(),
            },
            FailedArea {
                section: "Trade-offs and failure modes".into(),
                learning_objective: "recursive microtasks can starve rendering".into(),
            },
        ];
        let out = format_targeting(&areas);
        // Every missed objective must be named so the model cannot silently
        // drop one from the next round.
        assert!(out.contains("microtasks drain before the next macrotask"));
        assert!(out.contains("recursive microtasks can starve rendering"));
        assert!(out.contains("EVERY question this round must test one of them"));
        assert!(out.contains("Do not write questions about any other part of the course"));
    }
}

#[cfg(test)]
mod quality_gate_tests {
    use super::{
        normalize_course_headings, validate_generated_course, validate_generated_quiz, Exercise,
        ExitCheck, GeneratedCourse, GeneratedQuestion, REQUIRED_COURSE_SECTION_TITLES,
    };

    fn exit_check(index: usize) -> ExitCheck {
        ExitCheck {
            prompt: format!("What happens in scenario {index}?"),
            choices: vec!["A".into(), "B".into(), "C".into(), "D".into()],
            correct_answer: "A".into(),
            explanation: "A follows from the mechanism and the stated constraint.".into(),
            section: "Core mechanics".into(),
            learning_objective: format!("reason about mechanism {index}"),
        }
    }

    fn valid_quiz() -> Vec<GeneratedQuestion> {
        let mut questions = (0..3)
            .map(|index| GeneratedQuestion {
                prompt: format!("MCQ {index}"),
                kind: "mcq".into(),
                choices: Some(vec!["A".into(), "B".into(), "C".into(), "D".into()]),
                correct_answer: "A".into(),
                explanation: "The mechanism makes A the only valid choice.".into(),
                section: "Core mechanics".into(),
                learning_objective: "apply the mechanism".into(),
            })
            .collect::<Vec<_>>();
        questions.extend((0..2).map(|index| GeneratedQuestion {
            prompt: format!("Free response {index}"),
            kind: "free".into(),
            choices: None,
            correct_answer: "Explain the mechanism and connect the decision to evidence.".into(),
            explanation: "A strong answer names both the mechanism and the constraint.".into(),
            section: "Production architecture lens".into(),
            learning_objective: "defend a design decision".into(),
        }));
        questions
    }

    fn complete_course_markdown() -> String {
        super::fixture_course_markdown(
            "the runtime has stricter ordering rules than the physical comparison.",
        )
    }

    #[test]
    fn quiz_quality_gate_enforces_shape_and_choice_integrity() {
        let mut questions = valid_quiz();
        assert!(validate_generated_quiz(&questions).is_ok());
        questions[0].correct_answer = "not a choice".into();
        assert!(validate_generated_quiz(&questions)
            .unwrap_err()
            .contains("valid four-choice MCQ"));
    }

    #[test]
    fn course_heading_normalization_accepts_semantically_identical_atx_headings() {
        let markdown = "### Why This Matters\n\n## THE SIMPLE VERSION\n\n```md\n### Core Mechanics\n```\n\n#### Core Mechanics\n";
        let normalized = normalize_course_headings(markdown);

        assert!(normalized.starts_with("## Why this matters\n\n## The simple version"));
        assert!(normalized.contains("```md\n### Core Mechanics\n```"));
        assert!(normalized.ends_with("## Core mechanics\n"));
    }

    #[test]
    fn course_quality_gate_rejects_truncated_or_incomplete_payloads() {
        let course = GeneratedCourse {
            title: "A complete course".into(),
            markdown: complete_course_markdown(),
            resources: Vec::new(),
            key_takeaways: vec!["Use evidence.".into()],
            exit_questions: (0..5).map(exit_check).collect(),
            exercise: Some(Exercise {
                title: "Build the production slice".into(),
                instructions: "Implement the smallest useful slice, measure its behavior, test the failure path, document the chosen trade-off, and show how another engineer can run it. "
                    .repeat(6),
                starter_code: None,
                deliverable: Some("A tested artifact plus before-and-after evidence.".into()),
                hints: vec!["Start at the boundary.".into()],
            }),
            review_notes: Vec::new(),
            research_note: None,
        };
        assert!(validate_generated_course(&course).is_ok());

        let mut truncated = course;
        truncated.markdown = "## Why this matters\nToo short.".into();
        assert!(validate_generated_course(&truncated)
            .unwrap_err()
            .contains("too thin"));
    }

    #[test]
    fn a_lesson_budget_deepens_to_an_hour_and_no_further() {
        use super::LessonBudget;
        let base = LessonBudget::for_minutes(30);
        assert_eq!(base, LessonBudget::default());
        assert_eq!(base.scale_percent, 100);
        assert_eq!(base.min_words(), super::MIN_COURSE_WORDS);
        assert_eq!(base.max_words(), super::MAX_COURSE_WORDS);
        assert_eq!(base.target_words(), (1_500, 2_100));
        assert_eq!(base.plan().learn_minutes, 14);
        assert_eq!(base.plan().practice_minutes, 10);

        // Less than the base is still a base lesson: a short slot does not
        // produce a thin one.
        assert_eq!(LessonBudget::for_minutes(10).scale_percent, 100);
        assert_eq!(LessonBudget::for_minutes(10).minutes, 10);

        let mid = LessonBudget::for_minutes(45);
        assert_eq!(mid.scale_percent, 125);
        assert_eq!(mid.min_words(), 1_500);

        let hour = LessonBudget::for_minutes(60);
        assert_eq!(hour.scale_percent, 150);
        assert_eq!(hour.min_words(), 1_800);
        assert_eq!(hour.max_words(), 3_900);
        assert_eq!(
            hour.section_minimum(2),
            360,
            "core mechanics grows with the rest"
        );
        assert_eq!(hour.plan().learn_minutes, 28);

        // Past an hour the words stop growing; the time goes to more topics.
        let afternoon = LessonBudget::for_minutes(240);
        assert_eq!(afternoon.scale_percent, 150);
        assert_eq!(afternoon.minutes, 240, "the real length is still reported");
        assert!(afternoon.prompt_line().contains("60 minutes"));
        assert!(
            hour.prompt_line().contains("1800 words"),
            "{}",
            hour.prompt_line()
        );
        assert!(hour.prompt_line().contains("above 3900 words"));
        assert!(base
            .prompt_line()
            .contains("about 14 to read, 10 to practise and 6 to answer"));
    }

    /// The bundled example of the shape the tutor is asked for. If the gate
    /// ever rejects it, the contract and the example have drifted apart.
    #[test]
    fn the_shaped_reference_lesson_passes_the_gate_and_reads_in_fourteen_minutes() {
        let markdown = include_str!("../seed/lessons/cap-theorem-shaped.md");
        let course = GeneratedCourse {
            title: "CAP in practice".into(),
            markdown: markdown.into(),
            resources: Vec::new(),
            key_takeaways: vec!["Decide per route.".into()],
            exit_questions: (0..5).map(exit_check).collect(),
            exercise: Some(Exercise {
                title: "A partition timeline for one route".into(),
                instructions: "**You will produce:** a partition timeline for one route.\n\n### Steps\n1. Pick the email field or the avatar URL.\n2. Trace one partition through it.\n3. Decide the branch and name the mechanism.\n4. State the number that changes your mind.\n\n### Done when\n- The timeline names the detector and both branches.\n- The policy names a mechanism and an owner.\n- The revisit signal is a measurable number.\n- It reads as one row of the capstone table.".into(),
                starter_code: None,
                deliverable: Some("One route's timeline, policy and revisit signal.".into()),
                hints: vec!["Start from the request that crosses the link.".into()],
            }),
            review_notes: Vec::new(),
            research_note: None,
        };
        validate_generated_course(&course).unwrap();
        let (_, sections) = super::split_course_sections(markdown);
        assert_eq!(crate::lesson_shape::reading_minutes(&sections), 14);
        let words = markdown.split_whitespace().count();
        let (low, high) = super::LessonBudget::default().target_words();
        assert!((low..=high).contains(&words), "{words} words");
    }

    #[test]
    fn the_gate_holds_a_lesson_to_its_session_shape() {
        let base = |markdown: String| {
            GeneratedCourse {
            title: "A shaped course".into(),
            markdown,
            resources: Vec::new(),
            key_takeaways: vec!["Use evidence.".into()],
            exit_questions: (0..5).map(exit_check).collect(),
            exercise: Some(Exercise {
                title: "Build the production slice".into(),
                instructions: "Implement the smallest useful slice, measure its behavior, test the failure path, document the chosen trade-off, and show how another engineer can run it. "
                    .repeat(6),
                starter_code: None,
                deliverable: Some("A tested artifact plus before-and-after evidence.".into()),
                hints: vec!["Start at the boundary.".into()],
            }),
            review_notes: Vec::new(),
            research_note: None,
        }
        };
        assert!(validate_generated_course(&base(complete_course_markdown())).is_ok());

        // Past the ceiling the learner cannot finish inside the session.
        let mut long = complete_course_markdown();
        long.push_str(&" restatement".repeat(super::MAX_COURSE_WORDS));
        let error = validate_generated_course(&base(long)).unwrap_err();
        assert!(
            error.contains("too long") && error.contains("30-minute"),
            "{error}"
        );
        assert!(super::is_length_failure(&error));

        // Every section opens with a reading hint the reader can render.
        let unhinted = complete_course_markdown()
            .replace("*~1 min · Fixture orientation for Mental model.*\n\n", "");
        let error = validate_generated_course(&base(unhinted)).unwrap_err();
        assert!(
            error.contains("without a reading hint line: Mental model"),
            "{error}"
        );
        assert!(super::is_hint_failure(&error));

        // Two diagrams at least: the mental model and the production scenario.
        let undrawn = complete_course_markdown()
            .replace("```mermaid\nsequenceDiagram", "```text\nsequenceDiagram");
        let error = validate_generated_course(&base(undrawn)).unwrap_err();
        assert!(error.contains("1 mermaid diagram(s)"), "{error}");
    }

    #[test]
    fn the_base_lesson_is_too_thin_for_an_hour() {
        let course = GeneratedCourse {
            title: "A complete course".into(),
            markdown: complete_course_markdown(),
            resources: Vec::new(),
            key_takeaways: vec!["Use evidence.".into()],
            exit_questions: (0..5).map(exit_check).collect(),
            exercise: Some(Exercise {
                title: "Build the production slice".into(),
                instructions: "Implement the smallest useful slice, measure its behavior, test the failure path, document the chosen trade-off, and show how another engineer can run it. "
                    .repeat(6),
                starter_code: None,
                deliverable: Some("A tested artifact plus before-and-after evidence.".into()),
                hints: vec!["Start at the boundary.".into()],
            }),
            review_notes: Vec::new(),
            research_note: None,
        };
        assert!(validate_generated_course(&course).is_ok());
        let error =
            super::validate_generated_course_for(&course, super::LessonBudget::for_minutes(60))
                .unwrap_err();
        assert!(
            error.contains("too thin") && error.contains("60-minute"),
            "{error}"
        );
    }

    #[test]
    fn course_quality_gate_rejects_padding_around_a_thin_section() {
        let mut course = GeneratedCourse {
            title: "A padded course".into(),
            markdown: complete_course_markdown(),
            resources: Vec::new(),
            key_takeaways: vec!["Use evidence.".into()],
            exit_questions: (0..5).map(exit_check).collect(),
            exercise: Some(Exercise {
                title: "Build the production slice".into(),
                instructions: "Implement the smallest useful slice, measure its behavior, test the failure path, document the chosen trade-off, and show how another engineer can run it. "
                    .repeat(6),
                starter_code: None,
                deliverable: Some("A tested artifact plus before-and-after evidence.".into()),
                hints: vec!["Start at the boundary.".into()],
            }),
            review_notes: Vec::new(),
            research_note: None,
        };
        let practical_start = course.markdown.find("## Practical exercise").unwrap();
        let takeaways_start = course.markdown.find("## Key takeaways").unwrap();
        course.markdown.replace_range(
            practical_start..takeaways_start,
            "## Practical exercise\n\nDo the exercise.\n\n",
        );
        course.markdown.push_str(&" padding".repeat(400));

        let error = validate_generated_course(&course).unwrap_err();
        assert!(error.contains("## Practical exercise is too thin"));
    }

    #[test]
    fn sections_survive_a_split_and_rebuild_including_fenced_headings() {
        let markdown = complete_course_markdown();
        let with_fence = markdown.replace(
            "## Core mechanics\n\n",
            "## Core mechanics\n\n```md\n## Not a real heading\n```\n\n",
        );

        let mechanics = REQUIRED_COURSE_SECTION_TITLES
            .iter()
            .position(|title| *title == "Core mechanics")
            .expect("the contract has a core mechanics section");
        let (preamble, sections) = super::split_course_sections(&with_fence);
        assert!(sections[mechanics].contains("## Not a real heading"));
        let rebuilt = super::rebuild_course_body(&preamble, &sections);

        // Round-tripping must not change what the deterministic gate measures.
        assert_eq!(
            super::course_section_word_counts(&rebuilt).unwrap(),
            super::course_section_word_counts(&with_fence).unwrap()
        );
        for title in REQUIRED_COURSE_SECTION_TITLES {
            assert!(rebuilt.contains(&format!("## {title}")));
        }
    }

    #[test]
    fn a_whole_answer_wrapped_in_one_fence_is_unwrapped_but_inner_code_is_kept() {
        let wrapped = "```markdown\n## The simple version\n\n```js\nconst x = 1;\n```\n```";
        let unwrapped = super::strip_markdown_fence(wrapped);

        assert!(unwrapped.starts_with("## The simple version"));
        assert!(unwrapped.contains("```js"));
        assert_eq!(
            super::strip_markdown_fence("## Already plain prose"),
            "## Already plain prose"
        );
    }

    #[test]
    fn a_rewritten_section_is_reduced_to_its_own_prose() {
        let raw = "## Core mechanics\n\nThe real explanation.\n\n```md\n## Fenced heading\n```\n\n\
                   ## Runnable experiment\n\nContent that belongs to the next section.";

        let extracted = super::extract_single_section(raw);

        assert!(extracted.starts_with("The real explanation."));
        assert!(extracted.contains("## Fenced heading"));
        assert!(!extracted.contains("belongs to the next section"));
        assert!(!extracted.contains("## Core mechanics"));
        // A canonical title arriving as a subheading still ends the section,
        // because normalization would promote it to `##`.
        assert_eq!(
            super::extract_single_section(
                "Prose that stays.\n\n### Runnable experiment\n\nProse that must not."
            ),
            "Prose that stays."
        );
    }

    #[test]
    fn editorial_audit_requires_valid_scores_and_explicit_issues() {
        let mut value = serde_json::json!({
            "scores":{"coverage_depth":5,"mechanism_depth":5,"specificity":5,"production_transfer":5,"dossier_adherence":5,"exercise_alignment":5,"source_discipline":5},
            "issues":[]
        });
        let review: super::CourseEditorialReview = serde_json::from_value(value.clone()).unwrap();
        review.scores.validate().unwrap();
        assert!(review.issues.is_empty());
        for invalid in [0, 2, 6] {
            value["scores"]["source_discipline"] = serde_json::json!(invalid);
            let weak: super::CourseEditorialReview = serde_json::from_value(value.clone()).unwrap();
            assert!(weak.scores.validate().is_err());
        }
    }

    #[test]
    fn a_repeated_canonical_heading_is_demoted_instead_of_duplicating_a_section() {
        let markdown = format!(
            "{}\n\n### core mechanics\n\nA subheading that repeats a section name.",
            complete_course_markdown()
        );

        let normalized = super::normalize_course_headings(&markdown);

        assert_eq!(normalized.matches("## Core mechanics").count(), 2);
        assert!(normalized.contains("### Core mechanics"));
        assert!(super::course_section_word_counts(&normalized).is_ok());
    }

    #[test]
    fn depth_report_names_the_short_section_and_its_shortfall() {
        let thin = complete_course_markdown().replace(
            &"mechanism evidence decision experiment ".repeat(100),
            "one short paragraph. ",
        );
        let report = super::course_depth_report(&thin, super::LessonBudget::default());

        assert!(report.contains("whole course:"));
        assert!(
            report.contains("SHORT by about"),
            "expected a shortfall in: {report}"
        );
    }
}

#[cfg(all(test, unix))]
mod generation_policy_tests {
    use super::{
        CourseRequest, Exercise, ExitCheck, GenError, GeneratedCourse, GenerationProfile,
        Generator, Resource,
    };
    use std::{
        os::unix::fs::PermissionsExt,
        sync::{Arc, Mutex},
        time::Duration,
    };

    fn test_generator() -> Generator {
        Generator::new(
            "/definitely/not/a/provider".into(),
            None,
            std::env::temp_dir().join("sdr-generation-policy-test"),
            Arc::new(Mutex::new("unused".into())),
            Arc::new(Mutex::new("unused".into())),
            Arc::new(Mutex::new(String::new())),
            Default::default(),
        )
    }

    fn corrected_course() -> GeneratedCourse {
        let markdown =
            super::fixture_course_markdown("runtime ordering is stricter than a physical queue.");
        GeneratedCourse {
            title: "A corrected course".into(),
            markdown,
            resources: Vec::new(),
            key_takeaways: vec!["Use evidence.".into()],
            exit_questions: (0..5)
                .map(|index| ExitCheck {
                    prompt: format!("What follows from mechanism {index}?"),
                    choices: vec!["A".into(), "B".into(), "C".into(), "D".into()],
                    correct_answer: "A".into(),
                    explanation: "A follows from the stated runtime constraint.".into(),
                    section: "Core mechanics".into(),
                    learning_objective: "apply the runtime mechanism".into(),
                })
                .collect(),
            exercise: Some(Exercise {
                title: "Build the production slice".into(),
                instructions: "Implement the smallest useful slice, measure its behavior, test the failure path, document the chosen trade-off, and show how another engineer can run it. "
                    .repeat(6),
                starter_code: None,
                deliverable: Some("A tested artifact plus before-and-after evidence.".into()),
                hints: vec!["Start at the boundary.".into()],
            }),
            review_notes: Vec::new(),
            research_note: None,
        }
    }

    struct WritingProvider {
        dir: std::path::PathBuf,
        command: String,
    }

    impl WritingProvider {
        fn new(responses: &[String]) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("principia-writing-{:032x}", rand::random::<u128>()));
            std::fs::create_dir(&dir).unwrap();
            for (index, response) in responses.iter().enumerate() {
                std::fs::write(dir.join(format!("response-{index}")), response).unwrap();
            }
            let script = dir.join("provider.py");
            std::fs::write(
                &script,
                r#"#!/usr/bin/env python3
import sys,pathlib
root=pathlib.Path(__file__).parent
counter=root/'count'
index=int(counter.read_text()) if counter.exists() else 0
counter.write_text(str(index+1))
(root/f'prompt-{index}').write_text(sys.argv[-1])
print((root/f'response-{index}').read_text(),end='')
"#,
            )
            .unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self {
                command: script.to_str().unwrap().to_owned(),
                dir,
            }
        }

        fn calls(&self) -> usize {
            std::fs::read_to_string(self.dir.join("count"))
                .unwrap()
                .parse()
                .unwrap()
        }

        fn prompt(&self, index: usize) -> String {
            std::fs::read_to_string(self.dir.join(format!("prompt-{index}"))).unwrap()
        }
    }

    impl Drop for WritingProvider {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn metadata_json(course: &GeneratedCourse) -> String {
        let mut metadata = serde_json::to_value(course).unwrap();
        metadata.as_object_mut().unwrap().remove("markdown");
        metadata.to_string()
    }

    fn audit_json(score: u8) -> String {
        serde_json::json!({"scores":{"coverage_depth":score,"mechanism_depth":score,"specificity":score,"production_transfer":score,"dossier_adherence":score,"exercise_alignment":score,"source_discipline":score},"issues":if score<4 {vec![serde_json::json!({"section":"Core mechanics","reason":"Derive the permission check before describing its consequences."})]} else {vec![]}}).to_string()
    }

    #[test]
    fn structured_schemas_require_the_real_metadata_and_audit_fields() {
        fn check(value: &serde_json::Value, schema: &serde_json::Value) {
            if let Some(object) = value.as_object() {
                let actual: std::collections::BTreeSet<_> = object.keys().collect();
                let properties = schema["properties"].as_object().unwrap();
                assert_eq!(actual, properties.keys().collect());
                let required = schema["required"].as_array().unwrap();
                assert_eq!(actual.len(), required.len());
                for key in object.keys() {
                    assert!(required.contains(&serde_json::json!(key)));
                    check(&object[key], &properties[key]);
                }
            } else if let Some(items) = value.as_array() {
                for item in items {
                    check(item, &schema["items"]);
                }
            }
        }
        let mut course = corrected_course();
        course.resources.push(Resource {
            title: "Manual".into(),
            url: "https://www.gnu.org/".into(),
            kind: "docs".into(),
            why: "Primary reference".into(),
        });
        check(
            &serde_json::from_str::<serde_json::Value>(&metadata_json(&course)).unwrap(),
            &super::course_metadata_schema(),
        );
        check(
            &serde_json::from_str::<serde_json::Value>(&audit_json(5)).unwrap(),
            &super::course_audit_schema(),
        );
        let schema = super::course_metadata_schema();
        assert_eq!(schema["properties"]["exit_questions"]["minItems"], 5);
        assert_eq!(
            schema["properties"]["exit_questions"]["items"]["properties"]["choices"]["minItems"],
            4
        );
    }

    #[tokio::test]
    async fn writing_and_read_only_audit_preserve_literal_shell_prose() {
        let mut expected = corrected_course();
        expected
            .markdown
            .push_str("\n```bash\nprintf '%s\\n' \"${value}\" '{model}' '{prompt}'\n```\n");
        let provider = WritingProvider::new(&[
            expected.markdown.clone(),
            metadata_json(&expected),
            audit_json(5),
        ]);
        let generator = test_generator();
        let course = generator
            .write_course(
                "Write a Bash lesson as Markdown.",
                "custom",
                &provider.command,
                "saved-model",
                "Linux Bash",
            )
            .await
            .unwrap();
        assert_eq!(course.markdown.trim(), expected.markdown.trim());
        super::validate_generated_course(&course).unwrap();
        let brief = crate::db::CurriculumBrief::default();
        let accepted = generator
            .edit_course_quality(
                course,
                super::CourseEditContext {
                    curriculum: &brief,
                    dossier: "knows pipelines",
                    agent: "custom",
                    custom_bin: &provider.command,
                    model: "saved-model",
                    label: "Linux Bash",
                    budget: super::LessonBudget::default(),
                },
                &[],
            )
            .await
            .unwrap();
        assert_eq!(accepted.markdown.trim(), expected.markdown.trim());
        assert_eq!(provider.calls(), 3);
        assert!(provider.prompt(1).contains("FULL TEXT"));
        assert!(provider.prompt(2).contains("knows pipelines"));
    }

    #[tokio::test]
    async fn invalid_assessment_repair_keeps_valid_body_and_names_exact_answer_contract() {
        let mut invalid = corrected_course();
        invalid.exit_questions[0].choices = vec![
            "Allow access".into(),
            "Deny access".into(),
            "Change owner".into(),
            "Remove group".into(),
        ];
        invalid.exit_questions[0].correct_answer = "A".into();
        let markdown = invalid.markdown.clone();
        let mut valid = invalid.clone();
        valid.exit_questions[0].correct_answer = "Deny access".into();
        let provider = WritingProvider::new(&[metadata_json(&valid)]);
        let repaired = test_generator()
            .ensure_course_quality(
                invalid,
                "custom",
                &provider.command,
                "saved-model",
                "Linux Bash",
                super::LessonBudget::default(),
            )
            .await
            .unwrap();
        assert_eq!(repaired.markdown, markdown);
        assert_eq!(repaired.exit_questions[0].correct_answer, "Deny access");
        assert_eq!(provider.calls(), 1);
        assert!(provider
            .prompt(0)
            .contains("exit question 1: correct_answer must equal the full text"));
        valid.exit_questions[0].choices[1] = " Allow access ".into();
        assert!(super::validate_generated_course(&valid)
            .unwrap_err()
            .contains("distinct"));
    }

    #[tokio::test]
    async fn one_short_section_does_not_rewrite_other_valid_sections() {
        let mut course = corrected_course();
        let (preamble, mut sections) = super::split_course_sections(&course.markdown);
        // Thin, but shaped: the hint and the diagram stay so only depth fails.
        sections[5] = format!(
            "*~1 min · Fixture orientation for Production architecture lens.*\n\n```mermaid\nsequenceDiagram\n  Client->>Service: request\n```\n\nOne request crosses the boundary.\n\n{}",
            "production decision evidence ".repeat(40)
        );
        let original_sections = sections.clone();
        course.markdown = super::rebuild_course_body(&preamble, &sections);
        assert!(course.markdown.split_whitespace().count() > super::MIN_COURSE_WORDS);
        let provider = WritingProvider::new(&[format!(
            "```mermaid\nsequenceDiagram\n  Client->>Service: request\n```\n\nOne request crosses the boundary.\n\n{}",
            "ownership boundary observation rollback ".repeat(130)
        )]);
        let repaired = test_generator()
            .ensure_course_quality(
                course,
                "custom",
                &provider.command,
                "saved-model",
                "Bash",
                super::LessonBudget::default(),
            )
            .await
            .unwrap();
        let (_, final_sections) = super::split_course_sections(&repaired.markdown);
        for index in 0..10 {
            if index != 5 {
                assert_eq!(
                    final_sections[index].trim(),
                    original_sections[index].trim()
                );
            }
        }
        assert_eq!(provider.calls(), 1);
    }

    #[tokio::test]
    async fn editorial_corrections_require_an_audit_of_the_new_draft() {
        let course = corrected_course();
        let mut revised = course.clone();
        let (preamble, mut sections) = super::split_course_sections(&revised.markdown);
        sections[2].push_str("\nA corrected permission-check derivation.\n");
        let revised_section = sections[2].clone();
        revised.markdown = super::rebuild_course_body(&preamble, &sections);
        let provider = WritingProvider::new(&[
            audit_json(2),
            revised_section,
            metadata_json(&revised),
            audit_json(5),
        ]);
        let brief = crate::db::CurriculumBrief::default();
        let accepted = test_generator()
            .edit_course_quality(
                course,
                super::CourseEditContext {
                    curriculum: &brief,
                    dossier: "",
                    agent: "custom",
                    custom_bin: &provider.command,
                    model: "saved-model",
                    label: "Bash",
                    budget: super::LessonBudget::default(),
                },
                &[],
            )
            .await
            .unwrap();
        assert!(accepted
            .markdown
            .contains("corrected permission-check derivation"));
        assert!(provider
            .prompt(3)
            .contains("corrected permission-check derivation"));
        assert_eq!(provider.calls(), 4);
    }

    #[tokio::test]
    async fn classroom_course_does_not_substitute_another_provider_or_bundled_content() {
        let generator = test_generator();
        let profile = GenerationProfile {
            subject_id: "frontend-architecture".into(),
            agent: "custom".into(),
            model: "configured-model".into(),
            custom_bin: String::new(),
            prompt_version: "test".into(),
        };
        let curriculum = crate::db::CurriculumBrief::default();

        let error = generator
            .generate_classroom_course(
                CourseRequest {
                    title: "Boundaries",
                    category: "architecture",
                    dossier: "",
                    focus: "frontend-architecture",
                    curriculum: &curriculum,
                    budget: super::LessonBudget::default(),
                },
                "Teach the configured subject.",
                &profile,
            )
            .await
            .expect_err("missing configured provider must fail");

        assert!(matches!(error, GenError::NoBinary));
    }

    #[tokio::test]
    async fn exact_generation_repairs_json_with_the_same_configured_provider() {
        let script = std::env::temp_dir().join(format!(
            "sdr-json-repair-provider-{}.sh",
            std::process::id()
        ));
        std::fs::write(
            &script,
            r#"#!/bin/sh
case "$1" in
  *MALFORMED_RESPONSE*) printf '%s' '{"course":"preserved"}' ;;
  *) printf '%s' '{"course":"preserved"' ;;
esac
"#,
        )
        .expect("write fake provider");
        let mut permissions = std::fs::metadata(&script)
            .expect("read fake provider metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("make fake provider executable");

        let generator = test_generator();
        let (value, source) = generator
            .run_exact_for::<serde_json::Value>(
                "custom",
                script.to_str().expect("UTF-8 path"),
                "generate",
                false,
                Duration::from_secs(5),
                "configured-model",
            )
            .await
            .expect("same-provider repair should succeed");

        assert_eq!(value["course"], "preserved");
        assert_eq!(source, "custom");
        let _ = std::fs::remove_file(script);
    }

    #[tokio::test]
    async fn schema_repair_receives_the_contract_error_and_complete_long_response() {
        #[derive(serde::Deserialize)]
        struct Repaired {
            exit_questions: Vec<ExitCheck>,
            markdown: String,
        }
        let script = std::env::temp_dir().join(format!(
            "principia-schema-repair-{:032x}.py",
            rand::random::<u128>()
        ));
        std::fs::write(&script, r#"#!/usr/bin/env python3
import json,sys
prompt=sys.argv[-1]
if 'MALFORMED_RESPONSE:' in prompt:
    assert 'ORIGINAL_REQUEST:' in prompt
    assert 'exit_questions must contain full question objects' in prompt
    assert 'expected struct ExitCheck' in prompt
    assert 'END_OF_LESSON' in prompt
    print(json.dumps({'exit_questions':[{'prompt':'Which boundary applies?','choices':['A','B','C','D'],'correct_answer':'A','explanation':'The file mode supplies this constraint.','section':'Core mechanics','learning_objective':'Apply file permissions'}],'markdown':'END_OF_LESSON'}))
else:
    print(json.dumps({'exit_questions':['exercise'],'markdown':'x'*65000+' END_OF_LESSON'}))
"#).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let (value, source) = test_generator()
            .run_exact_for::<Repaired>(
                "custom",
                script.to_str().unwrap(),
                "exit_questions must contain full question objects, plus markdown",
                false,
                Duration::from_secs(30),
                "same-model",
            )
            .await
            .unwrap();
        assert_eq!(source, "custom");
        assert_eq!(value.exit_questions[0].correct_answer, "A");
        assert_eq!(value.markdown, "END_OF_LESSON");
        std::fs::remove_file(script).unwrap();
    }

    #[tokio::test]
    async fn quality_gate_failure_gets_one_same_provider_correction() {
        let script = std::env::temp_dir().join(format!(
            "sdr-quality-repair-provider-{}.sh",
            std::process::id()
        ));
        let response = std::env::temp_dir().join(format!(
            "sdr-quality-repair-response-{}.json",
            std::process::id()
        ));
        std::fs::write(&script, "#!/bin/sh\ncat \"$1\"\n").expect("write fake provider");
        std::fs::write(&response, corrected_course().markdown)
            .expect("write fake provider response");
        let mut permissions = std::fs::metadata(&script)
            .expect("read fake provider metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("make fake provider executable");

        let mut invalid = corrected_course();
        invalid.markdown = invalid.markdown.replace(
            "Where the analogy breaks: runtime ordering is stricter than a physical queue.",
            "The comparison is useful, but runtime ordering is stricter than a physical queue.",
        );
        let custom_bin = format!("{} {}", script.display(), response.display());
        let repaired = test_generator()
            .ensure_course_quality(
                invalid,
                "custom",
                &custom_bin,
                "configured-model",
                "test classroom course",
                super::LessonBudget::default(),
            )
            .await
            .expect("same-provider quality correction should succeed");

        assert!(repaired.markdown.contains("Where the analogy breaks:"));
        let _ = std::fs::remove_file(script);
        let _ = std::fs::remove_file(response);
    }

    fn retrieved(url: &str, host: &str) -> crate::research::ResearchSource {
        crate::research::ResearchSource {
            url: url.into(),
            title: format!("{host} document"),
            host: host.into(),
            excerpt: "Retrieved documentation excerpt used to write the lesson.".into(),
            words: 400,
            primary: true,
        }
    }

    #[tokio::test]
    async fn grounding_publishes_retrieved_links_and_drops_invented_ones() {
        let generator = test_generator();
        let sources = vec![
            retrieved(
                "https://developer.mozilla.org/en-US/docs/Web/API/Navigation_API",
                "developer.mozilla.org",
            ),
            retrieved("https://web.dev/articles/vitals", "web.dev"),
        ];
        for source in &sources {
            generator.researcher.prime(source.clone());
        }
        let mut course = corrected_course();
        course.markdown.push_str(&format!(
            "\n\nSee [MDN]({}) and [web.dev]({}).\n",
            sources[0].url, sources[1].url
        ));
        course.resources = vec![Resource {
            title: "Invented deep dive".into(),
            url: "https://sources-that-do-not-exist.invalid/guide".into(),
            kind: "article".into(),
            why: "hallucinated".into(),
        }];

        let grounded = generator
            .ensure_source_grounding(
                course,
                &sources,
                &[],
                "custom",
                "/definitely/not/a/provider",
                "configured-model",
                "grounding test",
                super::LessonBudget::default(),
            )
            .await
            .expect("a course citing its sources needs no correction");

        let urls: Vec<&str> = grounded
            .resources
            .iter()
            .map(|resource| resource.url.as_str())
            .collect();
        assert_eq!(urls, vec![sources[0].url.as_str(), sources[1].url.as_str()]);
    }

    #[tokio::test]
    async fn grounding_is_skipped_when_retrieval_itself_failed() {
        let mut course = corrected_course();
        course.resources = Vec::new();

        let grounded = test_generator()
            .ensure_source_grounding(
                course,
                &[],
                &[],
                "custom",
                "/definitely/not/a/provider",
                "configured-model",
                "offline grounding test",
                super::LessonBudget::default(),
            )
            .await
            .expect("a network failure must not fail the lesson");

        assert!(grounded.resources.is_empty());
    }

    #[tokio::test]
    async fn uncited_course_fails_after_one_same_provider_citation_correction() {
        let script = std::env::temp_dir().join(format!(
            "sdr-citation-repair-provider-{}.sh",
            std::process::id()
        ));
        let response = std::env::temp_dir().join(format!(
            "sdr-citation-repair-response-{}.json",
            std::process::id()
        ));
        std::fs::write(&script, "#!/bin/sh\ncat \"$1\"\n").expect("write fake provider");
        // The provider returns a structurally valid course that still cites nothing.
        std::fs::write(&response, corrected_course().markdown)
            .expect("write fake provider response");
        let mut permissions = std::fs::metadata(&script)
            .expect("read fake provider metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("make fake provider executable");

        let generator = test_generator();
        let sources = vec![
            retrieved(
                "https://developer.mozilla.org/en-US/docs/Web/API/Navigation_API",
                "developer.mozilla.org",
            ),
            retrieved("https://web.dev/articles/vitals", "web.dev"),
            retrieved("https://v8.dev/blog/hidden-classes", "v8.dev"),
        ];
        for source in &sources {
            generator.researcher.prime(source.clone());
        }
        let mut course = corrected_course();
        course.resources = Vec::new();

        let custom_bin = format!("{} {}", script.display(), response.display());
        let error = generator
            .ensure_source_grounding(
                course,
                &sources,
                &[],
                "custom",
                &custom_bin,
                "configured-model",
                "citation gate test",
                super::LessonBudget::default(),
            )
            .await
            .expect_err("an uncited course must not reach the learner");

        assert!(
            matches!(&error, GenError::Quality(reason) if reason.contains("still cites only 0")),
            "unexpected error: {error:?}"
        );
        let _ = std::fs::remove_file(script);
        let _ = std::fs::remove_file(response);
    }
}

#[cfg(test)]
mod pedagogy_tests {
    use super::{
        prepend_first_principles, with_pedagogy, with_teacher, PedagogyDomain,
        FIRST_PRINCIPLES_PROMPT,
    };

    #[test]
    fn first_principles_contract_wraps_day_one_and_personalized_teaching() {
        assert!(FIRST_PRINCIPLES_PROMPT.contains("first-principles.v1"));

        let day_one = with_teacher("", "TASK", "javascript");
        assert!(day_one.contains("PEDAGOGY CONTRACT: first-principles.v1"));
        assert!(day_one.contains("primitive runtime facts and constraints"));
        assert!(day_one.ends_with("TASK"));

        let personalized = with_teacher("Day 4: event loop is struggling", "TASK", "javascript");
        assert!(personalized.contains("STUDENT DOSSIER"));
        assert!(personalized.contains("Day 4: event loop is struggling"));
        assert!(personalized.contains("PEDAGOGY CONTRACT: first-principles.v1"));
    }

    #[test]
    fn every_teaching_domain_uses_the_same_sequence_with_a_domain_adapter() {
        let engineering = with_pedagogy("TASK", "typescript", PedagogyDomain::Engineering);
        let language = prepend_first_principles("TASK", PedagogyDomain::Language);

        for prompt in [&engineering, &language] {
            assert!(prompt.contains("First-principles sequence:"));
            assert!(prompt.contains("identify the missing building block"));
            assert!(!prompt.contains("{{DOMAIN_SEQUENCE}}"));
        }
        assert!(engineering.contains("mechanism → composition and boundaries"));
        assert!(language.contains("symbol and sound → word pattern → sentence frame"));
    }
}

#[cfg(test)]
mod chat_tests {
    use super::{
        course_headings, format_chat_history, validate_chat_reply, ChatReply, ChatTurn,
        CHAT_HISTORY_TURNS, CHAT_PROMPT,
    };

    #[test]
    fn empty_history_reads_as_first_question() {
        assert_eq!(
            format_chat_history(&[]),
            "(none yet, this is the first question)"
        );
    }

    #[test]
    fn history_renders_speaker_labels_in_order() {
        let history = vec![
            ChatTurn::user("Why does the timer fire last?".into()),
            ChatTurn::assistant(ChatReply {
                answer: "Because microtasks drain first.".into(),
                section: "The precise model".into(),
                follow_ups: Vec::new(),
            }),
        ];
        let rendered = format_chat_history(&history);
        assert_eq!(
            rendered,
            "Student: Why does the timer fire last?\nTutor: Because microtasks drain first."
        );
    }

    #[test]
    fn history_is_bounded_to_the_most_recent_turns() {
        let history: Vec<ChatTurn> = (0..(CHAT_HISTORY_TURNS + 4))
            .map(|i| {
                if i % 2 == 0 {
                    ChatTurn::user(format!("turn {i}"))
                } else {
                    ChatTurn::assistant(ChatReply {
                        answer: format!("turn {i}"),
                        section: "Mechanism".into(),
                        follow_ups: Vec::new(),
                    })
                }
            })
            .collect();
        let rendered = format_chat_history(&history);
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), CHAT_HISTORY_TURNS);
        // Oldest turns are dropped; the most recent one is always last.
        assert!(lines
            .last()
            .unwrap()
            .contains(&format!("turn {}", history.len() - 1)));
        assert!(!rendered.contains("turn 0\n") && !rendered.starts_with("turn 0"));
    }

    #[test]
    fn chat_prompt_has_exactly_one_of_each_required_placeholder() {
        for token in [
            "{{COURSE_TITLE}}",
            "{{LEARNER_OUTCOME}}",
            "{{CUMULATIVE_ARTIFACT}}",
            "{{COURSE}}",
            "{{EXERCISE}}",
            "{{HEADINGS}}",
            "{{HISTORY}}",
            "{{QUESTION}}",
        ] {
            assert_eq!(
                CHAT_PROMPT.matches(token).count(),
                1,
                "expected exactly one {token} in chat.txt"
            );
        }
        assert!(
            CHAT_PROMPT.matches("{{FOCUS_LABEL}}").count() >= 1,
            "expected at least one {{{{FOCUS_LABEL}}}} in chat.txt"
        );
    }

    #[test]
    fn chat_reply_gate_requires_real_section_and_three_distinct_follow_ups() {
        let headings =
            course_headings("# Event loop\n\n## The precise model\n\n## Tool-building exercise\n");
        let mut reply = ChatReply {
            answer: "Microtasks run at a checkpoint after the current stack empties, so tracing the queue explains why the timer callback appears later than the promise continuation.".into(),
            section: "the precise model".into(),
            follow_ups: vec![
                "Can you trace the queue order?".into(),
                "Which observation would prove this mechanism?".into(),
                "How does the exercise expose the checkpoint?".into(),
            ],
        };
        validate_chat_reply(&mut reply, &headings).unwrap();
        assert_eq!(reply.section, "The precise model");

        reply.follow_ups[2] = reply.follow_ups[0].clone();
        assert!(validate_chat_reply(&mut reply, &headings).is_err());
    }
}

#[cfg(test)]
mod exercise_tests {
    use super::{
        extract_tool_building_exercise, fallback_sources, pick_fallback, validate_generated_quiz,
    };

    #[test]
    fn extracts_tool_building_section_when_present() {
        let markdown = "## Intro\n\nSome text.\n\n## Tool-building exercise\n\nBuild a tiny tracer that logs call order.\n\n## Key takeaways\n\n- one\n";
        let exercise = extract_tool_building_exercise(markdown).expect("exercise present");
        assert_eq!(exercise.title, "Tool-building exercise");
        assert!(exercise.instructions.contains("Build a tiny tracer"));
        assert!(!exercise.instructions.contains("Key takeaways"));
        assert!(exercise.starter_code.is_none());
        assert!(exercise.hints.is_empty());
    }

    #[test]
    fn extracts_current_practical_exercise_section() {
        let markdown = "## Migration and observability\n\nWatch it.\n\n## Practical exercise\n\nWrite an ADR and a boundary test.\n\n## Key takeaways\n\n- one\n";
        let exercise = extract_tool_building_exercise(markdown).expect("exercise present");
        assert_eq!(exercise.title, "Practical exercise");
        assert!(exercise
            .instructions
            .contains("Write an ADR and a boundary test"));
        assert!(!exercise.instructions.contains("Key takeaways"));
    }

    #[test]
    fn extract_tool_building_exercise_is_none_without_marker() {
        let markdown = "## Intro\n\nNo exercise heading here at all.\n";
        assert!(extract_tool_building_exercise(markdown).is_none());
    }

    #[test]
    fn extract_tool_building_exercise_is_none_when_section_is_empty() {
        let markdown = "## Tool-building exercise\n\n## Key takeaways\n\n- one\n";
        assert!(extract_tool_building_exercise(markdown).is_none());
    }

    #[test]
    fn bundled_fallback_courses_carry_a_structured_exercise_with_deliverable() {
        for focus in crate::focus::selectable() {
            let fb = pick_fallback(focus, "");
            let exercise = fb
                .exercise
                .unwrap_or_else(|| panic!("{focus} fallback course missing structured exercise"));
            assert!(!exercise.title.is_empty(), "{focus} exercise title empty");
            assert!(
                !exercise.instructions.is_empty(),
                "{focus} exercise instructions empty"
            );
            assert!(
                exercise
                    .deliverable
                    .as_deref()
                    .is_some_and(|d| !d.is_empty()),
                "{focus} exercise missing a non-empty deliverable"
            );
            assert!(!exercise.hints.is_empty(), "{focus} exercise missing hints");
        }
    }

    #[test]
    fn every_selectable_fallback_has_sources_and_a_full_quiz() {
        for focus in crate::focus::selectable() {
            for source in fallback_sources(focus) {
                let fallback: super::FallbackCourse =
                    serde_json::from_str(source).expect("fallback JSON must parse");
                if fallback.kind == "retrieval" {
                    // Recall sets: five usable MCQs, no lesson structure, never picked as a lesson.
                    assert_eq!(
                        fallback.questions.len(),
                        5,
                        "{} retrieval set size",
                        fallback.slug
                    );
                    assert!(
                        fallback.questions.iter().all(|q| q.kind == "mcq"
                            && super::usable_mcq(
                                &q.prompt,
                                q.choices.as_deref(),
                                &q.correct_answer,
                                &q.explanation
                            )),
                        "{} retrieval questions must be usable MCQs",
                        fallback.slug
                    );
                    assert_ne!(
                        super::pick_fallback(focus, &fallback.title).slug,
                        fallback.slug
                    );
                    continue;
                }
                assert!(
                    fallback.resources.len() >= 3,
                    "{} needs at least three verified resources",
                    fallback.slug
                );
                let markdown = fallback.markdown.to_lowercase();
                let simple = markdown
                    .find("## the simple version")
                    .unwrap_or_else(|| panic!("{} lacks a simple foundation", fallback.slug));
                let mechanics = markdown
                    .find("## core mechanics")
                    .unwrap_or_else(|| panic!("{} lacks derived core mechanics", fallback.slug));
                assert!(
                    simple < mechanics,
                    "{} explains mechanics before its simple foundation",
                    fallback.slug
                );
                assert!(
                    markdown[simple..mechanics].contains("analogy breaks"),
                    "{} does not explain where its opening analogy breaks",
                    fallback.slug
                );
                // The first-principles chain ends in application, not
                // vocabulary: every bundled course carries a runnable
                // experiment and a tool-building exercise as the
                // reconstruction check.
                assert!(
                    markdown.contains("## runnable experiment"),
                    "{} lacks a runnable experiment",
                    fallback.slug
                );
                assert!(
                    markdown.contains("## tool-building exercise")
                        || markdown.contains("## practical exercise"),
                    "{} lacks its reconstruction-style exercise",
                    fallback.slug
                );
                validate_generated_quiz(&fallback.questions)
                    .unwrap_or_else(|error| panic!("{} quiz: {error}", fallback.slug));
            }
        }
    }
}

#[cfg(test)]
mod deepseek_tests {
    fn parse_deepseek_response(raw: &str) -> super::Result<String> {
        let request = crate::agents::RunRequest::new(
            crate::agents::Route {
                runner: crate::agents::RunnerId::DeepseekApi,
                model: "fixture".into(),
                custom_command: String::new(),
            },
            "fixture",
        );
        crate::agents::adapters::parse_compatible(serde_json::from_str(raw).unwrap(), &request)
            .map(|result| result.text)
    }

    #[test]
    fn extracts_chat_completion_content() {
        let raw = r#"{"choices":[{"message":{"content":"{\"status\":\"pong\"}"}}]}"#;
        assert_eq!(
            parse_deepseek_response(raw).unwrap(),
            r#"{"status":"pong"}"#
        );
    }

    #[test]
    fn rejects_empty_chat_completion() {
        let raw = r#"{"choices":[{"message":{"content":""}}]}"#;
        assert!(parse_deepseek_response(raw).is_err());
    }
}
