//! Weather Helper
//!
//! Core library for scanning Markdown notes, extracting travel metadata,
//! fetching Open‑Meteo daily highs/lows, and updating an idempotent weather
//! section in-place.
//!
//! See README for usage. The binary crate calls `run`.

use anyhow::{Context, Result, anyhow};
use chrono::{Datelike, Duration, Local, NaiveDate};
use regex::Regex;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_yaml::Value as YamlValue;
use std::{env, fs, path::Path};

/// Metadata extracted from a note's YAML frontmatter.
#[derive(Debug)]
pub struct NoteMeta {
    pub city: String,
    pub arrival: NaiveDate,
    pub departure: NaiveDate,
    pub path: String,
}

#[derive(Deserialize, Debug)]
struct GeocodeResp {
    results: Option<Vec<GeoItem>>,
}
#[derive(Deserialize, Debug)]
struct GeoItem {
    latitude: f64,
    longitude: f64,
    #[serde(default)]
    timezone: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct DailyTemps {
    pub time: Vec<String>,
    pub temperature_2m_max: Vec<f64>,
    pub temperature_2m_min: Vec<f64>,
}
#[derive(Deserialize, Debug)]
pub struct ForecastResp {
    pub daily: Option<DailyTemps>,
}

/// Single day of temperatures (Celsius).
#[derive(Clone, Debug, PartialEq)]
pub struct DayTemp {
    pub date: NaiveDate,
    pub tmax: f64,
    pub tmin: f64,
    pub tmax_f: f64,
    pub tmin_f: f64,
}

/// Summary of the dataset for presentation.
#[derive(Debug, PartialEq)]
pub struct Summary {
    pub max: String,
    pub min: String,
    pub note: String,
}

/// 9/5 AKA celsius conversion rate
const CONVERSION_RATE_CF: f64 = 9.0 / 5.0;

/// Gets the url for the geocode
fn geocode_base() -> String {
    env::var("OPEN_METEO_GEOCODE_BASE")
        .unwrap_or_else(|_| "https://geocoding-api.open-meteo.com/v1".to_string())
}
/// gets the url for the forecast
fn forecast_base() -> String {
    env::var("OPEN_METEO_FORECAST_BASE")
        .unwrap_or_else(|_| "https://api.open-meteo.com/v1".to_string())
}

/// Gets the url for the archive
fn archive_base() -> String {
    env::var("OPEN_METEO_ARCHIVE_BASE")
        .unwrap_or_else(|_| "https://archive-api.open-meteo.com/v1".to_string())
}

/// Helper function that takes in celsius and returns fahrenheit
fn celcius_to_farenheit(temp_c: f64) -> f64 {
    return temp_c * CONVERSION_RATE_CF + 32.0;
}

async fn get_json_with_retry<T: DeserializeOwned>(url: &str) -> Result<T> {
    let mut delay_ms = 100u64;
    let attempts = 3;
    for attempt in 1..=attempts {
        let resp = reqwest::get(url).await;
        match resp {
            Ok(r) => match r.error_for_status() {
                Ok(ok) => {
                    let parsed = ok
                        .json::<T>()
                        .await
                        .with_context(|| format!("failed to parse JSON from {url}"))?;
                    return Ok(parsed);
                }
                Err(e) => {
                    if attempt == attempts {
                        return Err(anyhow!(e)).with_context(|| format!("request failed: {url}"));
                    }
                }
            },
            Err(e) => {
                if attempt == attempts {
                    return Err(anyhow!(e)).with_context(|| format!("network error: {url}"));
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        delay_ms *= 2;
    }
    Err(anyhow!("unreachable retry loop"))
}

pub async fn run(root: &str) -> Result<()> {
    let mut notes = vec![];
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if let Some(ext) = entry.path().extension() {
            if ext == "md" {
                match extract_meta(entry.path()) {
                    Ok(meta) => notes.push(meta),
                    Err(e) => eprintln!(
                        "Failed to extract metadata from {}: {e}",
                        entry.path().display()
                    ),
                }
            }
        }
    }

    if notes.is_empty() {
        println!("No packing notes with city/arrival/departure found.");
        return Ok(());
    }

    let mut had_error = false;
    for note in notes {
        match process_note(&note).await {
            Ok(_) => println!("Updated weather: {}", note.path),
            Err(e) => {
                eprintln!("Skipping {}: {e}", note.path);
                had_error = true;
            }
        }
    }

    if had_error {
        eprintln!(
            "One or more notes could not be updated due to errors. Please check the log above."
        );
        std::process::exit(1);
    }

    Ok(())
}

/// Read the YAML frontmatter and extract required fields.
pub fn extract_meta(path: &Path) -> Result<NoteMeta> {
    let text = fs::read_to_string(path)?;
    let re = Regex::new(r"(?s)^---\s*(.*?)\s*---").unwrap();
    let caps = re
        .captures(&text)
        .ok_or_else(|| anyhow!("no YAML frontmatter"))?;
    let yaml_str = caps.get(1).unwrap().as_str();

    let yaml: YamlValue = serde_yaml::from_str(yaml_str)?;
    let city = yaml
        .get("city-place")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing 'city-place'"))?
        .trim()
        .to_string();

    let arrival_str = yaml
        .get("arrival")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing 'arrival' (YYYY-MM-DD)"))?;
    let departure_str = yaml
        .get("departure")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing 'departure' (YYYY-MM-DD)"))?;

    let arrival =
        NaiveDate::parse_from_str(arrival_str, "%Y-%m-%d").context("arrival must be YYYY-MM-DD")?;
    let departure = NaiveDate::parse_from_str(departure_str, "%Y-%m-%d")
        .context("departure must be YYYY-MM-DD")?;

    Ok(NoteMeta {
        city,
        arrival,
        departure,
        path: path.to_string_lossy().to_string(),
    })
}

/// Process a single note: geocode, fetch data, summarize, and update file.
pub async fn process_note(meta: &NoteMeta) -> Result<()> {
    let (lat, lon, tz) = geocode(&meta.city).await?;
    let today = Local::now().date_naive();
    let forecast_horizon = today + Duration::days(16);

    let start = meta.arrival.min(meta.departure);
    let end = meta.arrival.max(meta.departure);

    let (data, label) = if start <= forecast_horizon {
        let s = start.max(today);
        let e = end.min(forecast_horizon);
        let temps = fetch_daily(&lat, &lon, &s, &e, &tz).await?;
        (temps, format!("Forecast {} → {}", s, e))
    } else {
        let last_year = start.year() - 1;
        let s = NaiveDate::from_ymd_opt(last_year, start.month(), start.day())
            .ok_or_else(|| anyhow!("bad start date"))?;
        let e = NaiveDate::from_ymd_opt(last_year, end.month(), end.day())
            .ok_or_else(|| anyhow!("bad end date"))?;
        let temps = fetch_archive(&lat, &lon, &s, &e, &tz).await?;
        (temps, format!("Historic (proxy) {} → {}", s, e))
    };

    let summary = summarize(&data);
    let table = render_table(&data);
    let block = format!(
        "## Weather Forecast\n<!-- WEATHER:BEGIN -->\n**{}**  \n**Range**: {} / {}  \n\n{}\n\n{}\n<!-- WEATHER:END -->\n",
        label, summary.max, summary.min, summary.note, table
    );

    let mut content = fs::read_to_string(&meta.path)?;
    upsert_weather_block(&mut content, &block)?;
    fs::write(&meta.path, content)?;
    Ok(())
}

/// Insert or replace the weather block under the designated heading.
pub fn upsert_weather_block(content: &mut String, new_block: &str) -> Result<()> {
    let block_re =
        Regex::new("(?s)##\\s*Weather Forecast\\s*\n<!-- WEATHER:BEGIN -->.*?<!-- WEATHER:END -->")
            .unwrap();

    if block_re.is_match(content) {
        *content = block_re.replace(content, new_block).to_string();
        return Ok(());
    }

    let heading_re = Regex::new(r"(?m)^##\s*Weather Forecast\s*$").unwrap();
    if heading_re.is_match(content) {
        *content = heading_re
            .replace(
                content,
                "## Weather Forecast\n<!-- WEATHER:BEGIN -->\n<!-- WEATHER:END -->",
            )
            .to_string();
        let empty_block = Regex::new(
            "(?s)##\\s*Weather Forecast\\s*\n<!-- WEATHER:BEGIN -->\n<!-- WEATHER:END -->",
        )
        .unwrap();
        *content = empty_block.replace(content, new_block).to_string();
        Ok(())
    } else {
        content.push_str("\n\n");
        content.push_str(new_block);
        Ok(())
    }
}

/// Compute min/max strings and a human-friendly note for a set of `DayTemp`s.
pub fn summarize(data: &[DayTemp]) -> Summary {
    if data.is_empty() {
        return Summary {
            max: "n/a".into(),
            min: "n/a".into(),
            note: "_No data returned_".into(),
        };
    }
    let max = data.iter().fold(f64::MIN, |m, d| m.max(d.tmax_f));
    let min = data.iter().fold(f64::MAX, |m, d| m.min(d.tmin_f));
    let note = format!(
        "_{} days • High range {:.0}° → {:.0}° • Low range {:.0}° → {:.0}°_",
        data.len(),
        data.iter().map(|d| d.tmax_f).fold(f64::MAX, f64::min),
        data.iter().map(|d| d.tmax_f).fold(f64::MIN, f64::max),
        data.iter().map(|d| d.tmin_f).fold(f64::MAX, f64::min),
        data.iter().map(|d| d.tmin_f).fold(f64::MIN, f64::max),
    );
    Summary {
        max: format!("{:.0}°F", max),
        min: format!("{:.0}°F", min),
        note,
    }
}

/// Render a Markdown table of daily highs and lows.
pub fn render_table(data: &[DayTemp]) -> String {
    if data.is_empty() {
        return "_(no rows)_".into();
    }
    let mut s = String::from(
        "| Date | High (°F) | Low (°F) | High (°C) | Low (°C) |\n|---|---:|---:|---:|---:|\n",
    );
    for d in data {
        s.push_str(&format!(
            "| {} | {:.0} | {:.0} | {:.0} | {:.0} |\n",
            d.date, d.tmax_f, d.tmin_f, d.tmax, d.tmin
        ));
    }
    s
}

/// Geocode a city to `(latitude, longitude, timezone)` using Open‑Meteo.
pub async fn geocode(city: &str) -> Result<(f64, f64, String)> {
    let url = format!(
        "{}/search?name={}&country=IT&count=1",
        geocode_base(),
        urlencoding::encode(city)
    );
    let geo: GeocodeResp = get_json_with_retry(&url).await?;
    let item = geo
        .results
        .and_then(|mut v| v.pop())
        .ok_or_else(|| anyhow!("geocoding failed for city: {}", city))?;
    let tz = item.timezone.unwrap_or_else(|| "Europe/Rome".to_string());
    Ok((item.latitude, item.longitude, tz))
}

/// Fetch forecast daily highs/lows for a date range using Open‑Meteo forecast API.
pub async fn fetch_daily(
    lat: &f64,
    lon: &f64,
    start: &NaiveDate,
    end: &NaiveDate,
    tz: &str,
) -> Result<Vec<DayTemp>> {
    let url = format!(
        "{}/forecast?latitude={}&longitude={}&daily=temperature_2m_max,temperature_2m_min&start_date={}&end_date={}&timezone={}",
        forecast_base(),
        lat,
        lon,
        start,
        end,
        urlencoding::encode(if tz.is_empty() { "Europe/Rome" } else { tz })
    );
    let data: ForecastResp = get_json_with_retry(&url).await?;
    parse_daily(data)
}

/// Fetch historical proxy using ERA5 archive (same calendar span last year).
pub async fn fetch_archive(
    lat: &f64,
    lon: &f64,
    start: &NaiveDate,
    end: &NaiveDate,
    tz: &str,
) -> Result<Vec<DayTemp>> {
    let url = format!(
        "{}/era5?latitude={}&longitude={}&daily=temperature_2m_max,temperature_2m_min&start_date={}&end_date={}&timezone={}",
        archive_base(),
        lat,
        lon,
        start,
        end,
        urlencoding::encode(if tz.is_empty() { "Europe/Rome" } else { tz })
    );
    let data: ForecastResp = get_json_with_retry(&url).await?;
    parse_daily(data)
}

/// Convert Open‑Meteo `daily` arrays into a vector of `DayTemp`.
pub fn parse_daily(api: ForecastResp) -> Result<Vec<DayTemp>> {
    let d = api.daily.ok_or_else(|| anyhow!("no daily data"))?;
    let n_time = d.time.len();
    let n_max = d.temperature_2m_max.len();
    let n_min = d.temperature_2m_min.len();
    if n_time == 0 || n_max == 0 || n_min == 0 {
        return Ok(vec![]);
    }
    if n_time != n_max || n_time != n_min {
        return Err(anyhow!(
            "daily arrays have mismatched lengths: time={}, tmax={}, tmin={}",
            n_time,
            n_max,
            n_min
        ));
    }
    let mut out = Vec::with_capacity(n_time);
    for i in 0..n_time {
        let date = NaiveDate::parse_from_str(&d.time[i], "%Y-%m-%d")?;
        let tmax = d.temperature_2m_max[i];
        let tmin = d.temperature_2m_min[i];
        let tmax_f: f64 = celcius_to_farenheit(tmax);
        let tmin_f: f64 = celcius_to_farenheit(tmin);
        out.push(DayTemp {
            date,
            tmax,
            tmin,
            tmax_f,
            tmin_f,
        });
    }
    Ok(out)
}
