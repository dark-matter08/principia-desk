//! Live source retrieval for course generation.
//!
//! Providers either have no web access at all (DeepSeek's chat API) or have
//! search tools whose output we cannot audit, so asking a model for a reading
//! list produced two failure modes: courses with no sources, or plausible URLs
//! that 404. This module removes the model from that job. Rust fetches an
//! allowlist of primary documentation, extracts the readable prose, hands it to
//! the teacher as quoted material, and verifies every URL that ends up in front
//! of the learner.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Upper bound on bytes we will read from one document before extraction.
const MAX_DOCUMENT_BYTES: usize = 600_000;
/// Words of readable prose kept per source when building the prompt block.
const MAX_EXCERPT_WORDS: usize = 700;
/// How long a fetched document stays reusable across generation passes.
const CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// Where to read a page when its host is unreachable. The GNU manuals are
/// the whole reading list of the shell classes and gnu.org goes dark from
/// some networks, so its Bash and coreutils pages fall back to the same
/// material on man7.org. The hint names the section to quote, since a man
/// page holds the whole program where the manual page held one topic.
pub fn fallback_source(url: &str) -> Option<(String, String)> {
    let path = url
        .strip_prefix("https://www.gnu.org/software/")
        .or_else(|| url.strip_prefix("https://gnu.org/software/"))?;
    let (tool, rest) = path.split_once('/')?;
    let page = rest
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .trim_end_matches(".html");
    let words = |page: &str| {
        page.replace("_003f", "")
            .replace(['-', '_'], " ")
            .trim()
            .to_string()
    };
    let man = |name: &str| format!("https://man7.org/linux/man-pages/man1/{name}.1.html");
    match tool {
        "bash" => Some((man("bash"), words(page))),
        "coreutils" => {
            let command = page.strip_suffix("-invocation")?;
            Some((man(command), command.to_string()))
        }
        "grep" | "sed" | "gawk" | "make" | "tar" | "wget" | "bc" => Some((man(tool), words(page))),
        "findutils" => Some((man("find"), words(page))),
        "diffutils" => Some((man("diff"), words(page))),
        _ => None,
    }
}

