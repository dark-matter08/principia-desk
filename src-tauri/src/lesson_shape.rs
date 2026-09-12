//! The shape of a lesson that fits its session.
//!
//! A lesson is read, practised and checked inside one sitting, so its minutes
//! are split before it is written and the writer is held to the split. The
//! reader relies on a few point-form conventions the tutor must follow: a
//! reading hint under every section heading, labelled callouts, and diagrams
//! where a picture beats prose. The deterministic parts live here so the gate,
//! the correction prompts and the tests agree on them.

use serde::Serialize;

/// How a session's minutes divide. Reading gets under half of them: a lesson
/// is something the learner does, not only something they read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SessionPlan {
    pub learn_minutes: i64,
    pub practice_minutes: i64,
    pub check_minutes: i64,
}

impl SessionPlan {
    /// The split for a lesson of `depth_minutes` (the budgeted depth, thirty
    /// to sixty minutes). Thirty minutes read as 14 to read, 10 to practise
    /// and 6 to check.
    pub fn for_depth(depth_minutes: i64) -> Self {
        let learn_minutes = depth_minutes * 47 / 100;
        let check_minutes = depth_minutes * 20 / 100;
        Self {
            learn_minutes,
            practice_minutes: depth_minutes - learn_minutes - check_minutes,
            check_minutes,
        }
    }
}

/// The reading hint a section opens with: `*~3 min · what to look for*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionHint {
    pub minutes: u32,
    pub text: String,
}

/// The hint a section body (everything after its heading) opens with.
pub fn section_hint(body: &str) -> Option<SectionHint> {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .and_then(parse_hint_line)
}

/// The hint line itself, if `line` is one. Tolerates `~`, `≈`, `minutes` or
/// `min`, and any of the usual separators after the time.
pub fn parse_hint_line(line: &str) -> Option<SectionHint> {
    let line = line.trim();
    let inner = line
        .strip_prefix('*')
        .and_then(|rest| rest.strip_suffix('*'))
        .or_else(|| {
            line.strip_prefix('_')
                .and_then(|rest| rest.strip_suffix('_'))
        })?
        .trim();
    // Bold, or an empty italic, is not a hint.
    if inner.is_empty() || inner.starts_with('*') || inner.starts_with('_') {
        return None;
    }
    let inner = inner.trim_start_matches(['~', '≈']).trim_start();
    let digits: String = inner.chars().take_while(char::is_ascii_digit).collect();
    let minutes: u32 = digits.parse().ok()?;
    let rest = inner[digits.len()..].trim_start();
    let rest = ["minutes", "mins", "min"]
        .iter()
        .find_map(|unit| rest.strip_prefix(unit))?;
    let text = rest
        .trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, '·' | '-' | '–' | '—' | ':' | '•' | '|')
        })
        .trim();
    if text.is_empty() {
        return None;
    }
    Some(SectionHint {
        minutes,
        text: text.to_string(),
    })
}

/// The minutes the section hints add up to.
pub fn reading_minutes(sections: &[String]) -> u32 {
    sections
        .iter()
        .filter_map(|section| section_hint(section))
        .map(|hint| hint.minutes)
        .sum()
}

/// Titles of the sections that do not open with a hint line.
pub fn sections_without_hints<'a>(titles: &[&'a str], sections: &[String]) -> Vec<&'a str> {
    titles
        .iter()
        .zip(sections)
        .filter(|(_, section)| section_hint(section).is_none())
        .map(|(title, _)| *title)
        .collect()
}

/// How many mermaid diagrams the lesson draws.
pub fn diagram_count(markdown: &str) -> usize {
    markdown
        .lines()
        .filter(|line| {
            let trimmed = line.trim().to_ascii_lowercase();
            trimmed.starts_with("```mermaid") || trimmed.starts_with("~~~mermaid")
        })
        .count()
}

/// A rewritten section that lost its hint line gets the original's back, so
/// a correction aimed at substance cannot strip the reader's orientation.
pub fn keep_hint(original: &str, rewritten: &str) -> String {
    if section_hint(rewritten).is_some() {
        return rewritten.to_string();
    }
    let Some(hint_line) = original
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .filter(|line| parse_hint_line(line).is_some())
    else {
        return rewritten.to_string();
    };
    format!("{hint_line}\n\n{}", rewritten.trim_start())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thirty_minute_session_reads_practises_and_checks() {
        assert_eq!(
            SessionPlan::for_depth(30),
            SessionPlan {
                learn_minutes: 14,
                practice_minutes: 10,
                check_minutes: 6
            }
        );
        let hour = SessionPlan::for_depth(60);
        assert_eq!(
            hour.learn_minutes + hour.practice_minutes + hour.check_minutes,
            60
        );
        assert_eq!(hour.learn_minutes, 28);
    }

    #[test]
    fn hint_lines_are_read_in_their_common_spellings() {
        for line in [
            "*~3 min · Read closely: the exercise builds on this.*",
            "_≈3 minutes — Read closely: the exercise builds on this._",
            "*3 mins: Read closely: the exercise builds on this.*",
        ] {
            let hint = parse_hint_line(line).unwrap_or_else(|| panic!("{line}"));
            assert_eq!(hint.minutes, 3);
            assert!(hint.text.starts_with("Read closely"), "{}", hint.text);
        }
        assert_eq!(parse_hint_line("**Key idea:** bold is not a hint"), None);
        assert_eq!(parse_hint_line("*just italics*"), None);
        assert_eq!(
            parse_hint_line("*~3 min*"),
            None,
            "a time with no orientation"
        );
        assert_eq!(parse_hint_line("Plain prose"), None);
    }

    #[test]
    fn a_section_opens_with_its_hint_or_has_none() {
        let with = "\n\n*~2 min · Skim if you know queues.*\n\n- point one\n";
        assert_eq!(section_hint(with).map(|h| h.minutes), Some(2));
        let without = "- point one\n\n*~2 min · too late to count*\n";
        assert_eq!(section_hint(without), None);
        let sections = vec![with.to_string(), without.to_string(), with.to_string()];
        assert_eq!(reading_minutes(&sections), 4);
        assert_eq!(
            sections_without_hints(&["A", "B", "C"], &sections),
            vec!["B"]
        );
    }

    #[test]
    fn diagrams_are_counted_by_their_fences() {
        let markdown =
            "```mermaid\nflowchart LR\n```\n\n```js\nx\n```\n\n~~~Mermaid\nsequenceDiagram\n~~~\n";
        assert_eq!(diagram_count(markdown), 2);
    }

    #[test]
    fn a_rewrite_that_drops_the_hint_gets_it_back() {
        let original = "*~2 min · Watch the order of the steps.*\n\nold body";
        let rewritten = "new body with more substance";
        assert_eq!(
            keep_hint(original, rewritten),
            "*~2 min · Watch the order of the steps.*\n\nnew body with more substance"
        );
        let kept = "*~3 min · Its own hint.*\n\nnew body";
        assert_eq!(keep_hint(original, kept), kept);
        assert_eq!(keep_hint("no hint here", rewritten), rewritten);
    }
}
