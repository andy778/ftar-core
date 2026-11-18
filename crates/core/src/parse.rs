//! Readers for the OpenFTA file formats.
//!
//! `.fta`: line 1 = database name (or `NULL`); then a header record
//! `S NULL 0` followed by the note (`<len> <text>`); then the tree in
//! pre-order. Each node is `<TYPE> <ID|NULL> <nchildren>`; intermediate
//! events are followed by `<len> <text>`.
//!
//! `.ped`: one event per line, `id;group;<type><dormant>;description;prob;lambda;`
//! where the NUL byte stands for "no value".

use crate::error::{Error, Result};
use crate::model::{EventType, Node, PrimaryEvent, PrimaryEventDb, Tree, MAX_DEPTH};

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, i: 0 }
    }
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.i += 1;
        }
    }
    fn token(&mut self) -> Result<String> {
        self.skip_ws();
        let s = self.i;
        while self.peek().is_some_and(|c| !c.is_ascii_whitespace()) {
            self.i += 1;
        }
        if s == self.i {
            return Err(Error::Parse("unexpected end of file".into()));
        }
        Ok(String::from_utf8_lossy(&self.b[s..self.i]).into_owned())
    }
    fn int(&mut self) -> Result<usize> {
        let t = self.token()?;
        t.parse()
            .map_err(|_| Error::Parse(format!("expected number, found `{t}`")))
    }
    fn line(&mut self) -> String {
        let s = self.i;
        while self.peek().is_some_and(|c| c != b'\n') {
            self.i += 1;
        }
        let l = String::from_utf8_lossy(&self.b[s..self.i]).into_owned();
        if self.peek().is_some() {
            self.i += 1;
        }
        l.trim_end_matches('\r').to_string()
    }
    /// Exactly `n` characters, counting CRLF as one (as the original did in Windows text mode).
    fn chars(&mut self, n: usize) -> String {
        let mut out = Vec::new();
        let mut count = 0;
        while count < n {
            match self.peek() {
                None => break,
                Some(b'\r') if self.b.get(self.i + 1) == Some(&b'\n') => {
                    self.i += 1;
                }
                Some(c) => {
                    out.push(c);
                    self.i += 1;
                    count += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }
    /// Up to `n` characters, stopping at end of line (matches `fgets`).
    fn text_line(&mut self, n: usize) -> String {
        let mut out = Vec::new();
        while out.len() < n {
            match self.peek() {
                None | Some(b'\n') | Some(b'\r') => break,
                Some(c) => {
                    out.push(c);
                    self.i += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}

pub fn parse_fta(input: &[u8]) -> Result<Tree> {
    let mut c = Cursor::new(input);
    let db = c.line();
    let db_name = (db != "NULL" && !db.is_empty()).then_some(db);

    // header record "S NULL <n>"
    let _ = c.token()?;
    let _ = c.token()?;
    let _ = c.int()?;
    let note_len = c.int()?;
    let note = if note_len > 0 {
        c.i += 1; // field separator
        c.chars(note_len)
    } else {
        String::new()
    };

    let root = parse_node(&mut c, 1)?;
    Ok(Tree {
        db_name,
        note,
        root,
    })
}

fn parse_node(c: &mut Cursor, depth: usize) -> Result<Node> {
    if depth > MAX_DEPTH {
        return Err(Error::Parse(format!(
            "tree is nested more than {MAX_DEPTH} levels deep"
        )));
    }
    let ty = c.token()?;
    let mut chars = ty.chars();
    let code = chars.next().unwrap();
    let kind = EventType::from_code(code)
        .ok_or_else(|| Error::Parse(format!("unknown event type `{code}`")))?;
    let modifier = chars.next().filter(|m| *m != '\0');
    let id = c.token()?;
    let id = if id == "NULL" { String::new() } else { id };
    let n = c.int()?;

    let mut text = String::new();
    if kind == EventType::Intermediate {
        let len = c.int()?;
        if len > 0 {
            c.i += 1; // separator
            text = c.text_line(len);
        }
    }

    let mut children = Vec::new();
    if !matches!(
        kind,
        EventType::Basic | EventType::TransferIn | EventType::TransferOut
    ) {
        for _ in 0..n {
            children.push(parse_node(c, depth + 1)?);
        }
    }
    Ok(Node {
        kind,
        modifier,
        id,
        text,
        children,
    })
}

pub fn parse_ped(input: &[u8]) -> Result<PrimaryEventDb> {
    let mut db = PrimaryEventDb::default();
    let text = String::from_utf8_lossy(input);
    for (ln, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split(';').collect();
        if f.len() < 5 {
            return Err(Error::Parse(format!(
                "ped line {}: expected at least 5 fields",
                ln + 1
            )));
        }
        let mut tc = f[2].chars();
        let code = tc.next().unwrap_or('B');
        let kind = EventType::from_code(code)
            .ok_or_else(|| Error::Parse(format!("ped line {}: bad type `{code}`", ln + 1)))?;
        let modifier = tc.next().filter(|m| *m != '\0');
        let probability: f64 = f[4].trim().parse().map_err(|_| {
            Error::Parse(format!("ped line {}: bad probability `{}`", ln + 1, f[4]))
        })?;
        let lambda = f
            .get(5)
            .and_then(|s| s.chars().next())
            .filter(|m| *m != '\0');
        let ev = PrimaryEvent {
            id: f[0].to_string(),
            group: f[1].to_string(),
            kind,
            modifier,
            description: f[3].to_string(),
            probability,
            lambda,
        };
        db.events.insert(ev.id.clone(), ev);
    }
    Ok(db)
}
