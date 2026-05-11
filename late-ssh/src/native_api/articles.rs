use axum::{
    Json, Router,
    extract::{Query, State as AxumState},
    routing::get,
};
use late_core::models::article::Article;
use serde::{Deserialize, Serialize};

use crate::state::State;

use super::{ApiError, NativeAuthUser};

pub fn router() -> Router<State> {
    Router::new().route("/api/native/articles", get(get_articles))
}

#[derive(Deserialize)]
struct ArticlesParams {
    limit: Option<i64>,
}

#[derive(Serialize)]
struct ArticleItem {
    id: String,
    url: String,
    title: String,
    summary: String,
    ascii_art: String,
    created: String,
}

async fn get_articles(
    _auth: NativeAuthUser,
    Query(params): Query<ArticlesParams>,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<ArticleItem>>, ApiError> {
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let articles = Article::list_recent(&client, limit).await.map_err(|_| ApiError::Db)?;
    Ok(Json(
        articles
            .into_iter()
            .map(|a| ArticleItem {
                id: a.id.to_string(),
                url: a.url,
                title: a.title,
                summary: a.summary,
                ascii_art: a.ascii_art,
                created: a.created.to_rfc3339(),
            })
            .collect(),
    ))
}
