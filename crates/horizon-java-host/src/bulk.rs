//! #262: the bounded typed bulk envelope that carries many items of one op through one crossing (Java's `Bulk`).

use crate::wire::{In, Out};
use std::fmt;

/// The envelope's layout version; Java rejects any other, as the host jar ships with the game.
pub const VERSION: u8 = 1;
/// Most items one envelope carries.
pub const MAX_ITEMS: usize = 256;
/// Most bytes one request envelope takes, header included.
pub const MAX_BYTES: usize = 4096;
/// Request header: version, op, work budget in µs, item count.
pub const HEADER: usize = 1 + 1 + 4 + 4;
/// Reply header: version, outcome, items processed.
pub const REPLY_HEADER: usize = 1 + 1 + 4;

/// What one envelope does to each of its items; every op has a fixed context and record size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    /// Context: level key (int), game time (long). Record: packed block position (long).
    BlockEntityTick = 1,
}

impl Op {
    pub fn from_u8(id: u8) -> Option<Op> {
        match id {
            1 => Some(Op::BlockEntityTick),
            _ => None,
        }
    }

    pub const fn context_bytes(self) -> usize {
        match self {
            Op::BlockEntityTick => 4 + 8,
        }
    }

    pub const fn record_bytes(self) -> usize {
        match self {
            Op::BlockEntityTick => 8,
        }
    }

    /// Most items of this op under both the count and the byte bound.
    pub const fn max_items(self) -> usize {
        let by_bytes = (MAX_BYTES - HEADER - self.context_bytes()) / self.record_bytes();
        if by_bytes < MAX_ITEMS {
            by_bytes
        } else {
            MAX_ITEMS
        }
    }
}

/// How one item ended; items past `processed` never ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Status {
    /// Java held nothing for the item; nothing ran.
    Gone = 0,
    /// The item ran; its result follows.
    Done = 1,
    /// The item threw: its own effects are dropped, those before it stay, and the run ends there.
    Threw = 2,
}

impl Status {
    fn from_u8(id: u8) -> Option<Status> {
        match id {
            0 => Some(Status::Gone),
            1 => Some(Status::Done),
            2 => Some(Status::Threw),
            _ => None,
        }
    }
}

const OK: u8 = 0;
const REJECTED: u8 = 1;

/// Why an envelope or its reply is malformed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Version(u8),
    Op(u8),
    Short,
    Trailing(usize),
    Count(i64),
    Budget(i64),
    Bytes(usize),
    Status(u8),
    Length(i64),
    Processed(i64),
    Outcome(u8),
    Context(usize),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Version(v) => write!(f, "unknown bulk version {v}"),
            Error::Op(o) => write!(f, "unknown or unexpected bulk op {o}"),
            Error::Short => write!(f, "bulk envelope ends early"),
            Error::Trailing(n) => write!(f, "{n} bytes past the bulk envelope's end"),
            Error::Count(n) => write!(f, "bulk item count {n} out of bounds"),
            Error::Budget(n) => write!(f, "bulk work budget {n} µs out of bounds"),
            Error::Bytes(n) => write!(f, "bulk envelope of {n} bytes over {MAX_BYTES}"),
            Error::Status(s) => write!(f, "unknown bulk item status {s}"),
            Error::Length(n) => write!(f, "bulk item result length {n} out of bounds"),
            Error::Processed(n) => write!(f, "bulk reply says {n} items processed"),
            Error::Outcome(o) => write!(f, "unknown bulk reply outcome {o}"),
            Error::Context(n) => write!(f, "bulk context of {n} bytes, not the op's size"),
        }
    }
}

/// Writes a whole envelope into `out` (a per-depth scratch buffer); nothing is written when it is out of bounds.
pub fn write<T>(
    out: &mut Out,
    op: Op,
    budget_us: u32,
    context: impl FnOnce(&mut Out),
    records: &[T],
    record: impl Fn(&mut Out, &T),
) -> Result<(), Error> {
    if records.len() > op.max_items() {
        return Err(Error::Count(records.len() as i64));
    }
    if budget_us == 0 || budget_us > i32::MAX as u32 {
        return Err(Error::Budget(budget_us.into()));
    }
    let start = out.0.len();
    out.0.push(VERSION);
    out.0.push(op as u8);
    out.int(budget_us as i32);
    out.int(records.len() as i32);
    let at = out.0.len();
    context(out);
    if out.0.len() - at != op.context_bytes() {
        let n = out.0.len() - at;
        out.0.truncate(start);
        return Err(Error::Context(n));
    }
    for r in records {
        let at = out.0.len();
        record(out, r);
        let n = out.0.len() - at;
        if n != op.record_bytes() {
            out.0.truncate(start);
            return Err(Error::Length(n as i64));
        }
    }
    Ok(())
}

