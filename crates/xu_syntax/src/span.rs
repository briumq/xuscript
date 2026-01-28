//!
//!

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteIndex(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: ByteIndex,
    pub end: ByteIndex,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        Self {
            start: ByteIndex(start),
            end: ByteIndex(end),
        }
    }

    pub fn len(self) -> u32 {
        self.end.0.saturating_sub(self.start.0)
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    pub fn merge(self, other: Span) -> Span {
        let s = self.start.0.min(other.start.0);
        let e = self.end.0.max(other.end.0);
        Span::new(s, e)
    }
}
