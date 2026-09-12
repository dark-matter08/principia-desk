//! A lesson as a CSV file: the reading, its resources and exercise, and the
//! check questions, one row each, so a lesson can leave the desk for a
//! spreadsheet, a flashcard deck or a printout.
//!
//! One flat table with a `kind` column holds everything, because a lesson is
//! several kinds of record and a reader can filter by kind. The answer key
//! (correct answers and explanations) is included only once the check has
//! been submitted: the check is the point of the lesson, and a key that can
//! be read from a file before it is answered is not a check.

use crate::classroom::{self, StoredEngineeringLesson};
use crate::domain::assessments::{self, Owner, Purpose, ResponseStatus, Round};
use crate::domain::sessions::{self, SessionId};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

pub const COLUMNS: [&str; 14] = [
    "kind",
    "position",
    "section",
    "text",
    "detail",
    "choice_a",
    "choice_b",
    "choice_c",
    "choice_d",
    "correct_answer",
    "explanation",
    "your_answer",
    "result",
    "url",
];

/// One record of the lesson. Every kind uses `text`; questions fill the
/// choice and answer columns, resources the url, the rest `detail`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row {
    pub kind: &'static str,
    pub position: Option<usize>,
    pub section: String,
    pub text: String,
    pub detail: String,
    pub choices: [String; 4],
    pub correct_answer: String,
    pub explanation: String,
    pub your_answer: String,
    pub result: String,
    pub url: String,
}

impl Row {
    fn new(kind: &'static str, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            ..Self::default()
        }
    }
    fn meta(key: &str, value: impl Into<String>) -> Self {
        Self {
            section: key.into(),
            ..Self::new("meta", value)
        }
    }
    fn fields(&self) -> [String; 14] {
        [
            self.kind.to_string(),
            self.position.map(|p| p.to_string()).unwrap_or_default(),
            self.section.clone(),
            self.text.clone(),
            self.detail.clone(),
            self.choices[0].clone(),
            self.choices[1].clone(),
            self.choices[2].clone(),
            self.choices[3].clone(),
            self.correct_answer.clone(),
            self.explanation.clone(),
            self.your_answer.clone(),
            self.result.clone(),
            self.url.clone(),
        ]
    }
}

