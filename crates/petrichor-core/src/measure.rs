use std::collections::BTreeMap;

use crate::model::{Measure, Skin};
use crate::variables::Variables;
use petrichor_sys::SystemBackend;

pub fn evaluate(skin: &Skin) -> BTreeMap<String, String> {
    let mut variables = Variables::new();
    for (name, value) in &skin.variables {
        variables.set(name, value);
    }

    let mut values = BTreeMap::new();
    for measure in &skin.measures {
        let value = evaluate_measure(measure, &variables);
        values.insert(measure.name.clone(), value);
    }
    values
}

pub fn evaluate_measure(measure: &Measure, variables: &Variables) -> String {
    match measure.kind.trim().to_ascii_uppercase().as_str() {
        "TIME" => time_value(measure, variables),
        "UPTIME" => uptime_value(measure, variables),
        _ => String::new(),
    }
}

fn time_value(measure: &Measure, variables: &Variables) -> String {
    let format = measure
        .options
        .get("Format")
        .map(|value| variables.expand(value))
        .unwrap_or_else(|| "%H:%M:%S".to_string());
    chrono::Local::now().format(&format).to_string()
}

fn uptime_value(measure: &Measure, variables: &Variables) -> String {
    let total = petrichor_sys::backend().uptime_secs();
    let days = total / 86_400;
    let hours = (total % 86_400) / 3_600;
    let minutes = (total % 3_600) / 60;
    let seconds = total % 60;

    match measure.options.get("Format") {
        Some(format) => variables
            .expand(format)
            .replace("%d", &days.to_string())
            .replace("%H", &format!("{hours:02}"))
            .replace("%M", &format!("{minutes:02}"))
            .replace("%S", &format!("{seconds:02}")),
        None => format!("{days}d {hours:02}:{minutes:02}:{seconds:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Options;

    fn measure(kind: &str, entries: &[(&str, &str)]) -> Measure {
        Measure {
            name: "M".to_string(),
            kind: kind.to_string(),
            line: 1,
            options: Options {
                entries: entries
                    .iter()
                    .map(|(k, v)| crate::model::OptionEntry {
                        key: k.to_string(),
                        value: v.to_string(),
                        line: 1,
                    })
                    .collect(),
            },
        }
    }

    #[test]
    fn formats_time_value() {
        let m = measure("Time", &[("Format", "%Y")]);
        let value = evaluate_measure(&m, &Variables::new());
        assert_eq!(value.len(), 4);
        assert!(value.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn substitutes_variables_in_format() {
        let mut variables = Variables::new();
        variables.set("Sep", ":");
        let m = measure("Time", &[("Format", "%H#Sep#%M")]);
        let value = evaluate_measure(&m, &variables);
        assert!(value.contains(':'));
        assert_eq!(value.matches(':').count(), 1);
    }

    #[test]
    fn unknown_measure_is_empty() {
        let m = measure("Nope", &[]);
        assert!(evaluate_measure(&m, &Variables::new()).is_empty());
    }
}
