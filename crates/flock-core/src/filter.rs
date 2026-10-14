use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use crate::Status;

pub const NAMESPACE_PREFIX: &str = "ns:";
pub const LABEL_PREFIXES: [&str; 2] = ["label:", "l:"];
const LABEL_SEPARATOR: char = '=';
const TOKEN_KEYWORDS: [(&str, &str); 4] = [
    (NAMESPACE_PREFIX, NAMESPACE_PREFIX),
    ("namespace", NAMESPACE_PREFIX),
    (LABEL_PREFIXES[0], LABEL_PREFIXES[0]),
    (LABEL_PREFIXES[1], LABEL_PREFIXES[1]),
];

pub type LabelMatch = (String, Option<String>);

pub fn namespace_token(token: &str) -> Option<String> {
    token
        .strip_prefix(NAMESPACE_PREFIX)
        .map(str::to_ascii_lowercase)
}

pub fn label_token(token: &str) -> Option<LabelMatch> {
    let label = LABEL_PREFIXES
        .iter()
        .find_map(|prefix| token.strip_prefix(prefix))?;
    Some(match label.split_once(LABEL_SEPARATOR) {
        Some((key, value)) => (key.to_owned(), Some(value.to_owned())),
        None => (label.to_owned(), None),
    })
}

pub fn label_matches(wanted: &LabelMatch, labels: &BTreeMap<String, String>) -> bool {
    let (key, value) = wanted;
    match (labels.get(key), value) {
        (Some(actual), Some(value)) => actual == value,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

pub type LabelValues = BTreeMap<String, BTreeSet<String>>;

pub fn label_values<'a>(
    sets: impl IntoIterator<Item = &'a BTreeMap<String, String>>,
) -> LabelValues {
    let mut values = LabelValues::new();
    for (key, value) in sets.into_iter().flatten() {
        values.entry(key.clone()).or_default().insert(value.clone());
    }
    values
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterCandidates {
    pub namespaces: BTreeSet<String>,
    pub labels: LabelValues,
}

impl FilterCandidates {
    pub fn suggest(&self, input: &str) -> Vec<String> {
        if input.ends_with(char::is_whitespace) {
            return Vec::new();
        }
        let Some(token) = input.split_whitespace().last() else {
            return Vec::new();
        };
        if let Some(part) = token.strip_prefix(NAMESPACE_PREFIX) {
            return self
                .namespaces
                .iter()
                .filter(|name| name.starts_with(part))
                .map(|name| format!("{NAMESPACE_PREFIX}{name}"))
                .collect();
        }
        if let Some((prefix, part)) = LABEL_PREFIXES
            .iter()
            .find_map(|prefix| Some((*prefix, token.strip_prefix(prefix)?)))
        {
            return self.suggest_label(prefix, part);
        }
        let prefixes: BTreeSet<&str> = TOKEN_KEYWORDS
            .iter()
            .filter(|(keyword, _)| keyword.starts_with(token))
            .map(|(_, prefix)| *prefix)
            .collect();
        prefixes.into_iter().map(str::to_owned).collect()
    }

    fn suggest_label(&self, prefix: &str, part: &str) -> Vec<String> {
        match part.split_once(LABEL_SEPARATOR) {
            Some((key, part)) => self
                .labels
                .get(key)
                .into_iter()
                .flatten()
                .filter(|value| value.starts_with(part))
                .map(|value| format!("{prefix}{key}{LABEL_SEPARATOR}{value}"))
                .collect(),
            None => self
                .labels
                .keys()
                .filter(|key| key.starts_with(part))
                .map(|key| format!("{prefix}{key}{LABEL_SEPARATOR}"))
                .collect(),
        }
    }
}

pub fn extract_tokens(input: &str) -> (Vec<String>, String) {
    let mut tokens = Vec::new();
    let mut text = String::new();
    for part in input.split_inclusive(char::is_whitespace) {
        let token = part.trim_end();
        if part.ends_with(char::is_whitespace) && applied_token(token) {
            tokens.push(token.to_owned());
        } else {
            text.push_str(part);
        }
    }
    if text.trim().is_empty() {
        text.clear();
    }
    (tokens, text)
}

pub fn applied_token(token: &str) -> bool {
    if let Some(namespace) = namespace_token(token) {
        return !namespace.is_empty();
    }
    label_token(token)
        .is_some_and(|(key, value)| !key.is_empty() && value.is_none_or(|value| !value.is_empty()))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusFilter {
    #[default]
    All,
    Attention,
}

impl StatusFilter {
    pub const ALL: [Self; 2] = [Self::All, Self::Attention];

    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Attention => "Needs attention",
        }
    }

    pub fn at(ix: usize) -> Self {
        Self::ALL.get(ix).copied().unwrap_or_default()
    }

    pub fn keeps(self, status: Status) -> bool {
        match self {
            Self::All => true,
            Self::Attention => status != Status::Ok,
        }
    }

    pub fn counts(statuses: impl IntoIterator<Item = Status>) -> Vec<(Self, usize)> {
        let statuses: Vec<Status> = statuses.into_iter().collect();
        Self::ALL
            .iter()
            .map(|filter| {
                let count = statuses.iter().filter(|s| filter.keeps(**s)).count();
                (*filter, count)
            })
            .collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextFilter {
    pub words: Vec<String>,
    pub namespace: Option<String>,
    pub labels: Vec<LabelMatch>,
}

impl TextFilter {
    pub fn parse(input: &str) -> Self {
        let mut filter = Self::default();
        for token in input.split_whitespace() {
            if let Some(namespace) = namespace_token(token) {
                filter.namespace = Some(namespace);
            } else if let Some(label) = label_token(token) {
                filter.labels.push(label);
            } else {
                filter.words.push(token.to_ascii_lowercase());
            }
        }
        filter
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.namespace.is_none() && self.labels.is_empty()
    }

    pub fn matches_text(&self, fields: &[&str]) -> bool {
        self.words.iter().all(|word| {
            fields
                .iter()
                .any(|field| field.to_ascii_lowercase().contains(word.as_str()))
        })
    }

    pub fn matches_namespace(&self, namespace: Option<&str>) -> bool {
        self.namespace.as_ref().is_none_or(|wanted| {
            namespace.is_some_and(|ns| ns.to_ascii_lowercase().contains(wanted.as_str()))
        })
    }

    pub fn matches_labels(&self, labels: &BTreeMap<String, String>) -> bool {
        self.labels
            .iter()
            .all(|wanted| label_matches(wanted, labels))
    }

    pub fn matches(&self, namespace: Option<&str>, fields: &[&str]) -> bool {
        self.labels.is_empty() && self.matches_namespace(namespace) && self.matches_text(fields)
    }

    pub fn matches_labeled(
        &self,
        namespace: Option<&str>,
        labels: &BTreeMap<String, String>,
        fields: &[&str],
    ) -> bool {
        self.matches_namespace(namespace)
            && self.matches_labels(labels)
            && self.matches_text(fields)
    }

    pub fn highlights(&self, text: &str) -> Vec<Range<usize>> {
        highlights(&self.words, text)
    }
}

pub fn highlights(words: &[String], text: &str) -> Vec<Range<usize>> {
    let haystack = text.to_ascii_lowercase();
    let mut ranges: Vec<Range<usize>> = words
        .iter()
        .filter(|word| !word.is_empty())
        .flat_map(|word| {
            haystack
                .match_indices(word.as_str())
                .map(|(start, hit)| start..start + hit.len())
                .collect::<Vec<_>>()
        })
        .collect();
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn extracting_complete_tokens_keeps_free_text_and_its_word_boundaries() {
        assert_eq!(
            extract_tokens("checkout ns:payments api "),
            (vec!["ns:payments".into()], "checkout api ".into())
        );
        assert_eq!(
            extract_tokens("ns:payments  "),
            (vec!["ns:payments".into()], String::new())
        );
        assert_eq!(
            extract_tokens("label:app=cart checkout"),
            (vec!["label:app=cart".into()], "checkout".into())
        );
        assert_eq!(
            extract_tokens("checkout  l:app="),
            (Vec::new(), "checkout  l:app=".into())
        );
        assert_eq!(
            extract_tokens("ns:payments"),
            (Vec::new(), "ns:payments".into())
        );
    }

    #[test]
    fn completion_suggests_prefixes_namespaces_and_only_current_label_values() {
        let sets = [
            labels(&[("app", "cart"), ("tier", "api")]),
            labels(&[("app", "web")]),
        ];
        let none: Vec<String> = Vec::new();
        let candidates = FilterCandidates {
            namespaces: BTreeSet::from(["payments".into(), "platform".into()]),
            labels: label_values(&sets),
        };
        assert_eq!(candidates.suggest("n"), ["ns:"]);
        assert_eq!(candidates.suggest("na"), ["ns:"]);
        assert_eq!(candidates.suggest("l"), ["l:", "label:"]);
        assert_eq!(candidates.suggest("nginx"), none);
        assert_eq!(candidates.suggest("ns:"), ["ns:payments", "ns:platform"]);
        assert_eq!(candidates.suggest("checkout ns:pay"), ["ns:payments"]);
        assert_eq!(candidates.suggest("l:"), ["l:app=", "l:tier="]);
        assert_eq!(candidates.suggest("label:a"), ["label:app="]);
        assert_eq!(
            candidates.suggest("label:app="),
            ["label:app=cart", "label:app=web"]
        );
        assert_eq!(candidates.suggest("l:app=c"), ["l:app=cart"]);
        assert_eq!(candidates.suggest("label:unknown="), none);
        assert_eq!(candidates.suggest("checkout"), none);
        assert_eq!(candidates.suggest("ns:payments "), none);
        assert_eq!(candidates.suggest(""), none);
    }

    #[test]
    fn chips_require_complete_tokens_and_preserve_label_case() {
        for token in ["ns:payments", "label:App=Cart", "l:tier"] {
            assert!(applied_token(token), "{token}");
        }
        for token in ["ns:", "label:", "l:app=", "checkout", "label:=cart"] {
            assert!(!applied_token(token), "{token}");
        }
        let filter = TextFilter::parse("checkout ns:payments label:App=Cart");
        assert!(filter.matches_labeled(
            Some("payments"),
            &labels(&[("App", "Cart")]),
            &["checkout"]
        ));
        assert!(!filter.matches_labeled(
            Some("payments"),
            &labels(&[("App", "cart")]),
            &["checkout"]
        ));
    }

    #[test]
    fn parses_namespace_and_label_tokens_like_the_palette() {
        let filter = TextFilter::parse("Checkout ns:Pay label:app=cart l:tier api");
        assert_eq!(filter.words, ["checkout", "api"]);
        assert_eq!(filter.namespace.as_deref(), Some("pay"));
        assert_eq!(
            filter.labels,
            [
                ("app".to_owned(), Some("cart".to_owned())),
                ("tier".to_owned(), None)
            ]
        );
        assert!(TextFilter::parse("   ").is_empty());
    }

    #[test]
    fn every_word_must_hit_some_field() {
        let filter = TextFilter::parse("check API");
        assert!(filter.matches(None, &["checkout-api", "payments"]));
        assert!(filter.matches(None, &["checkout", "api-gw"]));
        assert!(!filter.matches(None, &["checkout", "payments"]));
        assert!(TextFilter::default().matches(None, &[]));
    }

    #[test]
    fn namespace_tokens_match_by_substring_and_need_a_namespace() {
        let filter = TextFilter::parse("ns:pay");
        assert!(filter.matches(Some("payments"), &[]));
        assert!(!filter.matches(Some("orders"), &[]));
        assert!(!filter.matches(None, &[]));
    }

    #[test]
    fn label_tokens_need_labels() {
        let filter = TextFilter::parse("l:app=cart");
        assert!(filter.matches_labeled(None, &labels(&[("app", "cart")]), &[]));
        assert!(!filter.matches_labeled(None, &labels(&[("app", "web")]), &[]));
        assert!(!filter.matches(None, &["app=cart"]));
        let key_only = TextFilter::parse("label:app");
        assert!(key_only.matches_labeled(None, &labels(&[("app", "web")]), &[]));
    }

    #[test]
    fn status_chips_count_everything_and_what_needs_attention() {
        let statuses = [Status::Ok, Status::Err, Status::Warn, Status::Ok];
        assert_eq!(
            StatusFilter::counts(statuses.iter().copied()),
            [(StatusFilter::All, 4), (StatusFilter::Attention, 2)]
        );
        assert_eq!(StatusFilter::at(9), StatusFilter::All);
    }

    #[test]
    fn highlights_merge_overlapping_hits() {
        let filter = TextFilter::parse("api pi CHECK");
        assert_eq!(
            filter.highlights("checkout-api-api"),
            vec![0..5, 9..12, 13..16]
        );
        assert_eq!(
            TextFilter::default().highlights("checkout"),
            Vec::<Range<usize>>::new()
        );
    }
}