/// [`write`] for block-entity ticks: the level's key and game time shared, one packed position per item.
pub fn block_entity_tick(
    out: &mut Out,
    budget_us: u32,
    level: u32,
    game_time: i64,
    positions: &[i64],
) -> Result<(), Error> {
    write(
        out,
        Op::BlockEntityTick,
        budget_us,
        |out| {
            out.int(level as i32);
            out.long(game_time);
        },
        positions,
        |out, &pos| out.long(pos),
    )
}

/// A request envelope that passed every check: its context and records are exactly the op's sizes.
#[derive(Debug, PartialEq, Eq)]
pub struct Request<'a> {
    pub op: Op,
    pub budget_us: u32,
    pub context: &'a [u8],
    pub records: &'a [u8],
}

impl<'a> Request<'a> {
    pub fn count(&self) -> usize {
        self.records.len() / self.op.record_bytes()
    }

    pub fn record(&self, i: usize) -> In<'a> {
        let size = self.op.record_bytes();
        In(&self.records[i * size..(i + 1) * size])
    }
}

/// Checks a whole request envelope as Java does before any item runs.
pub fn read(bytes: &[u8]) -> Result<Request<'_>, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Bytes(bytes.len()));
    }
    let mut input = In(bytes);
    let version = input.byte().ok_or(Error::Short)?;
    if version != VERSION {
        return Err(Error::Version(version));
    }
    let op = input.byte().ok_or(Error::Short)?;
    let op = Op::from_u8(op).ok_or(Error::Op(op))?;
    let budget = input.int().ok_or(Error::Short)?;
    if budget <= 0 {
        return Err(Error::Budget(budget.into()));
    }
    let count = input.int().ok_or(Error::Short)?;
    if count < 0 || count as usize > op.max_items() {
        return Err(Error::Count(count.into()));
    }
    let need = op.context_bytes() + count as usize * op.record_bytes();
    let rest = input.0;
    if rest.len() < need {
        return Err(Error::Short);
    }
    if rest.len() > need {
        return Err(Error::Trailing(rest.len() - need));
    }
    let (context, records) = rest.split_at(op.context_bytes());
    Ok(Request {
        op,
        budget_us: budget as u32,
        context,
        records,
    })
}

/// A reply that passed every check; its items are read in order.
#[derive(Debug)]
pub struct Reply<'a> {
    /// Java refused the whole envelope before any item ran.
    pub rejected: bool,
    pub processed: usize,
    items: &'a [u8],
}

impl<'a> Reply<'a> {
    /// The processed items in order: each status and the bytes of its result.
    pub fn items(&self) -> impl Iterator<Item = (Status, In<'a>)> + 'a {
        let mut input = In(self.items);
        std::iter::from_fn(move || {
            let status = Status::from_u8(input.byte()?)?;
            Some((status, In(input.bytes()?)))
        })
    }
}

/// Checks a whole reply to an envelope of `sent` items before any of its results is used.
pub fn read_reply(bytes: &[u8], sent: usize) -> Result<Reply<'_>, Error> {
    let mut input = In(bytes);
    let version = input.byte().ok_or(Error::Short)?;
    if version != VERSION {
        return Err(Error::Version(version));
    }
    let outcome = input.byte().ok_or(Error::Short)?;
    let processed = input.int().ok_or(Error::Short)?;
    match outcome {
        OK => {}
        REJECTED if processed == 0 && input.0.is_empty() => {
            return Ok(Reply {
                rejected: true,
                processed: 0,
                items: &[],
            });
        }
        REJECTED => return Err(Error::Processed(processed.into())),
        other => return Err(Error::Outcome(other)),
    }
    if processed < 0 || processed as usize > sent {
        return Err(Error::Processed(processed.into()));
    }
    let items = input.0;
    for i in 0..processed {
        let status = input.byte().ok_or(Error::Short)?;
        let status = Status::from_u8(status).ok_or(Error::Status(status))?;
        let len = input.int().ok_or(Error::Short)?;
        if len < 0 || len as usize > input.0.len() {
            return Err(Error::Length(len.into()));
        }
        if status == Status::Threw && (len != 0 || i + 1 != processed) {
            return Err(Error::Length(len.into()));
        }
        input.0 = &input.0[len as usize..];
    }
    if !input.0.is_empty() {
        return Err(Error::Trailing(input.0.len()));
    }
    Ok(Reply {
        rejected: false,
        processed: processed as usize,
        items,
    })
}

/// Writes a reply as Java's `Bulk.Reply` does, for tests and fixtures.
pub fn write_reply(out: &mut Out, items: &[(Status, &[u8])]) {
    out.0.push(VERSION);
    out.0.push(OK);
    out.int(items.len() as i32);
    for (status, bytes) in items {
        out.0.push(*status as u8);
        out.bytes(bytes);
    }
}

