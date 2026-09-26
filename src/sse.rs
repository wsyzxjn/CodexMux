//! Incremental Server-Sent Events framing for response capture.
//!
//! Lines end in LF, CRLF, or a lone CR, and a blank line ends an event, as in
//! the WHATWG event stream grammar. The splitter keeps its scan state across
//! chunks, so every byte is examined once and the events it yields do not
//! depend on how the stream was chunked.

use std::borrow::Cow;

use memchr::{memchr, memchr2};

/// Largest pending event the splitter buffers. An event that grows past it
/// is skipped whole, so a stream can never grow the buffer without bound.
pub const MAX_EVENT_BYTES: usize = 16 * 1024 * 1024;
const PENDING_CAPACITY_KEPT: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingLf {
    No,
    /// The previous CR ended a line inside the event.
    InEvent,
    /// The previous CR was a blank line; the LF belongs to no event.
    AfterBoundary,
}

#[derive(Debug)]
pub struct EventSplitter {
    /// Bytes of the unfinished event that arrived in earlier chunks.
    pending: Vec<u8>,
    /// The previous byte ended a line, so a terminator now is a blank line.
    at_line_start: bool,
    /// The previous terminator was a CR, so an LF right after it is part of it.
    pending_lf: PendingLf,
    /// The unfinished event outgrew the limit; it is dropped at its end.
    discarding: bool,
    max_event_bytes: usize,
}

impl Default for EventSplitter {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSplitter {
    pub fn new() -> Self {
        Self::with_max_event_bytes(MAX_EVENT_BYTES)
    }

    pub fn with_max_event_bytes(max_event_bytes: usize) -> Self {
        Self {
            pending: Vec::new(),
            at_line_start: true,
            pending_lf: PendingLf::No,
            discarding: false,
            max_event_bytes,
        }
    }

    /// Append `chunk` and hand every event it completes to `on_event`, in
    /// order. An event's bytes include its lines' terminators but not the
    /// blank line that ends it.
    pub fn push(&mut self, chunk: &[u8], mut on_event: impl FnMut(&[u8])) {
        let mut start = 0;
        let mut position = 0;
        while position < chunk.len() {
            let pending_lf = std::mem::replace(&mut self.pending_lf, PendingLf::No);
            if pending_lf != PendingLf::No && chunk[position] == b'\n' {
                position += 1;
                if pending_lf == PendingLf::AfterBoundary {
                    start = position;
                }
                continue;
            }
            let Some(offset) = memchr2(b'\r', b'\n', &chunk[position..]) else {
                self.at_line_start = false;
                break;
            };
            if offset > 0 {
                self.at_line_start = false;
            }
            let terminator = position + offset;
            let carriage_return = chunk[terminator] == b'\r';
            if self.at_line_start {
                self.dispatch(&chunk[start..terminator], &mut on_event);
                start = terminator + 1;
                if carriage_return {
                    self.pending_lf = PendingLf::AfterBoundary;
                }
            } else {
                self.at_line_start = true;
                if carriage_return {
                    self.pending_lf = PendingLf::InEvent;
                }
            }
            position = terminator + 1;
        }
        self.keep(&chunk[start..]);
    }

    /// End of stream. An event the stream left without its blank line is
    /// still handed to `on_event`: upstreams may close right after their last
    /// event, and the capture must not lose it. The splitter is then reset.
    pub fn finish(&mut self, mut on_event: impl FnMut(&[u8])) {
        let pending = std::mem::take(&mut self.pending);
        let discarding = self.discarding;
        *self = Self::with_max_event_bytes(self.max_event_bytes);
        if !discarding && !pending.is_empty() {
            on_event(&pending);
        }
    }

