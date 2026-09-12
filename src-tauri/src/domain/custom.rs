//! A learner's own classes: the brief they write, the draft the tutor or
//! they produce, the checks it is held to, and publishing it as a course
//! the rest of the desk teaches like a bundled one.
//!
//! A draft is always saveable. A course is published only when every check
//! passes: then its definition joins the catalog registry, its topics go
//! into `concepts` under the course id, and it gets a classroom program.

use crate::catalog::{self, OwnedCourse, OwnedEntryPoint};
use crate::db::{CurriculumBrief, DbError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The four stages every course has, in order. Custom courses name them
/// as they like but keep the ids, so phases, tiers, placement and the
/// curriculum map work unchanged.
pub const STAGES: [(&str, &str); 4] = [
    ("foundations", "Foundations"),
    ("mechanisms", "Core mechanisms"),
    ("production", "Practical application"),
    ("synthesis", "Integration and capstone"),
];

pub const MIN_TOPICS: usize = 6;
pub const MAX_TOPICS: usize = 60;

/// What the learner wrote before anything was drafted.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CourseBrief {
    pub title: String,
    pub outcome: String,
    #[serde(default)]
    pub background: String,
    #[serde(default)]
    pub trusted_hosts: Vec<String>,
    /// The tutor that drafts and later teaches the class.
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub custom_agent_bin: String,
}

/// One topic of the draft. The slug is bare; the course id is prefixed
/// when the course is published.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DraftTopic {
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub prereqs: Vec<String>,
    #[serde(default)]
    pub curriculum: CurriculumBrief,
}

/// The editable course: what the builder shows and the tutor writes.
/// Every field defaults so the tutor's answer, which never carries the id
/// and may miss a field, still parses and is then held to the validator.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CourseDraft {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub native_label: String,
    #[serde(default)]
    pub short_code: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub environment: String,
    #[serde(default)]
    pub source_hosts: Vec<String>,
    /// The four stages with the labels this course gives them.
    #[serde(default)]
    pub entry_points: Vec<OwnedEntryPoint>,
    #[serde(default)]
    pub topics: Vec<DraftTopic>,
}

/// One thing the validator objects to, addressed to a field or a topic so
/// the editor can point at it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DraftIssue {
    /// A header field name, or `topics/<slug>` for a topic.
    pub at: String,
    pub message: String,
}

/// What the tutor said about the draft when asked to read it back, and
/// what the learner did about it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewFinding {
    /// `high`, `medium` or `low`.
    #[serde(default)]
    pub severity: String,
    /// The bare slug of the topic concerned, or empty for the course.
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub fix: String,
    /// `open`, `fixed` or `dismissed`. Every finding has to leave `open`
    /// before the class can be published.
    #[serde(default = "open")]
    pub status: String,
    /// How it was settled: what the tutor changed, "by hand", or why it
    /// was dismissed.
    #[serde(default)]
    pub note: String,
    /// Settled in an earlier read and not raised again since; kept so what
    /// was decided stays in view.
    #[serde(default)]
    pub carried: bool,
}

fn open() -> String {
    "open".into()
}

/// The words a finding is recognised by across reads.
fn finding_words(message: &str) -> HashSet<String> {
    message
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(str::to_string)
        .collect()
}

/// Whether a fresh finding is the same objection as an earlier one: the
/// same topic, and messages that share most of their words.
fn same_finding(a: &ReviewFinding, b: &ReviewFinding) -> bool {
    if a.topic != b.topic {
        return false;
    }
    if a.message.trim().eq_ignore_ascii_case(b.message.trim()) {
        return true;
    }
    let (x, y) = (finding_words(&a.message), finding_words(&b.message));
    let shared = x.intersection(&y).count();
    let all = x.union(&y).count();
    all > 0 && shared * 2 >= all
}

/// A fresh read against the earlier ones. A fresh finding that repeats a
/// settled one keeps its settlement, so a dismissal or a fix is not undone
/// by reading again. Earlier findings the tutor does not raise again are
/// carried at the end, marked as such: settled ones as decided history,
/// open ones because a later read only confirms and is told not to repeat
/// them, so they stay the learner's to settle.
pub fn merge_reviews(previous: &[ReviewFinding], fresh: Vec<ReviewFinding>) -> Vec<ReviewFinding> {
    let mut used = vec![false; previous.len()];
    let mut merged: Vec<ReviewFinding> = fresh
        .into_iter()
        .map(|mut finding| {
            finding.status = "open".into();
            finding.note.clear();
            finding.carried = false;
            if let Some(index) = previous
                .iter()
                .enumerate()
                .find(|(i, old)| !used[*i] && same_finding(old, &finding))
                .map(|(i, _)| i)
            {
                used[index] = true;
                finding.status = previous[index].status.clone();
                finding.note = previous[index].note.clone();
            }
            finding
        })
        .collect();
    for (index, old) in previous.iter().enumerate() {
        if !used[index] {
            let mut kept = old.clone();
            kept.carried = true;
            merged.push(kept);
        }
    }
    merged
}

/// One primary source and whether the desk could fetch it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceCheck {
    pub topic: String,
    pub url: String,
    /// `reachable`, `unreachable` or `off-host`.
    pub state: String,
    /// The learner keeps an unreachable source knowingly; a lesson that
    /// cannot fetch it says so.
    #[serde(default)]
    pub accepted: bool,
}

/// The marks a class collects on its way to being published: hashes of
/// the draft as it stood when each check was made.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Marks {
    /// The draft the tutor last read back.
    #[serde(default)]
    pub review_hash: String,
    /// The draft the learner confirmed having read themselves.
    #[serde(default)]
    pub read_hash: String,
}

/// Where a class stands against what publishing needs, worked out from the
/// draft, the review, the source check and the marks. The interface draws
/// the Verify step from it and `publish` refuses on the same blockers.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Checks {
    /// The draft as it stands, so a mark can be compared to it.
    pub draft_hash: String,
    /// The tutor has read a draft back at least once.
    pub reviewed: bool,
    /// The review was made on the draft as it stands now.
    pub review_current: bool,
    pub open_findings: usize,
    /// The sources have been fetched at least once.
    pub fetched: bool,
    /// Sources in the draft that the last fetch did not see.
    pub unchecked_sources: usize,
    /// Unreachable or off-host sources still in the draft and not accepted.
    pub pending_sources: usize,
    /// The learner confirmed reading the draft as it stands now.
    pub read: bool,
    /// Why publishing is refused, in the order the steps come; empty when
    /// the class can be published.
    pub blockers: Vec<String>,
}

