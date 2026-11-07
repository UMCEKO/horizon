use miniz_oxide::DataFormat;
use miniz_oxide::inflate::stream::{InflateState, inflate};
use miniz_oxide::{MZFlush, MZStatus};

use prost::Message;

use crate::{Error, MAX_FORMAT};

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Field {
    Format(u32),
    Start(horizon_perf_format::Start),
    StringEntry(horizon_perf_format::StringEntry),
    History(horizon_perf_format::HealthWindow),
    SlowHistory(horizon_perf_format::SlowEvent),
    World(horizon_perf_format::WorldSnapshot),
    Mark(horizon_perf_format::Mark),
    Warning(horizon_perf_format::Warning),
    Health(horizon_perf_format::HealthSample),
    CounterDef(horizon_perf_format::CounterDef),
    Counter(horizon_perf_format::CounterSample),
    Slow(horizon_perf_format::SlowEvent),
    SpanBlock(horizon_perf_format::SpanBlock),
    Memory(horizon_perf_format::MemorySnapshot),
    Timeline(horizon_perf_format::TimelineChunk),
    Span(horizon_perf_format::SpanSummary),
    Workers(horizon_perf_format::Workers),
    SampleProfile(horizon_perf_format::SampleProfile),
    Gpu(horizon_perf_format::GpuPasses),
    Net(horizon_perf_format::NetTraffic),
    Bench(horizon_perf_format::BenchResult),
    Attachment(horizon_perf_format::Attachment),
    Stop(horizon_perf_format::Stop),
}

