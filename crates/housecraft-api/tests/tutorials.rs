//! Chief Architect tutorials replayed in HouseCraft.
//!
//! Each `docs/tutorials/*.json` file follows one tutorial from Chief's
//! Tutorial Guide step by step: the commands that do the step here, and
//! checks on the result. Steps HouseCraft cannot do yet are recorded as
//! gaps (with the FEATURES.md rows that would close them) and skipped, so
//! the files double as a parity tracker for the tutorials.
//!
//! In a step's commands, `"$name"` stands for an id saved by an earlier
//! step, `"$name.2"` for the third of a saved list, `"$named:Floor 1"` for
//! the object with that name (`"$named:default_set:Default"` to give its
//! kind too), `"$room_at:x,y"` / `"$wall_at:x,y"` (mm) for the room or wall
//! there and `"$floor1"` for the first floor.

use std::collections::BTreeMap;
use std::path::PathBuf;

use housecraft_api::model::*;
use housecraft_api::units::{Length, LengthUnit};
use housecraft_api::{Command, Session, query};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Tutorial {
    tutorial: String,
    /// An earlier tutorial whose steps run first (the plan carries on).
    #[serde(default)]
    include: Option<String>,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
struct Step {
    /// What the tutorial does at this point (our words).
    step: String,
    #[serde(default)]
    commands: Vec<Value>,
    /// Save created ids: name -> object kind (all created of that kind, in order).
    #[serde(default)]
    save: BTreeMap<String, String>,
    #[serde(default)]
    expect: Vec<Value>,
    /// Select `wall`, click the automatic dimension between it and `from`
    /// and type `length` (run after the commands).
    #[serde(default)]
    dimension_moves: Vec<DimMove>,
    /// Copy objects and Paste Hold Position on another floor.
    copy_to_floor: Option<CopyToFloor>,
    /// Something the tutorial does that HouseCraft cannot yet.
    gap: Option<String>,
}

#[derive(Deserialize)]
struct CopyToFloor {
    ids: Vec<String>,
    floor: String,
}

#[derive(Deserialize)]
struct DimMove {
    wall: String,
    from: String,
    length: String,
}

/// The automatic dimension segment between two walls: one end within each
/// wall's thickness, across both.
fn segment_between(doc: &Document, wall: Id, from: Id) -> Option<(Id, usize)> {
    use housecraft_api::geom::{DVec2, v2};
    let band = |id: Id| {
        let w = doc.get::<Wall>(id)?;
        let t = doc.get::<WallType>(w.wall_type)?.thickness().mm();
        Some((v2(w.start), (v2(w.end) - v2(w.start)).normalize_or_zero(), t / 2.0 + 1.0))
    };
    let (ws, wd, wt) = band(wall)?;
    let (fs, fd, ft) = band(from)?;
    let on = |p: DVec2, s: DVec2, d: DVec2, t: f64, axis: DVec2| d.dot(axis).abs() < 0.1 && (p - s).dot(axis).abs() <= t;
    for dim in doc.iter::<Dimension>().filter(|d| d.auto) {
        let DimensionGeometry::Linear { points, orientation, .. } = &dim.geometry else { continue };
        let axis = match orientation {
            DimOrientation::Horizontal => DVec2::X,
            DimOrientation::Vertical => DVec2::Y,
            DimOrientation::Aligned => continue,
        };
        for (i, w) in points.windows(2).enumerate() {
            let (a, b) = (v2(w[0]), v2(w[1]));
            if (on(a, ws, wd, wt, axis) && on(b, fs, fd, ft, axis)) || (on(b, ws, wd, wt, axis) && on(a, fs, fd, ft, axis)) {
                return Some((dim.id, i));
            }
        }
    }
    None
}

struct Run {
    session: Session,
    vars: BTreeMap<String, Vec<Id>>,
}

impl Run {
    fn doc(&self) -> &Document {
        self.session.document()
    }

    fn resolve(&self, s: &str) -> Option<Id> {
        let rest = s.strip_prefix('$')?;
        if rest == "floor1" {
            return Some(self.doc().floors_by_elevation().iter().find(|f| f.foundation.is_none())?.id);
        }
        // "$room_at:x,y" / "$wall_at:x,y" (mm): the room or wall there.
        let at = |spec: &str| -> Option<housecraft_api::geom::DVec2> {
            let (x, y) = spec.split_once(',')?;
            Some(housecraft_api::geom::DVec2::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
        };
        // "…@Floor name" picks the floor (default: the first floor).
        let (rest, on) = match rest.split_once('@') {
            Some((r, f)) => (r, Some(self.resolve(&format!("$named:floor:{f}"))?)),
            None => (rest, None),
        };
        if let Some(spec) = rest.strip_prefix("room_at:") {
            let p = at(spec)?;
            let floor = match on {
                Some(f) => f,
                None => self.resolve("$floor1")?,
            };
            let regions = housecraft_api::geom::rooms::detect(self.doc(), floor);
            let i = regions.room_at(p)?;
            return self.doc().rooms_on(floor).find(|r| regions.room_at(housecraft_api::geom::v2(r.anchor)) == Some(i)).map(|r| r.id);
        }
        if let Some(spec) = rest.strip_prefix("wall_at:") {
            let p = at(spec)?;
            let floor = match on {
                Some(f) => f,
                None => self.resolve("$floor1")?,
            };
            return self
                .doc()
                .iter::<Wall>()
                .filter(|w| w.floor == floor)
                .map(|w| {
                    let (c, _) = housecraft_api::geom::math::closest_on_segment(p, housecraft_api::geom::v2(w.start), housecraft_api::geom::v2(w.end));
                    (c.distance(p), w.id)
                })
                .filter(|(d, _)| *d < 300.0)
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, id)| id);
        }
        if let Some(name) = rest.strip_prefix("named:") {
            // "$named:kind:Name" when the name alone is ambiguous.
            let (kind, name) = match name.split_once(':') {
                Some((k, n)) if !k.contains(' ') => (Some(k), n),
                _ => (None, name),
            };
            let v = serde_json::to_value(self.doc()).ok()?;
            return v["objects"]
                .as_object()?
                .values()
                .find(|o| o["name"] == name && kind.is_none_or(|k| o["kind"] == k))
                .and_then(|o| o["id"].as_u64())
                .map(Id);
        }
        let (name, index) = match rest.split_once('.') {
            Some((n, i)) => (n, i.parse().ok()?),
            None => (rest, 0),
        };
        self.vars.get(name)?.get(index).copied()
    }

    /// Replace `$…` strings with ids.
    fn substitute(&self, v: &Value, step: &str) -> Value {
        match v {
            Value::String(s) if s.starts_with('$') => {
                let id = self.resolve(s).unwrap_or_else(|| panic!("{step}: unknown reference {s}"));
                Value::from(id.0)
            }
            Value::Array(a) => Value::Array(a.iter().map(|x| self.substitute(x, step)).collect()),
            Value::Object(m) => Value::Object(m.iter().map(|(k, x)| (k.clone(), self.substitute(x, step))).collect()),
            other => other.clone(),
        }
    }
}

