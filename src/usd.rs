use rand::random_bool;
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

    pub fn new_from_cents(cents: i64) -> Self {
        Self(Decimal::new(cents, 2))
    }

    pub fn value(&self) -> Decimal {
        self.0
    }

    pub fn random_rounded_even_split(&self) -> (USD, USD) {
        let half1 = (self.value() / dec!(2))
            .round_dp_with_strategy(2, rust_decimal::RoundingStrategy::AwayFromZero);
        let half2 = (self.value() / dec!(2))
            .round_dp_with_strategy(2, rust_decimal::RoundingStrategy::ToZero);
        assert_eq!(
            self.value(),
            (half1 + half2),
            "rounded splits not equal to starting total"
        );

        if random_bool(0.5) {
            return (USD::new(half1), USD::new(half2));
        } else {
            return (USD::new(half2), USD::new(half1));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::dec;

    #[test]
    fn new_from_cents() {
        let usd = USD::new_from_cents(1234);
        assert_eq!(usd.value(), dec!(12.34));
    }

    #[test]
    fn new_rounds_to_two_decimal_places() {
        let usd = USD::new(dec!(1.999));
        assert_eq!(usd.value(), dec!(2.00));
    }

    #[test]
    fn add() {
        assert_eq!(
            USD::new_from_cents(100) + USD::new_from_cents(250),
            USD::new_from_cents(350)
        );
    }

    #[test]
    fn sub() {
        assert_eq!(
            USD::new_from_cents(500) - USD::new_from_cents(150),
            USD::new_from_cents(350)
        );
    }

    #[test]
    fn neg() {
        assert_eq!(-USD::new_from_cents(500), USD::new_from_cents(-500));
    }

    #[test]
    fn mul() {
        assert_eq!(USD::new_from_cents(1000) * dec!(1.5), USD::new_from_cents(1500));
    }

    #[test]
    fn div() {
        assert_eq!(USD::new_from_cents(1000) / dec!(4), USD::new_from_cents(250));
    }

    #[test]
    fn display() {
        assert_eq!(format!("{}", USD::new_from_cents(1234)), "12.34");
    }

    #[test]
    fn random_even_split_sums_to_original() {
        // Test with even amount
        let even = USD::new_from_cents(1000);
        let (a, b) = even.random_rounded_even_split();
        assert_eq!(a + b, even);

        // Test with odd cent amount
        let odd = USD::new_from_cents(1001);
        let (a, b) = odd.random_rounded_even_split();
        assert_eq!(a + b, odd);
    }

    #[test]
    fn random_even_split_halves_are_close() {
        let amount = USD::new_from_cents(1001);
        let (a, b) = amount.random_rounded_even_split();
        // One should be 5.01, the other 5.00
        let max = std::cmp::max(a.value(), b.value());
        let min = std::cmp::min(a.value(), b.value());
        assert!(max - min <= dec!(0.01));
    }

    #[test]
    fn deserialize_valid() {
        let usd: USD = serde_json::from_str("\"12.34\"").unwrap();
        assert_eq!(usd, USD::new_from_cents(1234));
    }

    #[test]
    fn deserialize_rejects_sub_cent_precision() {
        let result: Result<USD, _> = serde_json::from_str("\"12.345\"");
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_allows_trailing_zeros() {
        let usd: USD = serde_json::from_str("\"12.3400\"").unwrap();
        assert_eq!(usd, USD::new_from_cents(1234));
    }

    #[test]
    fn serialize_roundtrip() {
        let original = USD::new_from_cents(4299);
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: USD = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }
}
