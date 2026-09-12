//! The bundled catalog is authored once in seed/catalog.json. The build script
//! compiles it into static definitions; native IPC and the browser preview use
//! that same manifest rather than maintaining separate lists of subjects.
//!
//! A learner's own classes join the same catalog at runtime: their
//! definitions are read from the database and registered here, and every
//! lookup answers from the compiled list first and the registry second, so
//! the rest of the desk never asks where a course came from.

use serde::Serialize;
use std::collections::HashSet;
use std::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SubjectKind {
    Engineering,
    Language,
}

impl SubjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Engineering => "engineering",
            Self::Language => "language",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct EntryPoint {
    pub id: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct CourseDefinition {
    pub id: &'static str,
    pub course_id: &'static str,
    pub kind: SubjectKind,
    pub label: &'static str,
    pub native_label: &'static str,
    pub short_code: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub version: &'static str,
    pub context: &'static str,
    pub outcome: &'static str,
    pub environment: &'static str,
    pub prompt_profile: &'static str,
    #[serde(skip_serializing)]
    pub prompt: &'static str,
    pub prerequisite_courses: &'static [&'static str],
    pub source_hosts: &'static [&'static str],
    pub reference_lessons: &'static [&'static str],
    #[serde(skip_serializing)]
    pub bundled_lessons: &'static [&'static str],
    pub capabilities: &'static [&'static str],
    pub entry_points: &'static [EntryPoint],
}

include!(concat!(env!("OUT_DIR"), "/catalog.rs"));

/// Custom courses registered at runtime. A definition is leaked into a
/// `'static` one on purpose: it is a few kilobytes, a learner makes a
/// handful, and it must outlive every reference the runtime holds.
static CUSTOM: RwLock<Vec<&'static CourseDefinition>> = RwLock::new(Vec::new());
/// Each custom course's topics in the seed's shape, keyed by id, so a
/// course snapshot can be built without a database in hand.
static CUSTOM_CURRICULA: RwLock<Vec<(String, serde_json::Value)>> = RwLock::new(Vec::new());

/// The prefix every custom course id carries, so it can never collide with
/// a bundled course or a later release's.
pub const CUSTOM_PREFIX: &str = "custom-";

pub fn is_custom(id: &str) -> bool {
    id.starts_with(CUSTOM_PREFIX)
}

fn leak(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

fn leak_all(values: Vec<String>) -> &'static [&'static str] {
    Box::leak(
        values
            .into_iter()
            .map(leak)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}

/// The owned shape of a definition, as a custom course stores it.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct OwnedCourse {
    pub id: String,
    pub label: String,
    pub native_label: String,
    pub short_code: String,
    pub title: String,
    pub summary: String,
    pub version: String,
    pub context: String,
    pub outcome: String,
    pub environment: String,
    pub source_hosts: Vec<String>,
    pub entry_points: Vec<OwnedEntryPoint>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct OwnedEntryPoint {
    pub id: String,
    pub label: String,
}

impl OwnedCourse {
    /// A `'static` definition for the runtime. The course is always an
    /// engineering course: the language runtime is another machine.
    fn leak(self) -> &'static CourseDefinition {
        let entries: Vec<EntryPoint> = self
            .entry_points
            .into_iter()
            .map(|entry| EntryPoint {
                id: leak(entry.id),
                label: leak(entry.label),
            })
            .collect();
        let id = leak(self.id);
        Box::leak(Box::new(CourseDefinition {
            id,
            course_id: id,
            kind: SubjectKind::Engineering,
            label: leak(self.label),
            native_label: leak(self.native_label),
            short_code: leak(self.short_code),
            title: leak(self.title),
            summary: leak(self.summary),
            version: leak(self.version),
            context: leak(self.context),
            outcome: leak(self.outcome),
            environment: leak(self.environment),
            prompt_profile: leak(format!("custom.{id}")),
            prompt: leak(self.prompt),
            prerequisite_courses: &[],
            source_hosts: leak_all(self.source_hosts),
            reference_lessons: &[],
            bundled_lessons: &[],
            capabilities: leak_all(if self.capabilities.is_empty() {
                vec![
                    "explanation".into(),
                    "knowledge_checks".into(),
                    "code".into(),
                    "diagrams".into(),
                    "tutor".into(),
                ]
            } else {
                self.capabilities
            }),
            entry_points: Box::leak(entries.into_boxed_slice()),
        }))
    }
}

/// Register a custom course with its topics, replacing an earlier
/// registration of the same id (a republished version).
pub fn register_custom(
    course: OwnedCourse,
    curriculum: serde_json::Value,
) -> &'static CourseDefinition {
    let definition = course.leak();
    let mut custom = CUSTOM.write().unwrap();
    custom.retain(|existing| existing.id != definition.id);
    custom.push(definition);
    let mut curricula = CUSTOM_CURRICULA.write().unwrap();
    curricula.retain(|(id, _)| id != definition.id);
    curricula.push((definition.id.to_string(), curriculum));
    definition
}

