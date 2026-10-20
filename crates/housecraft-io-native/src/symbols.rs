//! Reading 3D models to import as symbols: Wavefront OBJ (with MTL
//! colours), STL (binary or text) and glTF 2.0 (`.gltf` or `.glb`, base
//! colours per material).

use std::collections::HashMap;
use std::path::Path;

use housecraft_model::{Color, Id, SymbolDef, SymbolGroup};

use crate::Error;

/// The length unit a model file was drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelUnit {
    /// Guessed from the model's size.
    Auto,
    Millimetres,
    Centimetres,
    Metres,
    Inches,
    Feet,
}

impl ModelUnit {
    pub const ALL: [ModelUnit; 6] = [
        ModelUnit::Auto,
        ModelUnit::Millimetres,
        ModelUnit::Centimetres,
        ModelUnit::Metres,
        ModelUnit::Inches,
        ModelUnit::Feet,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ModelUnit::Auto => "Guess from size",
            ModelUnit::Millimetres => "Millimetres",
            ModelUnit::Centimetres => "Centimetres",
            ModelUnit::Metres => "Metres",
            ModelUnit::Inches => "Inches",
            ModelUnit::Feet => "Feet",
        }
    }

    fn mm(self) -> f32 {
        match self {
            ModelUnit::Auto | ModelUnit::Millimetres => 1.0,
            ModelUnit::Centimetres => 10.0,
            ModelUnit::Metres => 1000.0,
            ModelUnit::Inches => 25.4,
            ModelUnit::Feet => 304.8,
        }
    }

    /// A unit for a model whose largest dimension is `size` file units,
    /// assuming it is furniture-to-room sized.
    fn guess(size: f32) -> ModelUnit {
        if size <= 20.0 {
            ModelUnit::Metres
        } else if size <= 400.0 {
            ModelUnit::Inches
        } else {
            ModelUnit::Millimetres
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImportOptions {
    pub unit: ModelUnit,
    /// The file's up axis is +y (most OBJ exports); otherwise +z.
    pub y_up: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            unit: ModelUnit::Auto,
            y_up: true,
        }
    }
}

struct Raw {
    positions: Vec<[f32; 3]>,
    triangles: Vec<[u32; 3]>,
    groups: Vec<SymbolGroup>,
}

const GREY: Color = Color::rgb(190, 190, 190);

fn parse_mtl(text: &str) -> HashMap<String, Color> {
    let mut out = HashMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("newmtl") => current = it.next().map(str::to_string),
            Some("Kd") => {
                let v: Vec<f32> = it.filter_map(|x| x.parse().ok()).collect();
                if let (Some(name), [r, g, b, ..]) = (&current, v.as_slice()) {
                    let c = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
                    out.insert(name.clone(), Color::rgb(c(*r), c(*g), c(*b)));
                }
            }
            _ => {}
        }
    }
    out
}

/// Parse OBJ text; `mtl` resolves a material library file name to its text.
fn parse_obj(text: &str, mtl: impl Fn(&str) -> Option<String>) -> Result<Raw, Error> {
    let mut positions = Vec::new();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    let mut groups: Vec<SymbolGroup> = Vec::new();
    let mut colors: HashMap<String, Color> = HashMap::new();
    let start_group = |name: &str, color: Color, at: usize, groups: &mut Vec<SymbolGroup>| {
        if let Some(last) = groups.last_mut()
            && last.count == 0
        {
            groups.pop();
        }
        groups.push(SymbolGroup {
            first: at as u32,
            count: 0,
            color,
            name: name.to_string(),
        });
    };
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("");
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let v: Vec<f32> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if v.len() != 3 {
                    return Err(Error::Corrupt(format!("line {}: a point needs x y z", n + 1)));
                }
                positions.push([v[0], v[1], v[2]]);
            }
            Some("f") => {
                let idx: Vec<u32> = it
                    .map(|t| {
                        let i: i64 = t.split('/').next().unwrap_or("").parse().map_err(|_| {
                            Error::Corrupt(format!("line {}: bad face index {t:?}", n + 1))
                        })?;
                        // Negative indices count back from the last point.
                        let i = if i < 0 { positions.len() as i64 + i } else { i - 1 };
                        if i < 0 || i >= positions.len() as i64 {
                            return Err(Error::Corrupt(format!("line {}: face index out of range", n + 1)));
                        }
                        Ok(i as u32)
                    })
                    .collect::<Result<_, _>>()?;
                if groups.is_empty() {
                    start_group("", GREY, triangles.len(), &mut groups);
                }
                // Fan-triangulate polygons.
                for k in 1..idx.len().saturating_sub(1) {
                    triangles.push([idx[0], idx[k], idx[k + 1]]);
                    groups.last_mut().expect("started").count += 1;
                }
            }
            Some("mtllib") => {
                for file in it {
                    if let Some(t) = mtl(file) {
                        colors.extend(parse_mtl(&t));
                    }
                }
            }
            Some("usemtl") => {
                let name = it.next().unwrap_or("");
                let color = colors.get(name).copied().unwrap_or(GREY);
                start_group(name, color, triangles.len(), &mut groups);
            }
            _ => {}
        }
    }
    groups.retain(|g| g.count > 0);
    Ok(Raw {
        positions,
        triangles,
        groups,
    })
}

