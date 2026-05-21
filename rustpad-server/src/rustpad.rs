//! Eventually consistent server-side logic for Rustpad.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use futures::prelude::*;
use log::{info, warn};
use operational_transform::OperationSeq;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Notify};
use warp::ws::{Message, WebSocket};

use crate::{
    database::{Database, PersistedDocument},
    ot::transform_index,
};

/// The main object representing a collaborative session.
pub struct Rustpad {
    /// State modified by critical sections of the code.
    state: RwLock<State>,
    /// Incremented to obtain unique user IDs.
    count: AtomicU64,
    /// Used to notify clients of new text operations.
    notify: Notify,
    /// Used to inform all clients of metadata updates.
    update: broadcast::Sender<ServerMsg>,
    /// Set to true when the document is destroyed.
    killed: AtomicBool,
    /// Document identifier used for persistence.
    document_id: Option<String>,
    /// Database connection used for write-through persistence.
    database: Option<Database>,
}

/// Shared state involving multiple users, protected by a lock.
struct State {
    operations: Vec<UserOperation>,
    text: String,
    language: Option<String>,
    users: HashMap<u64, UserInfo>,
    cursors: HashMap<u64, CursorData>,
    created_at: u64,
    closed_at: Option<u64>,
    host_token: Option<String>,
    replay_events: Vec<ReplayEvent>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            operations: Vec::new(),
            text: String::new(),
            language: None,
            users: HashMap::new(),
            cursors: HashMap::new(),
            created_at: unix_ms(),
            closed_at: None,
            host_token: None,
            replay_events: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UserOperation {
    id: u64,
    operation: OperationSeq,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserInfo {
    name: String,
    hue: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CursorData {
    cursors: Vec<u32>,
    selections: Vec<(u32, u32)>,
}

/// A timestamped event used to reconstruct a room replay.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ReplayEvent {
    /// A user joined, changed info, or left the room.
    UserInfo {
        /// Room-relative timestamp in milliseconds.
        at_ms: u64,
        /// Socket user ID.
        id: u64,
        /// User info, or `None` when the user left.
        info: Option<UserInfo>,
    },
    /// A text edit operation was accepted.
    Edit {
        /// Room-relative timestamp in milliseconds.
        at_ms: u64,
        /// Socket user ID.
        id: u64,
        /// Operational transform operation.
        operation: OperationSeq,
    },
    /// A user's cursor or selection changed.
    Cursor {
        /// Room-relative timestamp in milliseconds.
        at_ms: u64,
        /// Socket user ID.
        id: u64,
        /// Cursor and selection positions.
        data: CursorData,
    },
    /// The editor language changed.
    Language {
        /// Room-relative timestamp in milliseconds.
        at_ms: u64,
        /// Monaco language identifier.
        language: String,
    },
    /// The room was stopped by the host.
    Closed {
        /// Room-relative timestamp in milliseconds.
        at_ms: u64,
    },
}

/// Serializable replay payload returned by the HTTP replay endpoint.
#[derive(Clone, Debug, Serialize)]
pub struct ReplayResponse {
    /// Document identifier.
    pub id: String,
    /// Unix timestamp in milliseconds when the room was created.
    pub created_at: u64,
    /// Unix timestamp in milliseconds when the room was stopped.
    pub closed_at: Option<u64>,
    /// Latest syntax language.
    pub language: Option<String>,
    /// Latest room text.
    pub final_text: String,
    /// Timeline events used by the replay player.
    pub events: Vec<ReplayEvent>,
}

/// A message received from the client over WebSocket.
#[derive(Clone, Debug, Serialize, Deserialize)]
enum ClientMsg {
    /// Joins the room with user information and an optional host token.
    Join {
        info: UserInfo,
        host_token: Option<String>,
    },
    /// Represents a sequence of local edits from the user.
    Edit {
        revision: usize,
        operation: OperationSeq,
    },
    /// Sets the language of the editor.
    SetLanguage(String),
    /// Sets the user's current information.
    ClientInfo(UserInfo),
    /// Sets the user's cursor and selection positions.
    CursorData(CursorData),
    /// Stops the room, if the host token matches.
    StopRoom { host_token: String },
}

/// A message sent to the client over WebSocket.
#[derive(Clone, Debug, Serialize, Deserialize)]
enum ServerMsg {
    /// Informs the client of their unique socket ID.
    Identity(u64),
    /// Sends room lifecycle and host information to one client.
    RoomState {
        created_at: u64,
        closed_at: Option<u64>,
        is_host: bool,
        host_token: Option<String>,
    },
    /// Informs clients that the room has been stopped.
    RoomClosed { replay_url: String, closed_at: u64 },
    /// Broadcasts text operations to all clients.
    History {
        start: usize,
        operations: Vec<UserOperation>,
    },
    /// Broadcasts the current language, last writer wins.
    Language(String),
    /// Broadcasts a user's information, or `None` on disconnect.
    UserInfo { id: u64, info: Option<UserInfo> },
    /// Broadcasts a user's cursor position.
    UserCursor { id: u64, data: CursorData },
}

impl From<ServerMsg> for Message {
    fn from(msg: ServerMsg) -> Self {
        let serialized = serde_json::to_string(&msg).expect("failed serialize");
        Message::text(serialized)
    }
}

impl Default for Rustpad {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(16);
        Self {
            state: Default::default(),
            count: Default::default(),
            notify: Default::default(),
            update: tx,
            killed: AtomicBool::new(false),
            document_id: None,
            database: None,
        }
    }
}

impl From<PersistedDocument> for Rustpad {
    fn from(document: PersistedDocument) -> Self {
        Self::new(None, None, Some(document))
    }
}

impl Rustpad {
    /// Create a new Rustpad document with optional persistence metadata.
    pub fn new(
        document_id: Option<String>,
        database: Option<Database>,
        document: Option<PersistedDocument>,
    ) -> Self {
        let rustpad = Self {
            document_id,
            database,
            ..Self::default()
        };
        let Some(document) = document else {
            return rustpad;
        };

        let (replay_events, operations) = restore_history(&document.text, document.replay_events);

        {
            let mut state = rustpad.state.write();
            state.text = document.text;
            state.language = document.language;
            state.created_at = if document.created_at == 0 {
                unix_ms()
            } else {
                document.created_at
            };
            state.closed_at = document.closed_at;
            state.host_token = document.host_token;
            state.replay_events = replay_events;
            state.operations = operations;
        }
        rustpad
    }
}

impl Rustpad {
    /// Handle a connection from a WebSocket.
    pub async fn on_connection(&self, socket: WebSocket) {
        let id = self.count.fetch_add(1, Ordering::Relaxed);
        info!("connection! id = {}", id);
        if let Err(e) = self.handle_connection(id, socket).await {
            warn!("connection terminated early: {}", e);
        }
        info!("disconnection, id = {}", id);
        let removed_user = {
            let mut state = self.state.write();
            let removed_user = state.users.remove(&id).is_some();
            state.cursors.remove(&id);
            if removed_user && state.closed_at.is_none() {
                let at_ms = event_ms(state.created_at);
                state.replay_events.push(ReplayEvent::UserInfo {
                    at_ms,
                    id,
                    info: None,
                });
            }
            removed_user
        };
        if removed_user {
            self.update
                .send(ServerMsg::UserInfo { id, info: None })
                .ok();
            self.persist().await;
        }
    }

