//! Reading vector PDF pages: paths become HouseCraft CAD polylines and
//! simple text becomes notes, at a chosen drawing scale. Embedded raster
//! images and text in CID (two-byte) fonts are skipped and counted.
//!
//! This is a small reader written from the PDF format's public structure:
//! numbered objects (also inside object streams), the catalog's page tree,
//! Flate-compressed content streams and form XObjects.

use std::collections::BTreeMap;
use std::io::Read;

use housecraft_model::Shape;
use housecraft_units::{Length, Point2};

use crate::dxf::{DxfDrawing, DxfText};

#[derive(Debug, Clone, PartialEq)]
enum Obj {
    Null,
    Bool(bool),
    Num(f64),
    Name(String),
    Str(Vec<u8>),
    Arr(Vec<Obj>),
    Dict(BTreeMap<String, Obj>),
    Ref(u32),
    Stream(BTreeMap<String, Obj>, Vec<u8>),
    /// A content-stream operator (or an unknown keyword).
    Op(String),
}

impl Obj {
    fn num(&self) -> Option<f64> {
        match self {
            Obj::Num(n) => Some(*n),
            _ => None,
        }
    }

    fn name(&self) -> Option<&str> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }

    fn dict(&self) -> Option<&BTreeMap<String, Obj>> {
        match self {
            Obj::Dict(d) | Obj::Stream(d, _) => Some(d),
            _ => None,
        }
    }
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' | b'\0')
}

