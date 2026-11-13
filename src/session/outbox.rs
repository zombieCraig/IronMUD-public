//! Bounded per-connection output queue.
//!
//! Game code queues text for a client with [`ClientSender::send`], which has
//! the same shape as `mpsc::UnboundedSender::send` so every call site stays
//! as it was. Unlike a bare unbounded channel it tracks how many bytes are
//! waiting for the socket. A client that stops reading (deliberately, or a
//! dead link the kernel hasn't noticed) would otherwise make the server
//! buffer every room message, combat line and broadcast for it until memory
//! runs out. Past [`MAX_QUEUED_OUTPUT_BYTES`] further output is refused and
//! the session is told to disconnect.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::sync::mpsc::error::SendError;

use crate::InputEvent;

/// Output a connection may have queued but not yet written to its socket.
pub const MAX_QUEUED_OUTPUT_BYTES: usize = 1024 * 1024;

/// How long one socket write may block before the writer gives up on the
/// client.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct ClientSender {
    tx: mpsc::UnboundedSender<String>,
    queued: Arc<AtomicUsize>,
    overflowed: Arc<AtomicBool>,
    /// Where to post `Disconnect` on overflow. `None` for detached senders
    /// (tests), which also have no byte limit.
    disconnect: Option<mpsc::Sender<InputEvent>>,
}

impl ClientSender {
    /// A bounded sender for a live connection. The writer task must call
    /// [`ClientSender::mark_written`] for every message it takes off `tx`'s
    /// receiver.
    pub fn new(tx: mpsc::UnboundedSender<String>, disconnect: mpsc::Sender<InputEvent>) -> Self {
        Self {
            tx,
            queued: Arc::new(AtomicUsize::new(0)),
            overflowed: Arc::new(AtomicBool::new(false)),
            disconnect: Some(disconnect),
        }
    }

    /// A sender with no byte limit, for tests and detached sessions whose
    /// receiver doesn't report drained bytes.
    pub fn unbounded(tx: mpsc::UnboundedSender<String>) -> Self {
        Self {
            tx,
            queued: Arc::new(AtomicUsize::new(0)),
            overflowed: Arc::new(AtomicBool::new(false)),
            disconnect: None,
        }
    }

    /// Queue `msg` for the client. Fails when the receiver is gone, or when
    /// the client already has too much unread output (in which case the
    /// session is told to disconnect, once).
    pub fn send(&self, msg: String) -> Result<(), SendError<String>> {
        if self.disconnect.is_none() {
            return self.tx.send(msg);
        }
        if self.overflowed.load(Ordering::Acquire) {
            return Err(SendError(msg));
        }
        let n = msg.len();
        let before = self.queued.fetch_add(n, Ordering::AcqRel);
        if before.saturating_add(n) > MAX_QUEUED_OUTPUT_BYTES {
            self.queued.fetch_sub(n, Ordering::AcqRel);
            self.overflow();
            return Err(SendError(msg));
        }
        self.tx.send(msg).inspect_err(|_| {
            self.queued.fetch_sub(n, Ordering::AcqRel);
        })
    }

    /// Writer side: `n` bytes of a queued message have left the queue.
    pub fn mark_written(&self, n: usize) {
        self.drain_handle().mark_written(n);
    }

    /// The writer's view of this queue. Unlike a sender clone it doesn't keep
    /// the channel open, so the writer still ends when the session goes.
    pub fn drain_handle(&self) -> DrainHandle {
        DrainHandle {
            queued: self.queued.clone(),
            overflowed: self.overflowed.clone(),
        }
    }

    /// True once the client has been cut off for not reading.
    pub fn is_overflowed(&self) -> bool {
        self.overflowed.load(Ordering::Acquire)
    }

    /// Bytes queued and not yet written.
    pub fn queued_bytes(&self) -> usize {
        self.queued.load(Ordering::Acquire)
    }

    fn overflow(&self) {
        if self.overflowed.swap(true, Ordering::AcqRel) {
            return;
        }
        tracing::warn!(
            "Client output queue exceeded {} bytes; disconnecting the session",
            MAX_QUEUED_OUTPUT_BYTES
        );
        if let Some(tx) = &self.disconnect {
            let _ = tx.try_send(InputEvent::Disconnect);
        }
    }
}

/// Writer-side accounting for a [`ClientSender`]'s queue.
#[derive(Clone, Debug)]
pub struct DrainHandle {
    queued: Arc<AtomicUsize>,
    overflowed: Arc<AtomicBool>,
}

impl DrainHandle {
    /// `n` bytes of a queued message have been taken off the queue.
    pub fn mark_written(&self, n: usize) {
        // Saturate rather than wrap if accounting ever drifts.
        let _ = self
            .queued
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |q| Some(q.saturating_sub(n)));
    }

    /// True once the client has been cut off for not reading.
    pub fn is_overflowed(&self) -> bool {
        self.overflowed.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_refuses_output_and_requests_disconnect() {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let (tx_in, mut rx_in) = mpsc::channel::<InputEvent>(4);
        let s = ClientSender::new(tx, tx_in);
        let chunk = "x".repeat(64 * 1024);
        for _ in 0..(MAX_QUEUED_OUTPUT_BYTES / chunk.len()) {
            s.send(chunk.clone()).unwrap();
        }
        assert!(s.send("one more".to_string()).is_err());
        assert!(s.is_overflowed());
        assert!(matches!(rx_in.try_recv(), Ok(InputEvent::Disconnect)));
        assert!(rx_in.try_recv().is_err(), "disconnect posted once");
        // Draining doesn't revive an overflowed session.
        while let Ok(m) = rx.try_recv() {
            s.mark_written(m.len());
        }
        assert_eq!(s.queued_bytes(), 0);
        assert!(s.send("late".to_string()).is_err());
    }

    #[test]
    fn draining_frees_budget() {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let (tx_in, _rx_in) = mpsc::channel::<InputEvent>(4);
        let s = ClientSender::new(tx, tx_in);
        let chunk = "y".repeat(MAX_QUEUED_OUTPUT_BYTES / 2);
        for _ in 0..10 {
            s.send(chunk.clone()).unwrap();
            let m = rx.try_recv().unwrap();
            s.mark_written(m.len());
        }
        assert!(!s.is_overflowed());
    }

    #[test]
    fn unbounded_sender_never_overflows() {
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let s = ClientSender::unbounded(tx);
        let chunk = "z".repeat(MAX_QUEUED_OUTPUT_BYTES);
        for _ in 0..3 {
            s.send(chunk.clone()).unwrap();
        }
    }
}