fn parse_stl(bytes: &[u8]) -> Result<Raw, Error> {
    let mut positions = Vec::new();
    let mut triangles = Vec::new();
    let binary_len = bytes
        .get(80..84)
        .map(|b| 84 + 50 * u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    if binary_len == Some(bytes.len()) {
        let n = (bytes.len() - 84) / 50;
        for t in 0..n {
            let base = 84 + t * 50 + 12;
            let mut tri = [0u32; 3];
            for (k, v) in tri.iter_mut().enumerate() {
                let o = base + k * 12;
                let f = |i: usize| f32::from_le_bytes(bytes[o + i * 4..o + i * 4 + 4].try_into().expect("4 bytes"));
                positions.push([f(0), f(1), f(2)]);
                *v = positions.len() as u32 - 1;
            }
            triangles.push(tri);
        }
    } else {
        let text = std::str::from_utf8(bytes).map_err(|_| Error::Corrupt("not an STL file".into()))?;
        let mut current = Vec::new();
        for line in text.lines() {
            let mut it = line.split_whitespace();
            if it.next() == Some("vertex") {
                let v: Vec<f32> = it.filter_map(|x| x.parse().ok()).collect();
                if v.len() == 3 {
                    positions.push([v[0], v[1], v[2]]);
                    current.push(positions.len() as u32 - 1);
                }
                if current.len() == 3 {
                    triangles.push([current[0], current[1], current[2]]);
                    current.clear();
                }
            }
        }
        if triangles.is_empty() {
            return Err(Error::Corrupt("the STL file has no triangles".into()));
        }
    }
    let count = triangles.len() as u32;
    Ok(Raw {
        positions,
        triangles,
        groups: vec![SymbolGroup {
            first: 0,
            count,
            color: GREY,
            name: String::new(),
        }],
    })
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let clean: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * i);
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..chunk.len().saturating_sub(1)]);
    }
    Some(out)
}

type Mat4 = [f64; 16];

fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut m = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            m[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    m
}

const IDENTITY4: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// A node's local matrix (column-major) from `matrix`, or from its
/// translation, rotation (quaternion) and scale.
fn node_matrix(n: &serde_json::Value) -> Mat4 {
    let nums = |k: &str| -> Option<Vec<f64>> { n.get(k)?.as_array().map(|a| a.iter().filter_map(|x| x.as_f64()).collect()) };
    if let Some(m) = nums("matrix").filter(|m| m.len() == 16) {
        return m.try_into().expect("16");
    }
    let t = nums("translation").filter(|v| v.len() == 3).unwrap_or(vec![0.0; 3]);
    let q = nums("rotation").filter(|v| v.len() == 4).unwrap_or(vec![0.0, 0.0, 0.0, 1.0]);
    let s = nums("scale").filter(|v| v.len() == 3).unwrap_or(vec![1.0; 3]);
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let r = [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
    ];
    [
        r[0] * s[0],
        r[1] * s[0],
        r[2] * s[0],
        0.0,
        r[3] * s[1],
        r[4] * s[1],
        r[5] * s[1],
        0.0,
        r[6] * s[2],
        r[7] * s[2],
        r[8] * s[2],
        0.0,
        t[0],
        t[1],
        t[2],
        1.0,
    ]
}

