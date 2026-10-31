//! Big-endian fields, as Java's `DataInputStream` and `DataOutputStream` read and write them.

#[derive(Default)]
pub struct Out(pub Vec<u8>);

impl Out {
    pub fn long(&mut self, v: i64) {
        self.0.extend(v.to_be_bytes());
    }
    pub fn int(&mut self, v: i32) {
        self.0.extend(v.to_be_bytes());
    }
    pub fn float(&mut self, v: f32) {
        self.0.extend(v.to_be_bytes());
    }
    pub fn double(&mut self, v: f64) {
        self.0.extend(v.to_be_bytes());
    }
    pub fn bool(&mut self, v: bool) {
        self.0.push(v as u8);
    }
    pub fn bytes(&mut self, v: &[u8]) {
        self.int(v.len() as i32);
        self.0.extend(v);
    }
    pub fn text(&mut self, v: &str) {
        self.bytes(v.as_bytes());
    }
}

pub struct In<'a>(pub &'a [u8]);

impl<'a> In<'a> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk()?;
        self.0 = rest;
        Some(*head)
    }
    pub fn int(&mut self) -> Option<i32> {
        self.take().map(i32::from_be_bytes)
    }
    pub fn float(&mut self) -> Option<f32> {
        self.take().map(f32::from_be_bytes)
    }
    pub fn double(&mut self) -> Option<f64> {
        self.take().map(f64::from_be_bytes)
    }
    pub fn bool(&mut self) -> Option<bool> {
        self.take::<1>().map(|b| b[0] != 0)
    }
    pub fn bytes(&mut self) -> Option<&'a [u8]> {
        let len = usize::try_from(self.int()?).ok()?;
        let (head, rest) = self.0.split_at_checked(len)?;
        self.0 = rest;
        Some(head)
    }
    pub fn text(&mut self) -> Option<&'a str> {
        std::str::from_utf8(self.bytes()?).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_round_trip_and_short_input_ends() {
        let mut out = Out::default();
        out.int(-5);
        out.float(1.5);
        out.double(-2.25);
        out.bool(true);
        out.text("minecraft:stone");
        let mut input = In(&out.0);
        assert_eq!(input.int(), Some(-5));
        assert_eq!(input.float(), Some(1.5));
        assert_eq!(input.double(), Some(-2.25));
        assert_eq!(input.bool(), Some(true));
        assert_eq!(input.text(), Some("minecraft:stone"));
        assert_eq!(input.int(), None);
    }
}
