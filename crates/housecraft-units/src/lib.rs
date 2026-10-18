//! Units of measure for HouseCraft.
//!
//! All lengths are stored internally as `f64` millimetres. Parsing accepts the
//! notations residential designers actually type (`12'6"`, `12'-6 1/2"`,
//! `3' 4"`, `150mm`, `3.5 m`, `1/2"`), and formatting can produce
//! architectural feet-inches rounded to a fractional denominator.

use std::borrow::Cow;
use std::fmt;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const MM_PER_INCH: f64 = 25.4;
pub const MM_PER_FOOT: f64 = 304.8;

/// A length, stored as millimetres.
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd)]
pub struct Length(f64);

impl Length {
    pub const ZERO: Length = Length(0.0);

    pub const fn from_mm(mm: f64) -> Self {
        Length(mm)
    }
    pub fn from_cm(cm: f64) -> Self {
        Length(cm * 10.0)
    }
    pub fn from_m(m: f64) -> Self {
        Length(m * 1000.0)
    }
    pub fn from_inches(inches: f64) -> Self {
        Length(inches * MM_PER_INCH)
    }
    pub fn from_feet(feet: f64) -> Self {
        Length(feet * MM_PER_FOOT)
    }
    pub fn from_feet_inches(feet: f64, inches: f64) -> Self {
        Length::from_feet(feet) + Length::from_inches(inches)
    }

    pub const fn mm(self) -> f64 {
        self.0
    }
    pub fn m(self) -> f64 {
        self.0 / 1000.0
    }
    pub fn inches(self) -> f64 {
        self.0 / MM_PER_INCH
    }
    pub fn feet(self) -> f64 {
        self.0 / MM_PER_FOOT
    }
    pub fn abs(self) -> Self {
        Length(self.0.abs())
    }
    pub fn is_finite(self) -> bool {
        self.0.is_finite()
    }

    /// Parse a length. A bare number with no unit marker is read in `default_unit`.
    pub fn parse(input: &str, default_unit: LengthUnit) -> Result<Length, ParseLengthError> {
        parse_length(input, default_unit)
    }

    /// Format for display in the given unit system.
    pub fn format(self, unit: DisplayUnit) -> String {
        format_length(self, unit)
    }
}

impl Add for Length {
    type Output = Length;
    fn add(self, rhs: Length) -> Length {
        Length(self.0 + rhs.0)
    }
}
impl AddAssign for Length {
    fn add_assign(&mut self, rhs: Length) {
        self.0 += rhs.0;
    }
}
impl Sub for Length {
    type Output = Length;
    fn sub(self, rhs: Length) -> Length {
        Length(self.0 - rhs.0)
    }
}
impl SubAssign for Length {
    fn sub_assign(&mut self, rhs: Length) {
        self.0 -= rhs.0;
    }
}
impl Mul<f64> for Length {
    type Output = Length;
    fn mul(self, rhs: f64) -> Length {
        Length(self.0 * rhs)
    }
}
impl Div<f64> for Length {
    type Output = Length;
    fn div(self, rhs: f64) -> Length {
        Length(self.0 / rhs)
    }
}
impl Neg for Length {
    type Output = Length;
    fn neg(self) -> Length {
        Length(-self.0)
    }
}
impl std::iter::Sum for Length {
    fn sum<I: Iterator<Item = Length>>(iter: I) -> Length {
        iter.fold(Length::ZERO, Add::add)
    }
}

impl fmt::Display for Length {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} mm", self.0)
    }
}

/// Serialized as a plain number of millimetres.
impl Serialize for Length {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f64(self.0)
    }
}

/// Deserialized from a number (millimetres) or a string in any supported notation
/// (`"12'6\""`, `"3.5 m"`, `"150"` = 150 mm).
impl<'de> Deserialize<'de> for Length {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Num(f64),
            Str(String),
        }
        match Repr::deserialize(d)? {
            Repr::Num(mm) => Ok(Length(mm)),
            Repr::Str(s) => {
                Length::parse(&s, LengthUnit::Millimeters).map_err(serde::de::Error::custom)
            }
        }
    }
}

impl JsonSchema for Length {
    fn schema_name() -> Cow<'static, str> {
        "Length".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A length. A number is millimetres; a string may use units, e.g. \"12'6\\\"\", \"12'-6 1/2\\\"\", \"30 1/2\\\"\", \"3.5 m\", \"150mm\".",
            "anyOf": [{ "type": "number" }, { "type": "string" }]
        })
    }
}

/// A 2D point in plan coordinates (x east, y north). Serialized as `[x, y]`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Point2(pub Length, pub Length);

