#[macro_use]
extern crate rocket;
extern crate serde;
extern crate serde_json;

use async_std::task;
use rocket::config::LogLevel;
use rocket::serde::json::Json;
use rocket::{Config, State};
use serde_json::{json, Value};
use slog::{o, Drain, Logger};
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// Define type aliases for readability.
type ExpiryTime = Instant;
type Db = Arc<Mutex<HashMap<String, (String, ExpiryTime)>>>;

/// Periodically cleans up expired keys in the database.
async fn cleanup_expired_keys(db: Db) {
    loop {
        task::sleep(Duration::from_secs(1)).await; // Runs every second
        let mut db = db.lock().unwrap();
        db.retain(|_, (_, expiry)| *expiry > Instant::now());
    }
}

/// Retrieves all active (non-expired) entries from the database.
#[get("/")]
fn get_all(db: &State<Db>) -> Json<HashMap<String, Value>> {
    let db = db.lock().unwrap();
    let mut response = HashMap::new();

    // Iterates over the database and filters out expired entries
    for (key, (serialized, expiry)) in db.iter() {
        if *expiry > Instant::now() {
            // Attempts to deserialize the stored JSON string.
            match serde_json::from_str(serialized) {
                Ok(deserialized) => {
                    response.insert(key.clone(), deserialized);
                }
                Err(e) => eprintln!("Error deserializing JSON: {:?}", e),
            }
        }
    }
    Json(response)
}

/// Retrieves a specific entry by key from the database, if it is not expired.
#[get("/<key>")]
fn get(key: &str, db: &State<Db>, log: &State<Logger>) -> Json<Value> {
    let db = db.lock().unwrap();
    match db.get(key) {
        Some((serialized, expiry)) if *expiry > Instant::now() => {
            match serde_json::from_str(serialized) {
                Ok(deserialized) => Json(deserialized),
                Err(e) => {
                    let status = "Error deserializing JSON".to_string();
                    slog::error!(log, "{}: {:?}", status, e);
                    Json(json!({ "status": status }))
                }
            }
        }
        Some(_) => {
            let status = format!("key expired: {}", key);
            Json(json!({ "status": status }))
        }
        None => {
            let status = format!("key not found: {}", key);
            Json(json!({ "status": status }))
        }
    }
}

/// Inserts or updates an entry in the database with an optional TTL.
#[post("/<key>?<ttl>", format = "json", data = "<entry>")]
fn create(
    key: &str,
    ttl: Option<u64>,
    entry: Json<Value>,
    db: &State<Db>,
    log: &State<Logger>,
) -> Json<Value> {
    let mut db = db.lock().unwrap();
    let expiry = Instant::now() + Duration::from_secs(ttl.unwrap_or(300)); // Defaults to 5 minutes
    match serde_json::to_string(&*entry) {
        Ok(serialized) => {
            db.insert(key.to_string(), (serialized, expiry));
            let status = format!("key inserted: {}", key);

            slog::info!(log, "{}", status);
            Json(json!({ "status": status }))
        }
        Err(e) => {
            eprintln!("Error serializing JSON: {:?}", e);
            Json(json!({"status": "Error Creating Item"}))
        }
    }
}

/// Removes a specific entry by key from the database.
#[delete("/<key>")]
fn delete(key: &str, db: &State<Db>, log: &State<Logger>) -> Json<Value> {
    let mut db = db.lock().unwrap();
    if db.remove(key).is_some() {
        let status = format!("key deleted: {}", key);

        slog::info!(log, "{}", status);
        Json(json!({ "status": status }))
    } else {
        Json(json!({"status": "Item Not Found"}))
    }
}

/// Configures and launches the Rocket application.
#[launch]
fn rocket() -> _ {
    let db: Db = Arc::new(Mutex::new(HashMap::new()));
    let db_clone = Arc::clone(&db);
    task::spawn(cleanup_expired_keys(db_clone));

    rocket::build()
        .configure(build_config())
        .manage(db)
        .manage(build_logger())
        .mount("/", routes![get, get_all, create, delete])
}

fn build_config() -> Config {
    let mut config = Config::release_default();
    config.log_level = LogLevel::Off;

    config
}

fn build_logger() -> Logger {
    let drain = io::stdout(); // log to stdout
    let drain = slog_json::Json::default(drain).fuse();
    let drain = Mutex::new(drain).fuse();
    Logger::root(drain, o!())
}
