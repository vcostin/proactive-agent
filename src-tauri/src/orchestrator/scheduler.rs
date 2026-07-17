use crate::monitor::{DeferredMessage, SchedulerState};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;

const QUEUE_FILE_NAME: &str = "deferred_queue.json";

/// On-disk envelope next to config.json.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct QueueFile {
    pending: Vec<DeferredMessage>,
}

pub struct ProactivityScheduler {
    pending: Vec<DeferredMessage>,
    last_fired: Option<DeferredMessage>,
    persist_path: Option<PathBuf>,
}

impl ProactivityScheduler {
    pub fn new() -> Self {
        Self {
            pending: vec![],
            last_fired: None,
            persist_path: None,
        }
    }

    /// Empty scheduler that persists mutations to `path`.
    pub fn with_persist(path: PathBuf) -> Self {
        Self {
            pending: vec![],
            last_fired: None,
            persist_path: Some(path),
        }
    }

    /// Load pending queue from disk (missing file → empty). Enables persistence at `path`.
    pub fn load(path: PathBuf) -> anyhow::Result<Self> {
        let pending = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            let file: QueueFile = serde_json::from_str(&text)?;
            file.pending
        } else {
            vec![]
        };
        Ok(Self {
            pending,
            last_fired: None,
            persist_path: Some(path),
        })
    }

    /// Canonical path: same directory as `config.json`.
    pub fn queue_path_beside_config(config_path: &Path) -> PathBuf {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(QUEUE_FILE_NAME)
    }

    /// Add a deferred message. Replaces any pending item with the same message+trigger.
    pub fn add(&mut self, msg: DeferredMessage) {
        self.pending
            .retain(|m| !(m.message == msg.message && m.trigger == msg.trigger));
        self.pending.push(msg);
        self.persist();
    }

    /// Remove a pending message by id. Returns true if something was cancelled.
    pub fn cancel(&mut self, id: &str) -> bool {
        let before = self.pending.len();
        self.pending.retain(|m| m.id != id);
        let removed = self.pending.len() < before;
        if removed {
            self.persist();
        }
        removed
    }

    /// Manually fire a pending message by id — used by the "Fire Now" debug button.
    pub fn fire_now(&mut self, id: &str) -> Option<DeferredMessage> {
        let pos = self.pending.iter().position(|m| m.id == id)?;
        let msg = self.pending.remove(pos);
        self.last_fired = Some(msg.clone());
        self.persist();
        Some(msg)
    }

    /// Drain all messages whose fire_at has passed. Called by the background loop / startup.
    pub fn drain_due(&mut self) -> Vec<DeferredMessage> {
        let now = Utc::now();
        let mut due = Vec::new();
        self.pending.retain(|m| {
            if m.fire_at <= now {
                due.push(m.clone());
                false
            } else {
                true
            }
        });
        if let Some(last) = due.last() {
            self.last_fired = Some(last.clone());
        }
        if !due.is_empty() {
            self.persist();
        }
        due
    }

    pub fn state(&self) -> SchedulerState {
        SchedulerState {
            pending: self.pending.clone(),
            last_fired: self.last_fired.clone(),
        }
    }

    fn persist(&self) {
        let Some(path) = &self.persist_path else {
            return;
        };
        if let Err(e) = save_queue(path, &self.pending) {
            eprintln!("[SCHEDULER] failed to persist deferred queue: {e}");
        }
    }
}

fn save_queue(path: &Path, pending: &[DeferredMessage]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = QueueFile {
        pending: pending.to_vec(),
    };
    std::fs::write(path, serde_json::to_string_pretty(&file)?)?;
    Ok(())
}

