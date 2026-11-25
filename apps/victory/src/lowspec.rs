//! The console profile chosen when the user does not choose one: the
//! Series S profile on a PC with little memory.
//!
//! The Series S profile renders the world at a lower resolution and the
//! title upscales it with its own upscaler. It is the faster profile on the
//! tested machine and commits less memory, which is what decides whether a
//! 16 GB PC holds a run at all.

use crate::Console;

/// Below this much physical memory the Series S profile is the default.
/// A Series X run commits about 20 GB; on 16 GB PCs it ran out where the
/// Series S profile peaked at about 18 GB and held with the page file.
pub const SERIES_S_BELOW: u64 = 24 << 30;

/// The console the title is told it runs on, and why: the user's choice,
/// else Series S below [`SERIES_S_BELOW`] of physical memory, else Series X
/// (also when the memory is not known).
pub fn console(chosen: Option<Console>, physical: Option<u64>) -> (Console, &'static str) {
    match (chosen, physical) {
        (Some(c), _) => (c, "chosen with --console"),
        (None, Some(bytes)) if bytes < SERIES_S_BELOW => (
            Console::SeriesS,
            "less than 24 GB of memory: the Series S profile commits less",
        ),
        (None, Some(_)) => (Console::SeriesX, "24 GB of memory or more"),
        (None, None) => (Console::SeriesX, "the PC's memory is not known"),
    }
}

/// The PC's physical memory in bytes.
#[cfg(windows)]
pub fn physical_memory() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: a sized MEMORYSTATUSEX for the call to fill.
    (unsafe { GlobalMemoryStatusEx(&mut m) } != 0).then_some(m.ullTotalPhys)
}

#[cfg(not(windows))]
pub fn physical_memory() -> Option<u64> {
    None
}

/// `VICTORY_FAST=1`: the scheduling switches measured fastest in
/// clankenjoyer/victory-fps (about 150 ms a frame standing still on an RTX
/// A4500, against 204-234 ms without them), for those that flamingo has and
/// leaves off. They change when work is submitted and waited for, not what
/// is drawn. Not measured on flamingo itself, hence off unless asked for;
/// a variable already set keeps its value. Left out: `VICTORY_VIEW_KNOWN`,
/// with which flamingo's write-watch tests miss a write told at one view
/// of a page at its others (stale pictures).
pub const FAST: &[(&str, &str)] = &[
    ("VICTORY_ASYNC_SUBMIT", "1"),
    ("VICTORY_ASYNC_PRESENT", "1"),
    ("VICTORY_QUEUE2", "1"),
    ("VICTORY_MERGE_RUNS", "65536"),
    ("VICTORY_LATE_WAIT_LISTS", "512"),
    ("VICTORY_INDIRECT_WINDOW", "1"),
];

/// The switches [`FAST`] would set: none unless `fast`, and none the
/// environment already sets (`set` says which it does).
pub fn fast_switches(fast: bool, set: impl Fn(&str) -> bool) -> Vec<(&'static str, &'static str)> {
    if !fast {
        return Vec::new();
    }
    FAST.iter().copied().filter(|(name, _)| !set(name)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_is_off_unless_asked_and_keeps_what_is_set() {
        assert!(fast_switches(false, |_| false).is_empty());
        assert_eq!(fast_switches(true, |_| false).len(), FAST.len());
        let kept = fast_switches(true, |n| n == "VICTORY_QUEUE2");
        assert_eq!(kept.len(), FAST.len() - 1);
        assert!(kept.iter().all(|(n, _)| *n != "VICTORY_QUEUE2"));
    }

    const GB: u64 = 1 << 30;

    #[test]
    fn the_user_choice_stands() {
        for chosen in [Console::SeriesX, Console::SeriesS] {
            for physical in [None, Some(8 * GB), Some(64 * GB)] {
                let picked = console(Some(chosen), physical).0;
                assert_eq!(picked as u8, chosen as u8);
            }
        }
    }

    #[test]
    fn series_s_below_24_gb_series_x_from_it() {
        let pick = |gb: u64| console(None, Some(gb * GB)).0;
        // 16 GB machines report a little less than 16 GiB.
        assert!(matches!(pick(15), Console::SeriesS));
        assert!(matches!(pick(16), Console::SeriesS));
        assert!(matches!(pick(23), Console::SeriesS));
        assert!(matches!(pick(24), Console::SeriesX));
        assert!(matches!(pick(32), Console::SeriesX));
        assert!(matches!(console(None, None).0, Console::SeriesX));
    }
}
