use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use tauri::Emitter;
use uuid::Uuid;

use tokio::sync::oneshot;

type PendingMap = DashMap<String, oneshot::Sender<Result<Option<serde_json::Value>, String>>>;
static PENDING: LazyLock<PendingMap> = LazyLock::new(|| DashMap::new());

pub async fn call_js<Args, Return>(name: &str, args: Args) -> Result<Return, String>
where
    Args: Serialize,
    Return: for<'de> Deserialize<'de>,
{
    println!("Calling JS function: {name}");
    let id = Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<Result<Option<serde_json::Value>, String>>();
    PENDING.insert(id.clone(), tx);

    println!("Emitting call-js {name} for {id}");
    if let Some(handle) = crate::APP_HANDLE.get() {
        handle
            .emit(
                "call-js",
                serde_json::json!({
                    "id": id,
                    "name": name,
                    "args": args,
                }),
            )
            .map_err(|e| e.to_string())?;
        println!("Waiting for JS response");

        let res = tokio::time::timeout(tokio::time::Duration::from_secs(5), rx).await;
        match res {
            Err(_) => {
                PENDING.remove(&id);
                Err("Timeout waiting for JS response".to_string())
            }
            Ok(Err(_)) => {
                PENDING.remove(&id);
                Err("JS response channel closed".to_string())
            }
            Ok(Ok(Ok(value))) => {
                match value {
                    Some(value) => serde_json::from_value(value).map_err(|e| e.to_string()),
                    None => Ok(serde_json::from_value(serde_json::Value::Null)
                        .map_err(|e| e.to_string())?),
                }
            }
            Ok(Ok(Err(e))) => Err(e),
        }
    } else {
        Err("APP_HANDLE not initialized".to_string())
    }
}

#[tauri::command]
pub fn js_response(id: String, result: Option<serde_json::Value>) -> Result<(), String> {
    println!("JS response for {id} {result:?}");
    match PENDING.remove(&id) {
        None => return Err("Request ID not found".to_string()),
        Some((_, tx)) => {
            tx.send(Ok(result))
                .map_err(|_| "Failed to send response".to_string())?;
            Ok(())
        }
    }
}
