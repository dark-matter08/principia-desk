//! Read-only progress across compatibility engines. Switch this projection with
//! the shared-runtime writers during their cutover; never union both copies.
use crate::classroom;
use chrono::{Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const PAGE_SIZE: i64 = 8;
const HISTORY: &str = r#"WITH history AS (
 SELECT 'primary' AS source, i.session_id AS owner_id, s.date,
 COALESCE(NULLIF(s.focus,''),c.focus,'legacy') AS subject_id,
 COALESCE(c.title,'Saved study session') AS title, s.status, s.quiz_score AS score,
 (SELECT id FROM courses WHERE session_date=s.date AND concept_id=s.concept_id ORDER BY id DESC LIMIT 1) AS course_id, s.started_at
 FROM sessions s JOIN primary_session_ids i ON i.legacy_date=s.date
 LEFT JOIN concepts c ON c.id=s.concept_id WHERE s.status!='pending'
 AND s.date NOT IN (SELECT legacy_key FROM legacy_crosswalk WHERE legacy_table='primary_import')
 UNION ALL
 SELECT 'classroom',CAST(id AS TEXT),session_date,subject_id,title,status,score,NULL,started_at FROM classroom_sessions
 UNION ALL
 SELECT 'language',CAST(id AS TEXT),session_date,language,
 CASE WHEN json_valid(lesson_json) THEN COALESCE(json_extract(lesson_json,'$.title'),unit_slug) ELSE unit_slug END,
 status,score,NULL,started_at FROM language_sessions
 UNION ALL
 SELECT 'study',s.id,json_extract(s.context_json,'$.selection.service_date'),
 COALESCE(cl.course_id,json_extract(s.context_json,'$.course.course_id')),
 COALESCE(json_extract(v.content_json,'$.title'),json_extract(s.context_json,'$.selection.title')),
 CASE s.status WHEN 'completed' THEN 'completed' WHEN 'skipped' THEN 'skipped' ELSE 'in_progress' END,
 COALESCE(json_extract(r.outcome_json,'$.result.score'),json_extract(r.outcome_json,'$.legacy.quiz_score')),
 CASE WHEN json_extract(v.content_json,'$.body.markdown') IS NULL THEN NULL ELSE 1 END, s.created_at
 FROM study_sessions s LEFT JOIN classes cl ON cl.id=s.class_id
 LEFT JOIN lesson_versions v ON v.id=s.lesson_version_id
 LEFT JOIN study_results r ON r.session_id=s.id
 WHERE s.owner_kind IN ('class','daily_routine')
)"#;

#[derive(Debug, Default, Deserialize)]
pub struct ProgressQuery {
    pub subject_id: Option<String>,
    #[serde(default)]
    pub search: String,
    pub status: Option<String>,
    #[serde(default)]
    pub page: i64,
}
#[derive(Debug, Serialize)]
pub struct ProgressEntry {
    pub source: String,
    pub owner_id: String,
    pub date: String,
    pub subject_id: String,
    pub title: String,
    pub status: String,
    pub score: Option<f64>,
    pub can_read: bool,
}
#[derive(Debug, Serialize)]
pub struct ProgressClass {
    pub subject_id: String,
    pub label: String,
    pub short_code: String,
    pub enabled: bool,
    pub progress: f64,
    pub progress_label: String,
    pub completed_sessions: i64,
}
#[derive(Debug, Serialize)]
pub struct ActivityDay {
    pub date: String,
    pub completed: i64,
}
#[derive(Debug, Serialize)]
pub struct DashboardView {
    pub today: String,
    pub classes: Vec<ProgressClass>,
    pub completed_sessions: i64,
    pub study_days: i64,
    pub streak: i64,
    pub activity: Vec<ActivityDay>,
    pub history: Vec<ProgressEntry>,
    pub history_total: i64,
    pub page: i64,
    pub page_size: i64,
}

/// One day of the study pulse: what was finished, and about how long it took.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PulseDay {
    pub date: String,
    pub completed: i64,
    pub minutes: i64,
    /// Short codes of the classes studied that day, for the tooltip.
    pub classes: Vec<String>,
}

