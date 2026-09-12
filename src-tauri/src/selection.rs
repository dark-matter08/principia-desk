//! Curriculum-aware topic selection (TEACHER.md §3).
//!
//! The wheel only shows *unlocked* concepts: tier-1 concepts unlock when at
//! least 70% of prerequisites have been practiced, while tier-2/3 concepts
//! require every prerequisite. Draws prefer the lowest unlocked, unvisited
//! tier so the first month forms coherent chains instead of jumping from a
//! foundation straight into an advanced leaf.

use crate::db::{self, Concept};
use rand::Rng;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};

/// Tier-1 allows one supporting prerequisite to lag in a larger set; deeper
/// material requires the complete foundation.
const TIER_ONE_UNLOCK_THRESHOLD: f64 = 0.7;
/// Draw-weight multiplier for concepts in a category the student is
/// struggling in or that is due for review (steers the wheel toward debt).
const HOT_CATEGORY_BOOST: f64 = 2.0;

/// Mastery states that count as "the student has practiced this".
fn is_practiced(state: &str) -> bool {
    matches!(
        state,
        "practicing" | "struggling" | "mastered" | "maintenance" | "decayed"
    )
}

/// Mastery states that count as "this module is done". Completed modules are
/// never re-served automatically; spaced-repetition retention lives in the
/// daily quiz, and an explicit opt-in revisit is the only way back to a full
/// lesson. Struggling/decayed concepts remain re-teachable: they were failed
/// or forgotten, not finished.
pub fn is_completed(state: &str) -> bool {
    matches!(state, "mastered" | "maintenance")
}

fn phase_rank(phase: &str) -> u8 {
    match phase {
        "foundations" => 0,
        "mechanisms" => 1,
        "production" => 2,
        "synthesis" => 3,
        "elective" => 4,
        _ => 5,
    }
}

fn prereqs_map(conn: &Connection, focus: &str) -> db::Result<HashMap<i64, Vec<String>>> {
    let mut stmt =
        conn.prepare("SELECT id, prereqs_json FROM concepts WHERE active = 1 AND focus = ?1")?;
    let rows = stmt.query_map(params![focus], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (id, json) = row?;
        out.insert(id, serde_json::from_str(&json).unwrap_or_default());
    }
    Ok(out)
}

fn states_by_slug(conn: &Connection, focus: &str) -> db::Result<HashMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT c.slug, COALESCE(m.state, 'unseen')
         FROM concepts c LEFT JOIN mastery m ON m.concept_id = c.id
         WHERE c.active = 1 AND c.focus = ?1",
    )?;
    let rows = stmt.query_map(params![focus], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    Ok(rows.collect::<std::result::Result<HashMap<_, _>, _>>()?)
}

/// All active concepts in a focus track split into (unlocked, locked).
pub fn pool_status(conn: &Connection, focus: &str) -> db::Result<(Vec<Concept>, Vec<Concept>)> {
    let all = db::all_concepts(conn, focus)?;
    let prereqs = prereqs_map(conn, focus)?;
    let states = states_by_slug(conn, focus)?;
    let mut unlocked = Vec::new();
    let mut locked = Vec::new();
    for c in all {
        let reqs = prereqs.get(&c.id).cloned().unwrap_or_default();
        let open = if reqs.is_empty() {
            true
        } else {
            let practiced = reqs
                .iter()
                .filter(|s| states.get(*s).map(|st| is_practiced(st)).unwrap_or(false))
                .count();
            let threshold = if c.tier >= 2 {
                1.0
            } else {
                TIER_ONE_UNLOCK_THRESHOLD
            };
            practiced as f64 / reqs.len() as f64 >= threshold
        };
        if open {
            unlocked.push(c);
        } else {
            locked.push(c);
        }
    }
    Ok((unlocked, locked))
}

/// Categories the student owes attention: any struggling/decayed concept,
/// or one whose spaced review is due.
fn hot_categories(conn: &Connection, today: &str, focus: &str) -> db::Result<HashSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT c.category
         FROM mastery m JOIN concepts c ON c.id = m.concept_id
         WHERE c.focus = ?2
           AND (m.state IN ('struggling','decayed')
            OR (m.next_review_date IS NOT NULL AND m.next_review_date <= ?1))",
    )?;
    let rows = stmt.query_map(params![today, focus], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<std::result::Result<HashSet<_>, _>>()?)
}

