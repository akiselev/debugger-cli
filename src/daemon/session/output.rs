//! Bounded, cursor-based buffer for debuggee output.
//!
//! Every buffered event has a monotonic sequence number, so independent
//! clients can each read from their own cursor without consuming data the
//! others have not seen. Old events expire only through the event-count and
//! byte-count bounds (or an explicit clear); sequence numbers keep advancing
//! across trimming and clears, so a stale cursor simply resumes at the oldest
//! retained event.

use std::collections::VecDeque;

/// Output event for buffering
#[derive(Debug, Clone)]
pub struct OutputEvent {
    pub category: String,
    pub output: String,
}

/// Bounded, in-memory buffer for debuggee output.
///
/// The DAP reader can produce arbitrarily large output events, so the buffer
/// enforces both an event-count and a byte-count limit. Keeping this logic
/// separate from `DebugSession` makes its accounting independently testable.
#[derive(Debug)]
pub struct OutputBuffer {
    events: VecDeque<OutputEvent>,
    /// Sequence number of the front event; `first_seq + events.len()` is the
    /// cursor position just past the newest event.
    first_seq: u64,
    max_events: usize,
    max_bytes: usize,
    current_bytes: usize,
}

impl OutputBuffer {
    pub fn new(max_events: usize, max_bytes: usize) -> Self {
        Self {
            events: VecDeque::new(),
            first_seq: 0,
            max_events,
            max_bytes,
            current_bytes: 0,
        }
    }

    pub fn push(&mut self, category: &str, output: &str) {
        if self.max_events == 0 || self.max_bytes == 0 {
            return;
        }

        let output = truncate_utf8_to_bytes(output, self.max_bytes);
        if output.is_empty() {
            return;
        }
        let output_bytes = output.len();

        while self.current_bytes + output_bytes > self.max_bytes && !self.events.is_empty() {
            self.pop_oldest();
        }

        while self.events.len() >= self.max_events && !self.events.is_empty() {
            self.pop_oldest();
        }

        self.events.push_back(OutputEvent {
            category: category.to_string(),
            output,
        });
        self.current_bytes += output_bytes;
    }

    fn pop_oldest(&mut self) {
        if let Some(removed) = self.events.pop_front() {
            self.current_bytes = self.current_bytes.saturating_sub(removed.output.len());
            self.first_seq += 1;
        }
    }

    /// Read events at or after `cursor` without consuming them.
    ///
    /// `None` reads from the oldest retained event. Returns the events and the
    /// cursor to pass on the next read to see only newer output. Cursors older
    /// than the retained window resume at the oldest event; cursors from the
    /// future clamp to the end.
    pub fn read_from(&self, cursor: Option<u64>) -> (Vec<OutputEvent>, u64) {
        let end = self.first_seq + self.events.len() as u64;
        let start = cursor
            .unwrap_or(self.first_seq)
            .clamp(self.first_seq, end);

        let events = self
            .events
            .iter()
            .skip((start - self.first_seq) as usize)
            .cloned()
            .collect();
        (events, end)
    }

    /// Discard all buffered events. Sequence numbers keep advancing, so
    /// cursors held by other clients stay valid.
    pub fn clear(&mut self) {
        self.first_seq += self.events.len() as u64;
        self.events.clear();
        self.current_bytes = 0;
    }
}

/// Return the longest valid UTF-8 prefix that fits within `max_bytes`.
fn truncate_utf8_to_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }

    let mut end = 0;
    for (start, character) in value.char_indices() {
        let character_end = start + character.len_utf8();
        if character_end > max_bytes {
            break;
        }
        end = character_end;
    }

    value[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::OutputBuffer;

    #[test]
    fn clearing_output_resets_byte_accounting() {
        let mut buffer = OutputBuffer::new(4, 4);
        buffer.push("stdout", "abcd");

        let (drained, _) = buffer.read_from(None);
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].output, "abcd");

        buffer.clear();
        assert_eq!(buffer.current_bytes, 0);

        buffer.push("stdout", "xyz");
        assert_eq!(buffer.current_bytes, 3);
        assert_eq!(buffer.read_from(None).0[0].output, "xyz");
    }

    #[test]
    fn output_is_truncated_on_a_utf8_boundary() {
        let mut buffer = OutputBuffer::new(4, 5);
        buffer.push("stdout", "ééé");

        let (output, _) = buffer.read_from(None);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].output, "éé");
        assert_eq!(buffer.current_bytes, 4);
    }

    #[test]
    fn zero_sized_buffers_discard_output() {
        let mut buffer = OutputBuffer::new(0, 32);
        buffer.push("stdout", "discard me");
        assert!(buffer.read_from(None).0.is_empty());
    }

    #[test]
    fn cursors_read_independently_and_survive_clears() {
        let mut buffer = OutputBuffer::new(16, 1024);
        buffer.push("stdout", "one");
        buffer.push("stdout", "two");

        // Two clients each read everything from their own cursor.
        let (a_events, a_cursor) = buffer.read_from(None);
        let (b_events, b_cursor) = buffer.read_from(None);
        assert_eq!(a_events.len(), 2);
        assert_eq!(b_events.len(), 2);
        assert_eq!(a_cursor, 2);
        assert_eq!(b_cursor, 2);

        // New output is visible to both, exactly once, from their cursors.
        buffer.push("stdout", "three");
        let (a_events, a_cursor) = buffer.read_from(Some(a_cursor));
        assert_eq!(a_events.len(), 1);
        assert_eq!(a_events[0].output, "three");

        // A clear keeps sequence numbers advancing.
        buffer.clear();
        buffer.push("stdout", "four");
        let (a_events, _) = buffer.read_from(Some(a_cursor));
        assert_eq!(a_events.len(), 1);
        assert_eq!(a_events[0].output, "four");

        // A stale cursor from before trimming resumes at the oldest event.
        let (b_events, _) = buffer.read_from(Some(b_cursor));
        assert_eq!(b_events.len(), 1);
        assert_eq!(b_events[0].output, "four");
    }

    #[test]
    fn trimming_by_bounds_advances_the_start_of_the_window() {
        let mut buffer = OutputBuffer::new(2, 1024);
        buffer.push("stdout", "one");
        buffer.push("stdout", "two");
        buffer.push("stdout", "three"); // evicts "one"

        let (events, cursor) = buffer.read_from(Some(0));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].output, "two");
        assert_eq!(cursor, 3);

        // A future cursor clamps to the end instead of panicking.
        assert!(buffer.read_from(Some(99)).0.is_empty());
    }
}
