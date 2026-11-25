use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dimension {
    Length,
    Angle,
    Voltage,
    Current,
    Resistance,
    Capacitance,
    Inductance,
    Frequency,
    Power,
    Area,
    Percent,
    Scalar,
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Dimension::Length => write!(f, "length"),
            Dimension::Angle => write!(f, "angle"),
            Dimension::Voltage => write!(f, "voltage"),
            Dimension::Current => write!(f, "current"),
            Dimension::Resistance => write!(f, "resistance"),
            Dimension::Capacitance => write!(f, "capacitance"),
            Dimension::Inductance => write!(f, "inductance"),
            Dimension::Frequency => write!(f, "frequency"),
            Dimension::Power => write!(f, "power"),
            Dimension::Area => write!(f, "area"),
            Dimension::Percent => write!(f, "percent"),
            Dimension::Scalar => write!(f, "scalar"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    // Length (canonical: mm)
    Mm,
    Cm,
    M,
    In,
    Ft,
    // Angle (canonical: deg)
    Deg,
    Rad,
    // Voltage (canonical: V)
    V,
    Mv,
    Kv,
    // Current (canonical: A)
    A,
    Ma,
    Ua,
    // Resistance (canonical: ohm)
    Ohm,
    Kohm,
    Mohm,
    // Capacitance (canonical: F)
    F,
    Mf,
    Uf,
    Nf,
    Pf,
    // Inductance (canonical: H)
    H,
    Mh,
    Uh,
    Nh,
    // Frequency (canonical: Hz)
    Hz,
    Khz,
    Mhz,
    // Power (canonical: W)
    W,
    Mw,
    Kw,
    // Area (canonical: mm2)
    Mm2,
    M2,
    // Percentage (canonical: %)
    Percent,
    // Dimensionless
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
            "V" => Some(Unit::V),
            "mV" => Some(Unit::Mv),
            "kV" => Some(Unit::Kv),
            "A" => Some(Unit::A),
            "mA" => Some(Unit::Ma),
            "uA" => Some(Unit::Ua),
            "ohm" | "Ohm" => Some(Unit::Ohm),
            "kohm" | "kOhm" => Some(Unit::Kohm),
            "Mohm" | "MOhm" => Some(Unit::Mohm),
            "F" => Some(Unit::F),
            "mF" => Some(Unit::Mf),
            "uF" => Some(Unit::Uf),
            "nF" => Some(Unit::Nf),
            "pF" => Some(Unit::Pf),
            "H" => Some(Unit::H),
            "mH" => Some(Unit::Mh),
            "uH" => Some(Unit::Uh),
            "nH" => Some(Unit::Nh),
            "Hz" => Some(Unit::Hz),
            "kHz" => Some(Unit::Khz),
            "MHz" => Some(Unit::Mhz),
            "W" => Some(Unit::W),
            "mW" => Some(Unit::Mw),
            "kW" => Some(Unit::Kw),
            "mm2" => Some(Unit::Mm2),
            "m2" => Some(Unit::M2),
            "%" => Some(Unit::Percent),
            "" => Some(Unit::None),
            _ => None,
        }
    }

    pub fn dimension(self) -> Dimension {
        match self {
            Unit::Mm | Unit::Cm | Unit::M | Unit::In | Unit::Ft => Dimension::Length,
            Unit::Deg | Unit::Rad => Dimension::Angle,
            Unit::V | Unit::Mv | Unit::Kv => Dimension::Voltage,
            Unit::A | Unit::Ma | Unit::Ua => Dimension::Current,
            Unit::Ohm | Unit::Kohm | Unit::Mohm => Dimension::Resistance,
            Unit::F | Unit::Mf | Unit::Uf | Unit::Nf | Unit::Pf => Dimension::Capacitance,
            Unit::H | Unit::Mh | Unit::Uh | Unit::Nh => Dimension::Inductance,
            Unit::Hz | Unit::Khz | Unit::Mhz => Dimension::Frequency,
            Unit::W | Unit::Mw | Unit::Kw => Dimension::Power,
            Unit::Mm2 | Unit::M2 => Dimension::Area,
            Unit::Percent => Dimension::Percent,
            Unit::None => Dimension::Scalar,
        }
    }

    /// Factor to convert into canonical base unit (mm, deg, V, A, ohm, F, Hz, W).
    pub fn to_canonical_factor(self) -> f64 {
        match self {
            Unit::Mm => 1.0,
            Unit::Cm => 10.0,
            Unit::M => 1000.0,
            Unit::In => 25.4,
            Unit::Ft => 304.8,
            Unit::Deg => 1.0,
            Unit::Rad => 180.0 / std::f64::consts::PI,
            Unit::V => 1.0,
            Unit::Mv => 1e-3,
            Unit::Kv => 1e3,
            Unit::A => 1.0,
            Unit::Ma => 1e-3,
            Unit::Ua => 1e-6,
            Unit::Ohm => 1.0,
            Unit::Kohm => 1e3,
            Unit::Mohm => 1e6,
            Unit::F => 1.0,
            Unit::Mf => 1e-3,
            Unit::Uf => 1e-6,
            Unit::Nf => 1e-9,
            Unit::Pf => 1e-12,
            Unit::H => 1.0,
            Unit::Mh => 1e-3,
            Unit::Uh => 1e-6,
            Unit::Nh => 1e-9,
            Unit::Hz => 1.0,
            Unit::Khz => 1e3,
            Unit::Mhz => 1e6,
            Unit::W => 1.0,
            Unit::Mw => 1e-3,
            Unit::Kw => 1e3,
            Unit::Mm2 => 1.0,
            Unit::M2 => 1e6,
            Unit::Percent => 1.0,
            Unit::None => 1.0,
        }
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Unit::Mm => "mm",
            Unit::Cm => "cm",
            Unit::M => "m",
            Unit::In => "in",
            Unit::Ft => "ft",
            Unit::Deg => "deg",
            Unit::Rad => "rad",
            Unit::V => "V",
            Unit::Mv => "mV",
            Unit::Kv => "kV",
            Unit::A => "A",
            Unit::Ma => "mA",
            Unit::Ua => "uA",
            Unit::Ohm => "ohm",
            Unit::Kohm => "kohm",
            Unit::Mohm => "Mohm",
            Unit::F => "F",
            Unit::Mf => "mF",
            Unit::Uf => "uF",
            Unit::Nf => "nF",
            Unit::Pf => "pF",
            Unit::H => "H",
            Unit::Mh => "mH",
            Unit::Uh => "uH",
            Unit::Nh => "nH",
            Unit::Hz => "Hz",
            Unit::Khz => "kHz",
            Unit::Mhz => "MHz",
            Unit::W => "W",
            Unit::Mw => "mW",
            Unit::Kw => "kW",
            Unit::Mm2 => "mm2",
            Unit::M2 => "m2",
            Unit::Percent => "%",
            Unit::None => "",
        };
        write!(f, "{s}")
    }
}

