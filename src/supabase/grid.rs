use serde_json::{Map, Value};
use std::time::{Duration, SystemTime};

/// Length of `YYYY-MM-DDTHH:MM`, the part of an RFC 3339 timestamp worth showing.
const MINUTE_PRECISION: usize = 16;

/// Rows of text under named columns: what every section of a project shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Grid {
    pub(crate) columns: Vec<String>,
    pub(crate) rows: Vec<Vec<String>>,
}

impl Grid {
    pub(crate) fn new(columns: &[&str], rows: Vec<Vec<String>>) -> Self {
        Self {
            columns: columns.iter().map(|&column| column.to_owned()).collect(),
            rows,
        }
    }

    /// The text in `row` under the column named `column`.
    pub(crate) fn value(&self, row: usize, column: &str) -> Option<&str> {
        let position = self.columns.iter().position(|name| name == column)?;
        Some(self.rows.get(row)?.get(position)?.as_str())
    }

    /// Query results, one JSON object per row with columns in select order.
    pub(crate) fn from_records(records: &[Map<String, Value>]) -> Self {
        let columns = records
            .first()
            .map_or_else(Vec::new, |record| record.keys().cloned().collect());
        let rows = records
            .iter()
            .map(|record| record.values().map(cell).collect())
            .collect();
        Self { columns, rows }
    }
}

fn cell(value: &Value) -> String {
    match value {
        Value::Null => "NULL".into(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Shortens an RFC 3339 timestamp to `YYYY-MM-DD HH:MM`.
pub(crate) fn timestamp(text: &str) -> String {
    // Text that is not a timestamp is shown unchanged.
    let minutes = text.get(..MINUTE_PRECISION).unwrap_or(text);
    minutes.replacen('T', " ", 1)
}

pub(crate) fn unix_timestamp(milliseconds: u64) -> String {
    let time = SystemTime::UNIX_EPOCH + Duration::from_millis(milliseconds);
    timestamp(&humantime::format_rfc3339_seconds(time).to_string())
}

#[cfg(test)]
mod tests {
    use super::{Grid, timestamp, unix_timestamp};
    use serde_json::{Map, Value};

    #[test]
    fn records_keep_column_order_and_show_every_type() {
        let records: Vec<Map<String, Value>> = serde_json::from_str(
            r#"[
              {"zeta": 1, "alpha": "text", "nothing": null, "tags": ["a"], "ok": true},
              {"zeta": 2.5, "alpha": "", "nothing": null, "tags": {"b": 1}, "ok": false}
            ]"#,
        )
        .unwrap();
        let grid = Grid::from_records(&records);
        assert_eq!(grid.columns, ["zeta", "alpha", "nothing", "tags", "ok"]);
        assert_eq!(
            grid.rows,
            [
                ["1", "text", "NULL", r#"["a"]"#, "true"],
                ["2.5", "", "NULL", r#"{"b":1}"#, "false"],
            ]
        );
        assert_eq!(grid.value(1, "alpha"), Some(""));
        assert_eq!(grid.value(1, "missing"), None);
        assert_eq!(grid.value(2, "alpha"), None);
        assert_eq!(Grid::from_records(&[]), Grid::default());
    }

    #[test]
    fn timestamps_show_minutes() {
        assert_eq!(timestamp("2024-03-09T17:45:12.345Z"), "2024-03-09 17:45");
        assert_eq!(timestamp("soon"), "soon");
        assert_eq!(unix_timestamp(1_709_999_999_000), "2024-03-09 15:59");
    }
}
