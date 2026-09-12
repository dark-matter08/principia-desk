//! The writing rules every generated text is held to, and the check that
//! reads for the tells of machine writing.
//!
//! The catalogue follows the signs Wikipedia's editors document for text
//! written by a language model (the "Signs of AI writing" page): the stock
//! vocabulary, the constructions that carry no information, the openers
//! and closers of a chat answer, the vague attributions, the rule of three,
//! the em dash as the default clause break, emoji and decorative bold. Each
//! is either a *hard* tell, which fails a text on one occurrence, or a
//! *soft* one, which fails it once a few have piled up.
//!
//! The catalogue lives in `seed/prose-tells.json`, shared with the
//! interface's mirror of this check. Three things use it:
//!
//! - [`CONTRACT`] is appended to the system prompt of every runner call, so
//!   the rules travel with every request the desk makes.
//! - [`scrub`] rewrites what can be fixed mechanically (the dashes) in every
//!   answer that comes back over the wire, outside code.
//! - [`objection`] reads a text for what remains and names it, so the gates
//!   that check lessons, drafts, chat replies and questions can send it back
//!   for a correction with the reason.

/// The rules, as every runner reads them.
pub const CONTRACT: &str = "WRITING RULES (a deterministic check reads every answer for these; an answer that breaks them is sent back):\n\
- No em dashes and no spaced en dashes. Break a clause with a comma, a colon, a full stop or parentheses.\n\
- No chat openers or closers: no \"Certainly\", \"Great question\", \"Sure\", \"I hope this helps\", \"Let me know\"; no \"In conclusion\" or \"In summary\" paragraph; no restating the question; no remark about being a model or about a training cutoff.\n\
- No stock vocabulary: delve, tapestry, testament, leverage, utilize, foster, robust, comprehensive, crucial, pivotal, vital, seamless, streamline, holistic, multifaceted, nuanced, intricate, meticulous, vibrant, groundbreaking, cutting-edge, state-of-the-art, transformative, revolutionize, empower, unleash, myriad, plethora, showcase, underscore, ever-evolving, game-changer, deep dive, dive into, \"the landscape of\", \"in the realm of\", \"navigating the complexities\".\n\
- No stock constructions: \"not only X but also Y\", \"it's not just X, it's Y\", \"it is important to note\", \"it is worth noting\", \"plays a crucial role\", \"cannot be overstated\", \"stands as a\", \"serves as a\", \"in today's world\", \"at its core\", \"when it comes to\", \"in the realm of\", \"a wide range of\", \"in order to\" (say \"to\").\n\
- No sentence opening with Moreover, Furthermore, Additionally, Overall, Ultimately, Notably, Importantly, Essentially.\n\
- No triplets for rhythm (\"fast, simple, and reliable\"); list what is true in whatever number it comes.\n\
- No vague attribution (\"experts say\", \"studies show\", \"it is widely known\"); name the source or drop the claim.\n\
- No emoji. No bold on ordinary words inside a sentence. No heading in Title Case unless the format asks for one.\n\
- Plain, specific, concrete: the mechanism, the number, the command, the file. Short sentences. Say a thing once.";

/// One thing the check reads for.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Tell {
    /// What is matched, lowercase, on word boundaries. A leading `^` means
    /// the phrase has to open a sentence.
    pub pattern: String,
    /// Fails the text on one occurrence.
    pub hard: bool,
}

#[derive(serde::Deserialize)]
struct Catalogue {
    soft_limit: usize,
    tells: Vec<Tell>,
}

/// The catalogue, shared with the interface's mirror of this check through
/// `seed/prose-tells.json`. Order is the order of a report.
fn catalogue() -> &'static Catalogue {
    static CATALOGUE: std::sync::LazyLock<Catalogue> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!("../seed/prose-tells.json"))
            .expect("seed/prose-tells.json is well formed")
    });
    &CATALOGUE
}

pub fn tells() -> &'static [Tell] {
    &catalogue().tells
}

/// Soft tells a text may carry before it is sent back.
pub fn soft_limit() -> usize {
    catalogue().soft_limit
}

/// One tell found in a text, with how often.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The pattern as it reads in a report, without the sentence-start mark.
    pub name: String,
    pub count: usize,
    pub hard: bool,
}

