//! Data processing tests (no network)
//! - parse_daily shape handling
//! - summarize formatting
//! - render_table output

use chrono::NaiveDate;
use weather_helper::{DayTemp, Summary, parse_daily, render_table, summarize};

#[test]
fn parse_daily_empty_ok() {
    let resp = serde_json::json!({
        "daily": {
            "time": [],
            "temperature_2m_max": [],
            "temperature_2m_min": []
        }
    });
    let resp: weather_helper::ForecastResp = serde_json::from_value(resp).unwrap();
    let out = parse_daily(resp).unwrap();
    assert!(out.is_empty());
}

#[test]
fn parse_daily_mismatched_lengths_errs() {
    let resp = serde_json::json!({
        "daily": {
            "time": ["2025-01-01", "2025-01-02"],
            "temperature_2m_max": [10.0],
            "temperature_2m_min": [1.0, 2.0]
        }
    });
    let resp: weather_helper::ForecastResp = serde_json::from_value(resp).unwrap();
    let err = parse_daily(resp).unwrap_err();
    assert!(err.to_string().contains("mismatched lengths"));
}

#[test]
fn summarize_formats() {
    let data = vec![
        DayTemp {
            date: NaiveDate::parse_from_str("2025-01-01", "%Y-%m-%d").unwrap(),
            tmax: 10.0,
            tmin: 0.0,
            tmax_f: 52.0,
            tmin_f: 32.0,
        },
        DayTemp {
            date: NaiveDate::parse_from_str("2025-01-02", "%Y-%m-%d").unwrap(),
            tmax: 12.0,
            tmin: 1.0,
            tmax_f: 56.0,
            tmin_f: 3.0,
        },
    ];
    let s: Summary = summarize(&data);
    assert_eq!(s.max, "56°F");
    assert_eq!(s.min, "3°F");
    assert!(s.note.contains("2 days"));
}

#[test]
fn render_table_outputs_rows() {
    let data = vec![
        DayTemp {
            date: NaiveDate::parse_from_str("2025-01-01", "%Y-%m-%d").unwrap(),
            tmax: 10.0,
            tmin: 0.0,
            tmax_f: 52.0,
            tmin_f: 32.0,
        },
        DayTemp {
            date: NaiveDate::parse_from_str("2025-01-02", "%Y-%m-%d").unwrap(),
            tmax: 12.0,
            tmin: 1.0,
            tmax_f: 56.0,
            tmin_f: 3.0,
        },
    ];
    let table = render_table(&data);
    assert!(table.contains("| 2025-01-01 | 52 | 32 |"));
    assert!(table.contains("| 2025-01-02 | 56 | 3 |"));
}
