use axum::extract::{Path, State};
use axum::Form;
use maud::{html, Markup};
use serde::Deserialize;

use crate::AppState;

#[derive(Deserialize)]
pub struct NotesForm {
    #[serde(default)]
    pub notes: String,
}

pub async fn handler(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    Form(form): Form<NotesForm>,
) -> Markup {
    let db_path = state.db_path.clone();
    let notes = form.notes.clone();
    tokio::task::spawn_blocking(move || {
        let conn = crate::db::open(&db_path).unwrap();
        let _ = conn.execute(
            "UPDATE sessions SET notes=?1 WHERE session_id=?2",
            [&notes, &sid],
        );
    })
    .await
    .ok();

    html! { span.notes-saved { "saved" } }
}