impl Field {
    pub fn side(&self) -> Option<horizon_perf_format::Side> {
        match self {
            Field::Format(_) => None,
            Field::Start(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::StringEntry(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::History(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::SlowHistory(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::World(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::Mark(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::Warning(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::Health(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::CounterDef(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::Counter(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::Slow(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::SpanBlock(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::Memory(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::Timeline(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::Span(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::Workers(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::SampleProfile(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::Gpu(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::Net(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::Bench(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
            Field::Attachment(_) => None,
            Field::Stop(x) => Some(
                horizon_perf_format::Side::try_from(x.side)
                    .unwrap_or(horizon_perf_format::Side::Unspecified),
            ),
        }
    }
}

pub struct FollowReader {
    raw: Vec<u8>,
    header_done: bool,
    deflate_start: usize,
    state: Option<Box<InflateState>>,
    inflated: Vec<u8>,
    cursor: usize,
    format_checked: bool,
}

impl Default for FollowReader {
    fn default() -> Self {
        Self::new()
    }
}

impl FollowReader {
    pub fn new() -> Self {
        Self {
            raw: Vec::new(),
            header_done: false,
            deflate_start: 0,
            state: None,
            inflated: Vec::new(),
            cursor: 0,
            format_checked: false,
        }
    }

    pub fn push(&mut self, data: &[u8]) -> Result<Vec<Field>, Error> {
        self.raw.extend_from_slice(data);
        if !self.header_done {
            if let Some(start) = parse_gzip_header(&self.raw) {
                self.header_done = true;
                self.deflate_start = start;
                self.state = Some(InflateState::new_boxed(DataFormat::Raw));
            } else {
                return Ok(Vec::new());
            }
        }
        let st = self.state.as_mut().unwrap();
        let mut tmp = vec![0u8; 64 * 1024];
        loop {
            let input = &self.raw[self.deflate_start..];
            if input.is_empty() {
                break;
            }
            let res = inflate(st, input, &mut tmp, MZFlush::None);
            let consumed = res.bytes_consumed;
            let written = res.bytes_written;
            if written > 0 {
                self.inflated.extend_from_slice(&tmp[..written]);
            }
            self.deflate_start += consumed;
            if consumed == 0 && written == 0 {
                break;
            }
            match res.status {
                Ok(MZStatus::StreamEnd) => break,
                Ok(_) => {}
                Err(_) => {
                    if consumed == 0 && written == 0 {
                        break;
                    }
                }
            }
        }
        let mut fields = Vec::new();
        loop {
            if self.cursor >= self.inflated.len() {
                break;
            }
            let (tag, n) = match read_varint(&self.inflated[self.cursor..]) {
                Some(v) => v,
                None => break,
            };
            let wire = (tag & 7) as u8;
            let field_no = (tag >> 3) as u32;
            if wire == 0 {
                let pos = self.cursor + n;
                let (val, m) = match read_varint(&self.inflated[pos..]) {
                    Some(v) => v,
                    None => break,
                };
                self.cursor = pos + m;
                if field_no == 1 {
                    if !self.format_checked {
                        if val as u32 != MAX_FORMAT {
                            return Err(Error::Format(val as u32, MAX_FORMAT));
                        }
                        self.format_checked = true;
                    }
                    fields.push(Field::Format(val as u32));
                }
            } else if wire == 2 {
                let pos = self.cursor + n;
                let (len, m) = match read_varint(&self.inflated[pos..]) {
                    Some(v) => v,
                    None => break,
                };
                let len = len as usize;
                let start = pos + m;
                let end = start + len;
                if end > self.inflated.len() {
                    break;
                }
                let payload = &self.inflated[start..end];
                self.cursor = end;
                let f = decode_field(field_no, payload)?;
                fields.push(f);
            } else {
                break;
            }
        }
        Ok(fields)
    }
}

fn decode_field(no: u32, payload: &[u8]) -> Result<Field, Error> {
    macro_rules! dec {
        ($t:ty, $variant:ident) => {{
            let m = <$t>::decode(payload).map_err(Error::Decode)?;
            Ok(Field::$variant(m))
        }};
    }
    match no {
        2 => dec!(horizon_perf_format::Start, Start),
        3 => dec!(horizon_perf_format::StringEntry, StringEntry),
        4 => dec!(horizon_perf_format::HealthWindow, History),
        5 => dec!(horizon_perf_format::SlowEvent, SlowHistory),
        6 => dec!(horizon_perf_format::WorldSnapshot, World),
        7 => dec!(horizon_perf_format::Mark, Mark),
        8 => dec!(horizon_perf_format::Warning, Warning),
        9 => dec!(horizon_perf_format::HealthSample, Health),
        10 => dec!(horizon_perf_format::CounterDef, CounterDef),
        11 => dec!(horizon_perf_format::CounterSample, Counter),
        12 => dec!(horizon_perf_format::SlowEvent, Slow),
        13 => dec!(horizon_perf_format::SpanBlock, SpanBlock),
        14 => dec!(horizon_perf_format::MemorySnapshot, Memory),
        15 => dec!(horizon_perf_format::TimelineChunk, Timeline),
        16 => dec!(horizon_perf_format::SpanSummary, Span),
        17 => dec!(horizon_perf_format::Workers, Workers),
        18 => dec!(horizon_perf_format::SampleProfile, SampleProfile),
        19 => dec!(horizon_perf_format::GpuPasses, Gpu),
        20 => dec!(horizon_perf_format::NetTraffic, Net),
        21 => dec!(horizon_perf_format::BenchResult, Bench),
        22 => dec!(horizon_perf_format::Attachment, Attachment),
        23 => dec!(horizon_perf_format::Stop, Stop),
        _ => Err(Error::Invalid(format!("unknown field {no}"))),
    }
}

fn read_varint(buf: &[u8]) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    let mut i = 0;
    while i < buf.len() && i < 10 {
        let byte = buf[i];
        if i == 9 {
            result |= ((byte & 0x01) as u64) << shift;
            i += 1;
            if byte & 0xfe == 0 {
                return Some((result, i));
            }
            return None;
        }
        result |= ((byte & 0x7f) as u64) << shift;
        i += 1;
        if byte & 0x80 == 0 {
            return Some((result, i));
        }
        shift += 7;
    }
    None
}

fn parse_gzip_header(buf: &[u8]) -> Option<usize> {
    if buf.len() < 10 {
        return None;
    }
    if buf[0] != 0x1f || buf[1] != 0x8b || buf[2] != 8 {
        return None;
    }
    let flags = buf[3];
    let mut pos = 10;
    if flags & 0x04 != 0 {
        if buf.len() < pos + 2 {
            return None;
        }
        let xlen = u16::from_le_bytes([buf[pos], buf[pos + 1]]) as usize;
        pos += 2;
        if buf.len() < pos + xlen {
            return None;
        }
        pos += xlen;
    }
    if flags & 0x08 != 0 {
        while pos < buf.len() && buf[pos] != 0 {
            pos += 1;
        }
        if pos >= buf.len() {
            return None;
        }
        pos += 1;
    }
    if flags & 0x10 != 0 {
        while pos < buf.len() && buf[pos] != 0 {
            pos += 1;
        }
        if pos >= buf.len() {
            return None;
        }
        pos += 1;
    }
    if flags & 0x02 != 0 {
        if buf.len() < pos + 2 {
            return None;
        }
        pos += 2;
    }
    Some(pos)
}