/// Strongly-typed dimensional quantity stored with its original unit and canonical conversion.
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

    pub fn to_volts(self) -> Result<f64, String> {
        if self.dimension() != Dimension::Voltage {
            return Err(format!(
                "Expected a voltage quantity (V/mV/kV), got {} ({})",
                self.dimension(),
                self.unit
            ));
        }
        Ok(self.raw_value * self.unit.to_canonical_factor())
    }

    pub fn to_amps(self) -> Result<f64, String> {
        if self.dimension() != Dimension::Current {
            return Err(format!(
                "Expected a current quantity (A/mA/uA), got {} ({})",
                self.dimension(),
                self.unit
            ));
        }
        Ok(self.raw_value * self.unit.to_canonical_factor())
    }

    pub fn to_watts(self) -> Result<f64, String> {
        if self.dimension() != Dimension::Power {
            return Err(format!(
                "Expected a power quantity (W/mW/kW), got {} ({})",
                self.dimension(),
                self.unit
            ));
        }
        Ok(self.raw_value * self.unit.to_canonical_factor())
    }

    pub fn to_mm2(self) -> Result<f64, String> {
        if self.dimension() != Dimension::Area {
            return Err(format!(
                "Expected a cross-section area quantity (mm2/m2), got {} ({})",
                self.dimension(),
                self.unit
            ));
        }
        Ok(self.raw_value * self.unit.to_canonical_factor())
    }

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
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VoltageSpec {
    Exact { volts: f64 },
    Range { min_v: f64, max_v: f64 },
}