impl Point2 {
    pub fn new(x: Length, y: Length) -> Self {
        Point2(x, y)
    }
    pub fn from_mm(x: f64, y: f64) -> Self {
        Point2(Length(x), Length(y))
    }
    pub fn x(self) -> Length {
        self.0
    }
    pub fn y(self) -> Length {
        self.1
    }
    pub fn distance(self, other: Point2) -> Length {
        Length((other.0.0 - self.0.0).hypot(other.1.0 - self.1.0))
    }
}

/// Units used to interpret bare numbers during parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LengthUnit {
    Millimeters,
    Centimeters,
    Meters,
    Inches,
    Feet,
}

impl LengthUnit {
    fn to_length(self, v: f64) -> Length {
        match self {
            LengthUnit::Millimeters => Length::from_mm(v),
            LengthUnit::Centimeters => Length::from_cm(v),
            LengthUnit::Meters => Length::from_m(v),
            LengthUnit::Inches => Length::from_inches(v),
            LengthUnit::Feet => Length::from_feet(v),
        }
    }
}

/// How lengths are presented to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "unit")]
pub enum DisplayUnit {
    /// Architectural `12'-6 1/2"`, rounded to `1/denominator` inch.
    FeetInches {
        denominator: u32,
    },
    /// Inches only, `150 1/2"`.
    Inches {
        denominator: u32,
    },
    Millimeters,
    Meters {
        decimals: u8,
    },
}

impl Default for DisplayUnit {
    fn default() -> Self {
        DisplayUnit::FeetInches { denominator: 16 }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("invalid length {input:?}: {reason}")]
pub struct ParseLengthError {
    pub input: String,
    pub reason: &'static str,
}

fn parse_length(input: &str, default_unit: LengthUnit) -> Result<Length, ParseLengthError> {
    let err = |reason| ParseLengthError {
        input: input.to_string(),
        reason,
    };
    let s = input.trim();
    if s.is_empty() {
        return Err(err("empty"));
    }
    let (neg, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, s),
    };

    let value = if let Some((feet, rest)) = s.split_once('\'') {
        let feet = parse_number(feet.trim()).ok_or_else(|| err("bad feet value"))?;
        let rest = rest.trim();
        let rest = rest.strip_prefix('-').unwrap_or(rest).trim();
        let rest = rest.strip_suffix('"').unwrap_or(rest).trim();
        let inches = if rest.is_empty() {
            0.0
        } else {
            parse_mixed(rest).ok_or_else(|| err("bad inches value"))?
        };
        Length::from_feet_inches(feet, inches)
    } else if let Some(rest) = s.strip_suffix('"').or_else(|| s.strip_suffix("in")) {
        Length::from_inches(parse_mixed(rest.trim()).ok_or_else(|| err("bad inches value"))?)
    } else if let Some(rest) = s.strip_suffix("ft") {
        Length::from_feet(parse_number(rest.trim()).ok_or_else(|| err("bad feet value"))?)
    } else if let Some(rest) = s.strip_suffix("mm") {
        Length::from_mm(parse_number(rest.trim()).ok_or_else(|| err("bad number"))?)
    } else if let Some(rest) = s.strip_suffix("cm") {
        Length::from_cm(parse_number(rest.trim()).ok_or_else(|| err("bad number"))?)
    } else if let Some(rest) = s.strip_suffix('m') {
        Length::from_m(parse_number(rest.trim()).ok_or_else(|| err("bad number"))?)
    } else {
        default_unit.to_length(parse_mixed(s).ok_or_else(|| err("bad number"))?)
    };

    if !value.is_finite() {
        return Err(err("not finite"));
    }
    Ok(if neg { -value } else { value })
}

/// `"6"`, `"6.5"`, `"1/2"`, `"6 1/2"`.
fn parse_mixed(s: &str) -> Option<f64> {
    let mut parts = s.split_whitespace();
    let first = parts.next()?;
    let second = parts.next();
    if parts.next().is_some() {
        return None;
    }
    match second {
        None => parse_number(first),
        Some(frac) if !first.contains('/') && frac.contains('/') => {
            Some(parse_number(first)? + parse_number(frac)?)
        }
        Some(_) => None,
    }
}

/// `"6"`, `"6.5"`, `"1/2"` (no sign; signs are handled by the caller).
fn parse_number(s: &str) -> Option<f64> {
    if let Some((n, d)) = s.split_once('/') {
        let n: f64 = n.trim().parse().ok()?;
        let d: f64 = d.trim().parse().ok()?;
        (d != 0.0 && n >= 0.0).then_some(n / d)
    } else if s.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        s.parse().ok()
    } else {
        None
    }
}

