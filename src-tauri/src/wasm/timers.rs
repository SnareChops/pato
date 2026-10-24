//! Repeating timers for plugins.
//!
//! Plugins only run when the core calls into them, so anything periodic
//! (polling a local API, countdowns) needs the core to wake them. `set-interval`
//! spawns a task that calls the owning plugin's `timers-events.on-tick`.
//!
//! Limits: intervals below `MIN_INTERVAL` are rejected, a plugin may hold at
//! most `MAX_TIMERS_PER_PLUGIN`, and ticks never overlap - the task awaits the
//! plugin's handler before waiting for the next tick, and ticks missed while
//! it was busy are skipped, not queued.

use std::sync::LazyLock;
use std::time::Duration;

use dashmap::DashMap;
use tokio::task::AbortHandle;
use tokio::time::{interval_at, Instant, MissedTickBehavior};
use wasmtime::component::{HasSelf, Val};

use crate::wasm;

mod plugin {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "timers-host",
        imports: { default: async },
    });
}
use plugin::pato::plugin::timers;

const TIMERS_EVENTS: &str = "pato:plugin/timers-events";

const MIN_INTERVAL: Duration = Duration::from_millis(50);
const MAX_TIMERS_PER_PLUGIN: usize = 16;

struct Timer {
    pid: String,
    task: AbortHandle,
}

static TIMERS: LazyLock<DashMap<String, Timer>> = LazyLock::new(DashMap::new);

fn validate(interval_ms: u32, owned: usize) -> Result<Duration, String> {
    let period = Duration::from_millis(u64::from(interval_ms));
    if period < MIN_INTERVAL {
        return Err(format!("interval must be at least {} ms", MIN_INTERVAL.as_millis()));
    }
    if owned >= MAX_TIMERS_PER_PLUGIN {
        return Err(format!("at most {MAX_TIMERS_PER_PLUGIN} timers per plugin"));
    }
    Ok(period)
}

impl timers::Host for wasm::HostData {
    async fn set_interval(&mut self, interval_ms: u32) -> Result<String, String> {
        let owned = TIMERS.iter().filter(|t| t.pid == self.pid).count();
        let period = validate(interval_ms, owned)?;

        let id = uuid::Uuid::new_v4().to_string();
        // Spawn onto the *ambient* runtime (the plugin's own), as the
        // websocket relay does - see the note in `websocket::connect`.
        let task = tokio::spawn(run_timer(id.clone(), self.pid.clone(), period)).abort_handle();
        TIMERS.insert(id.clone(), Timer { pid: self.pid.clone(), task });
        Ok(id)
    }

    async fn cancel(&mut self, id: String) {
        if let Some((_, timer)) = TIMERS.remove_if(&id, |_, t| t.pid == self.pid) {
            timer.task.abort();
        }
    }
}

async fn run_timer(id: String, pid: String, period: Duration) {
    let mut ticker = interval_at(Instant::now() + period, period);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        if let Err(e) = wasm::plugin_call(&pid, TIMERS_EVENTS, "on-tick", vec![Val::String(id.clone())], vec![]).await {
            // Stop rather than log every period forever.
            eprintln!("{TIMERS_EVENTS}|on-tick dispatch to {pid} failed, stopping timer: {e}");
            TIMERS.remove(&id);
            return;
        }
    }
}

pub fn init() -> Result<(), String> {
    println!("Initializing Timers module...");
    wasm::link(|linker| timers::add_to_linker::<_, HasSelf<_>>(linker, |host| host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_below_minimum_interval() {
        assert!(validate(49, 0).is_err());
        assert!(validate(0, 0).is_err());
        assert_eq!(validate(50, 0), Ok(Duration::from_millis(50)));
    }

    #[test]
    fn caps_timers_per_plugin() {
        assert!(validate(1000, MAX_TIMERS_PER_PLUGIN - 1).is_ok());
        assert!(validate(1000, MAX_TIMERS_PER_PLUGIN).is_err());
    }
}