/// Split a text into prose and code, so the check and the scrub leave
/// fenced blocks and inline code alone. Each part says whether it is code.
fn parts(text: &str) -> Vec<(bool, &str)> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let fence = rest.find("```");
        let tick = rest.find('`').filter(|at| Some(*at) != fence);
        match (fence, tick) {
            (Some(f), Some(t)) if t < f => inline(&mut out, &mut rest, t),
            (Some(f), _) => {
                out.push((false, &rest[..f]));
                let after = &rest[f + 3..];
                match after.find("```") {
                    Some(e) => {
                        let end = f + 3 + e + 3;
                        out.push((true, &rest[f..end]));
                        rest = &rest[end..];
                    }
                    // A fence that never closes is a mention of one, not code.
                    None => {
                        out.push((false, &rest[f..f + 3]));
                        rest = &rest[f + 3..];
                    }
                }
            }
            (None, Some(t)) => inline(&mut out, &mut rest, t),
            (None, None) => {
                out.push((false, rest));
                rest = "";
            }
        }
    }
    out
}

/// An inline code span closes on the same line; a stray tick is prose.
fn inline<'a>(out: &mut Vec<(bool, &'a str)>, rest: &mut &'a str, at: usize) {
    out.push((false, &rest[..at]));
    let after = &rest[at + 1..];
    let line = &after[..after.find('\n').unwrap_or(after.len())];
    match line.find('`') {
        Some(e) => {
            let end = at + 1 + e + 1;
            out.push((true, &rest[at..end]));
            *rest = &rest[end..];
        }
        None => {
            out.push((false, &rest[at..at + 1]));
            *rest = &rest[at + 1..];
        }
    }
}

/// Replace the dashes a model reaches for with punctuation a person uses,
/// outside code: an em dash, a spaced en dash, and the JSON escape of an em
/// dash (so a payload can be scrubbed before it is parsed). A dash that
/// opened a parenthetical closes it as a comma; one at the end of a line
/// disappears; one after a colon or a comma leaves that mark in place.
pub fn scrub(text: &str) -> String {
    if !text.contains('\u{2014}') && !text.contains(" \u{2013} ") && !text.contains("\\u2014") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for (code, part) in parts(text) {
        if code {
            out.push_str(part);
        } else {
            out.push_str(&scrub_prose(part));
        }
    }
    out
}

fn scrub_prose(text: &str) -> String {
    let dashed = text
        .replace("\\u2014", "\u{2014}")
        .replace(" \u{2013} ", "\u{2014}")
        .replace(" \u{2014} ", "\u{2014}")
        .replace(" \u{2014}", "\u{2014}")
        .replace("\u{2014} ", "\u{2014}");
    let mut out = String::with_capacity(dashed.len());
    let chars: Vec<char> = dashed.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '\u{2014}' {
            out.push(c);
            i += 1;
            continue;
        }
        let before = out.chars().rev().find(|c| !c.is_whitespace());
        let next = chars.get(i + 1).copied();
        let at_line_start = out
            .chars()
            .rev()
            .take_while(|c| *c != '\n')
            .all(|c| c.is_whitespace());
        i += 1;
        match (before, next) {
            // A dash at the end of a line or of the text: the clause just ends.
            (_, None | Some('\n')) => {}
            // A dash that opens a line stood for a bullet; the words stand on their own.
            _ if at_line_start => {}
            // After a mark that already breaks the clause, keep the mark.
            (Some(':' | ',' | ';' | '(' | '.' | '!' | '?'), _) => out.push(' '),
            // Before a closing mark, the dash is simply gone.
            (_, Some(')' | ',' | '.' | ';' | ':' | '!' | '?')) => {}
            _ => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push_str(", ");
            }
        }
    }
    out
}

/// How many em dashes a text still carries outside code.
pub fn em_dashes(text: &str) -> usize {
    parts(text)
        .into_iter()
        .filter(|(code, _)| !code)
        .map(|(_, part)| part.matches('\u{2014}').count())
        .sum()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '\''
}

