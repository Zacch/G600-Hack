//! Deciding how to get a profile onto the mouse with as few flash writes
//! as possible.
//!
//! The mouse has 3 slots. If the wanted profile already sits in a slot we
//! just select it. Otherwise we overwrite the least recently used slot that
//! is not active (writing the active slot corrupts it; see
//! g600_core::device).

use std::time::Instant;

use g600_core::profile::PROFILE_REPORT_LEN;

pub type Content = [u8; PROFILE_REPORT_LEN - 1];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// Already active.
    Nothing,
    /// Present in this (inactive) slot: select it.
    Select(usize),
    /// Write into this inactive slot, then select it.
    WriteThenSelect(usize),
}

/// What we believe each slot holds, and when each was last selected.
#[derive(Debug, Clone)]
pub struct SlotState {
    pub content: [Content; 3],
    pub last_used: [Option<Instant>; 3],
}

impl SlotState {
    pub fn new(content: [Content; 3]) -> Self {
        SlotState { content, last_used: [None; 3] }
    }

    pub fn plan(&self, wanted: &Content, active: usize) -> Plan {
        if self.content[active] == *wanted {
            return Plan::Nothing;
        }
        if let Some(s) = (0..3).find(|&s| s != active && self.content[s] == *wanted) {
            return Plan::Select(s);
        }
        // Never-used slots (None) sort first, then the oldest.
        let victim = (0..3)
            .filter(|&s| s != active)
            .min_by_key(|&s| self.last_used[s])
            .expect("two inactive slots always exist");
        Plan::WriteThenSelect(victim)
    }

    pub fn mark_used(&mut self, slot: usize) {
        self.last_used[slot] = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn c(n: u8) -> Content {
        [n; PROFILE_REPORT_LEN - 1]
    }

    #[test]
    fn reuses_and_evicts_lru() {
        let mut s = SlotState::new([c(1), c(2), c(3)]);
        assert_eq!(s.plan(&c(1), 0), Plan::Nothing);
        assert_eq!(s.plan(&c(3), 0), Plan::Select(2));
        // Unknown profile: lowest unused inactive slot.
        assert_eq!(s.plan(&c(9), 0), Plan::WriteThenSelect(1));

        let t0 = Instant::now();
        s.last_used = [Some(t0), Some(t0 + Duration::from_secs(5)), Some(t0 + Duration::from_secs(1))];
        // Active 0: slots 1 (newer) and 2 (older) are candidates -> 2.
        assert_eq!(s.plan(&c(9), 0), Plan::WriteThenSelect(2));
        // Active 2: never evict the active slot even if it is the oldest.
        s.last_used[2] = Some(t0 - Duration::from_secs(9));
        assert_eq!(s.plan(&c(9), 2), Plan::WriteThenSelect(0));
    }

    #[test]
    fn duplicate_content_in_active_slot_means_nothing() {
        let s = SlotState::new([c(1), c(1), c(2)]);
        assert_eq!(s.plan(&c(1), 1), Plan::Nothing);
    }
}
