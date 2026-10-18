//! Sun position from latitude, date and solar time.

/// Compass bearing (degrees clockwise from north) and altitude (degrees above
/// the horizon) of the sun at `latitude` degrees, on `day_of_year` (1-365),
/// at local solar `hour` (12 = solar noon). Accurate to about a degree, which
/// is plenty for shadow studies.
pub fn position(latitude: f64, day_of_year: u32, hour: f64) -> (f64, f64) {
    let lat = latitude.to_radians();
    let decl = (23.44f64).to_radians()
        * ((360.0 / 365.0 * (day_of_year as f64 - 81.0)).to_radians()).sin();
    let h = (15.0 * (hour - 12.0)).to_radians();
    let alt = (lat.sin() * decl.sin() + lat.cos() * decl.cos() * h.cos()).asin();
    // Azimuth from north, clockwise.
    let az = (-h.sin()).atan2(decl.tan() * lat.cos() - lat.sin() * h.cos());
    (az.to_degrees().rem_euclid(360.0), alt.to_degrees())
}

/// Unit vector toward the sun (z up, plan axes) from the document's north
/// angle and sun bearing/altitude.
pub fn direction(doc: &housecraft_model::Document) -> glam::DVec3 {
    let sun = doc.settings.sun;
    // Azimuth is clockwise from north; plan angles are counter-clockwise.
    let bearing = (doc.settings.north - sun.azimuth).to_radians();
    let alt = sun.altitude.to_radians();
    glam::DVec3::new(
        alt.cos() * bearing.cos(),
        alt.cos() * bearing.sin(),
        alt.sin(),
    )
}

/// Ground level in mm: 8" below the platform of the lowest storey when there
/// is a foundation, else just under the lowest floor.
pub fn grade(doc: &housecraft_model::Document) -> Option<f64> {
    let floors = doc.floors_by_elevation();
    let has_foundation = floors.iter().any(|f| f.foundation.is_some());
    let first = floors.iter().find(|f| f.foundation.is_none())?;
    Some(if has_foundation {
        (first.elevation - first.floor_structure).mm() - 203.2
    } else {
        first.elevation.mm() - 20.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equinox_noon_and_summer_morning() {
        // Equinox noon at 40°N: due south, 50° up.
        let (az, alt) = position(40.0, 80, 12.0);
        assert!((az - 180.0).abs() < 1.0, "{az}");
        assert!((alt - 50.0).abs() < 1.5, "{alt}");
        // June 21 at 9 am, 40°N: south-east-ish morning sun, well up.
        let (az, alt) = position(40.0, 172, 9.0);
        assert!(az > 70.0 && az < 110.0, "{az}");
        assert!(alt > 35.0 && alt < 55.0, "{alt}");
        // Southern hemisphere noon sun is to the north.
        let (az, _) = position(-33.9, 172, 12.0);
        assert!(az < 1.0 || az > 359.0, "{az}");
    }
}
