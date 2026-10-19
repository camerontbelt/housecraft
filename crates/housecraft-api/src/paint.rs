//! The material painter and plan materials manager.

use housecraft_model::*;

use crate::ApiError;
use crate::command::{Outcome, PaintScope};
use crate::ops::{self, expect, invalid};

pub(crate) fn paint(
    doc: &mut Document,
    material: Option<Id>,
    scope: PaintScope,
    ids: &[Id],
    surface: Option<Surface>,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    if let Some(m) = material {
        expect::<Material>(doc, m)?;
    }
    if scope == PaintScope::Plan {
        let Some(surface) = surface else {
            return invalid("painting the whole plan needs a surface (e.g. \"floor\")");
        };
        let set = doc.settings.active_default_set;
        let d = doc
            .get_mut::<DefaultSet>(set)
            .ok_or(ApiError::NotFound(set))?;
        match material {
            Some(m) => d.materials.insert(surface, m),
            None => d.materials.remove(&surface),
        };
        out.modified.push(set);
        return Ok(());
    }
    if ids.is_empty() {
        return invalid("choose the objects to paint");
    }
    let mut targets = Vec::new();
    for id in ids {
        if doc.object(*id).is_none() {
            return Err(ApiError::NotFound(*id));
        }
        let target = match scope {
            PaintScope::Floor if doc.kind_of(*id) == Some(ObjectKind::Floor) => *id,
            PaintScope::Floor => doc
                .floor_of(*id)
                .filter(|f| doc.kind_of(*f) == Some(ObjectKind::Floor))
                .ok_or_else(|| ApiError::Invalid(format!("{id} is not on a floor")))?,
            _ => *id,
        };
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    // Painting a surface replaces what was painted on it; painting every
    // surface (or resetting) clears all the object's paint.
    let old: Vec<Id> = doc
        .iter::<MaterialAssignment>()
        .filter(|a| targets.contains(&a.object) && (surface.is_none() || a.surface == surface))
        .map(|a| a.id)
        .collect();
    if !old.is_empty() {
        ops::delete(doc, &old, out)?;
    }
    if let Some(material) = material {
        for object in targets {
            out.created.push(doc.insert(MaterialAssignment {
                id: Id(0),
                object,
                surface,
                material,
            }));
        }
    }
    Ok(())
}

pub(crate) fn replace(
    doc: &mut Document,
    from: Id,
    to: Id,
    delete: bool,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    expect::<Material>(doc, from)?;
    expect::<Material>(doc, to)?;
    if from == to {
        return invalid("choose two different materials");
    }
    for object in doc.objects.values_mut() {
        let changed = match object {
            Object::MaterialAssignment(a) if a.material == from => {
                a.material = to;
                true
            }
            Object::DefaultSet(d) => {
                let mut any = false;
                for m in d.materials.values_mut().filter(|m| **m == from) {
                    *m = to;
                    any = true;
                }
                any
            }
            _ => false,
        };
        if changed {
            out.modified.push(object.id());
        }
    }
    if delete {
        ops::delete(doc, &[from], out)?;
    }
    Ok(())
}
