use crate::monitor::{DeferredMessage, SchedulerState};
use chrono::Utc;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;

pub struct ProactivityScheduler {
    pending: Vec<DeferredMessage>,
    last_fired: Option<DeferredMessage>,
}

impl ProactivityScheduler {
    pub fn new() -> Self {
        Self { pending: vec![], last_fired: None }
    }

    pub fn add(&mut self, msg: DeferredMessage) {
        self.pending.push(msg);
    }

    /// Manually fire a pending message by id — used by the "Fire Now" debug button.
    pub fn fire_now(&mut self, id: &str) -> Option<DeferredMessage> {
        let pos = self.pending.iter().position(|m| m.id == id)?;
        let msg = self.pending.remove(pos);
        self.last_fired = Some(msg.clone());
        Some(msg)
    }

    /// Drain all messages whose fire_at has passed. Called by the background loop.
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
        due
    }

    pub fn state(&self) -> SchedulerState {
        SchedulerState { pending: self.pending.clone(), last_fired: self.last_fired.clone() }
    }
}

/// Long-running tokio task — spawned once at startup.
/// Checks for due deferred messages every 30 seconds and emits them as Tauri events.
pub async fn run_scheduler_loop(
    scheduler: Arc<Mutex<ProactivityScheduler>>,
    app_handle: tauri::AppHandle,
) {
    let mut interval =
        tokio::time::interval(std::time::Duration::from_secs(30));
    loop {
        interval.tick().await;
        let due = scheduler.lock().await.drain_due();
        for msg in due {
            let _ = app_handle.emit("proactive_message", &msg);
        }
    }
}
