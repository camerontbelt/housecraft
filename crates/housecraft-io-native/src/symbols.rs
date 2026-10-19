//! Reading 3D models to import as symbols: Wavefront OBJ (with MTL
//! colours) and STL (binary or text).

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
        other => {
            return Err(Error::NotHouseCraft(format!(
                "can't import .{other} models (use OBJ or STL)"
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
}