/// The part of a long document that is about `hint`: a window starting a
/// little before the first line that mentions it, or the opening when nothing
/// does. A man page holds the whole program; the lesson wants one section.
pub fn focused_excerpt(prose: &str, hint: &str, limit: usize) -> String {
    // Match on stems, so "Redirections" finds the REDIRECTION section.
    let terms: Vec<String> = hint
        .split_whitespace()
        .map(|term| {
            term.trim_matches(|c: char| !c.is_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|term| term.len() >= 4)
        .map(|term| {
            let keep = term.len().saturating_sub(2).max(4);
            term.chars().take(keep).collect()
        })
        .collect();
    if terms.is_empty() {
        return first_words(prose, limit);
    }
    let lines: Vec<&str> = prose.lines().collect();
    let Some(index) = lines.iter().position(|line| {
        let lower = line.to_ascii_lowercase();
        terms.iter().any(|term| lower.contains(term.as_str()))
    }) else {
        return first_words(prose, limit);
    };
    let start = index.saturating_sub(3);
    first_words(&lines[start..].join("\n"), limit)
}

pub fn host_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest
        .split(['/', '?', '#'])
        .next()?
        .split('@')
        .next_back()?
        .split(':')
        .next()?
        .trim()
        .to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Whether a URL points at documentation we are willing to teach from and cite
/// as a primary source.
pub fn is_credible_source(url: &str) -> bool {
    host_of(url).is_some_and(|host| {
        crate::catalog::all()
            .into_iter()
            .flat_map(|course| course.source_hosts)
            .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
    })
}

/// Apply the selected course's policy before retrieving teaching material.
/// The general citation validator still recognizes the catalog-wide union.
pub fn is_source_for(focus: &str, url: &str) -> bool {
    crate::catalog::course(focus).is_some_and(|course| {
        host_of(url).is_some_and(|host| {
            course
                .source_hosts
                .iter()
                .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
        })
    })
}

/// Discovery follows the subject's source policy. Systems and shell lessons
/// use their authored primary references instead of unrelated browser results.
pub fn uses_mdn_discovery(focus: &str) -> bool {
    crate::catalog::course(focus)
        .is_some_and(|course| course.source_hosts.contains(&"developer.mozilla.org"))
}

/// GitHub's blob viewer is mostly application shell; the raw file is the actual
/// document. Rewrite so extraction sees prose instead of editor chrome.
pub fn normalize_fetch_url(url: &str) -> String {
    let trimmed = url.trim();
    if let Some(path) = trimmed.strip_prefix("https://github.com/") {
        if let Some((repo, blob_path)) = path.split_once("/blob/") {
            return format!("https://raw.githubusercontent.com/{repo}/{blob_path}");
        }
    }
    trimmed.to_string()
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "+".to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        // Entity names are short; scan a char-safe window so multi-byte text
        // after a stray `&` cannot split a character.
        let window = rest
            .char_indices()
            .take(12)
            .last()
            .map(|(index, character)| index + character.len_utf8())
            .unwrap_or(rest.len());
        let Some(end) = rest[..window].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some("&".to_string()),
            "lt" => Some("<".to_string()),
            "gt" => Some(">".to_string()),
            "quot" => Some("\"".to_string()),
            "apos" | "#39" => Some("'".to_string()),
            "nbsp" => Some(" ".to_string()),
            "mdash" => Some("—".to_string()),
            "ndash" => Some("–".to_string()),
            "hellip" => Some("…".to_string()),
            other => other
                .strip_prefix('#')
                .and_then(|digits| digits.parse::<u32>().ok())
                .and_then(char::from_u32)
                .map(|character| character.to_string()),
        };
        match decoded {
            Some(value) => {
                out.push_str(&value);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn strip_element(html: &str, tag: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0;
    while let Some(found) = lower[cursor..].find(&open) {
        let start = cursor + found;
        // Guard against `<script>` matching `<scriptish>`.
        let boundary = lower[start + open.len()..].chars().next();
        if boundary.is_some_and(|c| c.is_ascii_alphanumeric()) {
            out.push_str(&html[cursor..start + open.len()]);
            cursor = start + open.len();
            continue;
        }
        out.push_str(&html[cursor..start]);
        cursor = match lower[start..].find(&close) {
            Some(offset) => start + offset + close.len(),
            None => html.len(),
        };
    }
    out.push_str(&html[cursor.min(html.len())..]);
    out
}

fn inner_text_of(html: &str, tag: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find(&format!("<{tag}"))?;
    let content_start = start + lower[start..].find('>')? + 1;
    let end = content_start + lower[content_start..].find(&format!("</{tag}>"))?;
    let text = decode_entities(&strip_tags(&html[content_start..end]));
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!collapsed.is_empty()).then_some(collapsed)
}

fn strip_tags(html: &str) -> String {
    const BLOCK_TAGS: [&str; 14] = [
        "p", "div", "br", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "pre", "section",
        "article",
    ];
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find('>') else { break };
        let tag = rest[1..end]
            .trim_start_matches('/')
            .split([' ', '\t', '\n', '/'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if BLOCK_TAGS.contains(&tag.as_str()) {
            out.push('\n');
        } else {
            out.push(' ');
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Pull the document title and readable prose out of an HTML page. Navigation,
/// scripts, and boilerplate are dropped so the teacher receives content rather
/// than site chrome.
pub fn extract_readable(html: &str) -> (Option<String>, String) {
    let title = inner_text_of(html, "title").or_else(|| inner_text_of(html, "h1"));

    let mut body = html.to_string();
    for tag in [
        "script", "style", "noscript", "svg", "nav", "header", "footer", "aside", "form",
        "template", "iframe", "button",
    ] {
        body = strip_element(&body, tag);
    }
    while let Some(start) = body.find("<!--") {
        let end = body[start..]
            .find("-->")
            .map(|offset| start + offset + 3)
            .unwrap_or(body.len());
        body.replace_range(start..end, " ");
    }
    // Prefer the main content region when the page marks one.
    for tag in ["main", "article"] {
        if let Some(inner) = inner_text_of(&body, tag) {
            if inner.split_whitespace().count() > 200 {
                return (title, normalize_prose(&inner));
            }
        }
    }
    (title, normalize_prose(&decode_entities(&strip_tags(&body))))
}

fn normalize_prose(text: &str) -> String {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        // Single words and two-word fragments are almost always leftover
        // navigation; real prose and code lines are longer.
        if line.split_whitespace().count() >= 3 {
            lines.push(line);
        }
    }
    lines.join("\n")
}

fn first_words(text: &str, limit: usize) -> String {
    let mut words = 0;
    let mut out = String::new();
    for line in text.lines() {
        let line_words = line.split_whitespace().count();
        if words + line_words > limit {
            break;
        }
        words += line_words;
        out.push_str(line);
        out.push('\n');
    }
    if out.is_empty() {
        out = text
            .split_whitespace()
            .take(limit)
            .collect::<Vec<_>>()
            .join(" ");
    }
    out.trim().to_string()
}

/// One retrieved document, ready to quote in a prompt and cite in a course.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchSource {
    pub url: String,
    pub title: String,
    pub host: String,
    pub excerpt: String,
    pub words: usize,
    pub primary: bool,
}

#[derive(Clone)]
struct CachedDocument {
    title: Option<String>,
    prose: String,
    stored_at: Instant,
}

/// What one retrieval produced: the sources to teach from, every candidate
/// that was passed over with its reason, and the seeds a mirror stood in
/// for. A lesson with no reading list can then say why, not only that.
#[derive(Debug, Default, Clone)]
pub struct Gathered {
    pub sources: Vec<ResearchSource>,
    pub skipped: Vec<(String, String)>,
    pub substituted: Vec<(String, String)>,
    /// Pages the configured web search added beyond the curated seeds.
    pub discovered: Vec<String>,
    /// Why the web search produced nothing, when it was configured and failed.
    pub search_error: Option<String>,
}

impl Gathered {
    /// The hosts that could not be reached, for a note the learner can read.
    pub fn unreachable_hosts(&self) -> Vec<String> {
        let mut hosts: Vec<String> = self
            .skipped
            .iter()
            .filter_map(|(url, _)| host_of(url))
            .collect();
        hosts.sort();
        hosts.dedup();
        hosts
    }
}

/// Fetches and caches primary documentation for course generation.
#[derive(Clone)]
pub struct Researcher {
    client: reqwest::Client,
    cache: Arc<Mutex<HashMap<String, CachedDocument>>>,
    /// The web search the desk may use to find pages beyond the curated
    /// seeds; shared with the settings commands, which replace it on change.
    search: Arc<Mutex<crate::search::SearchConfig>>,
}

impl Default for Researcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Researcher {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(
                "principia-desk/0.1 (personal learning app; +https://developer.mozilla.org)",
            )
            .build()
            .unwrap_or_default();
        Self {
            client,
            cache: Arc::new(Mutex::new(HashMap::new())),
            search: Arc::new(Mutex::new(crate::search::SearchConfig::default())),
        }
    }

    /// Replace the search configuration for every lesson from now on.
    pub fn set_search(&self, config: crate::search::SearchConfig) {
        if let Ok(mut current) = self.search.lock() {
            *current = config;
        }
    }

    pub fn search_config(&self) -> crate::search::SearchConfig {
        self.search
            .lock()
            .map(|config| config.clone())
            .unwrap_or_default()
    }

    /// The HTTP client, for probes that share its timeout and user agent.
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Ask the configured engine for documentation pages about the topic,
    /// restricted to the subject's allowed hosts. Nothing configured, or an
    /// engine error, yields no candidates and a reason for the log.
    async fn discover_web(
        &self,
        focus: &str,
        topic: &str,
        limit: usize,
    ) -> (Vec<String>, Option<String>) {
        let config = self.search_config();
        if !config.enabled() {
            return (Vec::new(), None);
        }
        let Some(course) = crate::catalog::course(focus) else {
            return (Vec::new(), None);
        };
        let query = format!("{topic} {}", course.label);
        match crate::search::search(
            &self.client,
            &config,
            &query,
            limit + 4,
            course.source_hosts,
        )
        .await
        {
            Ok(results) => (
                results
                    .into_iter()
                    .map(|result| result.url)
                    .filter(|url| is_source_for(focus, url))
                    .collect(),
                None,
            ),
            Err(reason) => (Vec::new(), Some(reason)),
        }
    }

    fn cached(&self, url: &str) -> Option<(Option<String>, String)> {
        let cache = self.cache.lock().ok()?;
        let entry = cache.get(url)?;
        (entry.stored_at.elapsed() < CACHE_TTL).then(|| (entry.title.clone(), entry.prose.clone()))
    }

    fn store(&self, url: &str, title: Option<&str>, prose: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(
                url.to_string(),
                CachedDocument {
                    title: title.map(str::to_string),
                    prose: prose.to_string(),
                    stored_at: Instant::now(),
                },
            );
        }
    }

    /// Seed the cache so grounding logic can be exercised without network I/O.
    #[cfg(test)]
    pub fn prime(&self, source: ResearchSource) {
        self.store(&source.url, Some(&source.title), &source.excerpt);
    }

    /// Fetch one document and reduce it to citable teaching material.
    pub async fn fetch_source(&self, url: &str) -> std::result::Result<ResearchSource, String> {
        self.fetch_source_about(url, None).await
    }

    /// The same, quoting the part of the document about `hint` when one is
    /// given: what a mirror page needs, since it holds more than the topic.
    pub async fn fetch_source_about(
        &self,
        url: &str,
        hint: Option<&str>,
    ) -> std::result::Result<ResearchSource, String> {
        let (title, prose) = match self.cached(url) {
            Some(hit) => hit,
            None => {
                let (title, prose) = self.fetch_document(url).await?;
                self.store(url, title.as_deref(), &prose);
                (title, prose)
            }
        };
        let words = prose.split_whitespace().count();
        let host = host_of(url).unwrap_or_default();
        let excerpt = match hint {
            Some(hint) => focused_excerpt(&prose, hint, MAX_EXCERPT_WORDS),
            None => first_words(&prose, MAX_EXCERPT_WORDS),
        };
        Ok(ResearchSource {
            url: url.trim().to_string(),
            title: title.unwrap_or_else(|| host.clone()),
            host,
            excerpt,
            words,
            primary: true,
        })
    }

    /// Fetch one document and reduce it to its title and readable prose.
    async fn fetch_document(
        &self,
        url: &str,
    ) -> std::result::Result<(Option<String>, String), String> {
        if !is_credible_source(url) {
            return Err(format!("{url} is not on the credible-source allowlist"));
        }
        let fetch_url = normalize_fetch_url(url);
        let response = self
            .client
            .get(&fetch_url)
            .send()
            .await
            .map_err(|error| format!("{url} could not be fetched: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("{url} returned {}", response.status()));
        }
        // A redirect must not carry us off the allowlist.
        let final_url = response.url().to_string();
        if !is_credible_source(&final_url) {
            return Err(format!("{url} redirected off the allowlist to {final_url}"));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !(content_type.is_empty()
            || content_type.contains("html")
            || content_type.contains("text")
            || content_type.contains("markdown"))
        {
            return Err(format!("{url} is not a readable document ({content_type})"));
        }
        let body = response
            .text()
            .await
            .map_err(|error| format!("{url} body could not be read: {error}"))?;
        let truncated = if body.len() > MAX_DOCUMENT_BYTES {
            match body.char_indices().nth(MAX_DOCUMENT_BYTES) {
                Some((index, _)) => &body[..index],
                None => body.as_str(),
            }
        } else {
            body.as_str()
        };
        let looks_like_html = content_type.contains("html")
            || (content_type.is_empty() && truncated.to_ascii_lowercase().contains("<html"));
        let (title, prose) = if looks_like_html {
            extract_readable(truncated)
        } else {
            // Raw markdown and plain text are already prose; its first heading
            // is a better title than the host name.
            let heading = truncated
                .lines()
                .find_map(|line| line.trim().strip_prefix("# "))
                .map(|heading| heading.trim().to_string());
            (heading, normalize_prose(truncated))
        };
        let words = prose.split_whitespace().count();
        if words < 120 {
            return Err(format!("{url} yielded only {words} readable words"));
        }
        Ok((title, prose))
    }

    /// One MDN search. Best effort: a change to MDN's response shape must never
    /// break course generation.
    async fn search_mdn(&self, query: &str) -> Vec<serde_json::Value> {
        let url = format!(
            "https://developer.mozilla.org/api/v1/search?locale=en-US&q={}",
            percent_encode(query)
        );
        let Ok(response) = self.client.get(&url).send().await else {
            return Vec::new();
        };
        if !response.status().is_success() {
            return Vec::new();
        }
        response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|payload| {
                payload
                    .get("documents")
                    .and_then(|documents| documents.as_array())
                    .cloned()
            })
            .unwrap_or_default()
    }

    /// Ask MDN which documents cover this topic. The full topic is the better
    /// query: its distinctive terms are what make MDN's ranking useful, while a
    /// shortened query degrades into matching one common word.
    async fn discover_mdn(&self, topic: &str, limit: usize) -> Vec<String> {
        let documents = self.search_mdn(topic).await;
        relevant_mdn_documents(&serde_json::json!({ "documents": documents }), topic, limit)
    }

    /// Retrieve the teaching material for one lesson: the concept's curated
    /// primary sources first, then discovered documentation to fill the gap.
    pub async fn gather(
        &self,
        focus: &str,
        topic: &str,
        seeds: &[String],
        limit: usize,
    ) -> Gathered {
        let mut candidates: Vec<String> = seeds
            .iter()
            .map(|seed| seed.trim().to_string())
            .filter(|seed| is_source_for(focus, seed))
            .collect();
        if candidates.len() < limit && uses_mdn_discovery(focus) {
            for discovered in self.discover_mdn(topic, limit).await {
                if !candidates.contains(&discovered) {
                    candidates.push(discovered);
                }
            }
        }
        let mut gathered = Gathered::default();
        if candidates.len() < limit {
            let (found, error) = self.discover_web(focus, topic, limit).await;
            gathered.search_error = error;
            for discovered in found {
                if !candidates.contains(&discovered) {
                    gathered.discovered.push(discovered.clone());
                    candidates.push(discovered);
                }
            }
        }
        // A documentation landing page teaches far less than the page about the
        // actual mechanism, and some spec roots are enormous. Rank by how
        // specific the path is, keeping curated seeds ahead of discovery on ties.
        candidates.sort_by_key(|url| std::cmp::Reverse(path_specificity(url)));
        // Site roots teach nothing and can be enormous — the HTML standard's
        // single-page edition is over 10 MB. Skip them once real pages exist.
        if candidates
            .iter()
            .filter(|url| path_specificity(url) > 0)
            .count()
            >= limit
        {
            candidates.retain(|url| path_specificity(url) > 0);
        }
        // A small surplus absorbs individual fetch failures without pulling in
        // the vague pages we just deprioritized.
        candidates.truncate(limit.saturating_add(2));

        let mut failed: Vec<String> = Vec::new();
        for (url, outcome) in self.fetch_all(&candidates, None).await {
            match outcome {
                Ok(source) if gathered.sources.len() < limit => gathered.sources.push(source),
                Ok(_) => {}
                Err(reason) => {
                    gathered.skipped.push((url.clone(), reason));
                    failed.push(url);
                }
            }
        }
        // Whatever could not be reached may exist elsewhere; one mirror per
        // page, quoting the part of it about the page's own topic.
        if gathered.sources.len() < limit {
            let mut mirrors: Vec<(String, String, String)> = Vec::new();
            for url in &failed {
                let Some((mirror, hint)) = fallback_source(url) else {
                    continue;
                };
                if candidates.contains(&mirror) || mirrors.iter().any(|(_, m, _)| *m == mirror) {
                    continue;
                }
                mirrors.push((url.clone(), mirror, hint));
            }
            let targets: Vec<(String, Option<String>)> = mirrors
                .iter()
                .map(|(_, mirror, hint)| (mirror.clone(), Some(hint.clone())))
                .collect();
            for ((seed, mirror, _), (_, outcome)) in
                mirrors.iter().zip(self.fetch_hinted(&targets).await)
            {
                match outcome {
                    Ok(source) if gathered.sources.len() < limit => {
                        gathered.substituted.push((seed.clone(), mirror.clone()));
                        gathered.sources.push(source);
                    }
                    Ok(_) => {}
                    Err(reason) => gathered.skipped.push((mirror.clone(), reason)),
                }
            }
        }
        gathered
    }

    async fn fetch_all(
        &self,
        urls: &[String],
        hint: Option<&str>,
    ) -> Vec<(String, std::result::Result<ResearchSource, String>)> {
        let targets: Vec<(String, Option<String>)> = urls
            .iter()
            .map(|url| (url.clone(), hint.map(str::to_string)))
            .collect();
        self.fetch_hinted(&targets).await
    }

    /// Fetch several documents at once, each with its own excerpt hint. The
    /// order of the input is the order of the output.
    async fn fetch_hinted(
        &self,
        targets: &[(String, Option<String>)],
    ) -> Vec<(String, std::result::Result<ResearchSource, String>)> {
        let mut handles = Vec::new();
        for (url, hint) in targets {
            let researcher = self.clone();
            let url = url.clone();
            let hint = hint.clone();
            handles.push(tokio::spawn(async move {
                let outcome = researcher.fetch_source_about(&url, hint.as_deref()).await;
                (url, outcome)
            }));
        }
        let mut outcomes = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(outcome) => outcomes.push(outcome),
                Err(error) => log::warn!("research task failed: {error}"),
            }
        }
        outcomes
    }

    /// Confirm a URL the model produced actually resolves, so the reading list
    /// never contains an invented link.
    pub async fn url_resolves(&self, url: &str) -> bool {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return false;
        }
        if self.cached(url).is_some() {
            return true;
        }
        let target = normalize_fetch_url(url);
        match self.client.head(&target).send().await {
            Ok(response) if response.status().is_success() => true,
            // Some documentation hosts reject HEAD; fall back to a GET when
            // the host answered. A transport timeout already spent the request
            // budget; repeating it doubles the delay during an outage.
            Ok(_) => self
                .client
                .get(&target)
                .send()
                .await
                .is_ok_and(|response| response.status().is_success()),
            Err(_) => false,
        }
    }
}

/// Words too generic to signal that a document is about the topic.
const TOPIC_STOPWORDS: [&str; 12] = [
    "that", "this", "with", "from", "when", "what", "which", "their", "them", "your", "into",
    "than",
];

/// Content words, crudely singularized so `microtask` matches `microtasks`.
fn significant_words(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .map(|word| word.to_ascii_lowercase())
        .filter(|word| word.len() >= 4 && !TOPIC_STOPWORDS.contains(&word.as_str()))
        .map(|word| word.trim_end_matches('s').to_string())
        .collect()
}

/// The search and relevance string for one lesson. Category words matter: they
/// are often the terms that separate the right document from a name collision.
pub fn course_topic(title: &str, category: &str) -> String {
    format!("{} {}", title.trim(), category.trim())
        .replace([':', '—', '/'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Keyword search alone returns documents that merely share a word — an HTTP
/// status named "Loop Detected" for a topic about the event loop. Require real
/// overlap with the topic and rank by how much of it a document covers.
fn relevant_mdn_documents(payload: &serde_json::Value, topic: &str, limit: usize) -> Vec<String> {
    const MIN_TOPIC_OVERLAP: usize = 2;
    let topic_words = significant_words(topic);
    let Some(documents) = payload.get("documents").and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    let mut scored: Vec<(usize, String)> = documents
        .iter()
        .filter_map(|document| {
            let path = document.get("mdn_url")?.as_str()?;
            if !path.starts_with("/en-US/docs/") {
                return None;
            }
            let title = document.get("title").and_then(|value| value.as_str())?;
            let summary = document
                .get("summary")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let described = significant_words(&format!("{title} {summary}"));
            let overlap = topic_words
                .iter()
                .filter(|word| described.contains(word))
                .collect::<std::collections::BTreeSet<_>>()
                .len();
            // Word overlap alone can't tell `runtime.onMessage` in the add-on
            // API from the JavaScript runtime, so weigh which part of MDN the
            // document lives in: web-platform reference is what a frontend
            // course cites, while tutorials and Firefox add-on docs are not.
            let score = overlap
                + usize::from(
                    path.starts_with("/en-US/docs/Web/")
                        || path.starts_with("/en-US/docs/WebAssembly/"),
                );
            let score = score.saturating_sub(
                usize::from(
                    path.starts_with("/en-US/docs/Learn")
                        || path.starts_with("/en-US/docs/Mozilla/"),
                ) * 2,
            );
            // Topic overlap is the relevance gate; the subtree weighting only
            // ranks and rejects — it can never promote a document that failed.
            (overlap >= MIN_TOPIC_OVERLAP && score >= MIN_TOPIC_OVERLAP)
                .then(|| (score, format!("https://developer.mozilla.org{path}")))
        })
        .collect();
    // Overlapping queries return the same document; keep its best score once.
    scored.sort_by_key(|item| std::cmp::Reverse(item.0));
    let mut seen = std::collections::BTreeSet::new();
    scored
        .into_iter()
        .filter(|(_, url)| seen.insert(url.clone()))
        .take(limit)
        .map(|(_, url)| url)
        .collect()
}

/// How many path segments a URL has, as a proxy for how narrowly it covers one
/// mechanism rather than a whole section of a documentation site.
fn path_specificity(url: &str) -> usize {
    url.trim_end_matches('/')
        .split('/')
        .skip(3)
        .filter(|segment| !segment.is_empty())
        .count()
}

/// The prompt block that turns retrieved documents into quotable material.
pub fn format_source_material(sources: &[ResearchSource]) -> String {
    if sources.is_empty() {
        return String::new();
    }
    let documents = sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            format!(
                "[S{}] {}\nURL: {}\nPUBLISHER: {}\nEXCERPT:\n{}",
                index + 1,
                source.title,
                source.url,
                source.host,
                source.excerpt
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    let required = sources.len().min(3);
    format!(
        "RETRIEVED SOURCE MATERIAL (fetched live from primary documentation by this app, not by \
         you):\n\n{documents}\n\nHow to use this material:\n\
         - Teach from these documents. Ground every version-sensitive or standards claim in them.\n\
         - Cite them inline as markdown links using the exact URLs above, e.g. \
         `[MDN: Navigation API](https://developer.mozilla.org/...)`. At least {required} different \
         retrieved URLs must appear as inline links in the course body.\n\
         - Quote or paraphrase precisely, and attribute the publisher when the claim is \
         contested, recent, or partially supported across browsers.\n\
         - Every entry in `resources` must be a URL that appears above, or another URL you are \
         certain exists. Invented URLs are removed by a verification step, so guessing only \
         shrinks the learner's reading list.\n\
         - If the excerpts do not cover something you want to assert, say plainly that it is \
         outside the retrieved sources rather than inventing support.\n"
    )
}

/// How many distinct retrieved sources the course actually cites inline. A
/// trailing slash is not a citation error, so it is ignored on both sides.
pub fn cited_source_count(markdown: &str, sources: &[ResearchSource]) -> usize {
    sources
        .iter()
        .filter(|source| markdown.contains(source.url.trim_end_matches('/')))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn course_source_policies_do_not_route_shell_or_systems_lessons_to_mdn() {
        for focus in ["system-design", "linux-bash", "bash-scripting", "unknown"] {
            assert!(!uses_mdn_discovery(focus));
            assert!(!is_source_for(
                focus,
                "https://developer.mozilla.org/en-US/docs/Web/API"
            ));
        }
        assert!(uses_mdn_discovery("javascript"));
        assert!(is_source_for(
            "linux-bash",
            "https://www.gnu.org/software/bash/manual/html_node/Quoting.html"
        ));
        assert!(is_source_for(
            "system-design",
            "https://www.postgresql.org/docs/current/transaction-iso.html"
        ));
        assert!(!is_source_for(
            "linux-bash",
            "https://gnu.org.evil.example/manual"
        ));
        assert!(!is_source_for(
            "javascript",
            "https://www.gnu.org/software/bash/manual/html_node/Quoting.html"
        ));
    }

    #[test]
    fn allowlist_matches_hosts_and_subdomains_but_not_lookalikes() {
        assert!(is_credible_source(
            "https://developer.mozilla.org/en-US/docs/Web/API/Navigation_API"
        ));
        assert!(is_credible_source("https://hacks.mozilla.org/2026/01/post"));
        assert!(is_credible_source(
            "https://html.spec.whatwg.org/multipage/"
        ));
        assert!(!is_credible_source(
            "https://developer.mozilla.org.attacker.example/docs"
        ));
        assert!(!is_credible_source("https://medium.com/@someone/top-10-js"));
        assert!(!is_credible_source("not-a-url"));
    }

    #[test]
    fn github_blob_urls_are_fetched_as_raw_documents() {
        assert_eq!(
            normalize_fetch_url("https://github.com/whatwg/html/blob/main/README.md"),
            "https://raw.githubusercontent.com/whatwg/html/main/README.md"
        );
        assert_eq!(
            normalize_fetch_url("https://web.dev/articles/vitals"),
            "https://web.dev/articles/vitals"
        );
    }

    #[test]
    fn extraction_keeps_prose_and_drops_chrome_and_scripts() {
        let html = r#"<html><head><title>Navigation API - MDN</title>
            <style>body { color: red }</style></head>
            <body><nav>Skip to main content References</nav>
            <script>window.analytics = 1;</script>
            <main><p>The Navigation API provides the ability to intercept a navigation
            and render a new document without a full page load.</p>
            <p>It replaces ad hoc history interception with a single event that reports
            the destination, the navigation type, and whether it can be intercepted.</p>
            <p>Browser support is uneven, so progressive enhancement remains necessary
            when shipping this to production traffic today.</p></main>
            <footer>Mozilla footer links</footer></body></html>"#;
        let (title, prose) = extract_readable(html);

        assert_eq!(title.as_deref(), Some("Navigation API - MDN"));
        assert!(prose.contains("intercept a navigation"));
        assert!(!prose.contains("window.analytics"));
        assert!(!prose.contains("color: red"));
        assert!(!prose.contains("Mozilla footer links"));
    }

    #[test]
    fn entities_are_decoded_without_mangling_stray_ampersands() {
        assert_eq!(
            decode_entities("a &amp; b &lt;tag&gt; &quot;q&quot; &#65; Q&A"),
            "a & b <tag> \"q\" A Q&A"
        );
    }

    #[test]
    fn prompt_block_demands_inline_citation_of_real_urls() {
        let sources = vec![ResearchSource {
            url: "https://web.dev/articles/vitals".into(),
            title: "Web Vitals".into(),
            host: "web.dev".into(),
            excerpt: "Core Web Vitals are the subset of Web Vitals that apply to all pages.".into(),
            words: 400,
            primary: true,
        }];
        let block = format_source_material(&sources);

        assert!(block.contains("https://web.dev/articles/vitals"));
        assert!(block.contains(&format!("At least {} different", sources.len().min(3))));
        assert!(format_source_material(&[]).is_empty());
    }

    #[test]
    fn topic_string_keeps_category_terms_that_disambiguate_a_search() {
        assert_eq!(
            course_topic(
                "Event loop: macrotasks, microtasks, and rendering",
                "runtime"
            ),
            "Event loop macrotasks, microtasks, and rendering runtime"
        );
    }

    #[test]
    fn discovery_drops_documents_that_only_share_one_word_with_the_topic() {
        let payload = serde_json::json!({
            "documents": [
                {
                    "mdn_url": "/en-US/docs/Web/HTTP/Reference/Status/508",
                    "title": "508 Loop Detected",
                    "summary": "The server detected an infinite loop while processing the request."
                },
                {
                    "mdn_url": "/en-US/docs/Web/API/HTML_DOM_API/Microtask_guide",
                    "title": "Using microtasks in JavaScript with queueMicrotask()",
                    "summary": "A microtask runs after the current task and before rendering, \
                                which is why the event loop order matters."
                },
                {
                    "mdn_url": "/en-US/docs/WebAssembly/Reference/Control_flow/loop",
                    "title": "loop",
                    "summary": "The loop instruction creates a label for a control flow loop."
                },
                {
                    "mdn_url": "/en-US/docs/Learn_web_development/Core/Frameworks_libraries/Vue_rendering_lists",
                    "title": "Rendering a list of Vue components",
                    "summary": "Loop over an array to render a list, and see how rendering \
                                updates when the runtime data changes."
                },
                {
                    "mdn_url": "/en-US/docs/Mozilla/Add-ons/WebExtensions/API/runtime/onMessage",
                    "title": "runtime.onMessage",
                    "summary": "Fired when a message is sent; the runtime delivers the event to \
                                the listener."
                }
            ]
        });

        let found = relevant_mdn_documents(
            &payload,
            "Event loop: macrotasks, microtasks, and rendering runtime",
            5,
        );

        assert_eq!(
            found,
            vec!["https://developer.mozilla.org/en-US/docs/Web/API/HTML_DOM_API/Microtask_guide"]
        );
    }

    #[test]
    fn specific_pages_outrank_documentation_landing_pages() {
        assert!(
            path_specificity("https://developer.mozilla.org/en-US/docs/Web/API/Navigation_API")
                > path_specificity("https://developer.mozilla.org/en-US/docs/Web/JavaScript")
        );
        assert_eq!(path_specificity("https://html.spec.whatwg.org/"), 0);
    }

    #[test]
    fn citation_counting_only_credits_urls_present_in_the_course() {
        let sources = vec![
            ResearchSource {
                url: "https://web.dev/articles/vitals".into(),
                title: "Web Vitals".into(),
                host: "web.dev".into(),
                excerpt: "excerpt".into(),
                words: 400,
                primary: true,
            },
            ResearchSource {
                url: "https://v8.dev/blog/hidden-classes".into(),
                title: "Hidden classes".into(),
                host: "v8.dev".into(),
                excerpt: "excerpt".into(),
                words: 400,
                primary: true,
            },
        ];
        let markdown = "See [vitals](https://web.dev/articles/vitals) for the metric set.";

        assert_eq!(cited_source_count(markdown, &sources), 1);
        // A dropped trailing slash or an added anchor is still a citation.
        let spec = vec![ResearchSource {
            url: "https://html.spec.whatwg.org/multipage/".into(),
            title: "HTML Standard".into(),
            host: "html.spec.whatwg.org".into(),
            excerpt: "excerpt".into(),
            words: 400,
            primary: true,
        }];
        assert_eq!(
            cited_source_count(
                "[spec](https://html.spec.whatwg.org/multipage#event-loops)",
                &spec
            ),
            1
        );
    }

    /// Extraction is hand-rolled, so each curated publisher family needs a
    /// health check. Run with `--ignored` when adding a host to the allowlist.
    #[tokio::test]
    #[ignore = "hits the live network"]
    async fn live_extraction_yields_readable_prose_for_every_curated_host() {
        let researcher = Researcher::new();
        let mut failures = Vec::new();
        for url in [
            "https://developer.mozilla.org/en-US/docs/Web/JavaScript/EventLoop",
            "https://developer.mozilla.org/en-US/docs/Web/API/Performance",
            "https://html.spec.whatwg.org/multipage/webappapis.html",
            "https://nodejs.org/api/esm.html",
            "https://web.dev/articles/vitals",
            "https://www.typescriptlang.org/docs/handbook/2/everyday-types.html",
            "https://www.w3.org/WAI/standards-guidelines/wcag/",
            "https://github.com/tc39/proposal-temporal/blob/main/README.md",
        ] {
            match researcher.fetch_source(url).await {
                Ok(source) => println!("{:>6} words  {url}", source.words),
                Err(reason) => failures.push(reason),
            }
        }
        assert!(failures.is_empty(), "unreadable sources: {failures:#?}");
    }

    #[tokio::test]
    #[ignore = "hits the live network"]
    async fn live_fetch_returns_readable_primary_documentation() {
        let researcher = Researcher::new();
        let source = researcher
            .fetch_source("https://developer.mozilla.org/en-US/docs/Web/API/Navigation_API")
            .await
            .expect("MDN should be reachable");

        assert!(source.words > 200);
        assert!(source.excerpt.to_lowercase().contains("navigation"));
    }

    #[test]
    fn gnu_manual_pages_fall_back_to_the_same_material_on_man7() {
        assert_eq!(
            fallback_source("https://www.gnu.org/software/bash/manual/html_node/Redirections.html"),
            Some((
                "https://man7.org/linux/man-pages/man1/bash.1.html".to_string(),
                "Redirections".to_string()
            ))
        );
        assert_eq!(
            fallback_source(
                "https://www.gnu.org/software/bash/manual/html_node/What-is-a-shell_003f.html"
            )
            .map(|(_, hint)| hint),
            Some("What is a shell".to_string())
        );
        assert_eq!(
            fallback_source(
                "https://www.gnu.org/software/coreutils/manual/html_node/ls-invocation.html"
            ),
            Some((
                "https://man7.org/linux/man-pages/man1/ls.1.html".to_string(),
                "ls".to_string()
            ))
        );
        assert_eq!(
            fallback_source(
                "https://www.gnu.org/software/findutils/manual/html_node/find_html/Name.html"
            )
            .map(|(url, _)| url),
            Some("https://man7.org/linux/man-pages/man1/find.1.html".to_string())
        );
        assert_eq!(
            fallback_source("https://man7.org/linux/man-pages/man1/ls.1.html"),
            None
        );
        assert_eq!(
            fallback_source("https://www.gnu.org/software/emacs/manual/x.html"),
            None
        );
    }

    #[test]
    fn a_focused_excerpt_starts_just_before_the_topic_and_falls_back_to_the_opening() {
        let prose = (1..=40)
            .map(|n| format!("line {n} about the program"))
            .chain([
                "REDIRECTION".to_string(),
                "Before a command is executed, its input and output may be redirected.".to_string(),
            ])
            .chain((41..=60).map(|n| format!("line {n} about the program")))
            .collect::<Vec<_>>()
            .join("\n");
        let excerpt = focused_excerpt(&prose, "Redirections", 30);
        assert!(
            excerpt.starts_with("line 38 about the program"),
            "{excerpt}"
        );
        assert!(excerpt.contains("REDIRECTION"));
        assert!(focused_excerpt(&prose, "Nothing here", 30).starts_with("line 1 about"));
        assert!(
            focused_excerpt(&prose, "ls", 30).starts_with("line 1 about"),
            "short hints do not match"
        );
    }

    #[test]
    fn a_gathering_names_the_hosts_it_could_not_reach() {
        let gathered = Gathered {
            sources: Vec::new(),
            skipped: vec![
                ("https://www.gnu.org/a".into(), "timed out".into()),
                ("https://www.gnu.org/b".into(), "timed out".into()),
                ("https://man7.org/x".into(), "404".into()),
            ],
            substituted: Vec::new(),
            discovered: Vec::new(),
            search_error: None,
        };
        assert_eq!(
            gathered.unreachable_hosts(),
            vec!["man7.org", "www.gnu.org"]
        );
    }
}
