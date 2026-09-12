//! Commands for the web search the desk uses to find documentation.

use crate::search::{self, Availability, Provider, SearchResult};
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

type CmdResult<T> = Result<T, String>;

#[derive(Debug, Serialize)]
pub struct SearchSettingsView {
    pub provider: Provider,
    pub searxng_url: String,
    pub default_searxng_url: &'static str,
    pub brave_key_set: bool,
    pub tavily_key_set: bool,
    pub available: bool,
    pub why: String,
}

impl SearchSettingsView {
    fn from_availability(state: Availability) -> Self {
        Self {
            provider: state.provider,
            searxng_url: state.searxng_url,
            default_searxng_url: search::DEFAULT_SEARXNG_URL,
            brave_key_set: state.brave_key_set,
            tavily_key_set: state.tavily_key_set,
            available: state.ok,
            why: state.why,
        }
    }
}

/// The current choice and whether it works right now.
#[tauri::command]
pub async fn get_search_settings(state: State<'_, AppState>) -> CmdResult<SearchSettingsView> {
    let config = state.generator.researcher.search_config();
    let availability = search::availability(state.generator.researcher.client(), &config).await;
    Ok(SearchSettingsView::from_availability(availability))
}

/// Choose the provider and the SearXNG address; takes effect for the next
/// lesson prepared.
#[tauri::command]
pub async fn set_search_settings(
    state: State<'_, AppState>,
    provider: String,
    searxng_url: String,
) -> CmdResult<SearchSettingsView> {
    let provider = Provider::parse(&provider).ok_or("Unknown search provider.")?;
    let config = {
        let conn = state.db.0.lock().unwrap();
        search::save(&conn, provider, &searxng_url)?;
        search::load(&conn)
    };
    state.generator.researcher.set_search(config.clone());
    let availability = search::availability(state.generator.researcher.client(), &config).await;
    Ok(SearchSettingsView::from_availability(availability))
}

/// Store (or clear, when blank) a provider's API key in the keychain.
#[tauri::command]
pub async fn set_search_key(
    state: State<'_, AppState>,
    provider: String,
    value: String,
) -> CmdResult<SearchSettingsView> {
    let provider = Provider::parse(&provider).ok_or("Unknown search provider.")?;
    search::set_key(provider, &value)?;
    let config = {
        let conn = state.db.0.lock().unwrap();
        search::load(&conn)
    };
    state.generator.researcher.set_search(config.clone());
    let availability = search::availability(state.generator.researcher.client(), &config).await;
    Ok(SearchSettingsView::from_availability(availability))
}

/// Run one search with the saved configuration, so a learner can see what
/// the tutor would be handed.
#[tauri::command]
pub async fn test_search(
    state: State<'_, AppState>,
    query: String,
) -> CmdResult<Vec<SearchResult>> {
    if query.trim().is_empty() || query.len() > 400 {
        return Err("Type a short query to test with.".into());
    }
    let config = state.generator.researcher.search_config();
    if !config.enabled() {
        return Err("Choose a search provider first.".into());
    }
    search::search(state.generator.researcher.client(), &config, &query, 8, &[]).await
}
