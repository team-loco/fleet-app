#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Status {
    #[default]
    Ok,
    Suspended,
    Progress,
    Warn,
    Offline,
    Err,
}

impl Status {
    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Ok => "●",
            Self::Warn => "▲",
            Self::Err => "✕",
            Self::Progress => "◐",
            Self::Suspended => "‖",
            Self::Offline => "○",
        }
    }

    pub fn worst(statuses: impl IntoIterator<Item = Self>) -> Self {
        statuses.into_iter().max().unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub err: u32,
    pub warn: u32,
    pub sus: u32,
    pub total: Option<u32>,
}

impl Counts {
    pub const fn total(total: u32) -> Self {
        Self {
            err: 0,
            warn: 0,
            sus: 0,
            total: Some(total),
        }
    }

    pub const fn with_err(self, err: u32) -> Self {
        Self { err, ..self }
    }

    pub const fn with_warn(self, warn: u32) -> Self {
        Self { warn, ..self }
    }

    pub const fn with_sus(self, sus: u32) -> Self {
        Self { sus, ..self }
    }

    pub fn badges(&self) -> Vec<(Status, u32)> {
        let flagged = [
            (Status::Err, self.err),
            (Status::Warn, self.warn),
            (Status::Suspended, self.sus),
        ];
        flagged.into_iter().filter(|(_, n)| *n > 0).collect()
    }

    pub fn status(&self) -> Status {
        let present = self.badges().into_iter().map(|(status, _)| status);
        Status::worst(present)
    }
}

impl std::ops::Add for Counts {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        let total = match (self.total, other.total) {
            (Some(a), Some(b)) => Some(a + b),
            (Some(a), None) => Some(a),
            (None, b) => b,
        };
        Self {
            err: self.err + other.err,
            warn: self.warn + other.warn,
            sus: self.sus + other.sus,
            total,
        }
    }
}

impl std::iter::Sum for Counts {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), |acc, c| acc + c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_match_design() {
        let glyphs: Vec<_> = [
            Status::Ok,
            Status::Warn,
            Status::Err,
            Status::Progress,
            Status::Suspended,
            Status::Offline,
        ]
        .map(Status::glyph)
        .into();
        assert_eq!(glyphs, ["●", "▲", "✕", "◐", "‖", "○"]);
    }

    #[test]
    fn worst_of_picks_most_severe() {
        assert_eq!(
            Status::worst([Status::Ok, Status::Warn, Status::Suspended]),
            Status::Warn
        );
        assert_eq!(
            Status::worst([Status::Warn, Status::Err, Status::Offline]),
            Status::Err
        );
        assert_eq!(
            Status::worst([Status::Progress, Status::Suspended]),
            Status::Progress
        );
        assert_eq!(
            Status::worst([Status::Warn, Status::Offline]),
            Status::Offline
        );
    }

    #[test]
    fn worst_of_nothing_is_ok() {
        assert_eq!(Status::worst([]), Status::Ok);
    }

    #[test]
    fn counts_badges_skip_zero_and_order_by_severity() {
        let counts = Counts::total(14).with_warn(1).with_err(2);
        assert_eq!(counts.badges(), [(Status::Err, 2), (Status::Warn, 1)]);
        assert_eq!(counts.status(), Status::Err);
        assert_eq!(Counts::total(3).status(), Status::Ok);
    }

    #[test]
    fn counts_sum_adds_fields_and_known_totals() {
        let sum: Counts = [Counts::total(2).with_err(1), Counts::default().with_warn(3)]
            .into_iter()
            .sum();
        assert_eq!(sum, Counts::total(2).with_err(1).with_warn(3));
    }
}