fn format_length(len: Length, unit: DisplayUnit) -> String {
    match unit {
        DisplayUnit::Millimeters => format!("{} mm", round_to(len.mm(), 0)),
        DisplayUnit::Meters { decimals } => {
            format!("{:.*} m", decimals as usize, len.m())
        }
        DisplayUnit::Inches { denominator } => {
            let (neg, units) = to_fraction_units(len.inches(), denominator);
            let d = denominator.max(1) as u64;
            format!(
                "{}{}\"",
                if neg { "-" } else { "" },
                mixed(units / d, units % d, d)
            )
        }
        DisplayUnit::FeetInches { denominator } => {
            let (neg, units) = to_fraction_units(len.inches(), denominator);
            let d = denominator.max(1) as u64;
            let per_foot = 12 * d;
            let feet = units / per_foot;
            let rem = units % per_foot;
            format!(
                "{}{}'-{}\"",
                if neg { "-" } else { "" },
                feet,
                match mixed(0, rem % d, d) {
                    frac if frac == "0" => (rem / d).to_string(),
                    frac => format!("{} {frac}", rem / d),
                }
            )
        }
    }
}

/// Returns (is_negative, |value| in units of 1/denominator), rounded.
fn to_fraction_units(value: f64, denominator: u32) -> (bool, u64) {
    let d = denominator.max(1) as f64;
    let units = (value.abs() * d).round() as u64;
    (value < 0.0 && units != 0, units)
}

fn mixed(whole: u64, num: u64, den: u64) -> String {
    if num == 0 {
        return whole.to_string();
    }
    let g = gcd(num, den);
    let (n, d) = (num / g, den / g);
    if whole == 0 {
        format!("{n}/{d}")
    } else {
        format!("{whole} {n}/{d}")
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn round_to(v: f64, decimals: i32) -> f64 {
    let p = 10f64.powi(decimals);
    (v * p).round() / p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> f64 {
        Length::parse(s, LengthUnit::Inches).unwrap().inches()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn parses_imperial() {
        assert!(close(p("12'6\""), 150.0));
        assert!(close(p("12' 6\""), 150.0));
        assert!(close(p("12'-6 1/2\""), 150.5));
        assert!(close(p("12'"), 144.0));
        assert!(close(p("12'6"), 150.0));
        assert!(close(p("30 1/2\""), 30.5));
        assert!(close(p("1/2\""), 0.5));
        assert!(close(p("8ft"), 96.0));
        assert!(close(p("14in"), 14.0));
        assert!(close(p("-3'"), -36.0));
    }

    #[test]
    fn parses_metric_and_bare() {
        let mm = |s| Length::parse(s, LengthUnit::Millimeters).unwrap().mm();
        assert!(close(mm("150mm"), 150.0));
        assert!(close(mm("15 cm"), 150.0));
        assert!(close(mm("3.5m"), 3500.0));
        assert!(close(mm("150"), 150.0));
        assert!(close(p("6 1/2"), 6.5));
    }

    #[test]
    fn rejects_garbage() {
        for s in ["", "abc", "12'x", "1/0\"", "1 2 3", "--5"] {
            assert!(
                Length::parse(s, LengthUnit::Inches).is_err(),
                "{s:?} parsed"
            );
        }
    }

    #[test]
    fn formats_feet_inches() {
        let f = |inches: f64| {
            Length::from_inches(inches).format(DisplayUnit::FeetInches { denominator: 16 })
        };
        assert_eq!(f(150.5), "12'-6 1/2\"");
        assert_eq!(f(144.0), "12'-0\"");
        assert_eq!(f(0.0625), "0'-0 1/16\"");
        assert_eq!(f(11.999), "1'-0\"");
        assert_eq!(f(-30.25), "-2'-6 1/4\"");
    }

    #[test]
    fn formats_metric() {
        assert_eq!(
            Length::from_mm(3810.4).format(DisplayUnit::Millimeters),
            "3810 mm"
        );
        assert_eq!(
            Length::from_mm(3810.0).format(DisplayUnit::Meters { decimals: 3 }),
            "3.810 m"
        );
    }

    #[test]
    fn serde_accepts_numbers_and_strings() {
        let a: Length = serde_json::from_str("2540").unwrap();
        let b: Length = serde_json::from_str(r#""8'4\"""#).unwrap();
        assert!(close(a.mm(), b.mm()));
        assert_eq!(serde_json::to_string(&a).unwrap(), "2540.0");
        let pt: Point2 = serde_json::from_str(r#"[0, "10'"]"#).unwrap();
        assert!(close(pt.y().feet(), 10.0));
    }
}