fn length(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => Length::parse(s, LengthUnit::Inches).ok().map(|l| l.mm()),
        _ => None,
    }
}

/// Every measured segment of the floor's automatic dimensions, in mm.
fn auto_dimension_lengths(doc: &Document) -> Vec<f64> {
    let mut out = Vec::new();
    for d in doc.iter::<Dimension>().filter(|d| d.auto) {
        if let DimensionGeometry::Linear { points, orientation, .. } = &d.geometry {
            for w in points.windows(2) {
                let (a, b) = (w[0], w[1]);
                let (dx, dy) = ((b.x() - a.x()).mm().abs(), (b.y() - a.y()).mm().abs());
                out.push(match orientation {
                    DimOrientation::Horizontal => dx,
                    DimOrientation::Vertical => dy,
                    DimOrientation::Aligned => dx.hypot(dy),
                });
            }
        }
    }
    out
}

/// Check one expectation; `Err` says what differs.
fn check(run: &Run, e: &Value, step: &str) -> Result<(), String> {
    let doc = run.doc();
    let tol = 25.4 / 32.0;
    if let Some(kind) = e.get("count").and_then(Value::as_str) {
        let want = e["is"].as_u64().expect("count needs is") as usize;
        let v = serde_json::to_value(doc).unwrap();
        let n = v["objects"].as_object().unwrap().values().filter(|o| o["kind"] == kind).count();
        return if n == want { Ok(()) } else { Err(format!("{n} {kind} objects, expected {want}")) };
    }
    if let Some(n) = e.get("roof_planes").and_then(Value::as_u64) {
        // Sloped roof planes (not the gable-end infills).
        let found = doc.iter::<RoofPlane>().filter(|r| !r.gable).count();
        return if found == n as usize { Ok(()) } else { Err(format!("{found} roof planes, expected {n}")) };
    }
    if let Some(n) = e.get("rooms").and_then(Value::as_u64) {
        let floor = run.resolve("$floor1").unwrap();
        let found = housecraft_api::geom::rooms::detect(doc, floor).rooms.len();
        return if found == n as usize { Ok(()) } else { Err(format!("{found} enclosed rooms, expected {n}")) };
    }
    if let Some(target) = e.get("get") {
        let id = target.as_u64().map(Id).ok_or(format!("unknown {target}"))?;
        let obj = query::get_object(doc, id).ok_or("no such object")?;
        let path = e["path"].as_str().expect("get needs path");
        let actual = obj.pointer(path).ok_or(format!("{target} has no {path}"))?;
        let want = &e["is"];
        let same = match (length(actual), length(want)) {
            (Some(a), Some(b)) if actual.is_number() => (a - b).abs() < tol,
            _ => actual == want,
        };
        return if same { Ok(()) } else { Err(format!("{target}{path} is {actual}, expected {want}")) };
    }
    if let Some(list) = e.get("auto_dimensions_include").and_then(Value::as_array) {
        let mut have = auto_dimension_lengths(doc);
        for want in list {
            let w = length(want).ok_or(format!("bad length {want}"))?;
            match have.iter().position(|h| (h - w).abs() < tol) {
                Some(i) => {
                    have.remove(i);
                }
                None => {
                    let shown: Vec<String> = auto_dimension_lengths(doc).iter().map(|h| Length::from_mm(*h).format(doc.settings.display_unit)).collect();
                    return Err(format!("no automatic dimension of {want}; have {}", shown.join(", ")));
                }
            }
        }
        return Ok(());
    }
    if let Some(min) = e.get("living_area_at_least").and_then(Value::as_f64) {
        let floor = run.resolve("$floor1").unwrap();
        let sqft: f64 = query::room_areas(doc, floor).iter().map(|(_, a)| a / 92_903.04).sum();
        return if sqft >= min { Ok(()) } else { Err(format!("room area {sqft:.0} sq ft, expected at least {min}")) };
    }
    if let Some(target) = e.get("not_built") {
        let id = target.as_u64().map(Id).ok_or(format!("unknown {target}"))?;
        let mesh = housecraft_api::geom::mesh::build(doc, None);
        return if mesh.parts.iter().any(|p| p.object == id) { Err(format!("#{} is built in 3D", id.0)) } else { Ok(()) };
    }
    if let Some(target) = e.get("built") {
        let id = target.as_u64().map(Id).ok_or(format!("unknown {target}"))?;
        let mesh = housecraft_api::geom::mesh::build(doc, None);
        return if mesh.parts.iter().any(|p| p.object == id) { Ok(()) } else { Err(format!("#{} is not built in 3D", id.0)) };
    }
    if let Some(target) = e.get("stair_reaches") {
        // {"stair_reaches": id, "floor": id, "max_riser": length}: the stair climbs
        // exactly to that floor with no riser taller than allowed.
        let id = target.as_u64().map(Id).ok_or(format!("unknown {target}"))?;
        let st = doc.get::<Stair>(id).ok_or("no such stair")?;
        let g = housecraft_api::geom::stairs::geometry(doc, st);
        let top = g.base_z + g.risers as f64 * g.riser_height;
        let floor = e["floor"].as_u64().map(Id).and_then(|f| doc.get::<Floor>(f)).ok_or("no such floor")?;
        if (top - floor.elevation.mm()).abs() > 1.0 {
            return Err(format!("stair tops out at {:.0} mm, the floor is at {:.0} mm", top, floor.elevation.mm()));
        }
        if let Some(max) = e.get("max_riser").and_then(length)
            && g.riser_height > max + 0.1
        {
            return Err(format!("{} risers of {:.1} mm, more than {:.1}", g.risers, g.riser_height, max));
        }
        return Ok(());
    }
    if let Some(text) = e.get("text_exists").and_then(Value::as_str) {
        return if doc.iter::<Text>().any(|t| t.text == text) { Ok(()) } else { Err(format!("no text \"{text}\"")) };
    }
    panic!("{step}: unknown expectation {e}");
}

