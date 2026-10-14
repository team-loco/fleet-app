use chrono::{DateTime, Utc};
use serde_json::{Value, json};

pub const REQUESTED_AT: &str = "reconcile.fluxcd.io/requestedAt";
pub const FORCE_AT: &str = "reconcile.fluxcd.io/forceAt";
pub const RESET_AT: &str = "reconcile.fluxcd.io/resetAt";

pub fn reconcile(at: DateTime<Utc>) -> Value {
    let token = at.to_rfc3339();
    json!({"metadata": {"annotations": {REQUESTED_AT: token}}})
}

pub fn force(at: DateTime<Utc>) -> Value {
    let token = at.to_rfc3339();
    json!({"metadata": {"annotations": {REQUESTED_AT: token, FORCE_AT: token}}})
}

pub fn reset(at: DateTime<Utc>) -> Value {
    let token = at.to_rfc3339();
    json!({"metadata": {"annotations": {REQUESTED_AT: token, RESET_AT: token}}})
}

pub fn suspend() -> Value {
    json!({"spec": {"suspend": true}})
}
pub fn resume() -> Value {
    json!({"spec": {"suspend": false}})
}
