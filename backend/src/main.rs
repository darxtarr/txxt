use axum::Router;
use txxt_server::web_api::routes::map_routes;
use txxt_server::shared::models::settings::Settings;

fn create_app() -> Router {
    map_routes()
}

#[tokio::main]
async fn main() {
    let settings = Settings::load().unwrap();
    let app = create_app();
    let listener = tokio::net::TcpListener::bind(format!("{}:{}", settings.tcp_socket_binding, settings.tcp_socket_port))
        .await
        .expect("Failed to bind tcp listener");

    println!("Server running on http://localhost:{}", settings.tcp_socket_port);

    axum::serve(listener, app)
        .await
        .expect("Failed to start web server")
}