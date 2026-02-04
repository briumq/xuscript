use crate::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceId(pub u32);

#[derive(Clone, Debug)]
pub struct SourceText {
    text: String,
    line_starts: Vec<u32>,
}

impl SourceText {
    pub fn new(text: String) -> Self {
        let mut line_starts = Vec::with_capacity(text.len().saturating_div(64).max(32));
        line_starts.push(0u32);
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push((i + 1) as u32);
            }
        }
        Self { text, line_starts }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn slice(&self, span: Span) -> &str {
        let start = span.start.0 as usize;
        let end = span.end.0 as usize;
        &self.text[start..end]
    }

    pub fn line_col(&self, byte: u32) -> (u32, u32) {
        let byte = byte.min(self.text.len() as u32);
        let idx = match self.line_starts.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line = idx as u32;
        let line_start = self.line_starts[idx] as usize;
        let mut target = byte as usize;
        while target > line_start && !self.text.is_char_boundary(target) {
            target = target.saturating_sub(1);
        }
        let col = self.text[line_start..target].chars().count() as u32;
        (line, col)
    }
}

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub id: SourceId,
    pub name: String,
    pub text: SourceText,
}

impl SourceFile {
    pub fn new(id: SourceId, name: impl Into<String>, text: String) -> Self {
        Self {
            id,
            name: name.into(),
            text: SourceText::new(text),
        }
    }
}