/// The course as the builder sees it: everything stored, plus the issues
/// the validator raises against the current draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCourseView {
    pub id: String,
    pub version: i64,
    pub status: String,
    pub origin: String,
    pub brief: CourseBrief,
    pub draft: CourseDraft,
    pub issues: Vec<DraftIssue>,
    pub review: Vec<ReviewFinding>,
    pub sources: Vec<SourceCheck>,
    pub checks: Checks,
    /// The question bank the tutor wrote, if any: three cited questions
    /// per stage in the placement check's shape.
    pub bank: Option<crate::domain::placement::Bank>,
    pub created_at: String,
    pub updated_at: String,
    pub published_at: Option<String>,
    /// A tutor call in flight for this class (`draft`, `review`, `sources`,
    /// `bank`, `fix`), filled in by the command layer.
    #[serde(default)]
    pub working: Option<String>,
    /// The finding a fix in flight is on, filled in by the command layer.
    #[serde(default)]
    pub working_at: Option<usize>,
}

/// A row of the list the Classes page shows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCourseSummary {
    pub id: String,
    pub label: String,
    pub version: i64,
    pub status: String,
    pub origin: String,
    pub topics: usize,
    pub updated_at: String,
    #[serde(default)]
    pub working: Option<String>,
}

fn invalid(message: impl Into<String>) -> DbError {
    DbError::Invalid(message.into())
}

/// Whether an id names a learner's own class.
pub fn is_custom_id(id: &str) -> bool {
    catalog::is_custom(id)
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// A slug from free text: lowercase words joined by hyphens.
pub fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').chars().take(48).collect()
}

/// A course id from a title: the custom prefix and the slug, made unique
/// against the courses already known.
pub fn new_course_id(conn: &Connection, title: &str) -> Result<String> {
    let base = {
        let slug = slugify(title);
        if slug.is_empty() {
            "class".to_string()
        } else {
            slug
        }
    };
    let taken = |id: &str| -> Result<bool> {
        if catalog::course(id).is_some() {
            return Ok(true);
        }
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM custom_courses WHERE id=?1)",
            [id],
            |row| row.get(0),
        )?;
        Ok(exists)
    };
    let first = format!("{}{base}", catalog::CUSTOM_PREFIX);
    if !taken(&first)? {
        return Ok(first);
    }
    for n in 2..100 {
        let candidate = format!("{}{base}-{n}", catalog::CUSTOM_PREFIX);
        if !taken(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(invalid("could not find a free id for this class"))
}

/// A short code from a title: the initials of up to three words.
pub fn short_code(title: &str) -> String {
    let code: String = title
        .split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_ascii_alphanumeric()))
        .take(3)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if code.is_empty() {
        "MY".into()
    } else {
        code
    }
}

/// The stages with default labels.
pub fn default_stages() -> Vec<OwnedEntryPoint> {
    STAGES
        .iter()
        .map(|(id, label)| OwnedEntryPoint {
            id: (*id).into(),
            label: (*label).into(),
        })
        .collect()
}

/// The tier a phase maps to; the seed's rule that a prerequisite's tier
/// never exceeds the topic's keeps prerequisites in earlier or same stages.
pub fn tier_of(phase: &str) -> i64 {
    match phase {
        "foundations" => 0,
        "mechanisms" => 1,
        "production" => 2,
        _ => 3,
    }
}

fn host_of(url: &str) -> Option<String> {
    crate::research::host_of(url)
}

fn on_host(hosts: &[String], url: &str) -> bool {
    host_of(url).is_some_and(|host| {
        hosts
            .iter()
            .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
    })
}