/// Whether a match at `at` opens a sentence: nothing but whitespace, a
/// list marker or a quote since the last terminal mark or line start.
fn opens_sentence(lower: &[char], at: usize) -> bool {
    let mut i = at;
    while i > 0 {
        i -= 1;
        match lower[i] {
            c if c.is_whitespace() => continue,
            '-' | '*' | '>' | '#' | '"' | '(' | '\u{201c}' | '\u{2018}' | '\'' => continue,
            '.' | '!' | '?' | ':' | ';' | '\n' => return true,
            _ => return false,
        }
    }
    true
}

/// Count a pattern on word boundaries in lowercase text.
fn count(lower: &[char], pattern: &str) -> usize {
    let (sentence_start, needle) = match pattern.strip_prefix('^') {
        Some(rest) => (true, rest),
        None => (false, pattern),
    };
    let needle: Vec<char> = needle.chars().collect();
    if needle.is_empty() || lower.len() < needle.len() {
        return 0;
    }
    let mut found = 0;
    let mut i = 0;
    while i + needle.len() <= lower.len() {
        if lower[i..i + needle.len()] == needle[..] {
            let before_ok = i == 0 || !is_word(lower[i - 1]);
            let after = lower.get(i + needle.len()).copied();
            let after_ok = after.is_none_or(|c| !is_word(c));
            if before_ok && after_ok && (!sentence_start || opens_sentence(lower, i)) {
                found += 1;
                i += needle.len();
                continue;
            }
        }
        i += 1;
    }
    found
}

fn has_emoji(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(u32::from(c),
            0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B50 | 0x2B55 | 0x231A..=0x231B | 0x23E9..=0x23FA
        )
    })
}

/// Every tell a text carries outside code, hard ones first.
pub fn hits(text: &str) -> Vec<Hit> {
    let prose: String = parts(text)
        .into_iter()
        .filter(|(code, _)| !code)
        .map(|(_, part)| part)
        .collect::<Vec<_>>()
        .join("\n");
    let lower: Vec<char> = prose.to_lowercase().chars().collect();
    let mut found: Vec<Hit> = tells()
        .iter()
        .filter_map(|tell| {
            let count = count(&lower, &tell.pattern);
            (count > 0).then(|| Hit {
                name: tell.pattern.trim_start_matches('^').to_string(),
                count,
                hard: tell.hard,
            })
        })
        .collect();
    if has_emoji(&prose) {
        found.push(Hit {
            name: "emoji".into(),
            count: 1,
            hard: true,
        });
    }
    found.sort_by_key(|hit| (!hit.hard, std::cmp::Reverse(hit.count)));
    found
}

