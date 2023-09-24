#[macro_use]
extern crate rocket;
extern crate serde;
extern crate serde_json;

use async_std::task;
use rocket::config::LogLevel;
use rocket::response::status;
use rocket::serde::json::Json;
use rocket::{ Config, State };
use serde_json::{ json, Value };
use serde::Serialize;
use slog::{ o, Drain, Logger };
use std::collections::HashMap;
use std::io;
use std::sync::{ Arc, Mutex };
use std::time::{ Duration, Instant };

#[derive(Serialize)]
struct EntryWithMetadata {
    value: Value,
    ttl: Option<f64>,
}

#[derive(Serialize)]
struct TtlResponse {
    ttl: Option<f64>,
    status: &'static str,
}

// Define type aliases for readability.
type ExpiryTime = Option<Instant>;
type Db = Arc<Mutex<HashMap<String, (String, ExpiryTime)>>>;

/// Periodically cleans up expired keys in the database.
async fn cleanup_expired_keys(db: Db) {
    loop {
        let sleep_duration = {
            let db = db.lock().unwrap();
            db.iter()
                .filter_map(|(_, (_, expiry))| *expiry)
                .min()
                .map(|next_expiry| {
                    let now = Instant::now();
                    if next_expiry > now {
                        next_expiry.duration_since(now)
                    } else {
                        Duration::from_secs(0)
                    }
                })
                .unwrap_or(Duration::from_secs(1))
        };

        task::sleep(sleep_duration).await;

        let now = Instant::now();
        let mut db = db.lock().unwrap();
        db.retain(|_, (_, expiry)| expiry.map_or(true, |e| e > now));
    }
}

/// Retrieves all active (non-expired) entries from the database.
#[get("/")]
fn get_all(db: &State<Db>) -> Json<HashMap<String, EntryWithMetadata>> {
    let db = db.lock().unwrap();
    let mut response = HashMap::new();

    for (key, (serialized, expiry)) in db.iter() {
        match serde_json::from_str(serialized) {
            Ok(deserialized) => {
                let expiry_metadata = expiry.map(|expiry_time|
                    expiry_time.saturating_duration_since(Instant::now()).as_secs_f64()
                );
                let entry_with_metadata = EntryWithMetadata {
                    value: deserialized,
                    ttl: expiry_metadata,
                };
                response.insert(key.clone(), entry_with_metadata);
            }
            Err(e) => eprintln!("Error deserializing JSON: {:?}", e),
        }
    }
    Json(response)
}

/// Retrieves a specific entry by key from the database, if it is not expired.
#[get("/<key>")]
fn get(key: &str, db: &State<Db>, log: &State<Logger>) -> Json<Value> {
    let db = db.lock().unwrap();
    match db.get(key) {
        Some((serialized, _)) =>
            match serde_json::from_str(serialized) {
                Ok(deserialized) => Json(deserialized),
                Err(e) => {
                    let status = "Error deserializing JSON".to_string();
                    slog::error!(log, "{}: {:?}", status, e);
                    Json(json!({ "status": status }))
                }
            }
        None => {
            let status = format!("Key not found: {}", key);
            Json(json!({ "status": status }))
        }
    }
}

/// Retrieves the ttl for a specific entry by key from the database.
#[get("/ttl/<key>")]
fn get_ttl(key: &str, db: &State<Db>) -> Result<Json<TtlResponse>, status::NotFound<String>> {
    let db = db.lock().unwrap();
    match db.get(key) {
        Some((_, expiry)) => {
            let ttl = expiry.map(|expiry_time|
                expiry_time.saturating_duration_since(Instant::now()).as_secs_f64()
            );
            Ok(
                Json(TtlResponse {
                    ttl,
                    status: "success",
                })
            )
        }
        None => Err(status::NotFound(format!("Key not found: {}", key))),
    }
}

/// Inserts or updates an entry in the database with an optional TTL.
#[post("/<key>?<ttl>", format = "json", data = "<entry>")]
fn create(
    key: &str,
    ttl: Option<u64>,
    entry: Json<Value>,
    db: &State<Db>,
    log: &State<Logger>
) -> Json<Value> {
    let mut db = db.lock().unwrap();
    let expiry = ttl.map(|t| Instant::now() + Duration::from_secs(t));
    match serde_json::to_string(&*entry) {
        Ok(serialized) => {
            db.insert(key.to_string(), (serialized, expiry));
            let status = format!("Key inserted: {}", key);

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
        let status = format!("Key deleted: {}", key);

        slog::info!(log, "{}", status);
        Json(json!({ "status": status }))
    } else {
        Json(json!({"status": "Key Not Found"}))
    }
}

/// Configures and launches the Rocket application.
#[launch]
fn rocket() -> _ {
    let db: Db = Arc::new(Mutex::new(HashMap::new()));
    let db_clone = Arc::clone(&db);
    task::spawn(cleanup_expired_keys(db_clone));

    rocket
        ::build()
        .configure(build_config())
        .manage(db)
        .manage(build_logger())
        .mount("/", routes![get, get_all, get_ttl, create, delete])
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