fn load(path: &std::path::Path) -> Tutorial {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn run_tutorial(path: &std::path::Path) -> (usize, Vec<String>, Vec<String>) {
    let t = load(path);
    let mut run = Run { session: Session::new(Document::new(&t.tutorial)), vars: BTreeMap::new() };
    // Earlier tutorials first: this one carries on from their plan.
    let mut chain = Vec::new();
    let mut inc = t.include.clone();
    while let Some(name) = inc {
        let earlier = load(&path.with_file_name(&name));
        inc = earlier.include.clone();
        chain.push(earlier);
    }
    for earlier in chain.iter().rev() {
        let (_, _, failures) = run_steps(&mut run, earlier);
        assert!(failures.is_empty(), "{} (included) failed: {failures:?}", earlier.tutorial);
    }
    run_steps(&mut run, &t)
}

fn run_steps(run: &mut Run, t: &Tutorial) -> (usize, Vec<String>, Vec<String>) {
    let (mut done, mut gaps, mut failures) = (0, Vec::new(), Vec::new());
    for (i, s) in t.steps.iter().enumerate() {
        let label = format!("{} step {}: {}", t.tutorial, i + 1, s.step);
        if let Some(g) = &s.gap {
            gaps.push(format!("{label} — {g}"));
            continue;
        }
        let commands: Vec<Command> = s
            .commands
            .iter()
            .map(|c| serde_json::from_value(run.substitute(c, &label)).unwrap_or_else(|e| panic!("{label}: {e} in {c}")))
            .collect();
        if !commands.is_empty() {
            match run.session.apply(&commands) {
                Ok(out) => {
                    for (name, kind) in &s.save {
                        let ids: Vec<Id> = out
                            .created
                            .iter()
                            .copied()
                            .filter(|id| query::get_object(run.doc(), *id).is_some_and(|o| o["kind"] == kind.as_str()))
                            .collect();
                        run.vars.insert(name.clone(), ids);
                    }
                }
                Err(e) => {
                    // Shown now too: a later step referring to what this one
                    // should have made stops the run.
                    eprintln!("{label}: {e}");
                    failures.push(format!("{label}: {e}"));
                    continue;
                }
            }
        }
        if let Some(c) = &s.copy_to_floor {
            let floor = run.resolve(&c.floor).expect("floor");
            let objects: Vec<Value> = c
                .ids
                .iter()
                .map(|r| query::get_object(run.doc(), run.resolve(r).unwrap_or_else(|| panic!("{label}: unknown {r}"))).expect("object"))
                .collect();
            match run.session.apply(&[Command::InsertObjects { objects, floor: Some(floor), dx: Length::ZERO, dy: Length::ZERO }]) {
                Ok(out) => {
                    for (name, kind) in &s.save {
                        let ids: Vec<Id> = out.created.iter().copied().filter(|id| query::get_object(run.doc(), *id).is_some_and(|o| o["kind"] == kind.as_str())).collect();
                        run.vars.insert(name.clone(), ids);
                    }
                }
                Err(e) => {
                    failures.push(format!("{label}: {e}"));
                    continue;
                }
            }
        }
        let mut moved = true;
        for m in &s.dimension_moves {
            let (wall, from) = (run.resolve(&m.wall).expect("wall"), run.resolve(&m.from).expect("from"));
            let Some((dimension, segment)) = segment_between(run.doc(), wall, from) else {
                failures.push(format!("{label}: no automatic dimension between {} and {}", m.wall, m.from));
                moved = false;
                break;
            };
            let length = Length::parse(&m.length, LengthUnit::Inches).expect("length");
            if let Err(e) = run.session.apply(&[Command::MoveWallByDimension { dimension, segment, wall, length }]) {
                failures.push(format!("{label}: {e}"));
                moved = false;
                break;
            }
        }
        if !moved {
            continue;
        }
        for e in &s.expect {
            // {"missing": "$wall_at:…"}: nothing is there.
            if let Some(r) = e.get("missing").and_then(Value::as_str) {
                if let Some(id) = run.resolve(r) {
                    failures.push(format!("{label}: expected nothing at {r}, found #{}", id.0));
                }
                continue;
            }
            let e = run.substitute(e, &label);
            if let Err(why) = check(run, &e, &label) {
                failures.push(format!("{label}: {why}"));
            }
        }
        done += 1;
    }
    (done, gaps, failures)
}

#[test]
fn chief_tutorials() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/tutorials");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    assert!(!files.is_empty(), "no tutorials in {}", dir.display());
    let mut all_failures = Vec::new();
    for f in &files {
        let (done, gaps, failures) = run_tutorial(f);
        println!("{}: {done} steps replayed, {} gaps, {} failures", f.file_name().unwrap().to_string_lossy(), gaps.len(), failures.len());
        for g in &gaps {
            println!("  gap: {g}");
        }
        all_failures.extend(failures);
    }
    assert!(all_failures.is_empty(), "tutorial steps failed:\n{}", all_failures.join("\n"));
}
