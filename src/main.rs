use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::collections::HashSet;

fn duplicates(l: Vec<&str>) -> bool {
    let mut set = HashSet::new();
    for s in l.iter() {
        if set.contains(s) {
            return true;
        }
        set.insert(s);
    }
    false
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn check_duplicates(Json(payload): Json<Vec<String>>) -> StatusCode {
    let mut server_list = vec!["foo", "bar"];
    let mut client_list = payload.iter().map(|s| s.as_str()).collect();
    server_list.append(&mut client_list);
    if duplicates(server_list) {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::OK
    }
}

#[tokio::main]
async fn main() {
    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        // `POST /users` goes to `create_user`
        .route("/list_check", post(check_duplicates));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_duplicates() {
        let l = vec!["foo", "bar", "baz"];
        let l = vec!["", "bar", "barr"];
        assert!(!duplicates(l));
    }

    #[test]
    fn with_duplicates() {
        let l = vec!["foo", "foo"];
        assert!(duplicates(l));
    }
}
