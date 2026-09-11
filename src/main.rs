mod api;
mod apple;
mod crawler;
mod llm;
mod models;
mod network;
mod storage;

use actix_web::{App, HttpResponse, HttpServer, Result, error, web};
use serde::Deserialize;
use std::sync::Mutex;

// A struct describes one item. Vec<Todo> is a growable list of items.
struct Todo {
    title: String,
    done: bool,
}

// Serde turns the HTML form's `title` field into a Rust String.
#[derive(Deserialize)]
struct NewTodo {
    title: String,
}

// Data shares the list across requests; Mutex allows one request to edit it at a time.
type Todos = web::Data<Mutex<Vec<Todo>>>;

// GET /todos: read the list and return a complete HTML page.
async fn todo_index(todos: Todos) -> Result<HttpResponse> {
    let todos = todos
        .lock()
        .map_err(|_| error::ErrorInternalServerError("List unavailable"))?;
    let mut items = String::new();
    for (id, todo) in todos.iter().enumerate() {
        let title = escape_html(&todo.title);
        let (class, action) = if todo.done {
            ("done", "Undo")
        } else {
            ("", "Done")
        };
        items.push_str(&format!(
            "<li><span class=\"{class}\">{title}</span><form method=\"post\" action=\"/todos/{id}/toggle\"><button>{action}</button></form></li>"
        ));
    }
    if todos.is_empty() {
        items.push_str("<li>Your list is empty. Add your first task above.</li>");
    }
    let page = include_str!("todos.html").replace("<!-- ITEMS -->", &items);
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(page))
}

// POST /todos: Actix extracts the form body before calling this handler.
async fn add_todo(form: web::Form<NewTodo>, todos: Todos) -> Result<HttpResponse> {
    let title = form.title.trim();
    if title.is_empty() || title.chars().count() > 120 {
        return Ok(HttpResponse::BadRequest()
            .body("Use a title of 1-120 characters. Go back and try again."));
    }
    let mut todos = todos
        .lock()
        .map_err(|_| error::ErrorInternalServerError("List unavailable"))?;
    todos.push(Todo {
        title: title.to_owned(),
        done: false,
    });
    Ok(back_to_list())
}

// Path extracts the number from /todos/{id}/toggle.
// List positions work as IDs here because this small app never removes items.
async fn toggle_todo(id: web::Path<usize>, todos: Todos) -> Result<HttpResponse> {
    let mut todos = todos
        .lock()
        .map_err(|_| error::ErrorInternalServerError("List unavailable"))?;
    let todo = todos
        .get_mut(id.into_inner())
        .ok_or_else(|| error::ErrorNotFound("Task not found"))?;
    todo.done = !todo.done;
    Ok(back_to_list())
}

// A 303 tells the browser to GET /todos after a POST, so refresh won't repeat the edit.
fn back_to_list() -> HttpResponse {
    HttpResponse::SeeOther()
        .insert_header(("Location", "/todos"))
        .finish()
}

// User input is text, not HTML. Escape it before inserting it in the page.
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn routes(config: &mut web::ServiceConfig) {
    config
        .route("/", web::get().to(index))
        .route("/settings", web::get().to(settings))
        .route("/todos", web::get().to(todo_index))
        .route("/todos", web::post().to(add_todo))
        .route("/todos/{id}/toggle", web::post().to(toggle_todo));
}

// This macro starts the async runtime that drives the web server.
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Create state OUTSIDE the factory so all server workers share the same list.
    let todos = web::Data::new(Mutex::new(Vec::<Todo>::new()));
    let listener = std::net::TcpListener::bind(("127.0.0.1", 8080))?;
    let root = std::env::var("CRAWLER_DATA_DIR").unwrap_or_else(|_| "data".into());
    let store = storage::Store::open(std::path::Path::new(&root)).map_err(std::io::Error::other)?;
    store.recover().map_err(std::io::Error::other)?;
    let engine = web::Data::new(std::sync::Arc::new(crawler::Engine::new(store)));
    let server = HttpServer::new(move || {
        App::new()
            .app_data(todos.clone())
            .app_data(engine.clone())
            .configure(routes)
            .configure(api::routes)
    })
    .listen(listener)?;
    println!("Open http://127.0.0.1:8080 — press Ctrl+C to stop.");
    server.run().await
}

#[cfg(test)]
mod tests;

// Share the HTML layout between both pages; all inserted content is embedded HTML.
fn crawler_page(is_settings: bool) -> HttpResponse {
    let (title, content) = if is_settings {
        ("爬蟲設定", include_str!("settings.html"))
    } else {
        ("爬蟲目前工作狀態 dashboard", include_str!("dashboard.html"))
    };
    let page = include_str!("page.html")
        .replace("{{TITLE}}", title)
        .replace(
            "{{DASHBOARD_CURRENT}}",
            if is_settings {
                ""
            } else {
                "aria-current=\"page\""
            },
        )
        .replace(
            "{{SETTINGS_CURRENT}}",
            if is_settings {
                "aria-current=\"page\""
            } else {
                ""
            },
        )
        .replace("<!-- CONTENT -->", content);
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(page)
}

async fn index() -> HttpResponse {
    crawler_page(false)
}
async fn settings() -> HttpResponse {
    crawler_page(true)
}