/// Forget a custom course (a deleted draft that was never published, or a
/// retired class).
pub fn unregister_custom(id: &str) {
    CUSTOM.write().unwrap().retain(|existing| existing.id != id);
    CUSTOM_CURRICULA
        .write()
        .unwrap()
        .retain(|(existing, _)| existing != id);
    register_custom_bank(id, None);
}

/// Question banks of custom courses, keyed by id, in the diagnostic bank's
/// shape, so the placement check and unit challenges find them without a
/// database in hand.
static CUSTOM_BANKS: RwLock<Vec<(String, serde_json::Value)>> = RwLock::new(Vec::new());

pub fn register_custom_bank(id: &str, bank: Option<serde_json::Value>) {
    let mut banks = CUSTOM_BANKS.write().unwrap();
    banks.retain(|(existing, _)| existing != id);
    if let Some(bank) = bank {
        banks.push((id.to_string(), bank));
    }
}

pub fn custom_bank(id: &str) -> Option<serde_json::Value> {
    CUSTOM_BANKS
        .read()
        .unwrap()
        .iter()
        .find(|(existing, _)| existing == id)
        .map(|(_, bank)| bank.clone())
}

/// The topics of a registered custom course, in the seed's shape.
pub fn custom_curriculum(id: &str) -> Option<serde_json::Value> {
    CUSTOM_CURRICULA
        .read()
        .unwrap()
        .iter()
        .find(|(existing, _)| existing == id)
        .map(|(_, curriculum)| curriculum.clone())
}

pub fn course(id: &str) -> Option<&'static CourseDefinition> {
    COURSES.iter().find(|course| course.id == id).or_else(|| {
        CUSTOM
            .read()
            .unwrap()
            .iter()
            .copied()
            .find(|course| course.id == id)
    })
}

/// Every course the desk knows: the bundled ones, then the learner's own.
pub fn all() -> Vec<&'static CourseDefinition> {
    let mut courses: Vec<&'static CourseDefinition> = COURSES.iter().collect();
    courses.extend(CUSTOM.read().unwrap().iter().copied());
    courses
}

/// The ids of every engineering course, bundled and custom.
pub fn engineering_ids() -> Vec<&'static str> {
    all()
        .into_iter()
        .filter(|course| course.kind == SubjectKind::Engineering)
        .map(|course| course.id)
        .collect()
}

pub fn validate() -> Result<(), String> {
    let mut ids = HashSet::new();
    let mut course_ids = HashSet::new();
    for item in COURSES {
        if item.id.is_empty()
            || !item
                .id
                .bytes()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'-')
            || !ids.insert(item.id)
            || !course_ids.insert(item.course_id)
        {
            return Err(format!(
                "invalid or duplicate catalog identity: {}",
                item.id
            ));
        }
        if item.version.is_empty()
            || item.summary.is_empty()
            || item.outcome.is_empty()
            || item.source_hosts.is_empty()
        {
            return Err(format!("incomplete catalog definition: {}", item.id));
        }
        let mut entries = HashSet::new();
        if item.entry_points.is_empty()
            || item
                .entry_points
                .iter()
                .any(|entry| entry.label.is_empty() || !entries.insert(entry.id))
        {
            return Err(format!("invalid entry points: {}", item.id));
        }
        if !item.prompt.contains(&format!(
            "PROMPT PROFILE: {}.{}",
            item.prompt_profile, item.version
        )) {
            return Err(format!("prompt version mismatch: {}", item.id));
        }
        fn visit(id: &str, path: &mut HashSet<String>) -> Result<(), String> {
            if !path.insert(id.to_string()) {
                return Err(format!("course prerequisite cycle: {id}"));
            }
            let item = course(id).ok_or_else(|| format!("unknown prerequisite course: {id}"))?;
            for prerequisite in item.prerequisite_courses {
                visit(prerequisite, path)?;
            }
            path.remove(id);
            Ok(())
        }
        visit(item.id, &mut HashSet::new())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_the_nine_initial_courses_with_independent_shell_offerings() {
        validate().unwrap();
        assert_eq!(COURSES.len(), 9);
        assert_eq!(ENGINEERING_IDS.len(), 7);
        assert_eq!(
            course("bash-scripting").unwrap().prerequisite_courses,
            &["linux-bash"]
        );
        assert!(course("linux-bash")
            .unwrap()
            .prerequisite_courses
            .is_empty());
        assert!(course("system-design").is_some());
    }

    #[test]
    fn compiled_catalog_matches_the_authoring_manifest() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../seed/catalog.json")).unwrap();
        for (item, source) in COURSES.iter().zip(manifest["courses"].as_array().unwrap()) {
            let compiled = serde_json::to_value(item).unwrap();
            for (key, value) in source.as_object().unwrap() {
                if key != "prompt_path" {
                    assert_eq!(&compiled[key], value, "{} field {key}", item.id);
                }
            }
        }
    }
}
