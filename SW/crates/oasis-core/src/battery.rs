//! When to warn that the pad's battery is low. Port of `CheckLowBatteryNotice` in
//! `Services/DeviceStatusService.cs` and the thresholds of `BatterySocFormatter.cs`.

/// At or below this (and not charging) the user is told once.
pub const LOW_PERCENT: u8 = 20;
/// The warning is armed again once the battery is this far above the low mark, so a reading that
/// bounces around 20% does not warn over and over.
const REARM_ABOVE: u8 = LOW_PERCENT + 5;

/// Given a reading and whether a warning was already given, returns `(warn_now, warned)`.
pub fn low_battery_check(percent: u8, charging: bool, warned: bool) -> (bool, bool) {
    if charging {
        return (false, warned);
    }
    if percent <= LOW_PERCENT && !warned {
        return (true, true);
    }
    if percent > REARM_ABOVE {
        return (false, false);
    }
    (false, warned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_once_then_rearms_well_above_the_mark() {
        let (warn, warned) = low_battery_check(20, false, false);
        assert!(warn && warned);
        assert_eq!(low_battery_check(15, false, warned), (false, true), "not again");
        assert_eq!(low_battery_check(23, false, warned), (false, true), "still inside the margin");
        assert_eq!(low_battery_check(26, false, warned), (false, false), "re-armed");
        assert!(low_battery_check(19, false, false).0);
    }

    #[test]
    fn charging_never_warns() {
        assert_eq!(low_battery_check(3, true, false), (false, false));
    }
}