    fn dispatch(&mut self, tail: &[u8], on_event: &mut impl FnMut(&[u8])) {
        if std::mem::take(&mut self.discarding) {
            return;
        }
        if self.pending.len() + tail.len() > self.max_event_bytes {
            self.pending = Vec::new();
            return;
        }
        if self.pending.is_empty() {
            if !tail.is_empty() {
                on_event(tail);
            }
            return;
        }
        self.pending.extend_from_slice(tail);
        on_event(&self.pending);
        self.pending.clear();
        // Do not hold on to the allocation of one large event.
        self.pending.shrink_to(PENDING_CAPACITY_KEPT);
    }

    fn keep(&mut self, rest: &[u8]) {
        if self.discarding || rest.is_empty() {
            return;
        }
        if self.pending.len() + rest.len() > self.max_event_bytes {
            self.pending = Vec::new();
            self.discarding = true;
            return;
        }
        self.pending.extend_from_slice(rest);
    }
}

/// The event's `data`: every `data` line's value joined with LF. One
/// leading space is removed from each value, comment lines and other fields
/// are ignored, and `None` means the event carries no `data` line.
pub fn data(event: &[u8]) -> Option<Cow<'_, [u8]>> {
    let mut joined: Option<Cow<'_, [u8]>> = None;
    for line in Lines(event) {
        let Some(value) = data_value(line) else {
            continue;
        };
        match &mut joined {
            None => joined = Some(Cow::Borrowed(value)),
            Some(data) => {
                let data = data.to_mut();
                data.push(b'\n');
                data.extend_from_slice(value);
            }
        }
    }
    joined
}

fn data_value(line: &[u8]) -> Option<&[u8]> {
    let (field, value) = match memchr(b':', line) {
        Some(0) => return None,
        Some(colon) => (&line[..colon], &line[colon + 1..]),
        None => (line, &line[line.len()..]),
    };
    (field == b"data").then(|| value.strip_prefix(b" ").unwrap_or(value))
}

/// Lines of one event, split at LF, CRLF, or a lone CR.
struct Lines<'a>(&'a [u8]);