fn is_delim(c: u8) -> bool {
    matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn new(b: &'a [u8], i: usize) -> Self {
        Parser { b, i }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if is_ws(c) {
                self.i += 1;
            } else if c == b'%' {
                while let Some(c) = self.peek() {
                    if c == b'\n' || c == b'\r' {
                        break;
                    }
                    self.i += 1;
                }
            } else {
                break;
            }
        }
    }

    fn word(&mut self) -> &'a [u8] {
        let start = self.i;
        while let Some(c) = self.peek() {
            if is_ws(c) || is_delim(c) {
                break;
            }
            self.i += 1;
        }
        &self.b[start..self.i]
    }

    /// The next object; `None` at the end or on a stray delimiter.
    fn obj(&mut self) -> Option<Obj> {
        self.skip_ws();
        let c = self.peek()?;
        match c {
            b'/' => {
                self.i += 1;
                let raw = self.word();
                let mut name = String::new();
                let mut k = 0;
                while k < raw.len() {
                    if raw[k] == b'#'
                        && k + 2 < raw.len() + 1
                        && let Some(v) = raw.get(k + 1..k + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok())
                    {
                        name.push(v as char);
                        k += 3;
                    } else {
                        name.push(raw[k] as char);
                        k += 1;
                    }
                }
                Some(Obj::Name(name))
            }
            b'(' => Some(Obj::Str(self.literal())),
            b'<' if self.b.get(self.i + 1) == Some(&b'<') => {
                self.i += 2;
                let mut d = BTreeMap::new();
                loop {
                    self.skip_ws();
                    if self.b.get(self.i..self.i + 2) == Some(b">>") {
                        self.i += 2;
                        break;
                    }
                    let Some(Obj::Name(k)) = self.obj() else {
                        return Some(Obj::Dict(d));
                    };
                    let v = self.obj().unwrap_or(Obj::Null);
                    d.insert(k, v);
                }
                Some(Obj::Dict(d))
            }
            b'<' => {
                self.i += 1;
                let mut hex = Vec::new();
                while let Some(c) = self.peek() {
                    self.i += 1;
                    if c == b'>' {
                        break;
                    }
                    if c.is_ascii_hexdigit() {
                        hex.push(c);
                    }
                }
                if hex.len() % 2 == 1 {
                    hex.push(b'0');
                }
                let bytes = hex
                    .chunks(2)
                    .filter_map(|p| std::str::from_utf8(p).ok().and_then(|h| u8::from_str_radix(h, 16).ok()))
                    .collect();
                Some(Obj::Str(bytes))
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                loop {
                    self.skip_ws();
                    match self.peek() {
                        Some(b']') => {
                            self.i += 1;
                            break;
                        }
                        None => break,
                        _ => match self.obj() {
                            Some(o) => a.push(o),
                            None => {
                                self.i += 1;
                            }
                        },
                    }
                }
                Some(Obj::Arr(a))
            }
            b']' | b'>' | b')' | b'{' | b'}' => {
                self.i += 1;
                None
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => {
                let w = self.word();
                let n: f64 = std::str::from_utf8(w).ok().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                // "N G R" is a reference.
                if w.iter().all(|c| c.is_ascii_digit()) {
                    let save = self.i;
                    self.skip_ws();
                    let g = self.word();
                    if !g.is_empty() && g.iter().all(|c| c.is_ascii_digit()) {
                        self.skip_ws();
                        if self.word() == b"R" {
                            return Some(Obj::Ref(n as u32));
                        }
                    }
                    self.i = save;
                }
                Some(Obj::Num(n))
            }
            _ => {
                let w = self.word();
                if w.is_empty() {
                    self.i += 1;
                    return None;
                }
                Some(match w {
                    b"true" => Obj::Bool(true),
                    b"false" => Obj::Bool(false),
                    b"null" => Obj::Null,
                    _ => Obj::Op(String::from_utf8_lossy(w).into_owned()),
                })
            }
        }
    }

    fn literal(&mut self) -> Vec<u8> {
        self.i += 1;
        let mut out = Vec::new();
        let mut depth = 1;
        while let Some(c) = self.peek() {
            self.i += 1;
            match c {
                b'\\' => {
                    let Some(e) = self.peek() else { break };
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'\r' | b'\n' => {
                            if e == b'\r' && self.peek() == Some(b'\n') {
                                self.i += 1;
                            }
                        }
                        b'0'..=b'7' => {
                            let mut v = (e - b'0') as u32;
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(d @ b'0'..=b'7') => {
                                        v = v * 8 + (d - b'0') as u32;
                                        self.i += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(v as u8);
                        }
                        other => out.push(other),
                    }
                }
                b'(' => {
                    depth += 1;
                    out.push(c);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(c);
                }
                _ => out.push(c),
            }
        }
        out
    }
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= hay.len() {
        return None;
    }
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

/// The file's numbered objects.
struct Pdf {
    objects: BTreeMap<u32, Obj>,
}

impl Pdf {
    fn parse(b: &[u8]) -> Result<Pdf, String> {
        if !b.starts_with(b"%PDF") && find(b, b"%PDF", 0).is_none_or(|p| p > 1024) {
            return Err("not a PDF file".into());
        }
        let mut objects = BTreeMap::new();
        let mut i = 0;
        while let Some(at) = find(b, b"obj", i) {
            i = at + 3;
            // Preceded by "N G ".
            let head = &b[at.saturating_sub(24)..at];
            let text = String::from_utf8_lossy(head);
            let parts: Vec<&str> = text.split_whitespace().collect();
            if parts.len() < 2 || !text.ends_with(|c: char| c.is_whitespace()) {
                continue;
            }
            let (Ok(n), Ok(_g)) = (parts[parts.len() - 2].parse::<u32>(), parts[parts.len() - 1].parse::<u32>()) else {
                continue;
            };
            if b.get(at + 3).is_some_and(|c| !is_ws(*c) && !is_delim(*c)) {
                continue; // "objects" or similar
            }
            let mut p = Parser::new(b, at + 3);
            let Some(o) = p.obj() else { continue };
            p.skip_ws();
            let o = if b[p.i..].starts_with(b"stream") {
                let mut s = p.i + 6;
                if b.get(s) == Some(&b'\r') {
                    s += 1;
                }
                if b.get(s) == Some(&b'\n') {
                    s += 1;
                }
                let dict = o.dict().cloned().unwrap_or_default();
                let direct = dict.get("Length").and_then(Obj::num).map(|n| n as usize);
                let end = direct
                    .filter(|l| s + l <= b.len() && find(b, b"endstream", s + l).is_some_and(|e| e - (s + l) < 4))
                    .map(|l| s + l)
                    .or_else(|| find(b, b"endstream", s))
                    .unwrap_or(b.len());
                let mut data_end = end;
                while data_end > s && matches!(b[data_end - 1], b'\r' | b'\n') && direct.is_none() {
                    data_end -= 1;
                }
                i = end;
                Obj::Stream(dict, b[s..data_end].to_vec())
            } else {
                i = p.i;
                o
            };
            objects.insert(n, o);
        }
        let mut pdf = Pdf { objects };
        // Objects packed into object streams.
        let streams: Vec<Obj> = pdf
            .objects
            .values()
            .filter(|o| o.dict().is_some_and(|d| d.get("Type").and_then(Obj::name) == Some("ObjStm")))
            .cloned()
            .collect();
        for s in streams {
            let Ok(data) = pdf.decode(&s) else { continue };
            let d = s.dict().cloned().unwrap_or_default();
            let count = d.get("N").and_then(Obj::num).unwrap_or(0.0) as usize;
            let first = d.get("First").and_then(Obj::num).unwrap_or(0.0) as usize;
            let mut p = Parser::new(&data, 0);
            let mut index = Vec::new();
            for _ in 0..count {
                let (Some(n), Some(off)) = (p.obj().and_then(|o| o.num()), p.obj().and_then(|o| o.num())) else {
                    break;
                };
                index.push((n as u32, off as usize));
            }
            for (n, off) in index {
                let mut q = Parser::new(&data, first + off);
                if let Some(o) = q.obj() {
                    pdf.objects.entry(n).or_insert(o);
                }
            }
        }
        Ok(pdf)
    }

    fn get<'b>(&'b self, o: &'b Obj) -> &'b Obj {
        let mut o = o;
        for _ in 0..16 {
            match o {
                Obj::Ref(n) => o = self.objects.get(n).unwrap_or(&Obj::Null),
                _ => break,
            }
        }
        o
    }

    fn key<'b>(&'b self, d: &'b BTreeMap<String, Obj>, k: &str) -> Option<&'b Obj> {
        d.get(k).map(|o| self.get(o))
    }

    /// A stream's data with its filters undone (Flate only).
    fn decode(&self, s: &Obj) -> Result<Vec<u8>, String> {
        let Obj::Stream(d, data) = s else {
            return Err("not a stream".into());
        };
        let filters: Vec<String> = match self.key(d, "Filter") {
            Some(Obj::Name(n)) => vec![n.clone()],
            Some(Obj::Arr(a)) => a.iter().filter_map(|f| self.get(f).name().map(String::from)).collect(),
            _ => Vec::new(),
        };
        let mut out = data.clone();
        for f in filters {
            match f.as_str() {
                "FlateDecode" | "Fl" => {
                    let mut buf = Vec::new();
                    let mut z = flate2::read::ZlibDecoder::new(&out[..]);
                    if z.read_to_end(&mut buf).is_err() && buf.is_empty() {
                        let mut raw = flate2::read::DeflateDecoder::new(&out[..]);
                        buf.clear();
                        raw.read_to_end(&mut buf).map_err(|e| format!("bad compressed stream: {e}"))?;
                    }
                    out = buf;
                }
                other => return Err(format!("unsupported stream filter {other}")),
            }
        }
        Ok(out)
    }

    /// Leaf pages in order, each with its inherited resources and media box.
    fn pages(&self) -> Vec<(BTreeMap<String, Obj>, Obj, [f64; 4])> {
        let mut out = Vec::new();
        let root = self
            .objects
            .values()
            .find(|o| o.dict().is_some_and(|d| d.get("Type").and_then(Obj::name) == Some("Catalog")))
            .and_then(|c| c.dict())
            .and_then(|d| self.key(d, "Pages"));
        match root {
            Some(r) => self.walk(r, None, None, &mut out, 0),
            None => {
                for o in self.objects.values() {
                    if o.dict().is_some_and(|d| d.get("Type").and_then(Obj::name) == Some("Page")) {
                        self.walk(o, None, None, &mut out, 0);
                    }
                }
            }
        }
        out
    }

    fn walk(&self, node: &Obj, res: Option<&Obj>, mb: Option<[f64; 4]>, out: &mut Vec<(BTreeMap<String, Obj>, Obj, [f64; 4])>, depth: usize) {
        let Some(d) = self.get(node).dict() else { return };
        if depth > 32 {
            return;
        }
        let res = self.key(d, "Resources").or(res);
        let mb = self
            .key(d, "MediaBox")
            .and_then(|m| match m {
                Obj::Arr(a) if a.len() == 4 => {
                    let v: Vec<f64> = a.iter().map(|x| self.get(x).num().unwrap_or(0.0)).collect();
                    Some([v[0], v[1], v[2], v[3]])
                }
                _ => None,
            })
            .or(mb);
        match self.key(d, "Kids") {
            Some(Obj::Arr(kids)) => {
                for k in kids {
                    self.walk(k, res, mb, out, depth + 1);
                }
            }
            _ => out.push((d.clone(), res.cloned().unwrap_or(Obj::Null), mb.unwrap_or([0.0, 0.0, 612.0, 792.0]))),
        }
    }
}

type Matrix = [f64; 6];

fn mul(m: &Matrix, n: &Matrix) -> Matrix {
    [
        m[0] * n[0] + m[1] * n[2],
        m[0] * n[1] + m[1] * n[3],
        m[2] * n[0] + m[3] * n[2],
        m[2] * n[1] + m[3] * n[3],
        m[4] * n[0] + m[5] * n[2] + n[4],
        m[4] * n[1] + m[5] * n[3] + n[5],
    ]
}

fn apply(m: &Matrix, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// What a page's content draws, in PDF points.
#[derive(Default)]
struct Ink {
    paths: Vec<(Vec<(f64, f64)>, bool)>,
    texts: Vec<((f64, f64), String, f64, f64)>,
    images: usize,
    cid_text: usize,
}

struct Run<'p> {
    pdf: &'p Pdf,
    ink: Ink,
}

impl Run<'_> {
    fn content(&mut self, data: &[u8], resources: &Obj, ctm0: Matrix, depth: usize) {
        let mut p = Parser::new(data, 0);
        let mut ops: Vec<Obj> = Vec::new();
        let mut ctm = ctm0;
        let mut stack: Vec<Matrix> = Vec::new();
        let mut subpaths: Vec<(Vec<(f64, f64)>, bool)> = Vec::new();
        let mut cur: Vec<(f64, f64)> = Vec::new();
        let (mut tm, mut tlm) = (IDENTITY, IDENTITY);
        let (mut size, mut leading) = (12.0, 0.0);
        let mut font = Font::default();
        let res = self.pdf.get(resources).dict().cloned().unwrap_or_default();
        let n = |ops: &Vec<Obj>, k: usize| -> f64 {
            let len = ops.len();
            ops.get(len.wrapping_sub(k)).and_then(Obj::num).unwrap_or(0.0)
        };
        while p.i < data.len() {
            let Some(o) = p.obj() else {
                continue;
            };
            let Obj::Op(op) = o else {
                ops.push(o);
                continue;
            };
            let flush = |cur: &mut Vec<(f64, f64)>, subpaths: &mut Vec<(Vec<(f64, f64)>, bool)>, closed: bool| {
                if cur.len() >= 2 {
                    subpaths.push((std::mem::take(cur), closed));
                } else {
                    cur.clear();
                }
            };
            match op.as_str() {
                "q" => stack.push(ctm),
                "Q" => ctm = stack.pop().unwrap_or(ctm0),
                "cm" => {
                    let m = [n(&ops, 6), n(&ops, 5), n(&ops, 4), n(&ops, 3), n(&ops, 2), n(&ops, 1)];
                    ctm = mul(&m, &ctm);
                }
                "m" => {
                    flush(&mut cur, &mut subpaths, false);
                    cur.push(apply(&ctm, n(&ops, 2), n(&ops, 1)));
                }
                "l" => cur.push(apply(&ctm, n(&ops, 2), n(&ops, 1))),
                "c" | "v" | "y" => {
                    let p0 = *cur.last().unwrap_or(&(0.0, 0.0));
                    let (c1, c2, p3) = match op.as_str() {
                        "c" => (apply(&ctm, n(&ops, 6), n(&ops, 5)), apply(&ctm, n(&ops, 4), n(&ops, 3)), apply(&ctm, n(&ops, 2), n(&ops, 1))),
                        "v" => (p0, apply(&ctm, n(&ops, 4), n(&ops, 3)), apply(&ctm, n(&ops, 2), n(&ops, 1))),
                        _ => {
                            let e = apply(&ctm, n(&ops, 2), n(&ops, 1));
                            (apply(&ctm, n(&ops, 4), n(&ops, 3)), e, e)
                        }
                    };
                    for k in 1..=8 {
                        let t = k as f64 / 8.0;
                        let u = 1.0 - t;
                        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                        cur.push((
                            w[0] * p0.0 + w[1] * c1.0 + w[2] * c2.0 + w[3] * p3.0,
                            w[0] * p0.1 + w[1] * c1.1 + w[2] * c2.1 + w[3] * p3.1,
                        ));
                    }
                }
                "h" => {
                    if let Some(first) = cur.first().copied() {
                        flush(&mut cur, &mut subpaths, true);
                        cur.push(first);
                        cur.clear();
                    }
                }
                "re" => {
                    flush(&mut cur, &mut subpaths, false);
                    let (x, y, w, h) = (n(&ops, 4), n(&ops, 3), n(&ops, 2), n(&ops, 1));
                    let pts = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
                    subpaths.push((pts.iter().map(|(a, b)| apply(&ctm, *a, *b)).collect(), true));
                }
                "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" => {
                    let closes = !matches!(op.as_str(), "S");
                    flush(&mut cur, &mut subpaths, closes);
                    for (pts, closed) in subpaths.drain(..) {
                        self.ink.paths.push((pts, closed || closes));
                    }
                }
                "n" => {
                    // Clipping paths are not drawn.
                    cur.clear();
                    subpaths.clear();
                }
                "BT" => {
                    tm = IDENTITY;
                    tlm = IDENTITY;
                }
                "Tf" => {
                    size = n(&ops, 1);
                    let name = ops.get(ops.len().wrapping_sub(2)).and_then(Obj::name).unwrap_or("");
                    font = self.font(&res, name);
                }
                "TL" => leading = n(&ops, 1),
                "Td" | "TD" => {
                    if op == "TD" {
                        leading = -n(&ops, 1);
                    }
                    tlm = mul(&[1.0, 0.0, 0.0, 1.0, n(&ops, 2), n(&ops, 1)], &tlm);
                    tm = tlm;
                }
                "Tm" => {
                    tlm = [n(&ops, 6), n(&ops, 5), n(&ops, 4), n(&ops, 3), n(&ops, 2), n(&ops, 1)];
                    tm = tlm;
                }
                "T*" => {
                    tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -leading], &tlm);
                    tm = tlm;
                }
                "Tj" | "'" | "\"" | "TJ" => {
                    if op == "'" || op == "\"" {
                        tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -leading], &tlm);
                        tm = tlm;
                    }
                    let mut text = String::new();
                    let mut readable = true;
                    let parts: Vec<Option<&[u8]>> = match ops.last() {
                        Some(Obj::Str(s)) => vec![Some(s.as_slice())],
                        Some(Obj::Arr(a)) => a
                            .iter()
                            .filter_map(|x| match x {
                                Obj::Str(s) => Some(Some(s.as_slice())),
                                Obj::Num(k) if *k < -200.0 => Some(None),
                                _ => None,
                            })
                            .collect(),
                        _ => Vec::new(),
                    };
                    for part in parts {
                        match part.map(|b| font.decode(b)) {
                            None => text.push(' '),
                            Some(Some(t)) => text.push_str(&t),
                            Some(None) => readable = false,
                        }
                    }
                    if !readable {
                        self.ink.cid_text += 1;
                    } else {
                        let s: String = text.chars().filter(|c| !c.is_control()).collect();
                        let m = mul(&tm, &ctm);
                        let at = apply(&m, 0.0, 0.0);
                        let scale = (m[2] * m[2] + m[3] * m[3]).sqrt();
                        let rot = m[1].atan2(m[0]).to_degrees();
                        if !s.trim().is_empty() {
                            self.ink.texts.push((at, s.clone(), size * scale, rot));
                        }
                        // Advance roughly (no font metrics): half an em a character.
                        tm = mul(&[1.0, 0.0, 0.0, 1.0, s.chars().count() as f64 * size * 0.5, 0.0], &tm);
                    }
                }
                "Do" => {
                    let name = ops.last().and_then(Obj::name).unwrap_or("").to_string();
                    let x = self
                        .pdf
                        .key(&res, "XObject")
                        .and_then(|x| x.dict())
                        .and_then(|x| self.pdf.key(x, &name))
                        .cloned();
                    if let Some(x) = x {
                        let d = x.dict().cloned().unwrap_or_default();
                        match d.get("Subtype").and_then(Obj::name) {
                            Some("Form") if depth < 8 => {
                                let m = match self.pdf.key(&d, "Matrix") {
                                    Some(Obj::Arr(a)) if a.len() == 6 => {
                                        let v: Vec<f64> = a.iter().map(|o| self.pdf.get(o).num().unwrap_or(0.0)).collect();
                                        [v[0], v[1], v[2], v[3], v[4], v[5]]
                                    }
                                    _ => IDENTITY,
                                };
                                let r = self.pdf.key(&d, "Resources").cloned().unwrap_or_else(|| resources.clone());
                                if let Ok(data) = self.pdf.decode(&x) {
                                    self.content(&data, &r, mul(&m, &ctm), depth + 1);
                                }
                            }
                            Some("Image") => self.ink.images += 1,
                            _ => {}
                        }
                    }
                }
                "BI" => {
                    // Inline image: skip its data up to "EI".
                    if let Some(id) = find(data, b"ID", p.i) {
                        let mut k = id + 2;
                        while let Some(e) = find(data, b"EI", k) {
                            if e > 0 && is_ws(data[e - 1]) && data.get(e + 2).is_none_or(|c| is_ws(*c)) {
                                k = e + 2;
                                break;
                            }
                            k = e + 2;
                        }
                        p.i = k;
                    }
                    self.ink.images += 1;
                }
                _ => {}
            }
            ops.clear();
        }
    }
}

