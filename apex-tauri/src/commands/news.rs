use crate::dto::NewsItemDto;
use crate::state::AppState;
use crate::validation;
use apex_core::domain::models::Symbol;
use serde::Serialize;
use tauri::State;

/// Latest news items, newest first.
#[tauri::command]
pub async fn get_news(
    limit: Option<usize>,
    symbol: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<NewsItemDto>, String> {
    let limit = limit.unwrap_or(50).min(500);
    if let Some(ref s) = symbol {
        validation::validate_symbol(s)?;
        let items = state.news.get_news_for_symbol(&Symbol(s.clone()), limit);
        return Ok(items.iter().map(NewsItemDto::from).collect());
    }

    let items = state.news.latest_news(limit);
    Ok(items.iter().map(NewsItemDto::from).collect())
}

/// Search cached news items.
#[tauri::command]
pub async fn search_news(
    query: String,
    symbol: Option<String>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<NewsItemDto>, String> {
    validation::validate_string_length(&query, "query")?;
    if let Some(ref s) = symbol {
        validation::validate_symbol(s)?;
    }

    let items = state.news.search_news(
        &query,
        symbol.as_ref().map(|s| Symbol(s.clone())).as_ref(),
        limit.unwrap_or(50).min(500),
    );
    Ok(items.iter().map(NewsItemDto::from).collect())
}

/// Configured news feeds.
#[derive(Debug, Clone, Serialize)]
pub struct NewsFeedDto {
    pub name: String,
    pub url: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn list_news_feeds(state: State<'_, AppState>) -> Result<Vec<NewsFeedDto>, String> {
    Ok(state
        .news
        .list_feeds()
        .into_iter()
        .map(|f| NewsFeedDto {
            name: f.name,
            url: f.url,
            enabled: f.enabled,
        })
        .collect())
}
