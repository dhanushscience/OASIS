//! Basic value types shared by every module. Port of `Oasis.Core/Primitives.cs`.

use std::ops::{Add, BitAnd, BitOr, Not};
use std::time::Duration;

/// One of the pad's physical buttons. Index 0, 1, 2 = BTN 1, 2, 3 on the device.
///
/// The firmware may in theory send any number after `BTN:`; only 0..=2 are real buttons
/// ([`ButtonId::is_pad`]). Reducers ignore anything else rather than failing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ButtonId(pub i32);

impl ButtonId {
    pub const ONE: ButtonId = ButtonId(0);
    pub const TWO: ButtonId = ButtonId(1);
    pub const THREE: ButtonId = ButtonId(2);

    pub fn is_pad(self) -> bool {
        (0..=2).contains(&self.0)
    }

    /// The single-bit mask for this button; [`ButtonMask::NONE`] for a non-pad index.
    pub fn mask(self) -> ButtonMask {
        match self.0 {
            0 => ButtonMask::ONE,
            1 => ButtonMask::TWO,
            2 => ButtonMask::THREE,
            _ => ButtonMask::NONE,
        }
    }
}

/// A set of buttons held together (C# `[Flags] enum ButtonMask`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ButtonMask(pub u8);

impl ButtonMask {
    pub const NONE: ButtonMask = ButtonMask(0);
    pub const ONE: ButtonMask = ButtonMask(1);
    pub const TWO: ButtonMask = ButtonMask(2);
    pub const THREE: ButtonMask = ButtonMask(4);

    pub fn with(self, id: ButtonId) -> ButtonMask {
        self | id.mask()
    }

    pub fn without(self, id: ButtonId) -> ButtonMask {
        self & !id.mask()
    }

    pub fn contains(self, id: ButtonId) -> bool {
        (self & id.mask()).0 != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Buttons in this mask, lowest index first.
    pub fn buttons(self) -> impl Iterator<Item = ButtonId> {
        [ButtonId::ONE, ButtonId::TWO, ButtonId::THREE]
            .into_iter()
            .filter(move |b| self.contains(*b))
    }
}

impl BitOr for ButtonMask {
    type Output = ButtonMask;
    fn bitor(self, rhs: ButtonMask) -> ButtonMask {
        ButtonMask(self.0 | rhs.0)
    }
}

impl BitAnd for ButtonMask {
    type Output = ButtonMask;
    fn bitand(self, rhs: ButtonMask) -> ButtonMask {
        ButtonMask(self.0 & rhs.0)
    }
}

impl Not for ButtonMask {
    type Output = ButtonMask;
    fn not(self) -> ButtonMask {
        ButtonMask(!self.0 & 0b111)
    }
}

/// Monotonic timestamp in nanoseconds since an arbitrary origin.
///
/// Never wall-clock time: a clock adjustment in the middle of a hold would otherwise turn a tap
/// into a hold or the other way round (the C# `Instant` comment says the same).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Instant(pub i64);

impl Instant {
    pub fn from_millis(ms: i64) -> Instant {
        Instant(ms * 1_000_000)
    }

    pub fn plus(self, delta: Duration) -> Instant {
        Instant(self.0 + duration_nanos(delta))
    }

    /// Time elapsed from `earlier` to `self`; zero if `earlier` is later.
    pub fn since(self, earlier: Instant) -> Duration {
        Duration::from_nanos((self.0 - earlier.0).max(0) as u64)
    }

    pub fn is_at_or_after(self, other: Instant) -> bool {
        self.0 >= other.0
    }
}

impl Add<Duration> for Instant {
    type Output = Instant;
    fn add(self, rhs: Duration) -> Instant {
        self.plus(rhs)
    }
}

fn duration_nanos(d: Duration) -> i64 {
    i64::try_from(d.as_nanos()).unwrap_or(i64::MAX)
}

/// Source of time for hosts. Reducers never call this; hosts pass `now` in.
pub trait Clock {
    fn now(&self) -> Instant;
}

/// Real monotonic clock (`std::time::Instant`), anchored at first use.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        use std::sync::OnceLock;
        static ORIGIN: OnceLock<std::time::Instant> = OnceLock::new();
        let origin = *ORIGIN.get_or_init(std::time::Instant::now);
        Instant(duration_nanos(origin.elapsed()))
    }
}

/// Identifies one dispatched command (C# `CommandId(Guid)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandId(pub u128);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_add_remove_contains() {
        let m = ButtonMask::NONE.with(ButtonId::ONE).with(ButtonId::THREE);
        assert!(m.contains(ButtonId::ONE));
        assert!(!m.contains(ButtonId::TWO));
        assert_eq!(m.without(ButtonId::ONE), ButtonMask::THREE);
        assert!(m.without(ButtonId::ONE).without(ButtonId::THREE).is_empty());
        assert_eq!(ButtonId(7).mask(), ButtonMask::NONE);
    }

    #[test]
    fn instant_math() {
        let a = Instant::from_millis(100);
        let b = a.plus(Duration::from_millis(250));
        assert_eq!(b.since(a), Duration::from_millis(250));
        assert!(b.is_at_or_after(a));
        assert_eq!(a.since(b), Duration::ZERO);
    }
}
