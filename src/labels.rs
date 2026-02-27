use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write as _;
use tracing_core::Level;

use super::Error;
use super::ErrorI;

#[derive(Clone)]
pub struct FormattedLabels {
    seen_keys: HashSet<String>,
    formatted: String,
}

impl FormattedLabels {
    pub fn new() -> FormattedLabels {
        FormattedLabels {
            seen_keys: HashSet::new(),
            formatted: String::from("{"),
        }
    }
    pub fn add(&mut self, key: String, value: &str) -> Result<(), Error> {
        // Couldn't find documentation except for the promtail source code:
        // https://github.com/grafana/loki/blob/8c06c546ab15a568f255461f10318dae37e022d3/vendor/github.com/prometheus/prometheus/promql/parser/generated_parser.y#L597-L598
        //
        // Apparently labels that confirm to yacc's "IDENTIFIER" are okay. I
        // couldn't find which those are. Let's be conservative and allow
        // `[A-Za-z_]*`.
        for (i, b) in key.bytes().enumerate() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'_' => {}
                // The first byte outside of the above range must start a UTF-8
                // character.
                _ => {
                    let c = key[i..].chars().next().unwrap();
                    return Err(Error(ErrorI::InvalidLabelCharacter(key, c)));
                }
            }
        }
        if key == "level" {
            return Err(Error(ErrorI::ReservedLabelLevel));
        }

        // Couldn't find documentation except for the promtail source code:
        // https://github.com/grafana/loki/blob/8c06c546ab15a568f255461f10318dae37e022d3/clients/pkg/promtail/client/batch.go#L61-L75
        //
        // Go's %q displays the string in double quotes, escaping a few
        // characters, like Rust's {:?}.
        let old_len = self.formatted.len();
        let sep = if self.formatted.len() <= 1 { "" } else { "," };
        write!(&mut self.formatted, "{}{}={:?}", sep, key, value).unwrap();

        if let Some(duplicate_key) = self.seen_keys.replace(key) {
            self.formatted.truncate(old_len);
            return Err(Error(ErrorI::DuplicateLabel(duplicate_key)));
        }
        Ok(())
    }
    /// Check if a label name is already registered.
    pub fn contains(&self, key: &str) -> bool {
        self.seen_keys.contains(key)
    }
    /// Build the full Prometheus label string including dynamic labels.
    ///
    /// Dynamic label names are sorted alphabetically for deterministic ordering.
    pub fn finish_with_dynamic(&self, level: Level, dynamic: &HashMap<String, String>) -> String {
        let mut result = self.formatted.clone();
        if result.len() > 1 {
            result.push(',');
        }
        result.push_str(match level {
            Level::TRACE => "level=\"trace\"",
            Level::DEBUG => "level=\"debug\"",
            Level::INFO => "level=\"info\"",
            Level::WARN => "level=\"warn\"",
            Level::ERROR => "level=\"error\"",
        });
        if !dynamic.is_empty() {
            let mut keys: Vec<&String> = dynamic.keys().collect();
            keys.sort();
            for key in keys {
                write!(&mut result, ",{}={:?}", key, dynamic[key]).unwrap();
            }
        }
        result.push('}');
        result
    }
}

#[cfg(test)]
mod test {
    use super::FormattedLabels;
    use std::collections::HashMap;
    use tracing_core::Level;

    fn finish(labels: &FormattedLabels, level: Level) -> String {
        labels.finish_with_dynamic(level, &HashMap::new())
    }

    #[test]
    fn simple() {
        assert_eq!(
            finish(&FormattedLabels::new(), Level::TRACE),
            r#"{level="trace"}"#,
        );
        assert_eq!(
            finish(&FormattedLabels::new(), Level::DEBUG),
            r#"{level="debug"}"#,
        );
        assert_eq!(
            finish(&FormattedLabels::new(), Level::INFO),
            r#"{level="info"}"#,
        );
        assert_eq!(
            finish(&FormattedLabels::new(), Level::WARN),
            r#"{level="warn"}"#,
        );
        assert_eq!(
            finish(&FormattedLabels::new(), Level::ERROR),
            r#"{level="error"}"#,
        );
    }

    #[test]
    fn level() {
        assert!(FormattedLabels::new().add("level".into(), "").is_err());
        assert!(FormattedLabels::new().add("level".into(), "blurb").is_err());
    }

    #[test]
    fn duplicate() {
        let mut labels = FormattedLabels::new();
        labels.add("label".into(), "abc").unwrap();
        assert!(labels.clone().add("label".into(), "def").is_err());
        assert!(labels.clone().add("label".into(), "abc").is_err());
        assert!(labels.clone().add("label".into(), "").is_err());
    }
}