/// What a text carries, as a report reads it: the hard tells, then the
/// soft ones when there are more than pass. Nothing when it reads as
/// written by a person.
pub fn report(text: &str) -> Option<String> {
    let found = hits(text);
    let hard: Vec<&Hit> = found.iter().filter(|hit| hit.hard).collect();
    let soft: Vec<&Hit> = found.iter().filter(|hit| !hit.hard).collect();
    let soft_total: usize = soft.iter().map(|hit| hit.count).sum();
    let limit = soft_limit();
    if hard.is_empty() && soft_total <= limit {
        return None;
    }
    let list = |hits: &[&Hit]| {
        hits.iter()
            .map(|hit| {
                if hit.count > 1 {
                    format!("\"{}\" ({}x)", hit.name, hit.count)
                } else {
                    format!("\"{}\"", hit.name)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut reason = String::new();
    if !hard.is_empty() {
        reason.push_str(&list(&hard));
    }
    if soft_total > limit {
        if !hard.is_empty() {
            reason.push_str("; and ");
        }
        reason.push_str(&format!(
            "{soft_total} stock words where at most {limit} pass: {}",
            list(&soft)
        ));
    }
    Some(reason)
}

/// The prefix every objection carries, so a gate can tell this failure
/// from the others and the editor can shorten it.
pub const OBJECTION: &str = "reads like a machine wrote it: ";

/// The reason a text is sent back for a correction, or nothing: any hard
/// tell, or more soft ones than the limit allows.
pub fn objection(text: &str) -> Option<String> {
    report(text).map(|found| {
        format!("{OBJECTION}{found}. Replace each with plain words; do not add anything else.")
    })
}

/// Whether a gate failure came from this check.
pub fn is_objection(reason: &str) -> bool {
    reason.contains(OBJECTION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashes_become_commas_outside_code() {
        assert_eq!(
            scrub("A cache \u{2014} the fast one \u{2014} sits in front."),
            "A cache, the fast one, sits in front."
        );
        assert_eq!(scrub("word\u{2014}word"), "word, word");
        assert_eq!(scrub("first \u{2013} second"), "first, second");
        assert_eq!(scrub("Two things: \u{2014} a and b"), "Two things: a and b");
        assert_eq!(scrub("trailing \u{2014}\nnext"), "trailing\nnext");
        assert_eq!(scrub("\u{2014} a bullet\n"), "a bullet\n");
        assert_eq!(scrub("\"json \\u2014 escaped\""), "\"json, escaped\"");
        assert_eq!(scrub("(see \u{2014})"), "(see)");
        let code =
            "text \u{2014} here\n```\nlet x = \"a \u{2014} b\";\n```\nand `a \u{2014} b` inline";
        assert_eq!(
            scrub(code),
            "text, here\n```\nlet x = \"a \u{2014} b\";\n```\nand `a \u{2014} b` inline"
        );
        assert_eq!(em_dashes(code), 1);
        assert_eq!(scrub("1\u{2013}4 letters"), "1\u{2013}4 letters");
    }

    #[test]
    fn hard_tells_fail_on_one_and_soft_ones_on_a_pile() {
        assert!(objection("The buffer is flushed when it fills.").is_none());
        let reason = objection("Let's delve into how the buffer works.").unwrap();
        assert!(reason.contains("\"delve\""), "{reason}");
        assert!(is_objection(&reason));
        let reason = objection("This is not only fast but also robust.").unwrap();
        assert!(reason.contains("\"not only\""));
        assert!(objection("A robust, comprehensive and crucial design.").is_none());
        let reason = objection("A robust, comprehensive, crucial and pivotal design.").unwrap();
        assert!(reason.contains("4 stock words"), "{reason}");
        assert!(objection("The unleashed pipeline is a landscape of choices.").is_none());
        assert!(
            objection("The test harness unlocks the mutex and navigates to the page.").is_none()
        );
    }

    #[test]
    fn tells_are_read_on_word_boundaries_and_sentence_starts() {
        assert!(objection("The crucially named table and the pivotally placed row.").is_none());
        assert!(hits("Overall latency fell.")
            .iter()
            .any(|h| h.name == "overall"));
        assert!(!hits("The overall latency fell.")
            .iter()
            .any(|h| h.name == "overall"));
        assert!(hits("Done. In conclusion, it works.")
            .iter()
            .any(|h| h.name == "in conclusion" && h.hard));
        assert!(hits("- Moreover, the cache")
            .iter()
            .any(|h| h.name == "moreover"));
        assert!(hits("`delve`\n```\ndelve\n```\n").is_empty());
        assert!(hits("a stray ` tick then delve")
            .iter()
            .any(|h| h.name == "delve"));
        assert!(hits("a fenced ``` mention, then delve")
            .iter()
            .any(|h| h.name == "delve"));
        assert!(hits("Great work \u{1F680}")
            .iter()
            .any(|h| h.name == "emoji"));
    }

    #[test]
    fn the_contract_names_what_the_check_reads_for() {
        assert!(CONTRACT.contains("No em dashes"));
        assert!(CONTRACT.contains("delve"));
        assert!(CONTRACT.contains("not only X but also Y"));
        assert!(!CONTRACT.contains('\u{2014}'));
        assert!(
            objection(CONTRACT).is_some(),
            "the contract lists the tells it forbids"
        );
    }
}

#[cfg(test)]
mod bundled_tests {
    /// The bundled lessons are held to the same rules as generated ones.
    #[test]
    fn the_bundled_lessons_pass_the_writing_check() {
        let mut failures = Vec::new();
        for entry in std::fs::read_dir(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/seed/fallback_courses"
        ))
        .unwrap()
        {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            let course: serde_json::Value = serde_json::from_str(&text).unwrap();
            let markdown = course["markdown"].as_str().unwrap_or_default();
            if let Some(reason) = super::objection(markdown) {
                failures.push(format!("{}: {reason}", path.display()));
            }
            assert_eq!(super::em_dashes(markdown), 0, "{}", path.display());
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