impl<'a> Iterator for Lines<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        if self.0.is_empty() {
            return None;
        }
        let Some(end) = memchr2(b'\r', b'\n', self.0) else {
            return Some(std::mem::take(&mut self.0));
        };
        let line = &self.0[..end];
        let crlf = self.0[end] == b'\r' && self.0.get(end + 1) == Some(&b'\n');
        self.0 = &self.0[end + if crlf { 2 } else { 1 }..];
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split_all(chunks: &[&[u8]], max_event_bytes: usize) -> Vec<Vec<u8>> {
        let mut splitter = EventSplitter::with_max_event_bytes(max_event_bytes);
        let mut events = Vec::new();
        for chunk in chunks {
            splitter.push(chunk, |event| events.push(event.to_vec()));
        }
        splitter.finish(|event| events.push(event.to_vec()));
        events
    }

    fn split(input: &[u8]) -> Vec<Vec<u8>> {
        split_all(&[input], MAX_EVENT_BYTES)
    }

    fn data_strings(events: &[Vec<u8>]) -> Vec<String> {
        events
            .iter()
            .filter_map(|event| data(event))
            .map(|data| String::from_utf8(data.into_owned()).unwrap())
            .collect()
    }

    const INPUTS: &[&[u8]] = &[
        b"event: a\ndata: {\"city\":\"\xe6\x9d\xad\xe5\xb7\x9e\"}\n\ndata: two\ndata: lines\n\n",
        b"event: a\r\ndata: {\"city\":\"\xe6\x9d\xad\xe5\xb7\x9e\"}\r\n\r\ndata: two\r\ndata: lines\r\n\r\n",
        b"event: a\rdata: {\"city\":\"\xe6\x9d\xad\xe5\xb7\x9e\"}\r\rdata: two\rdata: lines\r\r",
        b": keep-alive\r\n\ndata: \xf0\x9f\x8c\xa4\r\r\ndata: last\n\r\n",
    ];

    #[test]
    fn every_line_ending_frames_the_same_events() {
        let expected = vec!["{\"city\":\"杭州\"}".to_owned(), "two\nlines".to_owned()];
        for input in &INPUTS[..3] {
            assert_eq!(data_strings(&split(input)), expected);
        }
        assert_eq!(data_strings(&split(INPUTS[3])), ["🌤", "last"]);
        assert_eq!(split(INPUTS[3]).len(), 3, "the comment is its own event");
    }

    /// The events must not depend on where the transport cut the stream,
    /// including between a CR and its LF and inside a UTF-8 sequence.
    #[test]
    fn events_do_not_depend_on_chunk_boundaries() {
        for input in INPUTS {
            let whole = split(input);
            for cut in 0..=input.len() {
                let (left, right) = input.split_at(cut);
                assert_eq!(
                    split_all(&[left, right], MAX_EVENT_BYTES),
                    whole,
                    "cut {cut}"
                );
            }
            let bytes: Vec<&[u8]> = input.chunks(1).collect();
            assert_eq!(split_all(&bytes, MAX_EVENT_BYTES), whole);
        }
    }

    #[test]
    fn a_lone_cr_boundary_dispatches_immediately_and_owns_the_next_lf() {
        let mut splitter = EventSplitter::new();
        let mut events = Vec::new();
        splitter.push(b"data: first\r\r", |event| events.push(event.to_vec()));
        assert_eq!(events, [b"data: first\r".to_vec()]);
        splitter.push(b"\ndata: second\n\n", |event| events.push(event.to_vec()));
        assert_eq!(events[1], b"data: second\n");
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn finish_returns_an_event_that_lost_its_blank_line() {
        for tail in [&b""[..], b"\n", b"\r\n", b"\r"] {
            let input = [b"data: done".as_slice(), tail].concat();
            let events = split(&input);
            assert_eq!(data_strings(&events), ["done"], "tail {tail:?}");
        }
        let mut splitter = EventSplitter::new();
        let mut events = 0;
        splitter.push(b"data: done\n\n", |_| events += 1);
        splitter.finish(|_| events += 1);
        assert_eq!(events, 1, "a stream that ended on a boundary has no tail");
    }

    #[test]
    fn oversized_events_are_skipped_whole_without_growing_the_buffer() {
        let big = [b"data: ".as_slice(), &[b'x'; 64], b"\n"].concat();
        let mut splitter = EventSplitter::with_max_event_bytes(32);
        let mut events = Vec::new();
        for chunk in big.chunks(8) {
            splitter.push(chunk, |event| events.push(event.to_vec()));
            assert!(splitter.pending.len() <= 32);
        }
        splitter.push(b"\ndata: next\n\n", |event| events.push(event.to_vec()));
        assert_eq!(events, [b"data: next\n".to_vec()]);

        // A complete oversized event in one chunk is skipped the same way.
        let whole = [big.as_slice(), b"\ndata: after\n\n"].concat();
        assert_eq!(data_strings(&split_all(&[&whole], 32)), ["after"]);

        // An oversized tail is never returned at the end of the stream.
        let mut splitter = EventSplitter::with_max_event_bytes(32);
        splitter.push(&big, |_| panic!("no complete event"));
        splitter.finish(|_| panic!("the oversized tail was discarded"));
    }

    #[test]
    fn data_follows_the_event_stream_field_rules() {
        assert_eq!(
            data(b"data: a\ndata:b\ndata:  c\n").unwrap().as_ref(),
            b"a\nb\n c"
        );
        assert_eq!(
            data(b": comment\nevent: x\nid: 1\nretry: 5\ndata: kept\n")
                .unwrap()
                .as_ref(),
            b"kept"
        );
        assert!(data(b"event: ping\n: comment\n").is_none());
        assert_eq!(data(b"data\n").unwrap().as_ref(), b"");
        assert!(matches!(data(b"data: one\n"), Some(Cow::Borrowed(b"one"))));
        assert_eq!(
            data(b"data: a\r\ndata: b\rdata: c").unwrap().as_ref(),
            b"a\nb\nc"
        );
    }
}
