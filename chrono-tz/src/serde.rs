use core::fmt;
use serde_core::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::timezones::Tz;

impl Serialize for Tz {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for Tz {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Tz;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "an IANA timezone string")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Tz, E> {
                value
                    .parse::<Tz>()
                    .map_err(|_| E::custom(SerdeError(value)))
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

struct SerdeError<'a>(&'a str);

impl fmt::Display for SerdeError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        const MAX_INPUT_LEN: usize = 128;
        if self.0.len() <= MAX_INPUT_LEN {
            write!(f, "failed to parse timezone: '{}'", self.0)
        } else {
            let mut end = MAX_INPUT_LEN;
            while !self.0.is_char_boundary(end) {
                end -= 1;
            }
            write!(f, "failed to parse timezone: '{}...'", &self.0[..end])
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::timezones::Tz::{self, Etc__UTC, Europe__London, UTC};
    use serde_test::{assert_de_tokens_error, assert_tokens, Token};

    #[test]
    fn serde_ok_both_ways() {
        assert_tokens(&Europe__London, &[Token::String("Europe/London")]);
        assert_tokens(&Etc__UTC, &[Token::String("Etc/UTC")]);
        assert_tokens(&UTC, &[Token::String("UTC")]);
    }

    #[test]
    fn serde_de_error() {
        assert_de_tokens_error::<Tz>(
            &[Token::Str("Europe/L")],
            "failed to parse timezone: 'Europe/L'",
        );
    }

    #[test]
    fn serde_de_error_exact_128_and_truncated_long_input() {
        let exact: &'static str = Box::leak("a".repeat(128).into_boxed_str());
        let expected_exact = format!("failed to parse timezone: '{exact}'");
        assert_de_tokens_error::<Tz>(&[Token::Str(exact)], &expected_exact);

        let too_long: &'static str = Box::leak("b".repeat(200).into_boxed_str());
        let expected_truncated = format!("failed to parse timezone: '{}...'", "b".repeat(128));
        assert_de_tokens_error::<Tz>(&[Token::Str(too_long)], &expected_truncated);

        // Multi-byte UTF-8 boundary: 127 'a's + 4-byte emoji -> truncates cleanly at 127 bytes.
        let mut s = "a".repeat(127);
        s.push('🦀');
        let utf8_long: &'static str = Box::leak(s.into_boxed_str());
        let expected_utf8 = format!("failed to parse timezone: '{}...'", "a".repeat(127));
        assert_de_tokens_error::<Tz>(&[Token::Str(utf8_long)], &expected_utf8);
    }
}
