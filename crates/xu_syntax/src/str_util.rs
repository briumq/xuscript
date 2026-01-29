pub fn unquote(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
        let inner = &s[1..bytes.len() - 1];
        unescape(inner)
    } else {
        s.to_string()
    }
}

pub fn unescape(s: &str) -> String {
    let mut res = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => res.push('\n'),
                Some('r') => res.push('\r'),
                Some('t') => res.push('\t'),
                Some('\\') => res.push('\\'),
                Some('"') => res.push('"'),
                Some('{') => res.push('{'),
                Some('}') => res.push('}'),
                Some(next) => {
                    res.push('\\');
                    res.push(next);
                }
                None => res.push('\\'),
            }
        } else {
            res.push(c);
        }
    }
    res
}

pub enum InterpolationPiece<'a> {
    Str(String),
    Expr(&'a str),
}

pub struct InterpolationParser<'a> {
    input: &'a str,
}

impl<'a> InterpolationParser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input }
    }

    pub fn parse<F>(&self, mut on_piece: F)
    where
        F: FnMut(InterpolationPiece<'a>),
    {
        let mut buffer = String::new();
        let mut i = 0;
        let bytes = self.input.as_bytes();

        'outer: while i < bytes.len() {
            let c = self.input[i..].chars().next().unwrap();
            i += c.len_utf8();

            if c == '\\' {
                if i < bytes.len() {
                    let next = self.input[i..].chars().next().unwrap();
                    i += next.len_utf8();
                    match next {
                        'n' => buffer.push('\n'),
                        'r' => buffer.push('\r'),
                        't' => buffer.push('\t'),
                        '\\' => buffer.push('\\'),
                        '"' => buffer.push('"'),
                        '{' => buffer.push('{'),
                        '}' => buffer.push('}'),
                        other => {
                            buffer.push('\\');
                            buffer.push(other);
                        }
                    }
                } else {
                    buffer.push('\\');
                }
            } else if c == '{' {
                let mut j = i;
                while j < bytes.len() {
                    let n = self.input[j..].chars().next().unwrap();
                    if n.is_whitespace() {
                        j += n.len_utf8();
                        continue;
                    }
                    if n == '"' {
                        buffer.push('{');
                        continue 'outer;
                    }
                    break;
                }
                if !buffer.is_empty() {
                    on_piece(InterpolationPiece::Str(std::mem::take(&mut buffer)));
                }

                let expr_start = i;
                let mut depth = 1;
                while i < bytes.len() {
                    let n = self.input[i..].chars().next().unwrap();
                    let n_len = n.len_utf8();
                    if n == '{' {
                        depth += 1;
                    } else if n == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    i += n_len;
                }
                let expr_end = i;
                if i < bytes.len() {
                    i += 1; // skip '}'
                }
                on_piece(InterpolationPiece::Expr(&self.input[expr_start..expr_end]));
            } else {
                buffer.push(c);
            }
        }

        if !buffer.is_empty() {
            on_piece(InterpolationPiece::Str(buffer));
        }
    }
}
