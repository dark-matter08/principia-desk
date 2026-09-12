//! Web search as something the desk does, not something a model claims to
//! have done.
//!
//! A tutor running on Ollama, OpenRouter or a bare chat API has no way to
//! look anything up, and one with its own search tools returns results the
//! desk cannot audit. So the desk searches for itself: through a SearXNG
//! instance (a URL, no key), the Brave Search API or Tavily (a key each).
//! What comes back is only ever a list of candidate pages. Every one still
//! has to pass the subject's source allowlist and be fetched by the desk
//! before a word of it reaches the tutor, so the reading list stays real.

use serde::{Deserialize, Serialize};

/// Remote Ledger runs a local SearXNG on this port; the same instance can
/// serve both apps, so it is the default.
pub const DEFAULT_SEARXNG_URL: &str = "http://127.0.0.1:8899";
pub const BRAVE_KEY: &str = "brave_api_key";
pub const TAVILY_KEY: &str = "tavily_api_key";
const PROVIDER_KEY: &str = "search_provider";
const SEARXNG_URL_KEY: &str = "searxng_url";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    None,
    Searxng,
    Brave,
    Tavily,
}

impl Provider {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" | "off" | "" => Some(Self::None),
            "searxng" => Some(Self::Searxng),
            "brave" => Some(Self::Brave),
            "tavily" => Some(Self::Tavily),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Searxng => "searxng",
            Self::Brave => "brave",
            Self::Tavily => "tavily",
        }
    }
}

/// Everything a search needs, snapshotted from the config table and the
/// keychain so the researcher never touches either mid-lesson.
#[derive(Debug, Clone, Default)]
pub struct SearchConfig {
    pub provider: Provider,
    pub searxng_url: String,
    pub brave_key: Option<String>,
    pub tavily_key: Option<String>,
}

impl SearchConfig {
    pub fn enabled(&self) -> bool {
        self.provider != Provider::None
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub engine: String,
}

/// What the settings page shows: the choice, and whether it works right now.
#[derive(Debug, Clone, Serialize)]
pub struct Availability {
    pub provider: Provider,
    pub ok: bool,
    pub why: String,
    pub searxng_url: String,
    pub brave_key_set: bool,
    pub tavily_key_set: bool,
}

pub fn load(conn: &rusqlite::Connection) -> SearchConfig {
    let provider = crate::db::get_config(conn, PROVIDER_KEY)
        .ok()
        .flatten()
        .and_then(|value| Provider::parse(&value))
        .unwrap_or_default();
    let searxng_url = crate::db::get_config(conn, SEARXNG_URL_KEY)
        .ok()
        .flatten()
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SEARXNG_URL.to_string());
    SearchConfig {
        provider,
        searxng_url: normalize_base(&searxng_url),
        brave_key: crate::keychain::get_secret(BRAVE_KEY),
        tavily_key: crate::keychain::get_secret(TAVILY_KEY),
    }
}

pub fn save(
    conn: &rusqlite::Connection,
    provider: Provider,
    searxng_url: &str,
) -> Result<(), String> {
    let base = normalize_base(searxng_url);
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err("The SearXNG address must start with http:// or https://.".into());
    }
    crate::db::set_config(conn, PROVIDER_KEY, provider.name()).map_err(|e| e.to_string())?;
    crate::db::set_config(conn, SEARXNG_URL_KEY, &base).map_err(|e| e.to_string())
}

pub fn normalize_base(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        DEFAULT_SEARXNG_URL.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Set or clear a provider's key. The name is fixed here so a caller cannot
/// write an arbitrary keychain item.
pub fn set_key(provider: Provider, value: &str) -> Result<(), String> {
    let name = match provider {
        Provider::Brave => BRAVE_KEY,
        Provider::Tavily => TAVILY_KEY,
        _ => return Err("This search provider does not use a key.".into()),
    };
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return Err("That does not look like an API key.".into());
    }
    crate::keychain::set_secret(name, value.trim())
}

/// The query as the engine should see it: the topic, then the hosts the
/// subject teaches from, so results are documentation rather than blog posts.
pub fn site_query(query: &str, hosts: &[&str]) -> String {
    let sites: Vec<String> = hosts
        .iter()
        .take(3)
        .map(|host| format!("site:{host}"))
        .collect();
    if sites.is_empty() {
        query.trim().to_string()
    } else {
        format!("{} ({})", query.trim(), sites.join(" OR "))
    }
}

