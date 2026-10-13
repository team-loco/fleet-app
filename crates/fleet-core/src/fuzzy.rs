use std::ops::Range;

const MATCHED: i64 = 1;
const CONSECUTIVE: i64 = 5;
const WORD_START: i64 = 8;
const PREFIX: i64 = 10;
const GAP: i64 = 1;
const SEPARATORS: &[char] = &['-', '_', '.', '/', ' ', ':'];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub score: i64,
    pub ranges: Vec<Range<usize>>,
}

pub fn fuzzy(query: &str, text: &str) -> Option<Match> {
    let needle: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    let first = *needle.first()?;
    let haystack: Vec<(usize, char)> = text
        .char_indices()
        .map(|(at, c)| (at, c.to_lowercase().next().unwrap_or(c)))
        .collect();
    haystack
        .iter()
        .enumerate()
        .filter(|(_, (_, c))| *c == first)
        .filter_map(|(start, _)| align(&needle, &haystack, start, text))
        .max_by_key(|m| m.score)
}

fn align(needle: &[char], haystack: &[(usize, char)], start: usize, text: &str) -> Option<Match> {
    let mut positions = Vec::with_capacity(needle.len());
    let mut from = start;
    for wanted in needle {
        let offset = haystack
            .get(from..)?
            .iter()
            .position(|(_, c)| c == wanted)?;
        positions.push(from + offset);
        from += offset + 1;
    }
    let score = score(&positions, haystack);
    let ranges = ranges(&positions, haystack, text.len());
    Some(Match { score, ranges })
}

fn score(positions: &[usize], haystack: &[(usize, char)]) -> i64 {
    let mut total = 0;
    let mut previous: Option<usize> = None;
    for &at in positions {
        total += MATCHED;
        let word_start = at == 0
            || at
                .checked_sub(1)
                .and_then(|before| haystack.get(before))
                .is_some_and(|(_, c)| SEPARATORS.contains(c));
        if word_start {
            total += WORD_START;
        }
        match previous {
            Some(before) if before + 1 == at => total += CONSECUTIVE,
            Some(before) => total -= GAP * i64::try_from(at - before - 1).unwrap_or(i64::MAX),
            None => {}
        }
        previous = Some(at);
    }
    if positions.first() == Some(&0) {
        total += PREFIX;
    }
    total
}

fn ranges(positions: &[usize], haystack: &[(usize, char)], len: usize) -> Vec<Range<usize>> {
    let byte_at = |ix: usize| haystack.get(ix).map_or(len, |(at, _)| *at);
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for &at in positions {
        let start = byte_at(at);
        let end = byte_at(at + 1);
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single(range: Range<usize>) -> Vec<Range<usize>> {
        vec![range]
    }

    #[test]
    fn contiguous_prefix_is_one_range() {
        let hit = fuzzy("checkout", "checkout-api-7f9c6d-x2lqp");
        assert_eq!(hit.map(|m| m.ranges), Some(single(0..8)));
    }

    #[test]
    fn subsequence_matches_with_split_ranges() {
        let hit = fuzzy("cka", "checkout-api");
        assert_eq!(hit.map(|m| m.ranges), Some(vec![0..1, 4..5, 9..10]));
    }

    #[test]
    fn missing_characters_do_not_match() {
        assert_eq!(fuzzy("checkout", "apps-payments"), None);
        assert_eq!(fuzzy("", "anything"), None);
    }

    #[test]
    fn case_and_whitespace_in_the_query_are_ignored() {
        let hit = fuzzy("Toggle  Res", "Toggle resources panel");
        assert!(hit.is_some());
    }

    #[test]
    fn picks_the_best_alignment() {
        let hit = fuzzy("api", "zap-api");
        assert_eq!(hit.map(|m| m.ranges), Some(single(4..7)));
    }

    #[test]
    fn prefix_and_word_starts_outscore_scattered_hits() {
        let score = |text| fuzzy("ca", text).map_or(i64::MIN, |m| m.score);
        assert!(score("checkout-api") > score("xcxa"));
        assert!(score("cart") > score("xcart"));
        assert!(score("x-ca") > score("xxca"));
    }

    #[test]
    fn ranges_are_byte_offsets() {
        let hit = fuzzy("8080", "8080 → 80");
        assert_eq!(hit.map(|m| m.ranges), Some(single(0..4)));
        let arrow = fuzzy("→8", "a → 8");
        assert_eq!(arrow.map(|m| m.ranges), Some(vec![2..5, 6..7]));
    }
}
