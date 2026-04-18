use maud::{html, Markup, DOCTYPE};
use super::assets::{CSS, JS};
use super::components::welcome;

pub fn full_page(sidebar_content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                title { "Total Recall" }
                link rel="stylesheet"
                    href="https://fonts.googleapis.com/css2?family=Syne:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&family=Instrument+Sans:wght@400;500&display=swap";
                link rel="stylesheet"
                    href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github-dark.min.css";
                script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/marked/marked.min.js" {}
                script src="https://unpkg.com/htmx.org@1.9.12" {}
                style { (maud::PreEscaped(CSS)) }
            }
            body {
                div id="topbar" {
                    span.wordmark { "Total Recall" }
                    div id="search-wrap" {
                        span.search-icon { "\u{2315}" }
                        input
                            id="search-input"
                            name="q"
                            placeholder="Search conversations\u{2026}"
                            hx-get="/search"
                            hx-trigger="input changed delay:250ms"
                            hx-target="#main";
                    }
                    button
                        id="refresh-btn"
                        title="Refresh index"
                        hx-post="/refresh"
                        hx-target="#sidebar"
                        hx-swap="outerHTML"
                    { "\u{27F3}" }
                }
                div id="shell" {
                    div id="left-panel" {
                        div id="sidenav" {
                            button
                                id="snav-timeline"
                                class="snav-btn active"
                                hx-get="/drawer?by=timeline"
                                hx-target="#sidebar"
                                hx-swap="outerHTML"
                                onclick="setNav('timeline')"
                            { "Timeline" }
                            button
                                id="snav-projects"
                                class="snav-btn"
                                hx-get="/drawer?by=projects"
                                hx-target="#sidebar"
                                hx-swap="outerHTML"
                                onclick="setNav('projects')"
                            { "Projects" }
                            button
                                id="snav-plans"
                                class="snav-btn"
                                hx-get="/plans/sidebar"
                                hx-target="#sidebar"
                                hx-swap="outerHTML"
                                onclick="setNav('plans')"
                            { "Plans" }
                        }
                        (sidebar_content)
                    }
                    div id="resize-handle" {}
                    div id="main" {
                        (welcome())
                    }
                }
                script { (maud::PreEscaped(JS)) }
            }
        }
    }
}