/// Every objection to the draft, addressed to the field or topic concerned.
/// An empty list means the draft can be published.
pub fn validate(draft: &CourseDraft) -> Vec<DraftIssue> {
    let mut issues = Vec::new();
    let mut issue = |at: &str, message: String| {
        issues.push(DraftIssue {
            at: at.into(),
            message,
        })
    };
    let words = |text: &str| text.split_whitespace().count();
    if !draft.id.starts_with(catalog::CUSTOM_PREFIX)
        || !draft
            .id
            .bytes()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'-')
    {
        issue(
            "id",
            "the id must be lowercase letters, digits and hyphens".into(),
        );
    }
    if draft.label.trim().is_empty() {
        issue("label", "give the class a name".into());
    }
    if draft.short_code.trim().is_empty() || draft.short_code.trim().chars().count() > 4 {
        issue(
            "short_code",
            "a short code is one to four characters".into(),
        );
    }
    if words(&draft.summary) < 6 {
        issue("summary", "the summary needs a sentence".into());
    }
    if words(&draft.outcome) < 15 {
        issue(
            "outcome",
            "the outcome names what you will be able to make or do at the end; say it in at least fifteen words".into(),
        );
    }
    if words(&draft.context) < 15 {
        issue(
            "context",
            "the context tells the tutor how this subject should be taught; at least fifteen words"
                .into(),
        );
    }
    if words(&draft.environment) < 3 {
        issue("environment", "name the working environment".into());
    }
    // The writing rules hold for a course's own words as for a lesson's.
    for (field, text) in [
        ("summary", &draft.summary),
        ("outcome", &draft.outcome),
        ("context", &draft.context),
    ] {
        if let Some(found) = crate::prose::report(text) {
            issue(field, format!("{}{found}", crate::prose::OBJECTION));
        }
    }
    if draft.source_hosts.is_empty() {
        issue("source_hosts", "add at least one documentation host".into());
    }
    for host in &draft.source_hosts {
        if host.trim().is_empty() || host.contains('/') || host.contains(' ') || !host.contains('.')
        {
            issue("source_hosts", format!("{host:?} is not a hostname"));
        }
    }
    let stage_ids: Vec<&str> = draft.entry_points.iter().map(|e| e.id.as_str()).collect();
    if stage_ids != STAGES.iter().map(|(id, _)| *id).collect::<Vec<_>>() {
        issue(
            "entry_points",
            "the four stages must be foundations, mechanisms, production and synthesis, in that order".into(),
        );
    }
    if draft
        .entry_points
        .iter()
        .any(|entry| entry.label.trim().is_empty())
    {
        issue("entry_points", "every stage needs a label".into());
    }
    if draft.topics.len() < MIN_TOPICS {
        issue(
            "topics",
            format!("a course needs at least {MIN_TOPICS} topics"),
        );
    }
    if draft.topics.len() > MAX_TOPICS {
        issue(
            "topics",
            format!("a course holds at most {MAX_TOPICS} topics"),
        );
    }
    let mut slugs = HashSet::new();
    let mut titles = HashSet::new();
    let index: HashMap<&str, &DraftTopic> = draft
        .topics
        .iter()
        .map(|topic| (topic.slug.as_str(), topic))
        .collect();
    for topic in &draft.topics {
        let at = format!("topics/{}", topic.slug);
        if topic.slug.is_empty()
            || !topic
                .slug
                .bytes()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'-')
        {
            issue(
                &at,
                "a topic slug is lowercase letters, digits and hyphens".into(),
            );
        }
        if !slugs.insert(topic.slug.as_str()) {
            issue(&at, "two topics share this slug".into());
        }
        if topic.title.trim().is_empty() {
            issue(&at, "the topic needs a title".into());
        } else if !titles.insert(topic.title.trim().to_lowercase()) {
            issue(&at, "two topics share this title".into());
        }
        if topic.category.trim().is_empty() {
            issue(&at, "give the topic a category".into());
        }
        if let Err(reason) = topic.curriculum.validate() {
            issue(&at, reason.into());
        }
        if let Some(found) = crate::prose::report(&topic_prose(topic)) {
            issue(&at, format!("{}{found}", crate::prose::OBJECTION));
        }
        for prerequisite in &topic.prereqs {
            match index.get(prerequisite.as_str()) {
                None => issue(&at, format!("prerequisite {prerequisite} is not a topic")),
                Some(earlier) => {
                    if tier_of(&earlier.curriculum.phase) > tier_of(&topic.curriculum.phase) {
                        issue(
                            &at,
                            format!("prerequisite {prerequisite} sits in a later stage"),
                        );
                    }
                    if prerequisite == &topic.slug {
                        issue(&at, "a topic cannot require itself".into());
                    }
                }
            }
        }
        for source in &topic.curriculum.primary_sources {
            if !on_host(&draft.source_hosts, source) {
                issue(
                    &at,
                    format!("source {source} is not on one of the course's hosts"),
                );
            }
        }
        if !topic.curriculum.related_concepts.is_empty() {
            issue(
                &at,
                "related concepts belong to bundled tracks; leave them empty".into(),
            );
        }
    }
    // Every stage but the elective needs a core topic.
    for (stage, _) in STAGES {
        if !draft
            .topics
            .iter()
            .any(|topic| topic.curriculum.phase == stage && topic.curriculum.core)
        {
            issue(
                "topics",
                format!("the {stage} stage needs at least one core topic"),
            );
        }
    }
    // No cycles among prerequisites.
    fn visit<'a>(
        slug: &'a str,
        index: &HashMap<&'a str, &'a DraftTopic>,
        visiting: &mut HashSet<&'a str>,
        visited: &mut HashSet<&'a str>,
    ) -> bool {
        if visited.contains(slug) {
            return true;
        }
        if !visiting.insert(slug) {
            return false;
        }
        if let Some(topic) = index.get(slug) {
            for prerequisite in &topic.prereqs {
                if !visit(prerequisite, index, visiting, visited) {
                    return false;
                }
            }
        }
        visiting.remove(slug);
        visited.insert(slug);
        true
    }
    let mut visited = HashSet::new();
    for topic in &draft.topics {
        if !visit(&topic.slug, &index, &mut HashSet::new(), &mut visited) {
            issue(
                &format!("topics/{}", topic.slug),
                "prerequisites go round in a circle".into(),
            );
        }
    }
    issues
}

/// Every sentence of a topic the learner or the tutor wrote, for the
/// writing check.
fn topic_prose(topic: &DraftTopic) -> String {
    let b = &topic.curriculum;
    [
        topic.title.as_str(),
        b.learner_outcome.as_str(),
        b.production_scenario.as_str(),
        b.evidence.as_str(),
        b.artifact.as_str(),
    ]
    .into_iter()
    .chain(b.mechanisms.iter().map(String::as_str))
    .chain(b.misconceptions.iter().map(String::as_str))
    .collect::<Vec<_>>()
    .join("\n")
}

/// An empty course for a learner who writes it themselves: the brief's
/// title and outcome, the four stages, one topic to start from.
pub fn blank_draft(id: &str, brief: &CourseBrief) -> CourseDraft {
    CourseDraft {
        id: id.into(),
        label: brief.title.trim().to_string(),
        native_label: String::new(),
        short_code: short_code(&brief.title),
        title: brief.title.trim().to_string(),
        summary: String::new(),
        context: String::new(),
        outcome: brief.outcome.trim().to_string(),
        environment: String::new(),
        source_hosts: brief.trusted_hosts.clone(),
        entry_points: default_stages(),
        topics: vec![DraftTopic {
            slug: "first-topic".into(),
            title: "First topic".into(),
            category: "fundamentals".into(),
            prereqs: vec![],
            curriculum: CurriculumBrief {
                phase: "foundations".into(),
                core: true,
                ..Default::default()
            },
        }],
    }
}