/// Writes the reply Java gives an envelope it refused.
pub fn write_rejected(out: &mut Out) {
    out.0.push(VERSION);
    out.0.push(REJECTED);
    out.int(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(positions: &[i64]) -> Vec<u8> {
        let mut out = Out::default();
        block_entity_tick(&mut out, 300, 2, 1234, positions).unwrap();
        out.0
    }

    #[test]
    fn an_empty_envelope_carries_only_its_context() {
        let bytes = envelope(&[]);
        assert_eq!(bytes.len(), HEADER + 12);
        let request = read(&bytes).unwrap();
        assert_eq!(request.count(), 0);
        let mut context = In(request.context);
        assert_eq!((context.int(), context.long()), (Some(2), Some(1234)));
    }

    #[test]
    fn the_layout_is_the_one_bulkcheck_reads() {
        let expected: [u8; 30] = [
            1, 1, 0, 0, 1, 44, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 4, 0xd2, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xf9,
        ];
        assert_eq!(envelope(&[-7]), expected);
    }

    #[test]
    fn a_single_and_a_full_envelope_keep_their_records_in_order() {
        let single = envelope(&[-7]);
        let request = read(&single).unwrap();
        assert_eq!(
            (request.op, request.budget_us, request.count()),
            (Op::BlockEntityTick, 300, 1)
        );
        assert_eq!(request.record(0).long(), Some(-7));
        let many: Vec<i64> = (0..Op::BlockEntityTick.max_items() as i64)
            .map(|i| i * 31 - 9)
            .collect();
        let bytes = envelope(&many);
        assert!(bytes.len() <= MAX_BYTES);
        let request = read(&bytes).unwrap();
        let back: Vec<i64> = (0..request.count())
            .map(|i| request.record(i).long().unwrap())
            .collect();
        assert_eq!(back, many);
    }

    #[test]
    fn the_item_bound_follows_the_byte_bound() {
        // 4096 bytes less a 10-byte header and 12 bytes of context leave room for 509 positions, over the 256 item cap.
        assert_eq!(Op::BlockEntityTick.max_items(), 256);
        let too_many = vec![0i64; 257];
        let mut out = Out::default();
        assert_eq!(
            block_entity_tick(&mut out, 300, 0, 0, &too_many),
            Err(Error::Count(257))
        );
        assert!(
            out.0.is_empty(),
            "nothing is written for a refused envelope"
        );
    }

    #[test]
    fn a_zero_budget_is_refused() {
        let mut out = Out::default();
        assert_eq!(
            block_entity_tick(&mut out, 0, 0, 0, &[1]),
            Err(Error::Budget(0))
        );
    }

    #[test]
    fn a_context_or_record_of_the_wrong_size_writes_nothing() {
        let mut out = Out::default();
        out.int(99);
        let wrong = write(
            &mut out,
            Op::BlockEntityTick,
            1,
            |o| o.int(0),
            &[1i64],
            |o, &p| o.long(p),
        );
        assert_eq!(wrong, Err(Error::Context(4)));
        assert_eq!(out.0, 99i32.to_be_bytes(), "what was there before stays");
        let wrong = write(
            &mut out,
            Op::BlockEntityTick,
            1,
            |o| {
                o.int(0);
                o.long(0)
            },
            &[1i32],
            |o, &p| o.int(p),
        );
        assert_eq!(wrong, Err(Error::Length(4)));
        assert_eq!(out.0, 99i32.to_be_bytes());
    }

    #[test]
    fn unknown_versions_and_ops_are_refused() {
        let mut bytes = envelope(&[1]);
        bytes[0] = 2;
        assert_eq!(read(&bytes), Err(Error::Version(2)));
        let mut bytes = envelope(&[1]);
        bytes[1] = 0;
        assert_eq!(read(&bytes), Err(Error::Op(0)));
        bytes[1] = 200;
        assert_eq!(read(&bytes), Err(Error::Op(200)));
    }

    #[test]
    fn lengths_that_disagree_with_the_count_are_refused() {
        let bytes = envelope(&[1, 2]);
        assert_eq!(read(&bytes[..bytes.len() - 1]), Err(Error::Short));
        let mut long = bytes.clone();
        long.push(0);
        assert_eq!(read(&long), Err(Error::Trailing(1)));
        let mut negative = bytes.clone();
        negative[6..10].copy_from_slice(&(-1i32).to_be_bytes());
        assert_eq!(read(&negative), Err(Error::Count(-1)));
        let mut huge = bytes.clone();
        huge[6..10].copy_from_slice(&i32::MAX.to_be_bytes());
        assert_eq!(read(&huge), Err(Error::Count(i32::MAX.into())));
        let mut bad_budget = bytes;
        bad_budget[2..6].copy_from_slice(&(-5i32).to_be_bytes());
        assert_eq!(read(&bad_budget), Err(Error::Budget(-5)));
        assert_eq!(read(&[VERSION]), Err(Error::Short));
        assert_eq!(read(&[]), Err(Error::Short));
    }

    #[test]
    fn an_oversized_envelope_is_refused_before_it_is_parsed() {
        let bytes = vec![0u8; MAX_BYTES + 1];
        assert_eq!(read(&bytes), Err(Error::Bytes(MAX_BYTES + 1)));
    }

    #[test]
    fn a_reply_gives_each_processed_item_once_in_order() {
        let mut out = Out::default();
        write_reply(
            &mut out,
            &[
                (Status::Done, &[1, 2]),
                (Status::Gone, &[]),
                (Status::Done, &[3]),
            ],
        );
        let reply = read_reply(&out.0, 3).unwrap();
        assert!(!reply.rejected);
        let items: Vec<(Status, Vec<u8>)> = reply.items().map(|(s, r)| (s, r.0.to_vec())).collect();
        assert_eq!(
            items,
            [
                (Status::Done, vec![1, 2]),
                (Status::Gone, vec![]),
                (Status::Done, vec![3])
            ]
        );
    }

    #[test]
    fn a_partial_reply_stops_at_the_item_that_threw() {
        let mut out = Out::default();
        write_reply(&mut out, &[(Status::Done, &[7]), (Status::Threw, &[])]);
        let reply = read_reply(&out.0, 5).unwrap();
        assert_eq!(reply.processed, 2);
        let statuses: Vec<Status> = reply.items().map(|(s, _)| s).collect();
        assert_eq!(statuses, [Status::Done, Status::Threw]);
        let mut after = Out::default();
        write_reply(&mut after, &[(Status::Threw, &[]), (Status::Done, &[7])]);
        assert!(
            read_reply(&after.0, 5).is_err(),
            "nothing runs after an item that threw"
        );
        let mut with_effects = Out::default();
        write_reply(&mut with_effects, &[(Status::Threw, &[1])]);
        assert!(
            read_reply(&with_effects.0, 1).is_err(),
            "an item that threw keeps no effects"
        );
    }

    #[test]
    fn a_reply_of_more_items_than_sent_is_refused() {
        let mut out = Out::default();
        write_reply(&mut out, &[(Status::Done, &[]), (Status::Done, &[])]);
        assert_eq!(read_reply(&out.0, 1).unwrap_err(), Error::Processed(2));
    }

    #[test]
    fn malformed_replies_are_refused_whole() {
        let mut out = Out::default();
        write_reply(&mut out, &[(Status::Done, &[1, 2, 3])]);
        let bytes = out.0;
        assert_eq!(
            read_reply(&bytes[..bytes.len() - 1], 1).unwrap_err(),
            Error::Length(3)
        );
        let mut extra = bytes.clone();
        extra.push(9);
        assert_eq!(read_reply(&extra, 1).unwrap_err(), Error::Trailing(1));
        let mut status = bytes.clone();
        status[REPLY_HEADER] = 7;
        assert_eq!(read_reply(&status, 1).unwrap_err(), Error::Status(7));
        let mut version = bytes.clone();
        version[0] = 9;
        assert_eq!(read_reply(&version, 1).unwrap_err(), Error::Version(9));
        let mut outcome = bytes;
        outcome[1] = 5;
        assert_eq!(read_reply(&outcome, 1).unwrap_err(), Error::Outcome(5));
        assert_eq!(read_reply(&[], 1).unwrap_err(), Error::Short);
    }

    #[test]
    fn a_rejected_envelope_ran_nothing() {
        let mut out = Out::default();
        write_rejected(&mut out);
        let reply = read_reply(&out.0, 4).unwrap();
        assert!(reply.rejected);
        assert_eq!((reply.processed, reply.items().count()), (0, 0));
    }

    #[test]
    fn an_empty_reply_processed_nothing() {
        let mut out = Out::default();
        write_reply(&mut out, &[]);
        let reply = read_reply(&out.0, 0).unwrap();
        assert_eq!((reply.rejected, reply.processed), (false, 0));
    }

    #[test]
    fn nested_envelopes_in_two_buffers_stay_apart() {
        // A callback during the outer run writes its own envelope at the next depth; the outer bytes are untouched.
        let outer = envelope(&[1, 2, 3]);
        let mut depth = [Out(outer.clone()), Out::default()];
        block_entity_tick(&mut depth[1], 50, 1, 99, &[42]).unwrap();
        let inner = read(&depth[1].0).unwrap();
        assert_eq!((inner.count(), inner.record(0).long()), (1, Some(42)));
        assert_eq!(depth[0].0, outer);
        let outer = read(&depth[0].0).unwrap();
        assert_eq!(
            (0..3)
                .map(|i| outer.record(i).long().unwrap())
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
    }
}
