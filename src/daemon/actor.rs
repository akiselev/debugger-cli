//! Session actor - owns the debug session and serializes access to it
//!
//! Connection tasks send commands over an mpsc channel; the actor executes
//! them one at a time, which preserves DAP request ordering. After every
//! command and on a periodic tick it reduces pending DAP events and publishes
//! a state snapshot on a watch channel, so `await` (and any future
//! subscription) can wait on state changes without occupying the actor.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot, watch};

use crate::common::config::Config;
use crate::dap::StoppedEventBody;
use crate::ipc::protocol::{Command, Response};

use super::handler;
use super::session::{DebugSession, SessionState};

/// How often the actor reduces DAP events when no commands arrive.
const EVENT_TICK: Duration = Duration::from_millis(100);

/// A command forwarded from a connection task, with a channel for the reply.
pub struct ActorRequest {
    pub id: u64,
    pub command: Command,
    pub reply: oneshot::Sender<Response>,
}

/// Published view of the session, updated after every event reduction.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionSnapshot {
    pub session_active: bool,
    pub state: Option<SessionState>,
    /// Full stopped-event body, when the stop came from an adapter event.
    pub last_stop: Option<StoppedEventBody>,
    /// Stop reason fallback for stops without an event (attach, stop-on-entry).
    pub stopped_reason: Option<String>,
    pub stopped_thread: Option<i64>,
    pub exit_code: Option<i32>,
}

/// Run the session actor until every request sender is dropped.
///
/// On exit the actor stops any remaining session, so daemon shutdown only
/// needs to drop its sender and await this task.
pub async fn run(
    config: Arc<Config>,
    mut requests: mpsc::Receiver<ActorRequest>,
    snapshots: watch::Sender<SessionSnapshot>,
) {
    let mut session: Option<DebugSession> = None;
    let mut tick = tokio::time::interval(EVENT_TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            request = requests.recv() => {
                let Some(ActorRequest { id, command, reply }) = request else {
                    break;
                };

                reduce_events(&mut session).await;
                let response = handler::handle_command(&mut session, &config, id, command).await;
                publish(&snapshots, &session);
                let _ = reply.send(response);
            }
            _ = tick.tick() => {
                reduce_events(&mut session).await;
                publish(&snapshots, &session);
            }
        }
    }

    tracing::debug!("Session actor shutting down");
    if let Some(mut active) = session.take() {
        let _ = active.stop().await;
    }
}

async fn reduce_events(session: &mut Option<DebugSession>) {
    if let Some(active) = session.as_mut() {
        if let Err(e) = active.process_events().await {
            tracing::warn!("Error processing events: {}", e);
        }
    }
}

fn publish(snapshots: &watch::Sender<SessionSnapshot>, session: &Option<DebugSession>) {
    let snapshot = match session {
        Some(active) => SessionSnapshot {
            session_active: true,
            state: Some(active.state()),
            last_stop: active.last_stop().cloned(),
            stopped_reason: active.stopped_reason().map(String::from),
            stopped_thread: active.stopped_thread(),
            exit_code: active.exit_code(),
        },
        None => SessionSnapshot::default(),
    };

    snapshots.send_if_modified(|current| {
        if *current == snapshot {
            false
        } else {
            *current = snapshot;
            true
        }
    });
}