/// Tidy a draft as it comes from the tutor, a file or the editor: trim,
/// slugify topic slugs, fix the stage ids, drop empty strings.
pub fn normalize(mut draft: CourseDraft) -> CourseDraft {
    let trim = |s: &mut String| *s = crate::prose::scrub(s.trim());
    trim(&mut draft.label);
    trim(&mut draft.native_label);
    trim(&mut draft.short_code);
    trim(&mut draft.title);
    trim(&mut draft.summary);
    trim(&mut draft.context);
    trim(&mut draft.outcome);
    trim(&mut draft.environment);
    draft.short_code = draft.short_code.to_uppercase();
    if draft.title.is_empty() {
        draft.title = draft.label.clone();
    }
    draft.source_hosts = draft
        .source_hosts
        .iter()
        .map(|host| {
            host.trim()
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/')
                .to_lowercase()
        })
        .filter(|host| !host.is_empty())
        .collect::<Vec<_>>();
    draft.source_hosts.dedup();
    let labels: HashMap<String, String> = draft
        .entry_points
        .iter()
        .map(|entry| (entry.id.clone(), crate::prose::scrub(entry.label.trim())))
        .collect();
    draft.entry_points = STAGES
        .iter()
        .map(|(id, label)| OwnedEntryPoint {
            id: (*id).into(),
            label: labels
                .get(*id)
                .filter(|l| !l.is_empty())
                .cloned()
                .unwrap_or_else(|| (*label).into()),
        })
        .collect();
    let mut renamed: HashMap<String, String> = HashMap::new();
    for topic in &mut draft.topics {
        let slug = slugify(if topic.slug.trim().is_empty() {
            &topic.title
        } else {
            &topic.slug
        });
        renamed.insert(topic.slug.clone(), slug.clone());
        topic.slug = slug;
        trim(&mut topic.title);
        trim(&mut topic.category);
        if topic.category.is_empty() {
            topic.category = "fundamentals".into();
        }
        if !STAGES.iter().any(|(id, _)| *id == topic.curriculum.phase)
            && topic.curriculum.phase != "elective"
        {
            topic.curriculum.phase = "foundations".into();
        }
        let b = &mut topic.curriculum;
        trim(&mut b.learner_outcome);
        trim(&mut b.production_scenario);
        trim(&mut b.evidence);
        trim(&mut b.artifact);
        let clean = |items: &mut Vec<String>| {
            *items = items
                .iter()
                .map(|s| crate::prose::scrub(s.trim()))
                .filter(|s| !s.is_empty())
                .collect();
        };
        clean(&mut b.mechanisms);
        clean(&mut b.misconceptions);
        b.primary_sources = b
            .primary_sources
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        b.related_concepts.clear();
    }
    for topic in &mut draft.topics {
        topic.prereqs = topic
            .prereqs
            .iter()
            .map(|p| renamed.get(p).cloned().unwrap_or_else(|| slugify(p)))
            .filter(|p| !p.is_empty())
            .collect();
        topic.prereqs.dedup();
    }
    draft
}

/// The prompt a published course teaches with, from the shared template.
pub fn render_prompt(draft: &CourseDraft, version: i64) -> String {
    include_str!("../../prompts/classroom/custom.txt")
        .replace("{{LABEL}}", &draft.label)
        .replace("{{ID}}", &draft.id)
        .replace("{{VERSION}}", &format!("v{version}"))
        .replace("{{TITLE}}", &draft.title)
        .replace("{{OUTCOME}}", &draft.outcome)
        .replace("{{CONTEXT}}", &draft.context)
        .replace("{{ENVIRONMENT}}", &draft.environment)
        .replace("{{HOSTS}}", &draft.source_hosts.join(", "))
}

/// The definition a published draft registers.
pub fn definition_of(draft: &CourseDraft, version: i64) -> OwnedCourse {
    OwnedCourse {
        id: draft.id.clone(),
        label: draft.label.clone(),
        native_label: if draft.native_label.is_empty() {
            "your own course".into()
        } else {
            draft.native_label.clone()
        },
        short_code: draft.short_code.clone(),
        title: draft.title.clone(),
        summary: draft.summary.clone(),
        version: format!("v{version}"),
        context: draft.context.clone(),
        outcome: draft.outcome.clone(),
        environment: draft.environment.clone(),
        source_hosts: draft.source_hosts.clone(),
        entry_points: draft.entry_points.clone(),
        capabilities: vec![],
        prompt: render_prompt(draft, version),
    }
}

/// The topics in the shape the bundled seed uses, slugs prefixed with the
/// course id so they never collide with another course's.
pub fn curriculum_of(draft: &CourseDraft) -> Value {
    let prefixed = |slug: &str| format!("{}-{slug}", draft.id);
    Value::Array(
        draft
            .topics
            .iter()
            .map(|topic| {
                serde_json::json!({
                    "slug": prefixed(&topic.slug),
                    "title": topic.title,
                    "category": topic.category,
                    "tier": tier_of(&topic.curriculum.phase),
                    "prereqs": topic.prereqs.iter().map(|p| prefixed(p)).collect::<Vec<_>>(),
                    "focus": draft.id,
                    "curriculum": topic.curriculum,
                })
            })
            .collect(),
    )
}

/// The file a class is exported as, and imported from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassFile {
    pub format: String,
    pub exported_at: String,
    pub brief: CourseBrief,
    pub draft: CourseDraft,
}

pub const CLASS_FILE_FORMAT: &str = "principia-class/1";

// --- storage ------------------------------------------------------------

struct Row {
    id: String,
    version: i64,
    status: String,
    origin: String,
    brief_json: String,
    draft_json: String,
    definition_json: Option<String>,
    curriculum_json: Option<String>,
    review_json: Option<String>,
    sources_json: Option<String>,
    bank_json: Option<String>,
    checks_json: Option<String>,
    created_at: String,
    updated_at: String,
    published_at: Option<String>,
}