/// One saved lesson, gathered for leaving the desk: what the reader saw,
/// the learner's own work, and the check with its key when the check is
/// done. The CSV is a flattening of this; the PDF is laid out from it.
#[derive(Debug, Clone, Serialize)]
pub struct LessonDocument {
    /// A file name without extension, safe on every platform.
    pub file_stem: String,
    /// The class the lesson belongs to, which names its folder.
    pub class_label: String,
    pub title: String,
    pub topic: String,
    pub category: String,
    pub date: String,
    pub status: String,
    pub score: Option<f64>,
    pub tutor: String,
    /// Whether correct answers and explanations are in the document.
    pub answer_key: bool,
    pub research_note: Option<String>,
    pub review_notes: Vec<String>,
    /// `beginner` or `standard`; the PDF names sections for the level.
    pub level: String,
    pub markdown: String,
    pub resources: Vec<crate::generator::Resource>,
    pub exercise: Option<ExerciseDocument>,
    pub questions: Vec<QuestionDocument>,
    pub language: Option<LanguageDocument>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct ExerciseDocument {
    pub title: String,
    pub instructions: String,
    pub deliverable: Option<String>,
    pub starter_code: Option<String>,
    pub hints: Vec<String>,
    pub draft: Option<String>,
    pub reflection: Option<String>,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QuestionDocument {
    pub position: usize,
    pub prompt: String,
    pub section: String,
    pub objective: String,
    pub choices: Vec<String>,
    pub correct_answer: Option<String>,
    pub explanation: Option<String>,
    pub your_answer: Option<String>,
    pub result: Option<String>,
}

/// The parts of a language lesson that are not Markdown.
#[derive(Debug, Clone, Serialize, Default)]
pub struct LanguageDocument {
    pub scenario: String,
    pub can_do: String,
    pub phrases: Vec<crate::language::Phrase>,
    pub dialogue: Vec<crate::language::DialogueLine>,
    pub speaking_prompt: String,
    pub writing_prompt: String,
    pub listen_text: String,
}

impl LessonDocument {
    pub fn csv(&self) -> String {
        csv(&rows(self))
    }
}

/// The CSV rows of a document: metadata first, then the reading, resources,
/// exercise and work, then one row per question.
pub fn rows(document: &LessonDocument) -> Vec<Row> {
    let mut rows = vec![Row::meta("title", document.title.clone())];
    match &document.language {
        Some(language) => {
            rows.push(Row::meta("scenario", language.scenario.clone()));
            rows.push(Row::meta("can_do", language.can_do.clone()));
        }
        None => {
            rows.push(Row::meta("topic", document.topic.clone()));
            rows.push(Row::meta("category", document.category.clone()));
        }
    }
    rows.push(Row::meta("class", document.class_label.clone()));
    rows.push(Row::meta("date", document.date.clone()));
    rows.push(Row::meta("status", document.status.clone()));
    if let Some(score) = document.score {
        rows.push(Row::meta("score", percent(Some(score))));
    }
    if !document.tutor.is_empty() {
        rows.push(Row::meta("tutor", document.tutor.clone()));
    }
    rows.push(key_note(document.answer_key));
    if let Some(note) = &document.research_note {
        rows.push(Row::meta("sources", note.clone()));
    }
    for note in &document.review_notes {
        rows.push(Row::meta("editor_note", note.clone()));
    }
    rows.extend(section_rows(&document.markdown));
    if let Some(language) = &document.language {
        rows.extend(language_rows(language));
    }
    rows.extend(resource_rows(&document.resources));
    if let Some(exercise) = &document.exercise {
        rows.extend(exercise_rows(exercise));
    }
    for question in &document.questions {
        rows.push(question_row(question));
    }
    rows
}

fn question_row(question: &QuestionDocument) -> Row {
    let mut row = Row::new("question", question.prompt.clone());
    row.position = Some(question.position);
    row.section = question.section.clone();
    row.detail = question.objective.clone();
    for (slot, choice) in row.choices.iter_mut().zip(&question.choices) {
        *slot = choice.clone();
    }
    row.correct_answer = question.correct_answer.clone().unwrap_or_default();
    row.explanation = question.explanation.clone().unwrap_or_default();
    row.your_answer = question.your_answer.clone().unwrap_or_default();
    row.result = question.result.clone().unwrap_or_default();
    row
}

/// RFC 4180 text: CRLF line ends, fields quoted when they need it, doubled
/// quotes inside. A UTF-8 byte-order mark opens the file so spreadsheets read
/// accents and arrows correctly. A field that a spreadsheet would run as a
/// formula (`=`, `+`, `-`, `@`, or a control character first) is prefixed
/// with an apostrophe, because lesson text is written by a language model and
/// a cell must never execute.
pub fn csv(rows: &[Row]) -> String {
    let mut out = String::from("\u{feff}");
    out.push_str(&COLUMNS.join(","));
    out.push_str("\r\n");
    for row in rows {
        let line = row
            .fields()
            .iter()
            .map(|value| field(value))
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&line);
        out.push_str("\r\n");
    }
    out
}

fn field(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let guarded = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    if guarded.contains([',', '"', '\n', '\r']) || guarded.starts_with('\'') {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

/// Split Markdown into its `##` sections, code fences respected. Text before
/// the first heading (the title, an intro) becomes a section with no name.
pub fn split_sections(markdown: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = vec![(String::new(), String::new())];
    let mut fence: Option<&str> = None;
    for line in markdown.lines() {
        let trimmed = line.trim();
        let marker = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(marker) = marker {
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
        }
        if fence.is_none() {
            if let Some(heading) = trimmed.strip_prefix("## ") {
                sections.push((
                    heading.trim().trim_end_matches('#').trim().into(),
                    String::new(),
                ));
                continue;
            }
        }
        let body = &mut sections.last_mut().expect("one section").1;
        body.push_str(line);
        body.push('\n');
    }
    sections
        .into_iter()
        .map(|(title, body)| (title, body.trim().to_string()))
        .filter(|(title, body)| !title.is_empty() || !body.is_empty())
        .collect()
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-');
    trimmed
        .chars()
        .take(60)
        .collect::<String>()
        .trim_end_matches('-')
        .to_string()
}

fn file_stem(subject: &str, date: &str, title: &str) -> String {
    let date = if date.is_empty() { "undated" } else { date };
    let mut stem = format!("{}-{date}", slug(subject));
    let title = slug(title);
    if !title.is_empty() {
        stem.push('-');
        stem.push_str(&title);
    }
    stem
}

fn percent(score: Option<f64>) -> String {
    score
        .map(|score| format!("{}%", (score * 100.0).round() as i64))
        .unwrap_or_default()
}

/// A question as the reader saw it, with its key, in either subject's shape.
struct Question {
    position: usize,
    prompt: String,
    choices: Vec<String>,
    correct_index: Option<usize>,
    correct_text: String,
    explanation: String,
    section: String,
    objective: String,
}

impl Question {
    fn from_value(position: usize, body: &serde_json::Value) -> Self {
        let text = |key: &str| body[key].as_str().unwrap_or_default().to_string();
        let choices = body["choices"]
            .as_array()
            .map(|choices| {
                choices
                    .iter()
                    .filter_map(|choice| choice.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let correct_index = body["correct_index"].as_u64().map(|index| index as usize);
        Self {
            position,
            prompt: text("prompt"),
            correct_text: text("correct_answer"),
            explanation: text("explanation"),
            section: if body["section"].is_string() {
                text("section")
            } else {
                text("strand")
            },
            objective: text("learning_objective"),
            choices,
            correct_index,
        }
    }

    fn correct_answer(&self) -> String {
        self.correct_index
            .and_then(|index| self.choices.get(index).cloned())
            .unwrap_or_else(|| self.correct_text.clone())
    }

    fn document(&self, key: bool, answer: Option<&str>) -> QuestionDocument {
        let result = answer.filter(|_| key).map(|answer| {
            if answer == self.correct_answer() {
                "correct".to_string()
            } else {
                "incorrect".to_string()
            }
        });
        QuestionDocument {
            position: self.position,
            prompt: self.prompt.clone(),
            section: self.section.clone(),
            objective: self.objective.clone(),
            choices: self.choices.clone(),
            correct_answer: key.then(|| self.correct_answer()),
            explanation: key.then(|| self.explanation.clone()),
            your_answer: answer.map(str::to_string),
            result,
        }
    }
}

/// The learner's answer to a question, resolved to its text.
fn answer_text(question: &Question, raw: &str) -> String {
    raw.parse::<usize>()
        .ok()
        .and_then(|index| question.choices.get(index).cloned())
        .unwrap_or_else(|| raw.to_string())
}

/// Questions and answers from a frozen check round: the questions as shown,
/// the submitted answers if the round was submitted, else the saved drafts.
fn round_questions(round: &Round) -> (Vec<Question>, Vec<Option<String>>, bool) {
    let questions: Vec<Question> = round
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| Question::from_value(index + 1, &item.body))
        .collect();
    let responses = round
        .submission
        .as_ref()
        .map(|submission| &submission.responses)
        .unwrap_or(&round.responses);
    let answers = round
        .items
        .iter()
        .zip(&questions)
        .map(|(item, question)| {
            responses
                .get(&item.id)
                .filter(|response| response.status == ResponseStatus::Answered)
                .map(|response| answer_text(question, &response.answer))
        })
        .collect();
    (questions, answers, round.submission.is_some())
}

fn exercise_rows(exercise: &ExerciseDocument) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut head = Row::new("exercise", exercise.title.clone());
    head.detail = exercise.instructions.clone();
    rows.push(head);
    if let Some(deliverable) = exercise
        .deliverable
        .as_deref()
        .filter(|d| !d.trim().is_empty())
    {
        rows.push(Row::new("exercise_deliverable", deliverable));
    }
    if let Some(starter) = exercise
        .starter_code
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        rows.push(Row::new("exercise_starter_code", starter));
    }
    for (index, hint) in exercise.hints.iter().enumerate() {
        let mut row = Row::new("exercise_hint", hint.clone());
        row.position = Some(index + 1);
        rows.push(row);
    }
    if let Some(draft) = exercise.draft.as_deref().filter(|d| !d.trim().is_empty()) {
        rows.push(Row::new("exercise_draft", draft));
    }
    if let Some(reflection) = exercise
        .reflection
        .as_deref()
        .filter(|r| !r.trim().is_empty())
    {
        rows.push(Row::new("exercise_reflection", reflection.trim()));
    }
    if exercise.completed {
        rows.push(Row::new("exercise_status", "completed"));
    }
    rows
}

fn exercise_document(
    exercise: &crate::generator::Exercise,
    work: Option<&crate::db::ExerciseView>,
) -> ExerciseDocument {
    ExerciseDocument {
        title: exercise.title.clone(),
        instructions: exercise.instructions.clone(),
        deliverable: exercise.deliverable.clone(),
        starter_code: exercise.starter_code.clone(),
        hints: exercise.hints.clone(),
        draft: work.and_then(|work| work.draft.clone()),
        reflection: work.map(|work| work.reflection.clone()),
        completed: work.is_some_and(|work| work.completed),
    }
}

fn resource_rows(resources: &[crate::generator::Resource]) -> Vec<Row> {
    resources
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            let mut row = Row::new("resource", resource.title.clone());
            row.position = Some(index + 1);
            row.detail = resource.why.clone();
            row.section = resource.kind.clone();
            row.url = resource.url.clone();
            row
        })
        .collect()
}

fn section_rows(markdown: &str) -> Vec<Row> {
    split_sections(markdown)
        .into_iter()
        .enumerate()
        .map(|(index, (title, body))| {
            let mut row = Row::new("section", body);
            row.position = Some(index + 1);
            row.section = title;
            row
        })
        .collect()
}

fn key_note(key: bool) -> Row {
    Row::meta(
        "answer_key",
        if key {
            "included"
        } else {
            "withheld until the check is submitted"
        },
    )
}

/// Gather one saved lesson by the same source and owner the Progress page
/// uses to open it.
pub fn document(conn: &Connection, source: &str, owner_id: &str) -> Result<LessonDocument, String> {
    match source {
        "study" => study(conn, owner_id),
        "classroom" => legacy_classroom(conn, parse_id(owner_id)?),
        "language" => legacy_language(conn, parse_id(owner_id)?),
        "primary" => primary(conn, owner_id),
        _ => Err("Unknown lesson source".into()),
    }
}

fn parse_id(owner_id: &str) -> Result<i64, String> {
    owner_id
        .parse()
        .map_err(|_| "Invalid lesson identity".to_string())
}

fn question_documents(
    questions: &[Question],
    answers: &[Option<String>],
    key: bool,
) -> Vec<QuestionDocument> {
    questions
        .iter()
        .enumerate()
        .map(|(index, question)| {
            question.document(key, answers.get(index).and_then(|answer| answer.as_deref()))
        })
        .collect()
}

fn stored_questions<T: Serialize>(questions: &[T]) -> Vec<Question> {
    questions
        .iter()
        .enumerate()
        .map(|(index, question)| {
            Question::from_value(
                index + 1,
                &serde_json::to_value(question).unwrap_or_default(),
            )
        })
        .collect()
}

fn language_rows(language: &LanguageDocument) -> Vec<Row> {
    let mut rows = Vec::new();
    for (index, phrase) in language.phrases.iter().enumerate() {
        let mut row = Row::new("phrase", phrase.target.clone());
        row.position = Some(index + 1);
        row.detail = phrase.translation.clone();
        row.section = phrase.note.clone();
        rows.push(row);
    }
    for (index, line) in language.dialogue.iter().enumerate() {
        let mut row = Row::new("dialogue", line.target.clone());
        row.position = Some(index + 1);
        row.section = line.speaker.clone();
        row.detail = line.translation.clone();
        rows.push(row);
    }
    for (kind, text) in [
        ("speaking_prompt", &language.speaking_prompt),
        ("writing_prompt", &language.writing_prompt),
        ("listening_text", &language.listen_text),
    ] {
        if !text.trim().is_empty() {
            rows.push(Row::new(kind, text.clone()));
        }
    }
    rows
}

fn language_document(stored: &crate::language::StoredLesson) -> LanguageDocument {
    LanguageDocument {
        scenario: stored.scenario.clone(),
        can_do: stored.can_do.clone(),
        phrases: stored.phrases.clone(),
        dialogue: stored.dialogue.clone(),
        speaking_prompt: stored.speaking_prompt.clone(),
        writing_prompt: stored.writing_prompt.clone(),
        listen_text: stored.listen_text.clone(),
    }
}

fn study(conn: &Connection, owner_id: &str) -> Result<LessonDocument, String> {
    let id = SessionId(owner_id.to_string());
    let session = sessions::get(conn, &id).map_err(|e| e.to_string())?;
    let lesson = sessions::lesson(conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or("This lesson has not been prepared yet, so there is nothing to export.")?;
    let course_id = session.context.course.course_id.clone();
    let spec = classroom::subject(&course_id)?;
    let program = classroom::program_row(conn, &course_id)?;
    let date = session.context.selection["service_date"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let round = assessments::latest(conn, &Owner::StudySession(id.0.clone()), Purpose::ExitCheck)
        .map_err(|e| e.to_string())?
        .and_then(|attempt| attempt.rounds.last().cloned());
    let result = sessions::result(conn, &id).map_err(|e| e.to_string())?;
    let status = if session.status.terminal() {
        format!("{:?}", session.status).to_lowercase()
    } else {
        "in progress".to_string()
    };
    let score = result
        .as_ref()
        .and_then(|result| result.outcome["result"]["score"].as_f64());
    let tutor = lesson.content.provenance["agent"]
        .as_str()
        .or_else(|| lesson.content.provenance["runner"].as_str())
        .unwrap_or(&program.agent)
        .to_string();
    let (shown, answers, submitted) = match &round {
        Some(round) => round_questions(round),
        None => (Vec::new(), Vec::new(), false),
    };
    let key = submitted || session.status.terminal();
    let title = lesson.content.title.clone();
    let file_stem = file_stem(&program.short_code, &date, &title);

    match spec.kind {
        crate::catalog::SubjectKind::Engineering => {
            let stored: StoredEngineeringLesson =
                serde_json::from_value(lesson.content.body.clone()).map_err(|e| e.to_string())?;
            let work = match &stored.exercise {
                Some(_) => crate::subjects::engineering::exercise_view(conn, &id)?,
                None => None,
            };
            let questions = if shown.is_empty() {
                stored_questions(&stored.questions)
            } else {
                shown
            };
            Ok(LessonDocument {
                file_stem,
                class_label: program.label,
                title,
                topic: stored.concept_title,
                category: stored.category,
                date,
                status,
                score,
                tutor,
                answer_key: key,
                research_note: stored.research_note,
                review_notes: stored.review_notes,
                level: stored.level,
                markdown: stored.markdown,
                resources: stored.resources,
                exercise: stored
                    .exercise
                    .as_ref()
                    .map(|exercise| exercise_document(exercise, work.as_ref())),
                questions: question_documents(&questions, &answers, key),
                language: None,
            })
        }
        crate::catalog::SubjectKind::Language => {
            let stored: crate::language::StoredLesson =
                serde_json::from_value(lesson.content.body.clone()).map_err(|e| e.to_string())?;
            let questions = if shown.is_empty() {
                stored_questions(&stored.questions)
            } else {
                shown
            };
            Ok(LessonDocument {
                file_stem,
                class_label: program.label,
                title,
                topic: stored.scenario.clone(),
                category: stored.phase_label.clone(),
                date,
                status,
                score,
                tutor,
                answer_key: key,
                research_note: None,
                review_notes: Vec::new(),
                level: "standard".into(),
                markdown: stored.markdown.clone(),
                resources: Vec::new(),
                exercise: None,
                questions: question_documents(&questions, &answers, key),
                language: Some(language_document(&stored)),
            })
        }
    }
}

/// The columns of a retired classroom session the export reads.
struct LegacyClassroomRow {
    label: String,
    short_code: String,
    date: String,
    status: String,
    score: Option<f64>,
    payload: String,
    response: String,
    exercise_completed: bool,
    reflection: String,
}

fn legacy_classroom(conn: &Connection, session_id: i64) -> Result<LessonDocument, String> {
    let row = conn
        .query_row(
            "SELECT p.label, p.short_code, s.session_date, s.status, s.score,
                    s.payload_json, s.response_json, s.exercise_completed, s.exercise_reflection
             FROM classroom_sessions s JOIN classroom_programs p ON p.subject_id = s.subject_id
             WHERE s.id = ?1",
            [session_id],
            |r| {
                Ok(LegacyClassroomRow {
                    label: r.get(0)?,
                    short_code: r.get(1)?,
                    date: r.get(2)?,
                    status: r.get(3)?,
                    score: r.get(4)?,
                    payload: r.get(5)?,
                    response: r.get(6)?,
                    exercise_completed: r.get::<_, i64>(7)? != 0,
                    reflection: r.get(8)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some(row) = row else {
        return Err("That lesson is no longer on this desk.".into());
    };
    let stored: StoredEngineeringLesson =
        serde_json::from_str(&row.payload).map_err(|e| e.to_string())?;
    let key = row.status == "completed";
    let exercise = match &stored.exercise {
        Some(exercise) => {
            let draft = crate::db::get_exercise_draft(conn, None, Some(session_id))
                .map_err(|e| e.to_string())?;
            let mut document = exercise_document(exercise, None);
            document.draft = draft;
            document.reflection = Some(row.reflection.clone());
            document.completed = row.exercise_completed;
            Some(document)
        }
        None => None,
    };
    let raw_answers: Vec<Option<String>> = serde_json::from_str::<serde_json::Value>(&row.response)
        .ok()
        .and_then(|value| value["answers"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .map(|answer| answer.as_u64().map(|index| index.to_string()))
        .collect();
    let questions = stored_questions(&stored.questions);
    let answers: Vec<Option<String>> = questions
        .iter()
        .enumerate()
        .map(|(index, question)| {
            raw_answers
                .get(index)
                .cloned()
                .flatten()
                .map(|raw| answer_text(question, &raw))
        })
        .collect();
    Ok(LessonDocument {
        file_stem: file_stem(&row.short_code, &row.date, &stored.title),
        class_label: row.label,
        title: stored.title,
        topic: stored.concept_title,
        category: stored.category,
        date: row.date,
        status: row.status.replace('_', " "),
        score: row.score,
        tutor: stored.source,
        answer_key: key,
        research_note: stored.research_note,
        review_notes: stored.review_notes,
        level: stored.level,
        markdown: stored.markdown,
        resources: stored.resources,
        exercise,
        questions: question_documents(&questions, &answers, key),
        language: None,
    })
}

fn legacy_language(conn: &Connection, session_id: i64) -> Result<LessonDocument, String> {
    let row: Option<(String, String, String, String, Option<f64>, String)> = conn
        .query_row(
            "SELECT s.language, p.label, s.session_date, s.status, s.score, s.lesson_json
             FROM language_sessions s JOIN classroom_programs p ON p.subject_id = s.language
             WHERE s.id = ?1",
            [session_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((language, label, date, status, score, payload)) = row else {
        return Err("That lesson is no longer on this desk.".into());
    };
    let stored: crate::language::StoredLesson =
        serde_json::from_str(&payload).map_err(|e| e.to_string())?;
    let key = status == "completed";
    let questions = stored_questions(&stored.questions);
    Ok(LessonDocument {
        file_stem: file_stem(&language, &date, &stored.title),
        class_label: label,
        title: stored.title.clone(),
        topic: stored.scenario.clone(),
        category: stored.phase_label.clone(),
        date,
        status: status.replace('_', " "),
        score,
        tutor: String::new(),
        answer_key: key,
        research_note: None,
        review_notes: Vec::new(),
        level: "standard".into(),
        markdown: stored.markdown.clone(),
        resources: Vec::new(),
        exercise: None,
        questions: question_documents(&questions, &[], key),
        language: Some(language_document(&stored)),
    })
}

/// A lesson of the retired daily routine: the course, its questions and the
/// answers given that day. Those sessions are finished, so the key is always
/// included.
fn primary(conn: &Connection, owner_id: &str) -> Result<LessonDocument, String> {
    let course_id: Option<i64> = conn
        .query_row(
            "SELECT c.id FROM primary_session_ids i
             JOIN sessions s ON s.date = i.legacy_date
             JOIN courses c ON c.session_date = s.date AND c.concept_id = s.concept_id
             WHERE i.session_id = ?1 ORDER BY c.id DESC LIMIT 1",
            [owner_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let course_id = course_id.ok_or("That lesson is no longer on this desk.")?;
    let course = crate::db::get_course(conn, course_id)
        .map_err(|e| e.to_string())?
        .ok_or("That lesson is no longer on this desk.")?;
    let concept = crate::db::get_concept(conn, course.concept_id).map_err(|e| e.to_string())?;
    let title = concept
        .as_ref()
        .map(|concept| concept.title.clone())
        .unwrap_or_else(|| "Saved study session".into());
    let focus = concept
        .as_ref()
        .map(|concept| concept.focus.clone())
        .unwrap_or_default();
    let resources: Vec<crate::generator::Resource> =
        serde_json::from_str(&course.resources_json).unwrap_or_default();
    let exercise =
        match crate::db::get_course_exercise(conn, course_id).map_err(|e| e.to_string())? {
            Some(exercise) => {
                let draft = crate::db::get_exercise_draft(conn, Some(course_id), None)
                    .map_err(|e| e.to_string())?;
                let (completed, reflection) =
                    crate::db::get_exercise_completion(conn, Some(course_id), None)
                        .map_err(|e| e.to_string())?;
                Some(ExerciseDocument {
                    title: exercise.title,
                    instructions: exercise.instructions,
                    deliverable: exercise.deliverable,
                    starter_code: exercise.starter_code,
                    hints: exercise.hints,
                    draft,
                    reflection: Some(reflection),
                    completed,
                })
            }
            None => None,
        };
    let attempts =
        crate::db::attempts_for_session(conn, &course.session_date).map_err(|e| e.to_string())?;
    let questions = crate::db::questions_for_course(conn, course_id).map_err(|e| e.to_string())?;
    let questions = questions
        .iter()
        .enumerate()
        .map(|(index, question)| {
            let choices: Vec<String> = question
                .choices_json
                .as_deref()
                .and_then(|json| serde_json::from_str(json).ok())
                .unwrap_or_default();
            let shaped = Question {
                position: index + 1,
                prompt: question.prompt.clone(),
                correct_index: None,
                correct_text: question.correct_answer.clone(),
                explanation: question.explanation.clone(),
                section: question.kind.clone(),
                objective: String::new(),
                choices,
            };
            let attempt = attempts
                .iter()
                .find(|attempt| attempt.question_id == question.id);
            let mut document =
                shaped.document(true, attempt.map(|attempt| attempt.user_answer.as_str()));
            if let Some(attempt) = attempt {
                document.result = Some(if attempt.correct {
                    "correct".into()
                } else {
                    "incorrect".into()
                });
            }
            document
        })
        .collect();
    Ok(LessonDocument {
        file_stem: file_stem(&focus, &course.session_date, &title),
        class_label: crate::focus::label(&focus).to_string(),
        title: title.clone(),
        topic: title,
        category: concept
            .as_ref()
            .map(|concept| concept.category.clone())
            .unwrap_or_default(),
        date: course.session_date,
        status: "completed".into(),
        score: None,
        tutor: course.source,
        answer_key: true,
        research_note: None,
        review_notes: Vec::new(),
        level: "standard".into(),
        markdown: course.markdown,
        resources,
        exercise,
        questions,
        language: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_quoted_escaped_and_never_formulas() {
        assert_eq!(field("plain"), "plain");
        assert_eq!(field(""), "");
        assert_eq!(field("a, b"), "\"a, b\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(field("two\nlines"), "\"two\nlines\"");
        assert_eq!(field("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(field("- a bullet"), "\"'- a bullet\"");
        assert_eq!(field("@mention"), "\"'@mention\"");
    }

    #[test]
    fn the_table_has_a_header_a_bom_and_crlf_line_ends() {
        let mut question = Row::new("question", "Which wins?");
        question.position = Some(1);
        question.choices[0] = "micro".into();
        question.choices[1] = "macro".into();
        question.correct_answer = "micro".into();
        let text = csv(&[Row::meta("title", "Event loop"), question]);
        let mut lines = text.split("\r\n");
        assert_eq!(
            lines.next().unwrap(),
            format!("\u{feff}{}", COLUMNS.join(","))
        );
        assert_eq!(lines.next().unwrap(), "meta,,title,Event loop,,,,,,,,,,");
        assert_eq!(
            lines.next().unwrap(),
            "question,1,,Which wins?,,micro,macro,,,micro,,,,"
        );
        assert_eq!(lines.next(), Some(""));
    }

    #[test]
    fn sections_split_on_second_level_headings_outside_fences() {
        let markdown = "# Title\n\nIntro line.\n\n## First\n\n*~1 min · hint*\n\n```md\n## not a heading\n```\n\n## Second ##\nbody";
        let sections = split_sections(markdown);
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].0, "");
        assert_eq!(sections[0].1, "# Title\n\nIntro line.");
        assert_eq!(sections[1].0, "First");
        assert!(sections[1].1.contains("## not a heading"));
        assert_eq!(sections[2], ("Second".to_string(), "body".to_string()));
    }

    #[test]
    fn file_stems_are_plain_and_bounded() {
        assert_eq!(
            file_stem(
                "SD",
                "2026-09-11",
                "CAP in practice: choosing partition behaviour, not a permanent label"
            ),
            "sd-2026-09-11-cap-in-practice-choosing-partition-behaviour-not-a-permanent"
        );
        assert_eq!(
            file_stem("german", "", "Sich vorstellen — Teil 1"),
            "german-undated-sich-vorstellen-teil-1"
        );
    }

    #[test]
    fn a_question_row_holds_the_key_only_when_asked() {
        let question = Question::from_value(
            2,
            &serde_json::json!({
                "prompt": "Which queue drains first?",
                "choices": ["microtasks", "macrotasks", "render", "idle"],
                "correct_index": 0,
                "explanation": "Microtasks run before the next macrotask.",
                "section": "Core mechanics",
                "learning_objective": "order the queues"
            }),
        );
        let hidden = question_row(&question.document(false, Some("macrotasks")));
        assert_eq!(hidden.correct_answer, "");
        assert_eq!(hidden.explanation, "");
        assert_eq!(hidden.your_answer, "macrotasks");
        assert_eq!(hidden.result, "");
        let shown = question_row(&question.document(true, Some("macrotasks")));
        assert_eq!(shown.correct_answer, "microtasks");
        assert_eq!(shown.result, "incorrect");
        assert_eq!(shown.section, "Core mechanics");
        assert_eq!(shown.detail, "order the queues");
        assert_eq!(answer_text(&question, "1"), "macrotasks");
        assert_eq!(answer_text(&question, "free text"), "free text");
    }
}
