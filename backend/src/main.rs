use std::sync::Arc;
use axum::Router;
use txxt_server::app_state::AppState;
use txxt_server::data_access::data_context::DataContext;
use txxt_server::web_api::routes::map_routes;
use txxt_server::shared::models::settings::Settings;

#[tokio::main]
async fn main() {
    let settings = Settings::load().unwrap();
    let data_context = DataContext::new(&settings.redb_file_path).expect("Could not initialize database");
    data_context.ensure_default_user().expect("Could not ensure default user");

    let app_state = Arc::new(AppState { data_context });

    let app = Router::new()
        .merge(map_routes(app_state));
    let listener = tokio::net::TcpListener::bind(format!("{}:{}", settings.tcp_socket_binding, settings.tcp_socket_port))
        .await
        .expect("Failed to bind tcp listener");

    println!("Server running on http://localhost:{}", settings.tcp_socket_port);

    axum::serve(listener, app)
        .await
        .expect("Failed to start web server")
}