/// Is a search possible right now? A cheap probe, not a search.
pub async fn availability(client: &reqwest::Client, config: &SearchConfig) -> Availability {
    let base = Availability {
        provider: config.provider,
        ok: false,
        why: String::new(),
        searxng_url: config.searxng_url.clone(),
        brave_key_set: config.brave_key.is_some(),
        tavily_key_set: config.tavily_key.is_some(),
    };
    match config.provider {
        Provider::None => Availability {
            why: "Web search is off. Lessons use only the curriculum's own sources.".into(),
            ..base
        },
        Provider::Searxng => {
            let probe = client
                .get(format!("{}/healthz", config.searxng_url))
                .timeout(std::time::Duration::from_secs(3))
                .send()
                .await;
            let up = match probe {
                Ok(response) => response.status().is_success(),
                Err(_) => client
                    .get(&config.searxng_url)
                    .timeout(std::time::Duration::from_secs(3))
                    .send()
                    .await
                    .is_ok_and(|response| response.status().is_success()),
            };
            if up {
                Availability {
                    ok: true,
                    why: format!("SearXNG is answering at {}.", config.searxng_url),
                    ..base
                }
            } else {
                Availability {
                    why: format!(
                        "SearXNG is not answering at {}. Start it, or point the desk at a running instance.",
                        config.searxng_url
                    ),
                    ..base
                }
            }
        }
        Provider::Brave => {
            if config.brave_key.is_some() {
                Availability {
                    ok: true,
                    why: "Brave Search key is set.".into(),
                    ..base
                }
            } else {
                Availability {
                    why: "No Brave Search key is set.".into(),
                    ..base
                }
            }
        }
        Provider::Tavily => {
            if config.tavily_key.is_some() {
                Availability {
                    ok: true,
                    why: "Tavily key is set.".into(),
                    ..base
                }
            } else {
                Availability {
                    why: "No Tavily key is set.".into(),
                    ..base
                }
            }
        }
    }
}

/// Run one search. Nothing configured yields an empty list rather than an
/// error, because a lesson that cannot search must go on without it.
pub async fn search(
    client: &reqwest::Client,
    config: &SearchConfig,
    query: &str,
    limit: usize,
    hosts: &[&str],
) -> Result<Vec<SearchResult>, String> {
    let limit = limit.clamp(1, 20);
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    match config.provider {
        Provider::None => Ok(Vec::new()),
        Provider::Searxng => {
            searxng(
                client,
                &config.searxng_url,
                &site_query(query, hosts),
                limit,
            )
            .await
        }
        Provider::Brave => {
            let key = config
                .brave_key
                .as_deref()
                .ok_or("No Brave Search key is set.")?;
            brave(client, key, &site_query(query, hosts), limit).await
        }
        Provider::Tavily => {
            let key = config
                .tavily_key
                .as_deref()
                .ok_or("No Tavily key is set.")?;
            tavily(client, key, query, limit, hosts).await
        }
    }
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn text(value: &serde_json::Value, limit: usize) -> String {
    let raw = value.as_str().unwrap_or_default();
    let flat = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(limit).collect()
}

fn strip_tags(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut inside = false;
    for c in value.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out
}

async fn searxng(
    client: &reqwest::Client,
    base: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, String> {
    let response = client
        .get(format!(
            "{base}/search?q={}&format=json&safesearch=0",
            percent_encode(query)
        ))
        .send()
        .await
        .map_err(|error| format!("SearXNG could not be reached at {base}: {error}"))?;
    if response.status().as_u16() == 403 {
        return Err("SearXNG refused the JSON API (403). Enable `json` under search.formats in its settings.yml.".into());
    }
    if !response.status().is_success() {
        return Err(format!("SearXNG answered {}", response.status()));
    }
    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("SearXNG returned unreadable JSON: {error}"))?;
    Ok(payload["results"]
        .as_array()
        .map(|results| {
            results
                .iter()
                .filter_map(|item| {
                    let url = text(&item["url"], 2048);
                    (!url.is_empty()).then(|| SearchResult {
                        title: text(&item["title"], 300),
                        url,
                        snippet: text(&item["content"], 500),
                        engine: text(&item["engine"], 40),
                    })
                })
                .take(limit)
                .collect()
        })
        .unwrap_or_default())
}

async fn brave(
    client: &reqwest::Client,
    key: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, String> {
    let response = client
        .get(format!(
            "https://api.search.brave.com/res/v1/web/search?q={}&count={limit}",
            percent_encode(query)
        ))
        .header("accept", "application/json")
        .header("x-subscription-token", key)
        .send()
        .await
        .map_err(|error| format!("Brave Search could not be reached: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Brave Search answered {}", response.status()));
    }
    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("Brave Search returned unreadable JSON: {error}"))?;
    Ok(payload["web"]["results"]
        .as_array()
        .map(|results| {
            results
                .iter()
                .filter_map(|item| {
                    let url = text(&item["url"], 2048);
                    (!url.is_empty()).then(|| SearchResult {
                        title: text(&item["title"], 300),
                        url,
                        snippet: strip_tags(&text(&item["description"], 600))
                            .chars()
                            .take(500)
                            .collect(),
                        engine: "brave".into(),
                    })
                })
                .take(limit)
                .collect()
        })
        .unwrap_or_default())
}

