use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub inspector_widths: BTreeMap<String, u16>,
    pub theme: ThemeChoice,
    pub refresh: Refresh,
    pub notifications: Notifications,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub const fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Refresh {
    pub metrics: PollInterval,
    pub prometheus: PollInterval,
    pub snapshots: SnapshotCap,
}

impl Default for Refresh {
    fn default() -> Self {
        Self {
            metrics: PollInterval::Seconds15,
            prometheus: PollInterval::Seconds60,
            snapshots: DEFAULT_SNAPSHOTS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PollInterval {
    #[serde(rename = "5s")]
    Seconds5,
    #[serde(rename = "15s")]
    Seconds15,
    #[serde(rename = "30s")]
    Seconds30,
    #[serde(rename = "60s")]
    Seconds60,
}

impl PollInterval {
    pub const ALL: [Self; 4] = [
        Self::Seconds5,
        Self::Seconds15,
        Self::Seconds30,
        Self::Seconds60,
    ];

    pub const fn duration(self) -> Duration {
        match self {
            Self::Seconds5 => Duration::from_secs(5),
            Self::Seconds15 => Duration::from_secs(15),
            Self::Seconds30 => Duration::from_secs(30),
            Self::Seconds60 => Duration::from_secs(60),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Seconds5 => "5s",
            Self::Seconds15 => "15s",
            Self::Seconds30 => "30s",
            Self::Seconds60 => "60s",
        }
    }
}

pub const DEFAULT_SNAPSHOTS: SnapshotCap = SnapshotCap::Millis100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotCap {
    #[serde(rename = "100ms")]
    Millis100,
    #[serde(rename = "250ms")]
    Millis250,
    #[serde(rename = "500ms")]
    Millis500,
    #[serde(rename = "1s")]
    Seconds1,
}

impl SnapshotCap {
    pub const ALL: [Self; 4] = [
        Self::Millis100,
        Self::Millis250,
        Self::Millis500,
        Self::Seconds1,
    ];

    pub const fn duration(self) -> Duration {
        match self {
            Self::Millis100 => Duration::from_millis(100),
            Self::Millis250 => Duration::from_millis(250),
            Self::Millis500 => Duration::from_millis(500),
            Self::Seconds1 => Duration::from_secs(1),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Millis100 => "100ms",
            Self::Millis250 => "250ms",
            Self::Millis500 => "500ms",
            Self::Seconds1 => "1s",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notifications {
    pub enabled: bool,
    pub warnings: bool,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            enabled: true,
            warnings: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_label_their_durations() {
        for interval in PollInterval::ALL {
            let seconds = interval.duration().as_secs();
            assert_eq!(interval.label(), format!("{seconds}s"));
        }
        assert_eq!(
            SnapshotCap::Millis500.duration(),
            Duration::from_millis(500)
        );
        assert_eq!(SnapshotCap::Seconds1.label(), "1s");
    }

    #[test]
    fn settings_round_trip_through_json() {
        let settings = AppSettings {
            theme: ThemeChoice::Dark,
            refresh: Refresh {
                metrics: PollInterval::Seconds5,
                prometheus: PollInterval::Seconds30,
                snapshots: SnapshotCap::Seconds1,
            },
            notifications: Notifications {
                enabled: false,
                warnings: true,
            },
            inspector_widths: BTreeMap::from([("resource-inspector".to_owned(), 640)]),
        };
        let json = serde_json::to_string(&settings).unwrap_or_default();
        assert!(json.contains("\"theme\":\"dark\""), "{json}");
        assert!(json.contains("\"metrics\":\"5s\""), "{json}");
        let read: Result<AppSettings, _> = serde_json::from_str(&json);
        assert_eq!(read.ok(), Some(settings));
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let read: Result<AppSettings, _> = serde_json::from_str(r#"{"refresh":{"metrics":"30s"}}"#);
        let expected = AppSettings {
            refresh: Refresh {
                metrics: PollInterval::Seconds30,
                ..Refresh::default()
            },
            ..AppSettings::default()
        };
        assert_eq!(read.ok(), Some(expected));
    }
}
