use rust_decimal::{Decimal, dec};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct USD(Decimal);

impl USD {
    pub fn new(value: Decimal) -> Self {
        Self(value.round_dp(2))
    }

    pub fn value(&self) -> Decimal {
        self.0
    }
}

impl fmt::Display for USD {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value().fmt(f)
    }
}

impl Add for USD {
    type Output = USD;

    fn add(self, other: USD) -> USD {
        USD::new(self.0 + other.0)
    }
}

impl Sub for USD {
    type Output = USD;

    fn sub(self, other: USD) -> USD {
        USD::new(self.0 - other.0)
    }
}

impl Mul<Decimal> for USD {
    type Output = USD;

    fn mul(self, factor: Decimal) -> USD {
        USD::new(self.0 * factor)
    }
}

impl Div<Decimal> for USD {
    type Output = USD;

    fn div(self, divisor: Decimal) -> USD {
        USD::new(self.0 / divisor)
    }
}

impl Neg for USD {
    type Output = USD;

    fn neg(self) -> Self::Output {
        USD::new(dec!(0)) - self
    }
}

impl<'de> Deserialize<'de> for USD {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // First, ensure that the string does not contain fractions of cents
        // i.e. that if it has more than 2 decimal places, all trailing places are 0
        let s = String::deserialize(deserializer)?;
        if let Some(decimal_pos) = s.find('.') {
            let decimal_part = &s[decimal_pos + 1..];

            if decimal_part.len() > 2 {
                let beyond_second = &decimal_part[2..];
                if !beyond_second.chars().all(|c| c == '0') {
                    return Err(serde::de::Error::custom(
                        "USD values cannot have non-zero digits beyond 2 decimal places",
                    ));
                }
            }
        }

        // Then, parse into a Decimal type
        let decimal = s
            .parse::<Decimal>()
            .map_err(|e| serde::de::Error::custom(format!("invalid decimal format: {}", e)))?;

        Ok(USD::new(decimal))
    }
}

impl Serialize for USD {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        rust_decimal::serde::str::serialize(&self.0, serializer)
    }
}
