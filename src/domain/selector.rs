use thiserror::Error;

use super::TorrentId;

const MIN_HASH_PREFIX: usize = 4;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum SelectorError {
    #[error("torrent '{0}' not found")]
    NotFound(String),
    #[error("selector '{0}' is ambiguous, use more characters")]
    Ambiguous(String),
    #[error("selector '{0}' is too short, use at least {MIN_HASH_PREFIX} characters")]
    TooShort(String),
}

pub fn resolve_selector(
    selector: &str,
    candidates: &[(TorrentId, String)],
) -> Result<TorrentId, SelectorError> {
    let selector = selector.trim();
    if let Ok(id) = selector.parse::<usize>() {
        return candidates
            .iter()
            .find(|(candidate, _)| candidate.0 == id)
            .map(|(candidate, _)| *candidate)
            .ok_or_else(|| SelectorError::NotFound(selector.to_owned()));
    }

    let needle = selector.to_ascii_lowercase();
    if needle.len() < MIN_HASH_PREFIX {
        return Err(SelectorError::TooShort(selector.to_owned()));
    }

    let mut matches = candidates
        .iter()
        .filter(|(_, hash)| hash.to_ascii_lowercase().starts_with(&needle));
    let Some((first, _)) = matches.next() else {
        return Err(SelectorError::NotFound(selector.to_owned()));
    };
    if matches.next().is_some() {
        return Err(SelectorError::Ambiguous(selector.to_owned()));
    }
    Ok(*first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates() -> Vec<(TorrentId, String)> {
        vec![
            (TorrentId(4), "aabbccdd11223344".to_owned()),
            (TorrentId(7), "aabbeeff55667788".to_owned()),
            (TorrentId(9), "ffff000011112222".to_owned()),
        ]
    }

    #[test]
    fn resolves_by_numeric_id() {
        assert_eq!(resolve_selector("7", &candidates()), Ok(TorrentId(7)));
        assert_eq!(resolve_selector(" 9 ", &candidates()), Ok(TorrentId(9)));
    }

    #[test]
    fn resolves_by_unique_hash_prefix() {
        assert_eq!(resolve_selector("ffff", &candidates()), Ok(TorrentId(9)));
        assert_eq!(resolve_selector("AABBCC", &candidates()), Ok(TorrentId(4)));
    }

    #[test]
    fn reports_missing_and_ambiguous_selectors() {
        assert_eq!(
            resolve_selector("123", &candidates()),
            Err(SelectorError::NotFound("123".to_owned()))
        );
        assert_eq!(
            resolve_selector("ff", &candidates()),
            Err(SelectorError::TooShort("ff".to_owned()))
        );
        assert_eq!(
            resolve_selector("aabb", &candidates()),
            Err(SelectorError::Ambiguous("aabb".to_owned()))
        );
    }
}
