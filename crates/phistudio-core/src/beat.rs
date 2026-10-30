use crate::ChartError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{cmp::Ordering, fmt, str::FromStr};

/// Exact RPE time: whole + numerator / denominator. Negative values use floor
/// notation, e.g. -0.25 is [-1, 3, 4], matching the Java editor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Beat {
    whole: i32,
    numerator: i32,
    denominator: i32,
}

impl Beat {
    pub const ZERO: Self = Self {
        whole: 0,
        numerator: 0,
        denominator: 1,
    };
    pub const ONE: Self = Self {
        whole: 1,
        ..Self::ZERO
    };

    pub fn new(whole: i32, numerator: i32, denominator: i32) -> Result<Self, ChartError> {
        if denominator == 0 {
            return Err(ChartError::Invalid("Beat denominator is zero".into()));
        }
        let denominator = i128::from(denominator);
        let n =
            i128::from(whole) * denominator.abs() + i128::from(numerator) * denominator.signum();
        Self::fraction(n, denominator.abs())
    }

    fn fraction(n: i128, d: i128) -> Result<Self, ChartError> {
        let whole = n.div_euclid(d);
        let numerator = n.rem_euclid(d);
        let (mut a, mut b) = (numerator, d);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        let overflow = || ChartError::Invalid("Beat is outside the RPE integer range".into());
        Ok(Self {
            whole: whole.try_into().map_err(|_| overflow())?,
            numerator: (numerator / a).try_into().map_err(|_| overflow())?,
            denominator: (d / a).try_into().map_err(|_| overflow())?,
        })
    }

    pub fn from_grid(value: f64, division: i32) -> Result<Self, ChartError> {
        if !value.is_finite()
            || division <= 0
            || value.floor() < i32::MIN as f64
            || value.floor() > i32::MAX as f64
        {
            return Err(ChartError::Invalid("Invalid beat or grid division".into()));
        }
        Self::new(
            value.floor() as i32,
            ((value - value.floor()) * division as f64).round() as i32,
            division,
        )
    }

    pub fn as_f64(self) -> f64 {
        self.whole as f64 + self.numerator as f64 / self.denominator as f64
    }
    pub fn triple(self) -> [i32; 3] {
        [self.whole, self.numerator, self.denominator]
    }
    pub fn checked_add(self, other: Self) -> Result<Self, ChartError> {
        self.combine(other, 1)
    }
    pub fn checked_sub(self, other: Self) -> Result<Self, ChartError> {
        self.combine(other, -1)
    }
    pub fn checked_mul(self, other: Self) -> Result<Self, ChartError> {
        let [w, n, d] = self.triple().map(i128::from);
        let [ow, on, od] = other.triple().map(i128::from);
        Self::fraction((w * d + n) * (ow * od + on), d * od)
    }
    fn combine(self, other: Self, sign: i128) -> Result<Self, ChartError> {
        let [w, n, d] = self.triple().map(i128::from);
        let [ow, on, od] = other.triple().map(i128::from);
        Self::fraction((w * d + n) * od + sign * (ow * od + on) * d, d * od)
    }
}

impl Ord for Beat {
    fn cmp(&self, other: &Self) -> Ordering {
        self.whole.cmp(&other.whole).then_with(|| {
            (i64::from(self.numerator) * i64::from(other.denominator))
                .cmp(&(i64::from(other.numerator) * i64::from(self.denominator)))
        })
    }
}
impl PartialOrd for Beat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl fmt::Display for Beat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.numerator == 0 {
            write!(f, "{}", self.whole)
        } else {
            write!(f, "{}:{}/{}", self.whole, self.numerator, self.denominator)
        }
    }
}
impl FromStr for Beat {
    type Err = ChartError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        let invalid =
            || ChartError::Invalid("Use a decimal beat or whole:numerator/denominator".into());
        if text.len() > 64 {
            return Err(invalid());
        }
        if let Some((whole, fraction)) = text.split_once(':') {
            let (n, d) = fraction.split_once('/').ok_or_else(invalid)?;
            return Self::new(
                whole.trim().parse().map_err(|_| invalid())?,
                n.trim().parse().map_err(|_| invalid())?,
                d.trim().parse().map_err(|_| invalid())?,
            );
        }
        let (negative, digits) = if let Some(rest) = text.strip_prefix('-') {
            (true, rest)
        } else {
            (false, text.strip_prefix('+').unwrap_or(text))
        };
        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
        if whole.is_empty()
            || !whole.bytes().all(|x| x.is_ascii_digit())
            || !fraction.bytes().all(|x| x.is_ascii_digit())
            || fraction.len() > 18
        {
            return Err(invalid());
        }
        let d = 10_i128.pow(fraction.len() as u32);
        let n = whole
            .parse::<i128>()
            .map_err(|_| invalid())?
            .checked_mul(d)
            .and_then(|x| {
                x.checked_add(if fraction.is_empty() {
                    0
                } else {
                    fraction.parse::<i128>().ok()?
                })
            })
            .ok_or_else(invalid)?;
        Self::fraction(if negative { -n } else { n }, d)
    }
}
impl Serialize for Beat {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.triple().serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Beat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let [w, n, d] = <[i32; 3]>::deserialize(deserializer)?;
        Self::new(w, n, d).map_err(serde::de::Error::custom)
    }
}