fn row(conn: &Connection, id: &str) -> Result<Row> {
    conn.query_row(
        "SELECT id, version, status, origin, brief_json, draft_json, definition_json,
                curriculum_json, review_json, sources_json, created_at, updated_at, published_at,
                bank_json, checks_json
         FROM custom_courses WHERE id=?1",
        [id],
        |r| {
            Ok(Row {
                id: r.get(0)?,
                version: r.get(1)?,
                status: r.get(2)?,
                origin: r.get(3)?,
                brief_json: r.get(4)?,
                draft_json: r.get(5)?,
                definition_json: r.get(6)?,
                curriculum_json: r.get(7)?,
                review_json: r.get(8)?,
                sources_json: r.get(9)?,
                created_at: r.get(10)?,
                updated_at: r.get(11)?,
                published_at: r.get(12)?,
                bank_json: r.get(13)?,
                checks_json: r.get(14)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| invalid("this class does not exist"))
}

/// The draft as stored, hashed, so a mark can say which draft it was made on.
fn draft_hash(draft_json: &str) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(draft_json.as_bytes()))
}

fn marks_of(row: &Row) -> Result<Marks> {
    Ok(row
        .checks_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or_default())
}

/// Where the class stands against what publishing needs.
pub fn checks_of(
    draft: &CourseDraft,
    draft_hash: &str,
    issues: &[DraftIssue],
    review: Option<&[ReviewFinding]>,
    sources: Option<&[SourceCheck]>,
    marks: &Marks,
) -> Checks {
    let urls: HashSet<&str> = draft
        .topics
        .iter()
        .flat_map(|topic| topic.curriculum.primary_sources.iter())
        .map(String::as_str)
        .collect();
    let checked: HashSet<&str> = sources
        .unwrap_or_default()
        .iter()
        .map(|check| check.url.as_str())
        .collect();
    let unchecked_sources = urls.iter().filter(|url| !checked.contains(*url)).count();
    let pending_sources = sources
        .unwrap_or_default()
        .iter()
        .filter(|check| {
            check.state != "reachable" && !check.accepted && urls.contains(check.url.as_str())
        })
        .map(|check| check.url.as_str())
        .collect::<HashSet<_>>()
        .len();
    let open_findings = review
        .unwrap_or_default()
        .iter()
        .filter(|finding| finding.status == "open")
        .count();
    let mut checks = Checks {
        draft_hash: draft_hash.to_string(),
        reviewed: review.is_some(),
        review_current: review.is_some() && marks.review_hash == draft_hash,
        open_findings,
        fetched: sources.is_some(),
        unchecked_sources,
        pending_sources,
        read: marks.read_hash == draft_hash,
        blockers: Vec::new(),
    };
    let plural = |n: usize, one: &str, many: &str| {
        if n == 1 {
            one.to_string()
        } else {
            many.to_string()
        }
    };
    if !issues.is_empty() {
        checks.blockers.push(format!(
            "{} {} to fix in the editor",
            issues.len(),
            plural(issues.len(), "thing", "things")
        ));
    }
    if !checks.reviewed {
        checks
            .blockers
            .push("the tutor has not read the draft back".into());
    } else if open_findings > 0 {
        checks.blockers.push(format!(
            "{open_findings} {} from the review still open",
            plural(open_findings, "finding", "findings")
        ));
    }
    if !checks.fetched {
        checks
            .blockers
            .push("the sources have not been fetched".into());
    } else {
        if unchecked_sources > 0 {
            checks.blockers.push(format!(
                "{unchecked_sources} {} added since the last fetch",
                plural(unchecked_sources, "source", "sources")
            ));
        }
        if pending_sources > 0 {
            checks.blockers.push(format!(
                "{pending_sources} unreachable {} neither replaced nor accepted",
                plural(pending_sources, "source", "sources")
            ));
        }
    }
    if !checks.read {
        checks
            .blockers
            .push("your own read-through of this version is not confirmed".into());
    }
    checks
}

fn view_of(row: Row) -> Result<CustomCourseView> {
    let draft: CourseDraft = serde_json::from_str(&row.draft_json)?;
    let issues = validate(&draft);
    let review: Option<Vec<ReviewFinding>> = row
        .review_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?;
    let sources: Option<Vec<SourceCheck>> = row
        .sources_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?;
    let marks = marks_of(&row)?;
    let hash = draft_hash(&row.draft_json);
    let checks = checks_of(
        &draft,
        &hash,
        &issues,
        review.as_deref(),
        sources.as_deref(),
        &marks,
    );
    Ok(CustomCourseView {
        id: row.id,
        version: row.version,
        status: row.status,
        origin: row.origin,
        brief: serde_json::from_str(&row.brief_json)?,
        draft,
        issues,
        review: review.unwrap_or_default(),
        sources: sources.unwrap_or_default(),
        checks,
        bank: row
            .bank_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?,
        created_at: row.created_at,
        updated_at: row.updated_at,
        published_at: row.published_at,
        working: None,
        working_at: None,
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<CustomCourseView> {
    view_of(row(conn, id)?)
}

pub fn list(conn: &Connection) -> Result<Vec<CustomCourseSummary>> {
    let mut statement = conn.prepare(
        "SELECT id, version, status, origin, draft_json, updated_at FROM custom_courses ORDER BY updated_at DESC",
    )?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id, version, status, origin, draft_json, updated_at)| {
            let draft: CourseDraft = serde_json::from_str(&draft_json)?;
            Ok(CustomCourseSummary {
                id,
                label: draft.label,
                version,
                status,
                origin,
                topics: draft.topics.len(),
                updated_at,
                working: None,
            })
        })
        .collect()
}

/// Start a class from a brief: a row with an empty draft. The tutor's
/// draft, a hand-written one or an import replaces the draft afterwards.
pub fn create(conn: &Connection, brief: &CourseBrief, origin: &str) -> Result<CustomCourseView> {
    if brief.title.trim().is_empty() {
        return Err(invalid("give the class a title"));
    }
    if brief.outcome.split_whitespace().count() < 5 {
        return Err(invalid(
            "say what you want to be able to do, in a sentence at least",
        ));
    }
    if !matches!(origin, "tutor" | "manual" | "import") {
        return Err(invalid("unknown origin"));
    }
    let id = new_course_id(conn, &brief.title)?;
    let draft = normalize(blank_draft(&id, brief));
    let stamp = now();
    conn.execute(
        "INSERT INTO custom_courses (id, version, status, origin, brief_json, draft_json, created_at, updated_at)
         VALUES (?1, 0, 'draft', ?2, ?3, ?4, ?5, ?5)",
        params![
            id,
            origin,
            serde_json::to_string(brief)?,
            serde_json::to_string(&draft)?,
            stamp
        ],
    )?;
    get(conn, &id)
}

/// Save the draft as it stands. Always allowed; the issues come back with
/// the view so the editor can show them.
pub fn save_draft(conn: &Connection, id: &str, draft: CourseDraft) -> Result<CustomCourseView> {
    let existing = row(conn, id)?;
    let mut draft = normalize(draft);
    draft.id = existing.id.clone();
    conn.execute(
        "UPDATE custom_courses SET draft_json=?2, updated_at=?3 WHERE id=?1",
        params![id, serde_json::to_string(&draft)?, now()],
    )?;
    get(conn, id)
}

pub fn save_brief(conn: &Connection, id: &str, brief: &CourseBrief) -> Result<CustomCourseView> {
    row(conn, id)?;
    conn.execute(
        "UPDATE custom_courses SET brief_json=?2, updated_at=?3 WHERE id=?1",
        params![id, serde_json::to_string(brief)?, now()],
    )?;
    get(conn, id)
}

fn save_marks(conn: &Connection, id: &str, marks: &Marks) -> Result<()> {
    conn.execute(
        "UPDATE custom_courses SET checks_json=?2 WHERE id=?1",
        params![id, serde_json::to_string(marks)?],
    )?;
    Ok(())
}

/// Keep what the tutor found, merged with what was already settled (see
/// [`merge_reviews`]). The mark says which draft was read.
pub fn save_review(
    conn: &Connection,
    id: &str,
    review: &[ReviewFinding],
) -> Result<CustomCourseView> {
    let existing = row(conn, id)?;
    let previous: Vec<ReviewFinding> = existing
        .review_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or_default();
    let review = merge_reviews(&previous, review.to_vec());
    conn.execute(
        "UPDATE custom_courses SET review_json=?2, updated_at=?3 WHERE id=?1",
        params![id, serde_json::to_string(&review)?, now()],
    )?;
    let mut marks = marks_of(&existing)?;
    marks.review_hash = draft_hash(&existing.draft_json);
    save_marks(conn, id, &marks)?;
    get(conn, id)
}

/// Settle one finding: `fixed` or `dismissed` with a note, or back to
/// `open`.
pub fn resolve_finding(
    conn: &Connection,
    id: &str,
    index: usize,
    status: &str,
    note: &str,
) -> Result<CustomCourseView> {
    if !matches!(status, "open" | "fixed" | "dismissed") {
        return Err(invalid("a finding is open, fixed or dismissed"));
    }
    let existing = row(conn, id)?;
    let mut review: Vec<ReviewFinding> = existing
        .review_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?
        .ok_or_else(|| invalid("the tutor has not read this class back"))?;
    let finding = review
        .get_mut(index)
        .ok_or_else(|| invalid("that finding is not in the review"))?;
    finding.status = status.into();
    finding.note = note.trim().chars().take(400).collect();
    conn.execute(
        "UPDATE custom_courses SET review_json=?2, updated_at=?3 WHERE id=?1",
        params![id, serde_json::to_string(&review)?, now()],
    )?;
    get(conn, id)
}

/// Keep the result of a fetch. A source the learner had accepted stays
/// accepted while it is still unreachable at the same address.
pub fn save_sources(
    conn: &Connection,
    id: &str,
    sources: &[SourceCheck],
) -> Result<CustomCourseView> {
    let existing = row(conn, id)?;
    let before: Vec<SourceCheck> = existing
        .sources_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or_default();
    let sources: Vec<SourceCheck> = sources
        .iter()
        .cloned()
        .map(|mut check| {
            check.accepted = check.state != "reachable"
                && before
                    .iter()
                    .any(|old| old.url == check.url && old.accepted);
            check
        })
        .collect();
    conn.execute(
        "UPDATE custom_courses SET sources_json=?2, updated_at=?3 WHERE id=?1",
        params![id, serde_json::to_string(&sources)?, now()],
    )?;
    get(conn, id)
}

/// Put another source in place of one that did not answer: in every topic
/// that cites it, and in the last fetch's record, where the new address
/// counts as reachable because it was fetched before it was chosen.
pub fn replace_source(
    conn: &Connection,
    id: &str,
    dead: &str,
    replacement: &str,
) -> Result<CustomCourseView> {
    let existing = row(conn, id)?;
    let mut draft: CourseDraft = serde_json::from_str(&existing.draft_json)?;
    let mut cited = 0;
    for topic in &mut draft.topics {
        for source in &mut topic.curriculum.primary_sources {
            if source == dead {
                *source = replacement.to_string();
                cited += 1;
            }
        }
    }
    if cited == 0 {
        return Err(invalid("that source is no longer in the draft"));
    }
    let draft = normalize(draft);
    conn.execute(
        "UPDATE custom_courses SET draft_json=?2, updated_at=?3 WHERE id=?1",
        params![id, serde_json::to_string(&draft)?, now()],
    )?;
    let mut sources: Vec<SourceCheck> = existing
        .sources_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or_default();
    for check in sources.iter_mut().filter(|check| check.url == dead) {
        check.url = replacement.to_string();
        check.state = "reachable".into();
        check.accepted = false;
    }
    conn.execute(
        "UPDATE custom_courses SET sources_json=?2 WHERE id=?1",
        params![id, serde_json::to_string(&sources)?],
    )?;
    get(conn, id)
}

/// The learner keeps an unreachable source knowingly, or withdraws that.
pub fn accept_source(
    conn: &Connection,
    id: &str,
    url: &str,
    accepted: bool,
) -> Result<CustomCourseView> {
    let existing = row(conn, id)?;
    let mut sources: Vec<SourceCheck> = existing
        .sources_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?
        .ok_or_else(|| invalid("the sources have not been fetched"))?;
    let mut found = false;
    for check in sources.iter_mut().filter(|check| check.url == url) {
        check.accepted = accepted && check.state != "reachable";
        found = true;
    }
    if !found {
        return Err(invalid("that source was not in the last fetch"));
    }
    conn.execute(
        "UPDATE custom_courses SET sources_json=?2, updated_at=?3 WHERE id=?1",
        params![id, serde_json::to_string(&sources)?, now()],
    )?;
    get(conn, id)
}

/// The learner confirms having read the draft as it stands, or withdraws
/// that. The mark is on this exact draft: an edit after it needs another.
pub fn mark_read(conn: &Connection, id: &str, read: bool) -> Result<CustomCourseView> {
    let existing = row(conn, id)?;
    let mut marks = marks_of(&existing)?;
    marks.read_hash = if read {
        draft_hash(&existing.draft_json)
    } else {
        String::new()
    };
    save_marks(conn, id, &marks)?;
    get(conn, id)
}

/// The bank a class may sample: the stored one with questions whose topic
/// the draft no longer has left out. None without questions.
fn bank_for(draft: &CourseDraft, bank_json: Option<&str>) -> Result<Option<Value>> {
    let Some(json) = bank_json else {
        return Ok(None);
    };
    let mut bank: crate::domain::placement::Bank = serde_json::from_str(json)?;
    let slugs: Vec<String> = draft
        .topics
        .iter()
        .map(|t| format!("{}-{}", draft.id, t.slug))
        .collect();
    bank.questions.retain(|q| slugs.contains(&q.competency));
    if bank.questions.is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::to_value(bank)?))
}