/// glTF 2.0: a `.glb` container or `.gltf` JSON whose buffers are data
/// URIs or files `read` can fetch. Triangle primitives of every mesh in
/// the default scene (or every mesh), coloured by base colour.
fn parse_gltf(bytes: &[u8], read: impl Fn(&str) -> Option<Vec<u8>>) -> Result<Raw, Error> {
    let bad = |m: &str| Error::Corrupt(format!("glTF: {m}"));
    let (json, bin): (serde_json::Value, Option<Vec<u8>>) = if bytes.starts_with(b"glTF") {
        let u32_at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().expect("4")) as usize);
        let mut o = 12;
        let (mut json, mut bin) = (None, None);
        while let (Some(len), Some(kind)) = (u32_at(o), u32_at(o + 4)) {
            let data = bytes.get(o + 8..o + 8 + len).ok_or_else(|| bad("truncated chunk"))?;
            match kind {
                0x4E4F_534A => json = Some(serde_json::from_slice(data).map_err(|e| bad(&e.to_string()))?),
                0x004E_4942 => bin = Some(data.to_vec()),
                _ => {}
            }
            o += 8 + len;
        }
        (json.ok_or_else(|| bad("no JSON chunk"))?, bin)
    } else {
        (serde_json::from_slice(bytes).map_err(|e| bad(&e.to_string()))?, None)
    };
    let arr = |k: &str| json.get(k).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let buffers: Vec<Vec<u8>> = arr("buffers")
        .iter()
        .enumerate()
        .map(|(i, b)| match b.get("uri").and_then(|u| u.as_str()) {
            Some(uri) if uri.starts_with("data:") => uri.split_once(',').and_then(|(_, d)| base64_decode(d)).unwrap_or_default(),
            Some(uri) => read(uri).unwrap_or_default(),
            None if i == 0 => bin.clone().unwrap_or_default(),
            None => Vec::new(),
        })
        .collect();
    let views = arr("bufferViews");
    let accessors = arr("accessors");
    // Accessor `i` as rows of numbers.
    let read_acc = |i: usize| -> Option<Vec<Vec<f64>>> {
        let a = accessors.get(i)?;
        let count = a.get("count")?.as_u64()? as usize;
        let width = match a.get("type")?.as_str()? {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => return None,
        };
        let ct = a.get("componentType")?.as_u64()?;
        let size = match ct {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return None,
        };
        let v = views.get(a.get("bufferView")?.as_u64()? as usize)?;
        let buf = buffers.get(v.get("buffer")?.as_u64()? as usize)?;
        let base = v.get("byteOffset").and_then(|x| x.as_u64()).unwrap_or(0) as usize + a.get("byteOffset").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        let stride = v.get("byteStride").and_then(|x| x.as_u64()).map_or(width * size, |s| s as usize);
        let mut rows = Vec::with_capacity(count);
        for e in 0..count {
            let mut row = Vec::with_capacity(width);
            for c in 0..width {
                let o = base + e * stride + c * size;
                let b = buf.get(o..o + size)?;
                row.push(match ct {
                    5126 => f32::from_le_bytes(b.try_into().ok()?) as f64,
                    5125 => u32::from_le_bytes(b.try_into().ok()?) as f64,
                    5123 => u16::from_le_bytes(b.try_into().ok()?) as f64,
                    5122 => i16::from_le_bytes(b.try_into().ok()?) as f64,
                    5121 => b[0] as f64,
                    _ => b[0] as i8 as f64,
                });
            }
            rows.push(row);
        }
        Some(rows)
    };
    let materials = arr("materials");
    let color_of = |m: Option<u64>| -> Color {
        m.and_then(|i| materials.get(i as usize))
            .and_then(|m| m.get("pbrMetallicRoughness")?.get("baseColorFactor")?.as_array().cloned())
            .map_or(GREY, |c| {
                let f = |i: usize| (c.get(i).and_then(|x| x.as_f64()).unwrap_or(0.75).clamp(0.0, 1.0) * 255.0).round() as u8;
                Color::rgb(f(0), f(1), f(2))
            })
    };
    let meshes = arr("meshes");
    let nodes = arr("nodes");
    // (mesh, world matrix) for every node reached from the scene's roots.
    let mut placed: Vec<(usize, Mat4)> = Vec::new();
    let scene = json.get("scene").and_then(|s| s.as_u64()).unwrap_or(0) as usize;
    let roots: Vec<usize> = arr("scenes")
        .get(scene)
        .and_then(|s| s.get("nodes"))
        .and_then(|n| n.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_u64()).map(|x| x as usize).collect())
        .unwrap_or_default();
    if roots.is_empty() {
        placed.extend((0..meshes.len()).map(|m| (m, IDENTITY4)));
    } else {
        let mut stack: Vec<(usize, Mat4, usize)> = roots.iter().map(|r| (*r, IDENTITY4, 0)).collect();
        while let Some((n, parent, depth)) = stack.pop() {
            let Some(node) = nodes.get(n) else { continue };
            if depth > 64 {
                continue;
            }
            let m = mat_mul(&parent, &node_matrix(node));
            if let Some(mesh) = node.get("mesh").and_then(|x| x.as_u64()) {
                placed.push((mesh as usize, m));
            }
            for c in node.get("children").and_then(|x| x.as_array()).into_iter().flatten().filter_map(|x| x.as_u64()) {
                stack.push((c as usize, m, depth + 1));
            }
        }
    }
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    let mut groups: Vec<SymbolGroup> = Vec::new();
    for (mi, m) in placed {
        let Some(mesh) = meshes.get(mi) else { continue };
        for prim in mesh.get("primitives").and_then(|p| p.as_array()).into_iter().flatten() {
            if prim.get("mode").and_then(|x| x.as_u64()).unwrap_or(4) != 4 {
                continue;
            }
            let Some(pos) = prim.get("attributes").and_then(|a| a.get("POSITION")).and_then(|x| x.as_u64()).and_then(|i| read_acc(i as usize)) else {
                continue;
            };
            let base = positions.len() as u32;
            for p in &pos {
                let (x, y, z) = (p[0], p[1], p[2]);
                positions.push([
                    (m[0] * x + m[4] * y + m[8] * z + m[12]) as f32,
                    (m[1] * x + m[5] * y + m[9] * z + m[13]) as f32,
                    (m[2] * x + m[6] * y + m[10] * z + m[14]) as f32,
                ]);
            }
            let idx: Vec<u32> = match prim.get("indices").and_then(|x| x.as_u64()) {
                Some(i) => read_acc(i as usize).ok_or_else(|| bad("bad indices"))?.iter().map(|r| r[0] as u32).collect(),
                None => (0..pos.len() as u32).collect(),
            };
            let first = triangles.len() as u32;
            for t in idx.chunks(3).filter(|t| t.len() == 3) {
                if t.iter().all(|i| (*i as usize) < pos.len()) {
                    triangles.push([base + t[0], base + t[1], base + t[2]]);
                }
            }
            let count = triangles.len() as u32 - first;
            if count > 0 {
                groups.push(SymbolGroup { first, count, color: color_of(prim.get("material").and_then(|x| x.as_u64())), name: String::new() });
            }
        }
    }
    if triangles.is_empty() {
        return Err(bad("no triangles in the model"));
    }
    Ok(Raw { positions, triangles, groups })
}