    /// Returns a snapshot of the latest text.
    pub fn text(&self) -> String {
        let state = self.state.read();
        state.text.clone()
    }

    /// Returns a snapshot of the current document for persistence.
    pub fn snapshot(&self) -> PersistedDocument {
        let state = self.state.read();
        PersistedDocument {
            text: state.text.clone(),
            language: state.language.clone(),
            created_at: state.created_at,
            closed_at: state.closed_at,
            host_token: state.host_token.clone(),
            replay_events: serde_json::to_value(&state.replay_events)
                .expect("failed to serialize replay events"),
        }
    }

    /// Returns a replay payload for this room.
    pub fn replay(&self, id: String) -> ReplayResponse {
        let state = self.state.read();
        ReplayResponse {
            id,
            created_at: state.created_at,
            closed_at: state.closed_at,
            language: state.language.clone(),
            final_text: state.text.clone(),
            events: state.replay_events.clone(),
        }
    }

    /// Returns a replay payload from a persisted document.
    pub fn replay_from_document(id: String, document: PersistedDocument) -> ReplayResponse {
        let (events, _) = restore_history(&document.text, document.replay_events);
        ReplayResponse {
            id,
            created_at: document.created_at,
            closed_at: document.closed_at,
            language: document.language,
            final_text: document.text,
            events,
        }
    }