/// A font's codes: one or two bytes each, and its map to Unicode.
#[derive(Default)]
struct Font {
    two_byte: bool,
    to_unicode: Option<BTreeMap<u32, String>>,
}

impl Font {
    /// Text for shown bytes; `None` for two-byte codes with no Unicode map.
    fn decode(&self, bytes: &[u8]) -> Option<String> {
        let width = if self.two_byte { 2 } else { 1 };
        match &self.to_unicode {
            Some(map) => Some(
                bytes
                    .chunks(width)
                    .map(|c| {
                        let code = c.iter().fold(0u32, |v, b| v << 8 | *b as u32);
                        map.get(&code).cloned().unwrap_or_else(|| if width == 1 { (code as u8 as char).to_string() } else { String::new() })
                    })
                    .collect(),
            ),
            None if !self.two_byte => Some(bytes.iter().map(|c| *c as char).collect()),
            None => None,
        }
    }
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.chunks(2).map(|c| (c[0] as u16) << 8 | *c.get(1).unwrap_or(&0) as u16).collect();
    String::from_utf16_lossy(&units)
}

fn code(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |v, b| v << 8 | *b as u32)
}

/// A ToUnicode CMap's `bfchar` and `bfrange` entries.
fn parse_cmap(data: &[u8]) -> BTreeMap<u32, String> {
    let mut map = BTreeMap::new();
    let mut p = Parser::new(data, 0);
    let mut mode = "";
    let mut args: Vec<Obj> = Vec::new();
    while p.i < data.len() {
        let Some(o) = p.obj() else { continue };
        match o {
            Obj::Op(op) => {
                match op.as_str() {
                    "beginbfchar" => mode = "char",
                    "beginbfrange" => mode = "range",
                    "endbfchar" | "endbfrange" => mode = "",
                    _ => {}
                }
                args.clear();
            }
            o if !mode.is_empty() => {
                args.push(o);
                match (mode, args.as_slice()) {
                    ("char", [Obj::Str(src), Obj::Str(dst)]) => {
                        map.insert(code(src), utf16(dst));
                        args.clear();
                    }
                    ("range", [Obj::Str(lo), Obj::Str(hi), dst]) => {
                        let (lo, hi) = (code(lo), code(hi));
                        for (k, c) in (lo..=hi.min(lo + 0xffff)).enumerate() {
                            let text = match dst {
                                Obj::Str(d) if !d.is_empty() => {
                                    let mut d = d.clone();
                                    let last = d.len() - 1;
                                    let v = d[last] as usize + k;
                                    d[last] = v as u8;
                                    if v > 0xff && last > 0 {
                                        d[last - 1] = d[last - 1].wrapping_add((v >> 8) as u8);
                                    }
                                    utf16(&d)
                                }
                                Obj::Arr(a) => match a.get(k) {
                                    Some(Obj::Str(d)) => utf16(d),
                                    _ => continue,
                                },
                                _ => continue,
                            };
                            map.insert(c, text);
                        }
                        args.clear();
                    }
                    (_, a) if a.len() >= 3 => args.clear(),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    map
}

impl Run<'_> {
    fn font(&self, res: &BTreeMap<String, Obj>, name: &str) -> Font {
        let Some(f) = self.pdf.key(res, "Font").and_then(|f| f.dict()).and_then(|f| self.pdf.key(f, name)).and_then(|f| f.dict()) else {
            return Font::default();
        };
        let two_byte = f.get("Subtype").and_then(Obj::name) == Some("Type0");
        let to_unicode = self.pdf.key(f, "ToUnicode").and_then(|s| self.pdf.decode(s).ok()).map(|d| parse_cmap(&d)).filter(|m| !m.is_empty());
        Font { two_byte, to_unicode }
    }
}

/// How many pages a PDF has.
pub fn page_count(bytes: &[u8]) -> Result<usize, String> {
    Ok(Pdf::parse(bytes)?.pages().len())
}

/// Read page `page` (from 0) of a PDF as CAD linework and notes, at
/// `scale` model millimetres per paper millimetre (48 for 1/4" = 1'-0";
/// 1 keeps the paper size). The page's bottom-left corner lands at the origin.
pub fn read(bytes: &[u8], page: usize, scale: f64) -> Result<DxfDrawing, String> {
    let pdf = Pdf::parse(bytes)?;
    let pages = pdf.pages();
    let count = pages.len();
    let (d, res, mb) = pages.into_iter().nth(page).ok_or_else(|| format!("the PDF has {count} page(s); pages count from 0"))?;
    let mut data = Vec::new();
    match pdf.key(&d, "Contents") {
        Some(Obj::Arr(a)) => {
            for c in a {
                data.extend(pdf.decode(pdf.get(c))?);
                data.push(b'\n');
            }
        }
        Some(s @ Obj::Stream(..)) => data = pdf.decode(s)?,
        _ => {}
    }
    let mut run = Run { pdf: &pdf, ink: Ink::default() };
    run.content(&data, &res, IDENTITY, 0);
    let k = 25.4 / 72.0 * scale;
    let pt = |(x, y): (f64, f64)| Point2::from_mm((x - mb[0]) * k, (y - mb[1]) * k);
    let mut out = DxfDrawing {
        unit_mm: k,
        ..Default::default()
    };
    for (mut pts, closed) in run.ink.paths {
        pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6);
        if closed && pts.len() > 2 && pts.first() == pts.last() {
            pts.pop();
        }
        let shape = match (pts.len(), closed) {
            (0 | 1, _) => continue,
            (2, _) => Shape::Line { a: pt(pts[0]), b: pt(pts[1]), arrow_start: false, arrow_end: false },
            _ => Shape::Polyline { points: pts.into_iter().map(pt).collect(), closed },
        };
        out.shapes.push(("PDF".into(), shape));
    }
    // Words placed one by one on the same baseline become one line.
    let mut lines: Vec<((f64, f64), String, f64, f64)> = Vec::new();
    for (at, text, h, rot) in run.ink.texts {
        if let Some(last) = lines.last_mut() {
            let (dx, dy) = (at.0 - last.0.0, at.1 - last.0.1);
            let r = last.3.to_radians();
            let along = dx * r.cos() + dy * r.sin();
            let across = -dx * r.sin() + dy * r.cos();
            let end = last.1.chars().count() as f64 * last.2 * 0.5;
            if (rot - last.3).abs() < 0.5 && (h - last.2).abs() < 0.05 * h && across.abs() < 0.2 * h && along > 0.0 && along < end + 1.2 * h {
                if !last.1.ends_with(' ') && !text.starts_with(' ') {
                    last.1.push(' ');
                }
                last.1.push_str(&text);
                continue;
            }
        }
        lines.push((at, text, h, rot));
    }
    for (at, text, h, rot) in lines {
        out.texts.push(DxfText {
            layer: "PDF".into(),
            at: pt(at),
            text,
            height: Length::from_mm(h * k),
            rotation: rot,
        });
    }
    if run.ink.images > 0 {
        out.skipped.push(("images".into(), run.ink.images));
    }
    if run.ink.cid_text > 0 {
        out.skipped.push(("text runs in embedded CID fonts".into(), run.ink.cid_text));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A minimal two-page PDF; page 2's content is Flate-compressed.
    fn sample() -> Vec<u8> {
        let page1 = b"q 2 0 0 2 0 0 cm 10 10 m 46 10 l S Q 0 0 72 36 re f BT /F1 12 Tf 72 144 Td (Hello) Tj ET".to_vec();
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(b"0 0 m 72 0 72 72 0 72 c h S").unwrap();
        let page2 = z.finish().unwrap();
        let mut f = b"%PDF-1.4\n".to_vec();
        let obj = |f: &mut Vec<u8>, n: u32, body: &[u8]| {
            f.extend(format!("{n} 0 obj\n").as_bytes());
            f.extend(body);
            f.extend(b"\nendobj\n");
        };
        obj(&mut f, 1, b"<< /Type /Catalog /Pages 2 0 R >>");
        obj(&mut f, 2, b"<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 /MediaBox [0 0 612 792] /Resources << /Font << /F1 7 0 R >> >> >>");
        obj(&mut f, 3, b"<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>");
        let mut s4 = format!("<< /Length {} >>\nstream\n", page1.len()).into_bytes();
        s4.extend(&page1);
        s4.extend(b"\nendstream");
        obj(&mut f, 4, &s4);
        obj(&mut f, 5, b"<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>");
        let mut s6 = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", page2.len()).into_bytes();
        s6.extend(&page2);
        s6.extend(b"\nendstream");
        obj(&mut f, 6, &s6);
        obj(&mut f, 7, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
        f.extend(b"trailer << /Root 1 0 R >>\n%%EOF\n");
        f
    }

    #[test]
    fn reads_paths_text_and_compressed_pages() {
        let f = sample();
        assert_eq!(page_count(&f).unwrap(), 2);
        // Scale 1: points to millimetres.
        let d = read(&f, 0, 1.0).unwrap();
        let mm = 25.4 / 72.0;
        let Shape::Line { a, b, .. } = &d.shapes[0].1 else { panic!("{:?}", d.shapes) };
        assert!((a.0.mm() - 20.0 * mm).abs() < 1e-6 && (b.0.mm() - 92.0 * mm).abs() < 1e-6);
        let Shape::Polyline { points, closed: true } = &d.shapes[1].1 else { panic!() };
        assert_eq!(points.len(), 4);
        assert_eq!(d.texts[0].text, "Hello");
        assert!((d.texts[0].height.mm() - 12.0 * mm).abs() < 1e-6);
        assert!((d.texts[0].at.1.mm() - 144.0 * mm).abs() < 1e-6);
        // Page 2: a curve, flattened; drawing scale applies.
        let d = read(&f, 1, 48.0).unwrap();
        let Shape::Polyline { points, closed: true } = &d.shapes[0].1 else { panic!("{:?}", d.shapes) };
        assert!(points.len() > 6);
        assert!(points.iter().all(|p| p.0.mm() <= 72.0 * mm * 48.0 + 1e-6));
        assert!(read(&f, 2, 1.0).is_err());
        assert!(read(b"hello", 0, 1.0).is_err());
    }

    #[test]
    fn two_byte_fonts_decode_through_their_unicode_map() {
        let map = parse_cmap(b"1 beginbfchar <0003> <0020> endbfchar 1 beginbfrange <0010> <0012> <0041> endbfrange 1 beginbfrange <0020> <0021> [<0078> <0079>] endbfrange");
        let f = Font { two_byte: true, to_unicode: Some(map) };
        assert_eq!(f.decode(&[0, 0x10, 0, 0x11, 0, 3, 0, 0x12, 0, 0x21]).as_deref(), Some("AB Cy"));
        assert_eq!(Font { two_byte: true, to_unicode: None }.decode(&[0, 1]), None);
        assert_eq!(Font::default().decode(b"Hi").as_deref(), Some("Hi"));
    }
}