/// Long-running tokio task — spawned once at startup.
/// Checks for due deferred messages every 30 seconds and emits them as Tauri events.
/// Skips the immediate first tick so startup overdue is owned by `flush_due_deferred`
/// (reliable invoke path once the UI has mounted).
pub async fn run_scheduler_loop(
    scheduler: Arc<Mutex<ProactivityScheduler>>,
    app_handle: tauri::AppHandle,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
    interval.tick().await; // consume immediate tick
    loop {
        interval.tick().await;
        let due = scheduler.lock().await.drain_due();
        for msg in due {
            let _ = app_handle.emit("proactive_message", &msg);
            let _ = app_handle.emit("scheduler_updated", ());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_queue_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "proactive-agent-{}-{}-{}.json",
            label,
            std::process::id(),
            Uuid::new_v4()
        ))
    }

    fn sample_msg(message: &str, trigger: &str, fire_at: DateTime<Utc>) -> DeferredMessage {
        DeferredMessage {
            id: Uuid::new_v4().to_string(),
            message: message.to_string(),
            trigger: trigger.to_string(),
            fire_at,
        }
    }

    #[test]
    fn pending_queue_survives_json_round_trip() {
        let path = temp_queue_path("roundtrip");
        let msg = sample_msg(
            "Did you finish that report?",
            "follow_up",
            Utc::now() + Duration::minutes(60),
        );

        {
            let mut sched = ProactivityScheduler::with_persist(path.clone());
            sched.add(msg.clone());
        }

        let loaded = ProactivityScheduler::load(path.clone()).expect("load queue");
        let pending = loaded.state().pending;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, msg.id);
        assert_eq!(pending[0].message, msg.message);
        assert_eq!(pending[0].trigger, msg.trigger);
        assert_eq!(pending[0].fire_at, msg.fire_at);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_missing_queue_file_yields_empty_pending() {
        let path = temp_queue_path("missing");
        assert!(!path.exists());

        let loaded = ProactivityScheduler::load(path.clone()).expect("missing file is ok");
        assert!(loaded.state().pending.is_empty());
        assert!(loaded.state().last_fired.is_none());
    }

    #[test]
    fn load_then_drain_returns_overdue_and_clears_pending() {
        let path = temp_queue_path("overdue");
        let overdue = sample_msg(
            "You left this pending",
            "restart",
            Utc::now() - Duration::minutes(5),
        );
        let future = sample_msg(
            "Still waiting",
            "later",
            Utc::now() + Duration::minutes(30),
        );

        {
            let mut sched = ProactivityScheduler::with_persist(path.clone());
            sched.add(overdue.clone());
            sched.add(future.clone());
        }

        let mut loaded = ProactivityScheduler::load(path.clone()).expect("load");
        let due = loaded.drain_due();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, overdue.id);
        assert_eq!(loaded.state().pending.len(), 1);
        assert_eq!(loaded.state().pending[0].id, future.id);

        // Persist cleared overdue from disk
        let reloaded = ProactivityScheduler::load(path.clone()).expect("reload");
        assert_eq!(reloaded.state().pending.len(), 1);
        assert_eq!(reloaded.state().pending[0].id, future.id);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn drain_due_returns_all_overdue_and_leaves_future() {
        let path = temp_queue_path("multi-overdue");
        let first = sample_msg(
            "First overdue nudge",
            "a",
            Utc::now() - Duration::minutes(10),
        );
        let second = sample_msg(
            "Second overdue nudge",
            "b",
            Utc::now() - Duration::minutes(1),
        );
        let future = sample_msg(
            "Not due yet",
            "c",
            Utc::now() + Duration::minutes(45),
        );

        {
            let mut sched = ProactivityScheduler::with_persist(path.clone());
            sched.add(first.clone());
            sched.add(second.clone());
            sched.add(future.clone());
        }

        // UI-ready flush path: load + drain_due (same as flush_due_deferred).
        let mut loaded = ProactivityScheduler::load(path.clone()).expect("load");
        let due = loaded.drain_due();
        let due_ids: Vec<_> = due.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(due.len(), 2, "all overdue items must deliver");
        assert!(due_ids.contains(&first.id.as_str()));
        assert!(due_ids.contains(&second.id.as_str()));
        assert!(!due_ids.contains(&future.id.as_str()));
        assert_eq!(loaded.state().pending.len(), 1);
        assert_eq!(loaded.state().pending[0].id, future.id);

        let reloaded = ProactivityScheduler::load(path.clone()).expect("reload");
        assert_eq!(reloaded.state().pending.len(), 1);
        assert_eq!(reloaded.state().pending[0].id, future.id);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn add_replaces_pending_with_same_message_and_trigger() {
        let mut sched = ProactivityScheduler::new();
        let first = sample_msg("Check in", "task", Utc::now() + Duration::minutes(10));
        let second = sample_msg("Check in", "task", Utc::now() + Duration::minutes(20));
        let other = sample_msg("Check in", "other", Utc::now() + Duration::minutes(15));

        sched.add(first);
        sched.add(second.clone());
        sched.add(other.clone());

        let pending = sched.state().pending;
        assert_eq!(pending.len(), 2);
        assert!(pending.iter().any(|m| m.id == second.id));
        assert!(pending.iter().any(|m| m.id == other.id));
    }

    #[test]
    fn cancel_removes_by_id_and_persists() {
        let path = temp_queue_path("cancel");
        let keep = sample_msg("Keep me", "a", Utc::now() + Duration::minutes(5));
        let drop = sample_msg("Drop me", "b", Utc::now() + Duration::minutes(5));

        let mut sched = ProactivityScheduler::with_persist(path.clone());
        sched.add(keep.clone());
        sched.add(drop.clone());
        assert!(sched.cancel(&drop.id));
        assert!(!sched.cancel(&drop.id));

        let loaded = ProactivityScheduler::load(path.clone()).expect("load");
        assert_eq!(loaded.state().pending.len(), 1);
        assert_eq!(loaded.state().pending[0].id, keep.id);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn queue_path_beside_config() {
        let config = PathBuf::from("/tmp/com.proactive.agent/config.json");
        assert_eq!(
            ProactivityScheduler::queue_path_beside_config(&config),
            PathBuf::from("/tmp/com.proactive.agent/deferred_queue.json")
        );
    }
}