/// Turn raw file geometry into a symbol: convert to millimetres, make z up,
/// and put the origin at the centre of the bottom of the back.
fn finish(raw: Raw, name: &str, opts: ImportOptions) -> Result<SymbolDef, Error> {
    if raw.triangles.is_empty() {
        return Err(Error::Corrupt("the model has no faces".into()));
    }
    let mut pts: Vec<[f32; 3]> = raw
        .positions
        .iter()
        .map(|p| if opts.y_up { [p[0], -p[2], p[1]] } else { *p })
        .collect();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &pts {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f32, f32::max);
    let unit = match opts.unit {
        ModelUnit::Auto => ModelUnit::guess(size),
        u => u,
    };
    let k = unit.mm();
    // The back is the +y side (the front faces −y, toward the viewer).
    let origin = [(lo[0] + hi[0]) / 2.0, hi[1], lo[2]];
    for p in &mut pts {
        for a in 0..3 {
            p[a] = (p[a] - origin[a]) * k;
        }
    }
    Ok(SymbolDef {
        id: Id(0),
        name: name.to_string(),
        positions: pts,
        triangles: raw.triangles,
        groups: raw.groups,
        behavior: Default::default(),
    })
}

/// Read an `.obj` or `.stl` file as a symbol named after the file.
pub fn read_model(path: &Path, opts: ImportOptions) -> Result<SymbolDef, Error> {
    let name = path
        .file_stem()
        .map_or("Symbol".into(), |s| s.to_string_lossy().into_owned());
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let bytes = std::fs::read(path)?;
    let raw = match ext.as_str() {
        "obj" => {
            let text = String::from_utf8_lossy(&bytes);
            let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
            parse_obj(&text, |f| std::fs::read_to_string(dir.join(f)).ok())?
        }
        "stl" => parse_stl(&bytes)?,
        "gltf" | "glb" => {
            let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
            let raw = parse_gltf(&bytes, |uri| std::fs::read(dir.join(uri)).ok())?;
            // glTF is in metres with y up.
            let opts = ImportOptions { unit: if opts.unit == ModelUnit::Auto { ModelUnit::Metres } else { opts.unit }, y_up: true };
            return finish(raw, &name, opts);
        }
        other => {
            return Err(Error::NotHouseCraft(format!(
                "can't import .{other} models (use OBJ, STL or glTF)"
            )));
        }
    };
    finish(raw, &name, opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBE: &str = "mtllib cube.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\nv 0 1 1\n\
usemtl red\nf 1 2 3 4\nf 5 6 7 8\nusemtl blue\nf 1 2 6 5\nf 2/1/1 3/1/1 7/1/1 6/1/1\nf -5 -1 -4 -8\nf 4 3 7 8\n";

    #[test]
    fn obj_with_colours_and_units() {
        let mtl = "newmtl red\nKd 1 0 0\nnewmtl blue\nKd 0 0 1\n";
        let raw = parse_obj(CUBE, |f| (f == "cube.mtl").then(|| mtl.to_string())).unwrap();
        assert_eq!(raw.triangles.len(), 12);
        assert_eq!(raw.groups.len(), 2);
        assert_eq!(raw.groups[0].color, Color::rgb(255, 0, 0));
        assert_eq!(raw.groups[1].count, 8);
        // A 1-unit cube guesses metres: 1000 mm, z up, origin at bottom-back-centre.
        let def = finish(raw, "cube", ImportOptions::default()).unwrap();
        let s = def.size();
        assert!(s.iter().all(|v| (v - 1000.0).abs() < 1e-3), "{s:?}");
        let (lo, hi) = def.bounds();
        assert!(lo[2].abs() < 1e-3 && hi[1].abs() < 1e-3 && (lo[0] + hi[0]).abs() < 1e-3);
    }

    #[test]
    fn stl_binary_and_text() {
        let text = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 30 0 0\nvertex 0 20 0\nendloop\nendfacet\nendsolid";
        let raw = parse_stl(text.as_bytes()).unwrap();
        assert_eq!(raw.triangles.len(), 1);
        let mut bin = vec![0u8; 80];
        bin.extend(1u32.to_le_bytes());
        bin.extend([0u8; 12]);
        for v in [[0.0f32, 0.0, 0.0], [30.0, 0.0, 0.0], [0.0, 0.0, 20.0]] {
            for x in v {
                bin.extend(x.to_le_bytes());
            }
        }
        bin.extend([0u8; 2]);
        let raw = parse_stl(&bin).unwrap();
        assert_eq!(raw.positions.len(), 3);
        let def = finish(raw, "t", ImportOptions { unit: ModelUnit::Inches, y_up: false }).unwrap();
        assert!((def.size()[0] - 30.0 * 25.4).abs() < 1e-2);
        assert!((def.size()[2] - 20.0 * 25.4).abs() < 1e-2);
    }

    #[test]
    fn gltf_embedded_triangle_with_a_node_transform() {
        // One triangle (0,0,0) (1,0,0) (0,1,0), moved 2 m along x, red.
        let mut bin = Vec::new();
        for v in [0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        for i in [0u16, 1, 2, 0] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut b64 = String::new();
        for c in bin.chunks(3) {
            let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
            for k in 0..4 {
                b64.push(if k <= c.len() { T[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
            }
        }
        let json = format!(
            r#"{{"asset": {{"version": "2.0"}}, "scene": 0, "scenes": [{{"nodes": [0]}}],
                "nodes": [{{"mesh": 0, "translation": [2, 0, 0]}}],
                "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0}}, "indices": 1, "material": 0}}]}}],
                "materials": [{{"pbrMetallicRoughness": {{"baseColorFactor": [1, 0, 0, 1]}}}}],
                "buffers": [{{"byteLength": {len}, "uri": "data:application/octet-stream;base64,{b64}"}}],
                "bufferViews": [{{"buffer": 0, "byteOffset": 0, "byteLength": 36}}, {{"buffer": 0, "byteOffset": 36, "byteLength": 6}}],
                "accessors": [{{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}}, {{"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}}]}}"#,
            len = bin.len()
        );
        let raw = parse_gltf(json.as_bytes(), |_| None).unwrap();
        assert_eq!(raw.triangles.len(), 1);
        assert_eq!(raw.positions[1], [3.0, 0.0, 0.0]);
        assert_eq!(raw.groups[0].color, Color::rgb(255, 0, 0));
        // The same as a .glb container.
        let js = json.replace(&format!(r#", "uri": "data:application/octet-stream;base64,{b64}""#), "");
        let mut jb = js.into_bytes();
        while jb.len() % 4 != 0 {
            jb.push(b' ');
        }
        let mut bb = bin.clone();
        while bb.len() % 4 != 0 {
            bb.push(0);
        }
        let total = 12 + 8 + jb.len() + 8 + bb.len();
        let mut glb = b"glTF".to_vec();
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(jb.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
        glb.extend_from_slice(&jb);
        glb.extend_from_slice(&(bb.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004E_4942u32.to_le_bytes());
        glb.extend_from_slice(&bb);
        let raw = parse_gltf(&glb, |_| None).unwrap();
        assert_eq!(raw.triangles.len(), 1);
    }
}
