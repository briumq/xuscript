use std::cell::Cell;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::rc::Rc;
use std::str;

const INLINE_CAP: usize = 22;
const CHAR_COUNT_UNKNOWN: u32 = u32::MAX;

#[derive(Clone)]
pub enum Text {
    Inline { len: u8, buf: [u8; INLINE_CAP] },
    Heap { data: Rc<String>, char_count: Cell<u32> },
}

impl Text {
    pub fn new() -> Self {
        Self::Inline {
            len: 0,
            buf: [0u8; INLINE_CAP],
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Text::Inline { len, buf } => {
                let s = &buf[..*len as usize];
                unsafe { str::from_utf8_unchecked(s) }
            }
            Text::Heap { data, .. } => data.as_str(),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Text::Inline { len, .. } => *len as usize,
            Text::Heap { data, .. } => data.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of Unicode characters (not bytes)
    pub fn char_count(&self) -> usize {
        match self {
            Text::Inline { len, buf } => {
                let byte_len = *len as usize;
                // Fast path: if all bytes are ASCII, char count equals byte count
                let s = &buf[..byte_len];
                if s.iter().all(|&b| b < 128) {
                    byte_len
                } else {
                    let s = unsafe { str::from_utf8_unchecked(s) };
                    s.chars().count()
                }
            }
            Text::Heap { data, char_count } => {
                let cached = char_count.get();
                if cached != CHAR_COUNT_UNKNOWN {
                    cached as usize
                } else {
                    let count = data.chars().count() as u32;
                    char_count.set(count);
                    count as usize
                }
            }
        }
    }

    pub fn from_str(s: &str) -> Self {
        if s.len() <= INLINE_CAP {
            let mut buf = [0u8; INLINE_CAP];
            buf[..s.len()].copy_from_slice(s.as_bytes());
            return Self::Inline {
                len: s.len() as u8,
                buf,
            };
        }
        Self::Heap { data: Rc::new(s.to_string()), char_count: Cell::new(CHAR_COUNT_UNKNOWN) }
    }

    pub fn from_string(s: String) -> Self {
        if s.len() <= INLINE_CAP {
            let mut buf = [0u8; INLINE_CAP];
            buf[..s.len()].copy_from_slice(s.as_bytes());
            return Self::Inline {
                len: s.len() as u8,
                buf,
            };
        }
        Self::Heap { data: Rc::new(s), char_count: Cell::new(CHAR_COUNT_UNKNOWN) }
    }

    pub fn into_string(self) -> String {
        match self {
            Text::Inline { len, buf } => {
                let s = &buf[..len as usize];
                let ss = unsafe { str::from_utf8_unchecked(s) };
                ss.to_string()
            }
            Text::Heap { data, .. } => match Rc::try_unwrap(data) {
                Ok(s) => s,
                Err(r) => (*r).clone(),
            },
        }
    }

    pub fn push_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        match self {
            Text::Inline { len, buf } => {
                let cur = *len as usize;
                let new_len = cur + s.len();
                if new_len <= INLINE_CAP {
                    buf[cur..new_len].copy_from_slice(s.as_bytes());
                    *len = new_len as u8;
                    return;
                }
                let mut out = String::with_capacity(new_len);
                out.push_str(unsafe { str::from_utf8_unchecked(&buf[..cur]) });
                out.push_str(s);
                *self = Text::Heap { data: Rc::new(out), char_count: Cell::new(CHAR_COUNT_UNKNOWN) };
            }
            Text::Heap { data, char_count } => {
                let hm = Rc::make_mut(data);
                hm.reserve(s.len());
                hm.push_str(s);
                // Invalidate cached char count
                char_count.set(CHAR_COUNT_UNKNOWN);
            }
        }
    }

    pub fn push_i64(&mut self, i: i64) {
        let mut buf = [0u8; 32];
        let digits = write_i64_to_buf(i, &mut buf);
        let s = unsafe { str::from_utf8_unchecked(digits) };
        self.push_str(s);
    }

    pub fn concat2(a: &Text, b: &Text) -> Text {
        let al = a.len();
        let bl = b.len();
        let total = al + bl;
        if total <= INLINE_CAP {
            let mut buf = [0u8; INLINE_CAP];
            buf[..al].copy_from_slice(a.as_str().as_bytes());
            buf[al..total].copy_from_slice(b.as_str().as_bytes());
            return Text::Inline {
                len: total as u8,
                buf,
            };
        }

        let mut out = String::with_capacity(total);
        out.push_str(a.as_str());
        out.push_str(b.as_str());
        Text::Heap { data: Rc::new(out), char_count: Cell::new(CHAR_COUNT_UNKNOWN) }
    }

    /// Concatenate multiple strings efficiently by pre-calculating total length
    pub fn concat_many(parts: &[&str]) -> Text {
        let total: usize = parts.iter().map(|s| s.len()).sum();
        if total == 0 {
            return Text::new();
        }
        if total <= INLINE_CAP {
            let mut buf = [0u8; INLINE_CAP];
            let mut pos = 0;
            for s in parts {
                buf[pos..pos + s.len()].copy_from_slice(s.as_bytes());
                pos += s.len();
            }
            return Text::Inline {
                len: total as u8,
                buf,
            };
        }
        let mut out = String::with_capacity(total);
        for s in parts {
            out.push_str(s);
        }
        Text::Heap { data: Rc::new(out), char_count: Cell::new(CHAR_COUNT_UNKNOWN) }
    }

    /// Check if the string is ASCII-only (fast path for many operations)
    #[inline]
    pub fn is_ascii(&self) -> bool {
        match self {
            Text::Inline { len, buf } => buf[..*len as usize].iter().all(|&b| b < 128),
            Text::Heap { data, .. } => data.is_ascii(),
        }
    }
}

fn write_i64_to_buf(i: i64, buf: &mut [u8; 32]) -> &[u8] {
    const LUT: &[u8; 200] = b"0001020304050607080910111213141516171819\
2021222324252627282930313233343536373839\
4041424344454647484950515253545556575859\
6061626364656667686970717273747576777879\
8081828384858687888990919293949596979899";

    let mut end = buf.len();
    if i == 0 {
        end -= 1;
        buf[end] = b'0';
        return &buf[end..];
    }
    let neg = i < 0;
    let mut n = if neg {
        i.wrapping_neg() as u64
    } else {
        i as u64
    };

    while n >= 100 {
        let rem = (n % 100) as usize;
        n /= 100;
        end -= 2;
        let idx = rem * 2;
        buf[end] = LUT[idx];
        buf[end + 1] = LUT[idx + 1];
    }
    if n < 10 {
        end -= 1;
        buf[end] = b'0' + n as u8;
    } else {
        let rem = n as usize;
        end -= 2;
        let idx = rem * 2;
        buf[end] = LUT[idx];
        buf[end + 1] = LUT[idx + 1];
    }
    if neg {
        end -= 1;
        buf[end] = b'-';
    }
    &buf[end..]
}

impl Default for Text {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for Text {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Text::Heap { data: a, .. }, Text::Heap { data: b, .. }) => Rc::ptr_eq(a, b) || a.as_str() == b.as_str(),
            (Text::Inline { len: l1, buf: b1 }, Text::Inline { len: l2, buf: b2 }) => {
                l1 == l2 && b1[..*l1 as usize] == b2[..*l2 as usize]
            }
            _ => self.as_str() == other.as_str(),
        }
    }
}

impl Eq for Text {}

impl Hash for Text {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().as_bytes().hash(state);
    }
}

impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for Text {
    fn from(value: &str) -> Self {
        Text::from_str(value)
    }
}

impl From<String> for Text {
    fn from(value: String) -> Self {
        Text::from_string(value)
    }
}

impl AsRef<str> for Text {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Text {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
