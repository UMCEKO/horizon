use horizon_perf_format::Capture;
use prost::Message;

#[test]
fn encode_decode_roundtrip() {
    let mut c = Capture::default();
    c.format = 1;
    let bytes = c.encode_to_vec();
    let back = Capture::decode(bytes.as_slice()).unwrap();
    assert_eq!(back.format, 1);
}

#[test]
fn unknown_field_number_keeps_known_values() {
    // Append an unknown field number (999, varint) to a valid capture.
    let mut c = Capture::default();
    c.format = 1;
    let mut bytes = c.encode_to_vec();
    // field 999, wire type varint (tag = 999<<3 | 0)
    let tag: u32 = 999 << 3;
    // encode varint tag
    let mut buf = Vec::new();
    let mut v = tag;
    loop {
        let byte = (v & 0x7f) as u8;
        if v < 0x80 {
            buf.push(byte);
            break;
        }
        buf.push(byte | 0x80);
        v >>= 7;
    }
    // value 42 varint
    let mut val = 42u32;
    loop {
        let byte = (val & 0x7f) as u8;
        if val < 0x80 {
            buf.push(byte);
            break;
        }
        buf.push(byte | 0x80);
        val >>= 7;
    }
    bytes.extend_from_slice(&buf);
    let back = Capture::decode(bytes.as_slice()).unwrap();
    assert_eq!(back.format, 1);
}