/// The home page's view of the habit: streaks, the last half year of days,
/// and this week against its target.
#[derive(Debug, Clone, Serialize)]
pub struct StudyPulse {
    pub today: String,
    pub streak: i64,
    pub longest_streak: i64,
    pub study_days: i64,
    pub completed_sessions: i64,
    /// Every day of the window, oldest first, starting on a Monday so a
    /// heat map lays out in whole weeks.
    pub days: Vec<PulseDay>,
    pub week_minutes: i64,
    pub week_target_minutes: i64,
    pub week_sessions: i64,
    /// Share of submitted checks that passed, 0 to 1, when any were.
    pub pass_rate: Option<f64>,
}

/// Days covered by the pulse: twenty-six weeks, the way a contribution
/// graph shows half a year.
pub const PULSE_WEEKS: i64 = 26;

pub fn pulse(conn: &Connection, today: &str) -> Result<StudyPulse, String> {
    let today_date = NaiveDate::parse_from_str(today, "%Y-%m-%d").map_err(|e| e.to_string())?;
    let read = || -> rusqlite::Result<StudyPulse> {
        // Completed sessions by day, with the class and its session length.
        let mut statement = conn.prepare(&format!(
            "{HISTORY} SELECT h.date, h.subject_id, COALESCE(p.short_code, upper(substr(h.subject_id,1,2))), COALESCE(p.session_minutes, 30), COUNT(*)
             FROM history h LEFT JOIN classroom_programs p ON p.subject_id = h.subject_id
             WHERE h.status='completed' AND h.date<=?1 GROUP BY h.date, h.subject_id ORDER BY h.date"
        ))?;
        let rows = statement
            .query_map([today], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut by_day: std::collections::BTreeMap<String, PulseDay> =
            std::collections::BTreeMap::new();
        for (date, _subject, short_code, minutes, count) in &rows {
            let day = by_day.entry(date.clone()).or_insert_with(|| PulseDay {
                date: date.clone(),
                completed: 0,
                minutes: 0,
                classes: Vec::new(),
            });
            day.completed += count;
            day.minutes += minutes * count;
            if !day.classes.contains(short_code) {
                day.classes.push(short_code.clone());
            }
        }
        let completed_sessions = by_day.values().map(|day| day.completed).sum();
        let study_days = by_day.len() as i64;

        // Streaks: the current one may start yesterday; the longest is over all.
        let dates: Vec<NaiveDate> = by_day
            .keys()
            .filter_map(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
            .collect();
        let mut streak = 0;
        let mut expected = today_date;
        for day in dates.iter().rev() {
            if streak == 0 && *day == today_date - Duration::days(1) {
                expected = *day;
            }
            if *day != expected {
                break;
            }
            streak += 1;
            expected -= Duration::days(1);
        }
        let mut longest_streak = 0;
        let mut run = 0;
        let mut previous: Option<NaiveDate> = None;
        for day in &dates {
            run = match previous {
                Some(last) if *day == last + Duration::days(1) => run + 1,
                _ => 1,
            };
            longest_streak = longest_streak.max(run);
            previous = Some(*day);
        }

        // The window: whole weeks, Monday first, ending today.
        let weekday = chrono::Datelike::weekday(&today_date).num_days_from_monday() as i64;
        let first = today_date - Duration::days(weekday + 7 * (PULSE_WEEKS - 1));
        let mut days = Vec::new();
        let mut cursor = first;
        while cursor <= today_date {
            let key = cursor.to_string();
            days.push(by_day.get(&key).cloned().unwrap_or(PulseDay {
                date: key,
                completed: 0,
                minutes: 0,
                classes: Vec::new(),
            }));
            cursor += Duration::days(1);
        }

        // This week against the classes' own targets.
        let week_start = today_date - Duration::days(weekday);
        let (week_minutes, week_sessions) = by_day
            .values()
            .filter(|day| {
                NaiveDate::parse_from_str(&day.date, "%Y-%m-%d")
                    .is_ok_and(|date| date >= week_start)
            })
            .fold((0, 0), |(minutes, sessions), day| {
                (minutes + day.minutes, sessions + day.completed)
            });
        // A class with a target of its own counts that; otherwise what its
        // study times add up to, each day at its own length.
        let mut statement = conn.prepare(
            "SELECT p.subject_id, p.target_weekly_minutes FROM classroom_programs p WHERE p.enabled=1",
        )?;
        let targets = statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut week_target_minutes = 0;
        for (subject_id, target) in targets {
            week_target_minutes += if target > 0 {
                target
            } else {
                crate::classroom::weekly_minutes_scheduled(conn, &subject_id).unwrap_or(0)
            };
        }

        let (passed, submitted): (i64, i64) = conn.query_row(
            &format!(
                "{HISTORY} SELECT COALESCE(SUM(CASE WHEN score >= 0.8 THEN 1 ELSE 0 END),0), COUNT(*) FROM history WHERE status='completed' AND score IS NOT NULL AND date<=?1"
            ),
            [today],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(StudyPulse {
            today: today.to_string(),
            streak,
            longest_streak,
            study_days,
            completed_sessions,
            days,
            week_minutes,
            week_target_minutes,
            week_sessions,
            pass_rate: (submitted > 0).then(|| passed as f64 / submitted as f64),
        })
    };
    read().map_err(|e| e.to_string())
}

pub fn read(
    conn: &Connection,
    today: &str,
    query: &ProgressQuery,
) -> Result<DashboardView, String> {
    if query
        .status
        .as_deref()
        .is_some_and(|s| !matches!(s, "completed" | "in_progress" | "skipped"))
    {
        return Err("Unknown progress status".into());
    }
    let today_date = NaiveDate::parse_from_str(today, "%Y-%m-%d").map_err(|e| e.to_string())?;
    let snapshot = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let conn = &*snapshot;
    let read = || -> rusqlite::Result<_> {
        let mut statement = conn.prepare(&format!("{HISTORY} SELECT date,COUNT(*) FROM history WHERE status='completed' AND date<=?1 AND (?2 IS NULL OR subject_id=?2) GROUP BY date ORDER BY date DESC"))?;
        let completed_dates = statement
            .query_map(params![today, query.subject_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let completed_sessions = completed_dates.iter().map(|(_, n)| n).sum();
        let study_days = completed_dates.len() as i64;
        let mut streak = 0;
        let mut expected = today_date;
        for (day, _) in &completed_dates {
            let Ok(day) = NaiveDate::parse_from_str(day, "%Y-%m-%d") else {
                continue;
            };
            if streak == 0 && day == today_date - Duration::days(1) {
                expected = day;
            }
            if day != expected {
                break;
            }
            streak += 1;
            expected -= Duration::days(1);
        }
        let activity = (0..28)
            .rev()
            .map(|ago| {
                let date = (today_date - Duration::days(ago)).to_string();
                let completed = completed_dates
                    .iter()
                    .find(|(d, _)| d == &date)
                    .map(|(_, n)| *n)
                    .unwrap_or(0);
                ActivityDay { date, completed }
            })
            .collect();
        let mut statement = conn.prepare(&format!("{HISTORY} SELECT subject_id,COUNT(*) FROM history WHERE status='completed' AND date<=?1 GROUP BY subject_id"))?;
        let counts = statement
            .query_map([today], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?;
        let filter = "WHERE (?1 IS NULL OR subject_id=?1) AND (?2 IS NULL OR status=?2) AND (instr(lower(title),lower(?3))>0 OR instr(lower(subject_id),lower(?3))>0)";
        let search = query.search.trim();
        let history_total: i64 = conn.query_row(
            &format!("{HISTORY} SELECT COUNT(*) FROM history {filter}"),
            params![query.subject_id, query.status, search],
            |r| r.get(0),
        )?;
        let page = query
            .page
            .max(0)
            .min((history_total - 1).max(0) / PAGE_SIZE);
        let mut statement = conn.prepare(&format!("{HISTORY} SELECT source,owner_id,date,subject_id,title,status,score,source IN ('classroom','language') OR course_id IS NOT NULL FROM history {filter} ORDER BY date DESC,started_at DESC,source,owner_id DESC LIMIT ?4 OFFSET ?5"))?;
        let history = statement
            .query_map(
                params![
                    query.subject_id,
                    query.status,
                    search,
                    PAGE_SIZE,
                    page * PAGE_SIZE
                ],
                |r| {
                    Ok(ProgressEntry {
                        source: r.get(0)?,
                        owner_id: r.get(1)?,
                        date: r.get(2)?,
                        subject_id: r.get(3)?,
                        title: r.get(4)?,
                        status: r.get(5)?,
                        score: r.get(6)?,
                        can_read: r.get(7)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((
            completed_sessions,
            study_days,
            streak,
            activity,
            counts,
            history_total,
            page,
            history,
        ))
    };
    let (completed_sessions, study_days, streak, activity, counts, history_total, page, history) =
        read().map_err(|e| e.to_string())?;
    let classes = classroom::program_views(conn, today)?
        .into_iter()
        .map(|p| ProgressClass {
            completed_sessions: counts.get(&p.subject_id).copied().unwrap_or(0),
            subject_id: p.subject_id,
            label: p.label,
            short_code: p.short_code,
            enabled: p.enabled,
            progress: p.progress,
            progress_label: p.progress_label,
        })
        .collect();
    snapshot.commit().map_err(|e| e.to_string())?;
    Ok(DashboardView {
        today: today.into(),
        classes,
        completed_sessions,
        study_days,
        streak,
        activity,
        history,
        history_total,
        page,
        page_size: PAGE_SIZE,
    })
}

#[derive(Debug, Serialize)]
pub struct ProgressLesson {
    pub title: String,
    pub date: String,
    pub markdown: String,
    pub course_id: Option<i64>,
    pub classroom_session_id: Option<i64>,
    pub study_session_id: Option<String>,
}

/// Resolve an immutable owner, never a date supplied by an archive row.
pub fn lesson(
    conn: &Connection,
    source: &str,
    owner_id: &str,
) -> Result<Option<ProgressLesson>, String> {
    if source == "primary" {
        return conn.query_row("SELECT c.id,s.date,COALESCE(k.title,'Saved study session'),c.markdown FROM primary_session_ids i JOIN sessions s ON s.date=i.legacy_date JOIN courses c ON c.session_date=s.date AND c.concept_id=s.concept_id LEFT JOIN concepts k ON k.id=c.concept_id WHERE i.session_id=?1 ORDER BY c.id DESC LIMIT 1",[owner_id],|r|Ok(ProgressLesson {course_id:Some(r.get(0)?),classroom_session_id:None,study_session_id:None,date:r.get(1)?,title:r.get(2)?,markdown:r.get(3)?})).optional().map_err(|e|e.to_string());
    }
    if source == "study" {
        // The immutable lesson version, never a regenerated substitute.
        return conn.query_row("SELECT json_extract(s.context_json,'$.selection.service_date'),json_extract(v.content_json,'$.title'),json_extract(v.content_json,'$.body.markdown') FROM study_sessions s JOIN lesson_versions v ON v.id=s.lesson_version_id WHERE s.id=?1",[owner_id],|r|Ok(ProgressLesson {course_id:None,classroom_session_id:None,study_session_id:Some(owner_id.to_string()),date:r.get(0)?,title:r.get(1)?,markdown:r.get(2)?})).optional().map_err(|e|e.to_string());
    }
    let sql = match source {
        "classroom" => "SELECT session_date,title,payload_json FROM classroom_sessions WHERE id=?1",
        "language" => {
            "SELECT session_date,unit_slug,lesson_json FROM language_sessions WHERE id=?1"
        }
        _ => return Err("Unknown lesson source".into()),
    };
    let id: i64 = owner_id.parse().map_err(|_| "Invalid lesson identity")?;
    let row = conn
        .query_row(sql, [id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .optional()
        .map_err(|e| e.to_string())?;
    row.map(|(date, title, payload)| {
        let body: serde_json::Value =
            serde_json::from_str(&payload).map_err(|_| "Saved lesson could not be read")?;
        let markdown = body["markdown"]
            .as_str()
            .ok_or("Saved lesson has no reading content")?
            .to_string();
        Ok(ProgressLesson {
            title: body["title"].as_str().unwrap_or(&title).into(),
            date,
            markdown,
            course_id: None,
            classroom_session_id: (source == "classroom").then_some(id),
            study_session_id: None,
        })
    })
    .transpose()
}