/// Keep the bank the tutor wrote. It is checked against the draft's topics
/// and stages the way an authored bank is checked, once the class is
/// published; until then it is held as written.
pub fn save_bank(
    conn: &Connection,
    id: &str,
    bank: &crate::domain::placement::Bank,
) -> Result<CustomCourseView> {
    let existing = row(conn, id)?;
    let json = serde_json::to_string(bank)?;
    conn.execute(
        "UPDATE custom_courses SET bank_json=?2, updated_at=?3 WHERE id=?1",
        params![id, json, now()],
    )?;
    if existing.status == "published" {
        let draft: CourseDraft = serde_json::from_str(&existing.draft_json)?;
        catalog::register_custom_bank(id, bank_for(&draft, Some(&json))?);
    }
    get(conn, id)
}

/// A learner disputes a written key: the question stops counting and is
/// left out of every sample until it is corrected in the builder.
pub fn void_question(
    conn: &Connection,
    id: &str,
    question_id: &str,
    reason: &str,
) -> Result<CustomCourseView> {
    let existing = row(conn, id)?;
    let Some(json) = existing.bank_json.as_deref() else {
        return Err(invalid("this class has no question bank"));
    };
    let mut bank: crate::domain::placement::Bank = serde_json::from_str(json)?;
    let question = bank
        .questions
        .iter_mut()
        .find(|q| q.id == question_id)
        .ok_or_else(|| invalid("that question is not in the bank"))?;
    question.voided = true;
    question.void_reason = reason.trim().chars().take(400).collect();
    save_bank(conn, id, &bank)
}

