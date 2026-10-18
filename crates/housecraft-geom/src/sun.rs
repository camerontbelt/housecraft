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
