use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dimension {
    Length,
    Angle,
    Scalar,
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Dimension::Length => write!(f, "length"),
            Dimension::Angle => write!(f, "angle"),
            Dimension::Scalar => write!(f, "scalar"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Mm,
    Cm,
    M,
    In,
    Ft,
    Deg,
    Rad,
    None,
}

impl Unit {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mm" => Some(Unit::Mm),
            "cm" => Some(Unit::Cm),
            "m" => Some(Unit::M),
            "in" => Some(Unit::In),
            "ft" => Some(Unit::Ft),
            "deg" => Some(Unit::Deg),
            "rad" => Some(Unit::Rad),
            "" => Some(Unit::None),
            _ => None,
        }
    }

    pub fn dimension(self) -> Dimension {
        match self {
            Unit::Mm | Unit::Cm | Unit::M | Unit::In | Unit::Ft => Dimension::Length,
            Unit::Deg | Unit::Rad => Dimension::Angle,
            Unit::None => Dimension::Scalar,
        }
    }

    /// Factor to convert this unit into canonical base unit (mm for length, deg for angle).
    pub fn to_canonical_factor(self) -> f64 {
        match self {
            Unit::Mm => 1.0,
            Unit::Cm => 10.0,
            Unit::M => 1000.0,
            Unit::In => 25.4,
            Unit::Ft => 304.8,
            Unit::Deg => 1.0,
            Unit::Rad => 180.0 / std::f64::consts::PI,
            Unit::None => 1.0,
        }
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unit::Mm => write!(f, "mm"),
            Unit::Cm => write!(f, "cm"),
            Unit::M => write!(f, "m"),
            Unit::In => write!(f, "in"),
            Unit::Ft => write!(f, "ft"),
            Unit::Deg => write!(f, "deg"),
            Unit::Rad => write!(f, "rad"),
            Unit::None => write!(f, ""),
        }
    }
}

/// Strongly-typed dimensional quantity stored with its original unit and canonical value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Quantity {
    pub raw_value: f64,
    pub unit: Unit,
}

impl Quantity {
    pub fn new(raw_value: f64, unit: Unit) -> Self {
        Self { raw_value, unit }
    }

    pub fn mm(val: f64) -> Self {
        Self {
            raw_value: val,
            unit: Unit::Mm,
        }
    }

    pub fn dimension(self) -> Dimension {
        self.unit.dimension()
    }

    /// Convert length quantity to canonical millimeters, rejecting non-length units.
    pub fn to_mm(self) -> Result<f64, String> {
        if self.dimension() != Dimension::Length {
            return Err(format!(
                "Expected a length quantity (mm/cm/m/in/ft), got {} ({})",
                self.dimension(),
                self.unit
            ));
        }
        Ok(self.raw_value * self.unit.to_canonical_factor())
    }

    /// Convert angle quantity to degrees, rejecting non-angle units.
    pub fn to_deg(self) -> Result<f64, String> {
        if self.dimension() != Dimension::Angle {
            return Err(format!(
                "Expected an angle quantity (deg/rad), got {} ({})",
                self.dimension(),
                self.unit
            ));
        }
        Ok(self.raw_value * self.unit.to_canonical_factor())
    }

    pub fn add(self, other: Quantity) -> Result<Quantity, String> {
        if self.dimension() != other.dimension() {
            return Err(format!(
                "Dimension mismatch: cannot add {} ({}) and {} ({})",
                self.dimension(),
                self.unit,
                other.dimension(),
                other.unit
            ));
        }
        let canon = self.raw_value * self.unit.to_canonical_factor()
            + other.raw_value * other.unit.to_canonical_factor();
        let target_unit = match self.dimension() {
            Dimension::Length => Unit::Mm,
            Dimension::Angle => Unit::Deg,
            Dimension::Scalar => Unit::None,
        };
        Ok(Quantity::new(canon, target_unit))
    }
}