/// Publish the draft: register the definition, write the topics, make sure
/// the class has a program. Refused while the validator objects.
pub fn publish(conn: &Connection, id: &str) -> Result<CustomCourseView> {
    let existing = row(conn, id)?;
    let draft: CourseDraft = serde_json::from_str(&existing.draft_json)?;
    let issues = validate(&draft);
    if let Some(first) = issues.first() {
        return Err(invalid(format!(
            "the draft is not ready to publish: {} ({} and {} more)",
            first.message,
            first.at,
            issues.len().saturating_sub(1)
        )));
    }
    let current = get(conn, id)?;
    if !current.checks.blockers.is_empty() {
        return Err(invalid(format!(
            "the class is not ready to publish: {}",
            current.checks.blockers.join("; ")
        )));
    }
    let brief: CourseBrief = serde_json::from_str(&existing.brief_json)?;
    let version = existing.version + 1;
    let definition = definition_of(&draft, version);
    let curriculum = curriculum_of(&draft);
    // The definition has to be registered before the topics are written:
    // the seed validator checks the focus against the catalog. The seed
    // writer runs its own transaction, so the steps run in an order that
    // leaves nothing half-made if one fails: topics, then the row, then
    // the program.
    let spec = catalog::register_custom(definition.clone(), curriculum.clone());
    let written = (|| -> Result<()> {
        crate::db::seed_concepts(conn, &serde_json::to_string(&curriculum)?)?;
        // Topics the draft no longer has leave, unless a lesson was taught
        // on them: those stay as history and as part of the route.
        let keep: Vec<String> = curriculum
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["slug"].as_str().unwrap().to_string())
            .collect();
        let mut statement =
            conn.prepare("SELECT slug, times_picked FROM concepts WHERE focus=?1")?;
        let stale: Vec<String> = statement
            .query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter(|(slug, picked)| !keep.contains(slug) && *picked == 0)
            .map(|(slug, _)| slug)
            .collect();
        drop(statement);
        for slug in stale {
            conn.execute(
                "DELETE FROM concepts WHERE slug=?1 AND focus=?2",
                params![slug, id],
            )?;
        }
        let stamp = now();
        conn.execute(
            "UPDATE custom_courses SET version=?2, status='published', definition_json=?3, curriculum_json=?4,
                    prompt=?5, updated_at=?6, published_at=?6 WHERE id=?1",
            params![
                id,
                version,
                serde_json::to_string(&definition)?,
                serde_json::to_string(&curriculum)?,
                definition.prompt,
                stamp
            ],
        )?;
        crate::classroom::ensure_program(
            conn,
            spec,
            &brief.agent,
            &brief.model,
            &brief.custom_agent_bin,
        )
        .map_err(DbError::Invalid)?;
        catalog::register_custom_bank(id, bank_for(&draft, existing.bank_json.as_deref())?);
        Ok(())
    })();
    if let Err(error) = written {
        // Put the registry back the way it was.
        match existing.definition_json.as_deref() {
            Some(json) => {
                if let (Ok(previous), Some(curriculum)) = (
                    serde_json::from_str::<OwnedCourse>(json),
                    existing
                        .curriculum_json
                        .as_deref()
                        .and_then(|c| serde_json::from_str::<Value>(c).ok()),
                ) {
                    catalog::register_custom(previous, curriculum);
                }
            }
            None => catalog::unregister_custom(id),
        }
        return Err(error);
    }
    get(conn, id)
}

/// Register every published course at startup, before programs are read.
/// A profile from before custom courses existed has none to load.
pub fn load_published(conn: &Connection) -> Result<usize> {
    let present: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='custom_courses')",
        [],
        |row| row.get(0),
    )?;
    if !present {
        return Ok(0);
    }
    let mut statement = conn.prepare(
        "SELECT id, definition_json, curriculum_json, draft_json, bank_json FROM custom_courses WHERE status='published'",
    )?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut loaded = 0;
    for (id, definition, curriculum, draft, bank) in rows {
        let (Some(definition), Some(curriculum)) = (definition, curriculum) else {
            log::warn!("custom course {id} is published without a definition");
            continue;
        };
        let definition: OwnedCourse = serde_json::from_str(&definition)?;
        let curriculum: Value = serde_json::from_str(&curriculum)?;
        catalog::register_custom(definition, curriculum);
        let draft: CourseDraft = serde_json::from_str(&draft)?;
        catalog::register_custom_bank(&id, bank_for(&draft, bank.as_deref())?);
        loaded += 1;
    }
    Ok(loaded)
}

/// A draft that was never published can be deleted outright. A published
/// course keeps its history: it is retired instead (paused, hidden from
/// the catalogue), which a later pass may add.
pub fn delete_draft(conn: &Connection, id: &str) -> Result<()> {
    let existing = row(conn, id)?;
    if existing.status == "published" {
        return Err(invalid(
            "a published class keeps its lessons; retire it from its settings instead",
        ));
    }
    conn.execute("DELETE FROM custom_courses WHERE id=?1", [id])?;
    Ok(())
}

/// The export: the brief and the draft, nothing personal.
pub fn export(conn: &Connection, id: &str) -> Result<ClassFile> {
    let view = get(conn, id)?;
    Ok(ClassFile {
        format: CLASS_FILE_FORMAT.into(),
        exported_at: now(),
        brief: view.brief,
        draft: view.draft,
    })
}