/// Weighted random draw over the least-picked *unlocked, uncompleted*
/// concepts, biased toward categories with open debt. Completed modules are
/// excluded so a finished track never re-serves a lesson. Marks the pick and
/// returns it.
pub fn draw(conn: &Connection, date: &str, focus: &str) -> db::Result<Option<Concept>> {
    draw_from(conn, date, focus, false)
}

/// Opt-in revisit: draw from the *completed* pool (least-picked first), used
/// only when the learner explicitly asks to redo a finished module.
pub fn draw_completed(conn: &Connection, date: &str, focus: &str) -> db::Result<Option<Concept>> {
    draw_from(conn, date, focus, true)
}

/// Whether the focus track still has at least one unlocked, uncompleted
/// concept to teach.
pub fn drawable_exists(conn: &Connection, focus: &str) -> db::Result<bool> {
    Ok(!drawable(conn, focus)?.is_empty())
}

/// All unlocked concepts the wheel may still serve: completed modules are
/// filtered out so finished tracks stop drawing automatically.
pub fn drawable(conn: &Connection, focus: &str) -> db::Result<Vec<Concept>> {
    let (unlocked, _) = pool_status(conn, focus)?;
    let states = states_by_slug(conn, focus)?;
    Ok(unlocked
        .into_iter()
        .filter(|c| {
            !states
                .get(&c.slug)
                .map(|state| is_completed(state))
                .unwrap_or(false)
        })
        .collect())
}

fn draw_from(
    conn: &Connection,
    date: &str,
    focus: &str,
    completed_only: bool,
) -> db::Result<Option<Concept>> {
    let (unlocked, _) = pool_status(conn, focus)?;
    let states = states_by_slug(conn, focus)?;
    let unlocked: Vec<Concept> = unlocked
        .into_iter()
        .filter(|c| {
            let completed = states
                .get(&c.slug)
                .map(|state| is_completed(state))
                .unwrap_or(false);
            completed == completed_only
        })
        .collect();
    if unlocked.is_empty() {
        return Ok(None);
    }
    // No repeats until the unlocked pool exhausts a full lap.
    let min_picked = unlocked.iter().map(|c| c.times_picked).min().unwrap_or(0);
    let least_picked: Vec<&Concept> = unlocked
        .iter()
        .filter(|c| c.times_picked == min_picked)
        .collect();
    let core_first = least_picked.iter().any(|concept| concept.curriculum.core);
    let core_pool: Vec<&Concept> = least_picked
        .into_iter()
        .filter(|concept| !core_first || concept.curriculum.core)
        .collect();
    let min_phase = core_pool
        .iter()
        .map(|concept| phase_rank(&concept.curriculum.phase))
        .min()
        .unwrap_or(0);
    let phase_pool: Vec<&Concept> = core_pool
        .into_iter()
        .filter(|concept| phase_rank(&concept.curriculum.phase) == min_phase)
        .collect();
    let min_tier = phase_pool.iter().map(|c| c.tier).min().unwrap_or(0);
    let pool: Vec<&Concept> = phase_pool
        .into_iter()
        .filter(|concept| concept.tier == min_tier)
        .collect();
    let hot = hot_categories(conn, date, focus).unwrap_or_default();
    let weight_of = |c: &Concept| -> f64 {
        let base = c.weight.max(0.01);
        if hot.contains(&c.category) {
            base * HOT_CATEGORY_BOOST
        } else {
            base
        }
    };
    let total: f64 = pool.iter().map(|c| weight_of(c)).sum();
    let mut roll = rand::thread_rng().gen_range(0.0..total);
    let mut chosen = (*pool.last().expect("non-empty pool")).clone();
    for c in &pool {
        roll -= weight_of(c);
        if roll <= 0.0 {
            chosen = (*c).clone();
            break;
        }
    }
    db::mark_concept_picked(conn, chosen.id, date)?;
    Ok(Some(chosen))
}