async fn tavily(
    client: &reqwest::Client,
    key: &str,
    query: &str,
    limit: usize,
    hosts: &[&str],
) -> Result<Vec<SearchResult>, String> {
    // Newer keys authenticate with the header; older ones with the body field.
    let mut body = serde_json::json!({ "query": query, "max_results": limit, "api_key": key });
    if !hosts.is_empty() {
        body["include_domains"] = serde_json::json!(hosts);
    }
    let response = client
        .post("https://api.tavily.com/search")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {key}"))
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("Tavily could not be reached: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Tavily answered {}", response.status()));
    }
    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("Tavily returned unreadable JSON: {error}"))?;
    Ok(payload["results"]
        .as_array()
        .map(|results| {
            results
                .iter()
                .filter_map(|item| {
                    let url = text(&item["url"], 2048);
                    (!url.is_empty()).then(|| SearchResult {
                        title: text(&item["title"], 300),
                        url,
                        snippet: text(&item["content"], 500),
                        engine: "tavily".into(),
                    })
                })
                .take(limit)
                .collect()
        })
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_parse_from_their_names_and_off_synonyms() {
        assert_eq!(Provider::parse("SearXNG"), Some(Provider::Searxng));
        assert_eq!(Provider::parse("brave"), Some(Provider::Brave));
        assert_eq!(Provider::parse("tavily"), Some(Provider::Tavily));
        assert_eq!(Provider::parse("off"), Some(Provider::None));
        assert_eq!(Provider::parse(""), Some(Provider::None));
        assert_eq!(Provider::parse("google"), None);
    }

    #[test]
    fn the_engine_query_names_the_subject_hosts() {
        assert_eq!(
            site_query(
                "bash redirection operators",
                &["gnu.org", "man7.org", "kernel.org", "extra.org"]
            ),
            "bash redirection operators (site:gnu.org OR site:man7.org OR site:kernel.org)"
        );
        assert_eq!(site_query("  plain  ", &[]), "plain");
    }

    #[test]
    fn a_searxng_address_is_normalized_and_defaulted() {
        assert_eq!(
            normalize_base("http://localhost:8899///"),
            "http://localhost:8899"
        );
        assert_eq!(normalize_base("   "), DEFAULT_SEARXNG_URL);
    }

    #[test]
    fn brave_descriptions_lose_their_markup() {
        assert_eq!(strip_tags("a <strong>bold</strong> claim"), "a bold claim");
    }

    #[tokio::test]
    async fn nothing_configured_searches_nothing_and_says_why() {
        let client = reqwest::Client::new();
        let config = SearchConfig::default();
        assert_eq!(
            search(&client, &config, "anything", 5, &[]).await.unwrap(),
            Vec::new()
        );
        let state = availability(&client, &config).await;
        assert!(!state.ok);
        assert!(state.why.contains("off"));
        let brave = SearchConfig {
            provider: Provider::Brave,
            ..SearchConfig::default()
        };
        assert!(search(&client, &brave, "anything", 5, &[])
            .await
            .unwrap_err()
            .contains("No Brave Search key"));
    }

    /// Needs a SearXNG answering on the default port with JSON enabled.
    #[tokio::test]
    #[ignore = "hits a local SearXNG"]
    async fn a_local_searxng_answers_with_documentation_pages() {
        let client = reqwest::Client::new();
        let config = SearchConfig {
            provider: Provider::Searxng,
            searxng_url: DEFAULT_SEARXNG_URL.into(),
            ..SearchConfig::default()
        };
        let results = search(
            &client,
            &config,
            "bash redirection operators",
            5,
            &["gnu.org", "man7.org"],
        )
        .await
        .unwrap();
        for result in &results {
            println!("{}: {} ({})", result.title, result.url, result.engine);
        }
        assert!(!results.is_empty());
        assert!(results.iter().all(|result| result.url.starts_with("http")));
    }
}