/// An import makes a new draft with a fresh id, at the review step.
pub fn import(conn: &Connection, file: ClassFile) -> Result<CustomCourseView> {
    if file.format != CLASS_FILE_FORMAT {
        return Err(invalid(format!(
            "this is not a class file the desk understands ({})",
            file.format
        )));
    }
    let mut brief = file.brief;
    if brief.title.trim().is_empty() {
        brief.title = file.draft.label.clone();
    }
    if brief.outcome.split_whitespace().count() < 5 {
        brief.outcome = file.draft.outcome.clone();
    }
    let created = create(conn, &brief, "import")?;
    let mut draft = file.draft;
    draft.id = created.id.clone();
    if draft.label.trim().is_empty() {
        draft.label = brief.title.clone();
    }
    save_draft(conn, &created.id, draft)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topic(slug: &str, phase: &str, core: bool, prereqs: &[&str]) -> DraftTopic {
        DraftTopic {
            slug: slug.into(),
            title: format!("Topic {slug}"),
            category: "fundamentals".into(),
            prereqs: prereqs.iter().map(|p| p.to_string()).collect(),
            curriculum: CurriculumBrief {
                phase: phase.into(),
                core,
                learner_outcome: "Explain the thing with a worked example and a measured result of your own".into(),
                mechanisms: vec!["one mechanism".into(), "another mechanism".into()],
                production_scenario: "A service in production meets this problem on a busy day and someone has to fix it".into(),
                misconceptions: vec!["it is not magic".into()],
                evidence: "A trace showing the mechanism at work".into(),
                artifact: "A short note with the trace and the fix".into(),
                primary_sources: vec![
                    "https://doc.rust-lang.org/book/".into(),
                    "https://docs.rs/clap/latest/clap/".into(),
                ],
                related_concepts: vec![],
            },
        }
    }

    fn good_draft() -> CourseDraft {
        CourseDraft {
            id: "custom-rust-cli".into(),
            label: "Rust for CLI tools".into(),
            native_label: "systems programming".into(),
            short_code: "RC".into(),
            title: "Rust for command-line tools".into(),
            summary: "Build small, fast command-line tools in Rust with care.".into(),
            context: "Teach ownership and error handling through small tools that are run and measured, comparing each choice with what the compiler and the operating system actually do.".into(),
            outcome: "Build and ship a small command-line tool with argument parsing, clean error handling, tests and a release binary, and explain each design choice with evidence.".into(),
            environment: "A terminal with a Rust toolchain and a text editor.".into(),
            source_hosts: vec!["doc.rust-lang.org".into(), "docs.rs".into()],
            entry_points: default_stages(),
            topics: vec![
                topic("ownership", "foundations", true, &[]),
                topic("errors", "foundations", true, &["ownership"]),
                topic("clap", "mechanisms", true, &["errors"]),
                topic("io", "mechanisms", false, &["ownership"]),
                topic("testing", "production", true, &["clap"]),
                topic("release", "synthesis", true, &["testing", "io"]),
            ],
        }
    }

    #[test]
    fn a_complete_draft_passes_and_each_rule_is_named_when_it_fails() {
        assert!(validate(&good_draft()).is_empty());
        let mut few = good_draft();
        few.topics.truncate(3);
        assert!(validate(&few)
            .iter()
            .any(|i| i.at == "topics" && i.message.contains("at least")));
        let mut circular = good_draft();
        circular.topics[0].prereqs = vec!["errors".into()];
        assert!(validate(&circular)
            .iter()
            .any(|i| i.message.contains("circle")));
        let mut later = good_draft();
        later.topics[0].prereqs = vec!["release".into()];
        assert!(validate(&later)
            .iter()
            .any(|i| i.message.contains("later stage")));
        let mut off_host = good_draft();
        off_host.topics[1].curriculum.primary_sources[0] = "https://example.com/x".into();
        assert!(validate(&off_host)
            .iter()
            .any(|i| i.at == "topics/errors" && i.message.contains("hosts")));
        let mut no_core = good_draft();
        no_core.topics[4].curriculum.core = false;
        assert!(validate(&no_core)
            .iter()
            .any(|i| i.message.contains("production stage")));
        let mut bad_stage = good_draft();
        bad_stage.entry_points.remove(1);
        assert!(validate(&bad_stage).iter().any(|i| i.at == "entry_points"));
    }

    #[test]
    fn normalizing_slugifies_hosts_and_topics_and_keeps_prerequisites_pointing_right() {
        let mut messy = good_draft();
        messy.source_hosts = vec!["https://Doc.Rust-Lang.org/".into(), " docs.rs ".into()];
        messy.topics[0].slug = "Own ership!".into();
        messy.topics[1].prereqs = vec!["Own ership!".into()];
        messy.short_code = "rc".into();
        let tidy = normalize(messy);
        assert_eq!(tidy.source_hosts, vec!["doc.rust-lang.org", "docs.rs"]);
        assert_eq!(tidy.topics[0].slug, "own-ership");
        assert_eq!(tidy.topics[1].prereqs, vec!["own-ership"]);
        assert_eq!(tidy.short_code, "RC");
        assert_eq!(tidy.entry_points.len(), 4);
    }

    #[test]
    fn a_tutor_answer_without_an_id_still_parses() {
        let raw = r#"{"label": "Rust", "title": "Rust for tools", "topics": [{"title": "Ownership", "curriculum": {"phase": "foundations", "core": true}}]}"#;
        let draft: CourseDraft = serde_json::from_str(raw).unwrap();
        assert_eq!(draft.id, "");
        assert_eq!(draft.topics[0].slug, "");
        let tidy = normalize(draft);
        assert_eq!(tidy.topics[0].slug, "ownership");
        assert_eq!(tidy.entry_points.len(), 4);
        assert!(!validate(&tidy).is_empty());
    }

    #[test]
    fn the_curriculum_and_prompt_carry_the_course_id() {
        let draft = good_draft();
        let curriculum = curriculum_of(&draft);
        let first = &curriculum[0];
        assert_eq!(first["slug"], "custom-rust-cli-ownership");
        assert_eq!(first["focus"], "custom-rust-cli");
        assert_eq!(curriculum[1]["prereqs"][0], "custom-rust-cli-ownership");
        assert_eq!(curriculum[5]["tier"], 3);
        let prompt = render_prompt(&draft, 2);
        assert!(prompt.contains("PROMPT PROFILE: custom.custom-rust-cli.v2"));
        assert!(prompt.contains("doc.rust-lang.org, docs.rs"));
        assert_eq!(slugify("Rust for CLI tools!"), "rust-for-cli-tools");
        assert_eq!(short_code("rust for cli"), "RFC");
    }
}
