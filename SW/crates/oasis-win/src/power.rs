//! Whether the PC runs on its battery, and whether Battery Saver is on: on battery the CPU and
//! graphics card are slowed down, so things that wait for them (the dictation polish) need longer.

use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Power {
    /// Running on the battery (not plugged in). A PC without a battery counts as plugged in.
    pub on_battery: bool,
    /// Windows' Battery Saver is on.
    pub saver: bool,
}

/// The power state now. Cheap (no I/O); if Windows does not answer, plugged in.
#[allow(unsafe_code)]
pub fn status() -> Power {
    let mut raw = SYSTEM_POWER_STATUS {
        ACLineStatus: 255,
        BatteryFlag: 0,
        BatteryLifePercent: 0,
        SystemStatusFlag: 0,
        BatteryLifeTime: 0,
        BatteryFullLifeTime: 0,
    };
    // SAFETY: `raw` is a valid, writable struct for the call.
    if unsafe { GetSystemPowerStatus(&mut raw) } == 0 {
        return Power::default();
    }
    Power { on_battery: raw.ACLineStatus == 0, saver: raw.SystemStatusFlag == 1 }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_power_state_can_be_read() {
        let _ = super::status();
    }
}
