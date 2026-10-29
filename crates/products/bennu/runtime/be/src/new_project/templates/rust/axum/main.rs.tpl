use axum::{routing::get, Router};

async fn index() -> &'static str {
    "Hello from {{title}}!"
}

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(index)).route("/health", get(health));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:{{port}}")
        .await
        .expect("could not bind the port");
    println!("listening on http://127.0.0.1:{{port}}");
    axum::serve(listener, app).await.expect("server error");
}
