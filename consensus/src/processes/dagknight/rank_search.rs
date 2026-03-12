use kaspa_consensus_core::KType;
use kaspa_core::debug;

#[derive(Clone)]
pub struct RankSearchResult<T> {
    pub k: KType,
    pub result: T,
}

pub struct RankSearcher;

impl RankSearcher {
    /// K-searching logic:
    /// 1. Search for an upper bound using powers of 2
    ///    1.1 For each unsuccessful step along the way, move the lower bound k up as well
    ///    1.2 Also exits if lkg_k is a max
    /// 2. Binary search between lower bound k and lkg_k
    pub fn search<T, F>(mut evaluate: F) -> Option<RankSearchResult<T>>
    where
        F: FnMut(KType) -> Option<T>,
    {
        let mut result = None;

        let mut increments: KType = 1;
        let mut lkg_k: KType = 0;
        let mut lower_k: KType = 0;
        let mut found_lkg = false;

        while !found_lkg && lkg_k != u16::MAX {
            debug!("Finding upper bound k = {}", lkg_k);
            if let Some(r) = evaluate(lkg_k) {
                debug!("Found a valid result at upper bound k = {}", lkg_k);
                result = Some(r);
                found_lkg = true;
            } else {
                lower_k = lkg_k + 1;
                lkg_k = increments;
                increments = increments.saturating_mul(2);
            }
        }

        while lower_k < lkg_k {
            let k_to_check = lower_k + ((lkg_k - lower_k) / 2);

            if let Some(r) = evaluate(k_to_check) {
                debug!("Found a valid result at mid k = {} | low = {} | hi = {}", k_to_check, lower_k, lkg_k);
                lkg_k = k_to_check;
                result = Some(r);
            } else {
                lower_k = k_to_check + 1;
            }
        }

        result.map(|r| RankSearchResult { k: lkg_k, result: r })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_full_range_before_max() {
        let mut max_evals = 0;
        let mut max_eval_k;
        for curr_k in 0..u16::MAX {
            let evalations = std::cell::Cell::new(0);
            let evaluate = |k: KType| {
                evalations.set(evalations.get() + 1);
                if k >= curr_k { Some(k) } else { None }
            };
            let result = RankSearcher::search(evaluate);
            assert!(result.is_some());
            assert_eq!(result.unwrap().k, curr_k);

            if evalations.get() > max_evals {
                max_evals = evalations.get();
                max_eval_k = curr_k;
                println!("Max evals changed | evals = {} | k = {}", max_evals, max_eval_k);
            }
        }
    }

    #[test]
    fn test_search_at_max() {
        // max k is treated specially and is an out of bound value
        let curr_k = u16::MAX;
        println!("Testing search for k = {}", curr_k);
        let evaluate = |k: KType| if k >= curr_k { Some(k) } else { None };
        let result = RankSearcher::search(evaluate);
        assert!(result.is_none());
    }
}