    /// Returns the current revision.
    pub fn revision(&self) -> usize {
        let state = self.state.read();
        state.operations.len()
    }

    /// Kill this object immediately, dropping all current connections.
    pub fn kill(&self) {
        self.killed.store(true, Ordering::Relaxed);
        self.notify.notify_waiters();
    }

    /// Returns if this Rustpad object has been killed.
    pub fn killed(&self) -> bool {
        self.killed.load(Ordering::Relaxed)
    }

    async fn handle_connection(&self, id: u64, mut socket: WebSocket) -> Result<()> {
        let mut update_rx = self.update.subscribe();

        let mut revision: usize = self.send_initial(id, &mut socket).await?;

        loop {
            // In order to avoid the "lost wakeup" problem, we first request a
            // notification, **then** check the current state for new revisions.
            // This is the same approach that `tokio::sync::watch` takes.
            let notified = self.notify.notified();
            if self.killed() {
                break;
            }
            if self.revision() > revision {
                revision = self.send_history(revision, &mut socket).await?
            }

            tokio::select! {
                _ = notified => {}
                update = update_rx.recv() => {
                    socket.send(update?.into()).await?;
                }
                result = socket.next() => {
                    match result {
                        None => break,
                        Some(message) => {
                            for msg in self.handle_message(id, message?).await? {
                                socket.send(msg.into()).await?;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn send_initial(&self, id: u64, socket: &mut WebSocket) -> Result<usize> {
        socket.send(ServerMsg::Identity(id).into()).await?;
        let mut messages = Vec::new();
        let revision = {
            let state = self.state.read();
            if !state.operations.is_empty() {
                messages.push(ServerMsg::History {
                    start: 0,
                    operations: state.operations.clone(),
                });
            }
            if let Some(language) = &state.language {
                messages.push(ServerMsg::Language(language.clone()));
            }
            for (&id, info) in &state.users {
                messages.push(ServerMsg::UserInfo {
                    id,
                    info: Some(info.clone()),
                });
            }
            for (&id, data) in &state.cursors {
                messages.push(ServerMsg::UserCursor {
                    id,
                    data: data.clone(),
                });
            }
            state.operations.len()
        };
        for msg in messages {
            socket.send(msg.into()).await?;
        }
        Ok(revision)
    }

    async fn send_history(&self, start: usize, socket: &mut WebSocket) -> Result<usize> {
        let operations = {
            let state = self.state.read();
            let len = state.operations.len();
            if start < len {
                state.operations[start..].to_owned()
            } else {
                Vec::new()
            }
        };
        let num_ops = operations.len();
        if num_ops > 0 {
            let msg = ServerMsg::History { start, operations };
            socket.send(msg.into()).await?;
        }
        Ok(start + num_ops)
    }

    async fn handle_message(&self, id: u64, message: Message) -> Result<Vec<ServerMsg>> {
        let msg: ClientMsg = match message.to_str() {
            Ok(text) => serde_json::from_str(text).context("failed to deserialize message")?,
            Err(()) => return Ok(Vec::new()), // Ignore non-text messages
        };
        let mut replies = Vec::new();
        let mut persist = false;
        match msg {
            ClientMsg::Join { info, host_token } => {
                let (room_state, user_info) = self.join_room(id, info, host_token);
                replies.push(room_state);
                if let Some(user_info) = user_info {
                    self.update.send(user_info).ok();
                    persist = true;
                }
            }
            ClientMsg::Edit {
                revision,
                operation,
            } => {
                if self
                    .apply_edit(id, revision, operation)
                    .context("invalid edit operation")?
                {
                    self.notify.notify_waiters();
                    persist = true;
                }
            }
            ClientMsg::SetLanguage(language) => {
                if let Some(msg) = self.set_language(language) {
                    self.update.send(msg).ok();
                    persist = true;
                }
            }
            ClientMsg::ClientInfo(info) => {
                if let Some(msg) = self.set_user_info(id, info) {
                    self.update.send(msg).ok();
                    persist = true;
                }
            }
            ClientMsg::CursorData(data) => {
                if let Some(msg) = self.set_cursor_data(id, data) {
                    self.update.send(msg).ok();
                    persist = true;
                }
            }
            ClientMsg::StopRoom { host_token } => {
                if let Some(msg) = self.stop_room(host_token)? {
                    self.update.send(msg.clone()).ok();
                    replies.push(msg);
                    persist = true;
                }
            }
        }
        if persist {
            self.persist().await;
        }
        Ok(replies)
    }

    fn apply_edit(&self, id: u64, revision: usize, mut operation: OperationSeq) -> Result<bool> {
        info!(
            "edit: id = {}, revision = {}, base_len = {}, target_len = {}",
            id,
            revision,
            operation.base_len(),
            operation.target_len()
        );
        let state = self.state.upgradable_read();
        if state.closed_at.is_some() {
            return Ok(false);
        }
        let len = state.operations.len();
        if revision > len {
            bail!("got revision {}, but current is {}", revision, len);
        }
        for history_op in &state.operations[revision..] {
            operation = operation.transform(&history_op.operation)?.0;
        }
        if operation.target_len() > 256 * 1024 {
            bail!(
                "target length {} is greater than 256 KiB maximum",
                operation.target_len()
            );
        }
        let new_text = operation.apply(&state.text)?;
        let mut state = RwLockUpgradableReadGuard::upgrade(state);
        for (_, data) in state.cursors.iter_mut() {
            for cursor in data.cursors.iter_mut() {
                *cursor = transform_index(&operation, *cursor);
            }
            for (start, end) in data.selections.iter_mut() {
                *start = transform_index(&operation, *start);
                *end = transform_index(&operation, *end);
            }
        }
        let at_ms = event_ms(state.created_at);
        state.operations.push(UserOperation {
            id,
            operation: operation.clone(),
        });
        state.replay_events.push(ReplayEvent::Edit {
            at_ms,
            id,
            operation,
        });
        state.text = new_text;
        Ok(true)
    }

    fn join_room(
        &self,
        id: u64,
        info: UserInfo,
        host_token: Option<String>,
    ) -> (ServerMsg, Option<ServerMsg>) {
        let mut assigned_host_token = None;
        let mut user_info = None;
        let (created_at, closed_at, is_host) = {
            let mut state = self.state.write();
            let is_host = match state.host_token.clone() {
                Some(token) => host_token.as_deref() == Some(token.as_str()),
                None => {
                    let token = host_token.unwrap_or_else(generate_host_token);
                    state.host_token = Some(token.clone());
                    assigned_host_token = Some(token);
                    true
                }
            };
            if state.closed_at.is_none() {
                let at_ms = event_ms(state.created_at);
                state.users.insert(id, info.clone());
                state.replay_events.push(ReplayEvent::UserInfo {
                    at_ms,
                    id,
                    info: Some(info.clone()),
                });
                user_info = Some(ServerMsg::UserInfo {
                    id,
                    info: Some(info),
                });
            }
            (state.created_at, state.closed_at, is_host)
        };
        (
            ServerMsg::RoomState {
                created_at,
                closed_at,
                is_host,
                host_token: assigned_host_token,
            },
            user_info,
        )
    }

    fn set_user_info(&self, id: u64, info: UserInfo) -> Option<ServerMsg> {
        let mut state = self.state.write();
        if state.closed_at.is_some() {
            return None;
        }
        let at_ms = event_ms(state.created_at);
        state.users.insert(id, info.clone());
        state.replay_events.push(ReplayEvent::UserInfo {
            at_ms,
            id,
            info: Some(info.clone()),
        });
        Some(ServerMsg::UserInfo {
            id,
            info: Some(info),
        })
    }

    fn set_cursor_data(&self, id: u64, data: CursorData) -> Option<ServerMsg> {
        let mut state = self.state.write();
        if state.closed_at.is_some() {
            return None;
        }
        let at_ms = event_ms(state.created_at);
        state.cursors.insert(id, data.clone());
        state.replay_events.push(ReplayEvent::Cursor {
            at_ms,
            id,
            data: data.clone(),
        });
        Some(ServerMsg::UserCursor { id, data })
    }

    fn set_language(&self, language: String) -> Option<ServerMsg> {
        let mut state = self.state.write();
        if state.closed_at.is_some() {
            return None;
        }
        let at_ms = event_ms(state.created_at);
        state.language = Some(language.clone());
        state.replay_events.push(ReplayEvent::Language {
            at_ms,
            language: language.clone(),
        });
        Some(ServerMsg::Language(language))
    }

    fn stop_room(&self, host_token: String) -> Result<Option<ServerMsg>> {
        let mut state = self.state.write();
        if state.host_token.as_deref() != Some(host_token.as_str()) {
            bail!("invalid host token");
        }
        if let Some(closed_at) = state.closed_at {
            return Ok(Some(ServerMsg::RoomClosed {
                replay_url: self.replay_url(),
                closed_at,
            }));
        }
        let closed_at = unix_ms();
        let at_ms = closed_at.saturating_sub(state.created_at);
        state.closed_at = Some(closed_at);
        state.replay_events.push(ReplayEvent::Closed { at_ms });
        Ok(Some(ServerMsg::RoomClosed {
            replay_url: self.replay_url(),
            closed_at,
        }))
    }

    fn replay_url(&self) -> String {
        self.document_id
            .as_ref()
            .map(|id| format!("#{id}"))
            .unwrap_or_default()
    }

    async fn persist(&self) {
        let Some(database) = self.database.clone() else {
            return;
        };
        let Some(document_id) = self.document_id.clone() else {
            return;
        };
        let snapshot = self.snapshot();
        if let Err(e) = database.store(&document_id, &snapshot).await {
            warn!("failed to persist document {}: {}", document_id, e);
        }
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime returned before UNIX_EPOCH")
        .as_millis() as u64
}

fn event_ms(created_at: u64) -> u64 {
    unix_ms().saturating_sub(created_at)
}

fn restore_history(
    text: &str,
    replay_events: serde_json::Value,
) -> (Vec<ReplayEvent>, Vec<UserOperation>) {
    let mut replay_events: Vec<ReplayEvent> =
        serde_json::from_value(replay_events).unwrap_or_default();
    let mut operations = Vec::new();
    let mut replay_text = String::new();

    for event in &replay_events {
        if let ReplayEvent::Edit { id, operation, .. } = event {
            match operation.apply(&replay_text) {
                Ok(next) => {
                    replay_text = next;
                    operations.push(UserOperation {
                        id: *id,
                        operation: operation.clone(),
                    });
                }
                Err(_) => {
                    operations.clear();
                    replay_text.clear();
                    break;
                }
            }
        }
    }

    if replay_text != text {
        let mut operation = OperationSeq::default();
        operation.delete(bytecount::num_chars(replay_text.as_bytes()) as u64);
        operation.insert(text);
        let at_ms = replay_events
            .iter()
            .map(replay_event_time)
            .max()
            .unwrap_or(0);
        operations.push(UserOperation {
            id: u64::MAX,
            operation: operation.clone(),
        });
        replay_events.push(ReplayEvent::Edit {
            at_ms,
            id: u64::MAX,
            operation,
        });
    }

    (replay_events, operations)
}

fn replay_event_time(event: &ReplayEvent) -> u64 {
    match event {
        ReplayEvent::UserInfo { at_ms, .. }
        | ReplayEvent::Edit { at_ms, .. }
        | ReplayEvent::Cursor { at_ms, .. }
        | ReplayEvent::Language { at_ms, .. }
        | ReplayEvent::Closed { at_ms } => *at_ms,
    }
}

fn generate_host_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}
