//! Whether the engine is in the middle of something, for the "restart only when nothing is going
//! on" rule (`docs/VOICE_MODEL_SETUP.md` §5): a recording or transcription, a pad action or a
//! Quick Launch run in progress. Work holds a [`Guard`] while it runs.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static WORK: AtomicU32 = AtomicU32::new(0);
static VOICE: AtomicBool = AtomicBool::new(false);

/// Held while an action or a Quick Launch setup runs; the engine counts as busy until it drops.
pub struct Guard(());

impl Drop for Guard {
    fn drop(&mut self) {
        WORK.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn work() -> Guard {
    WORK.fetch_add(1, Ordering::SeqCst);
    Guard(())
}

/// The voice session is not idle: listening, transcribing or waiting to be typed.
pub fn set_voice_active(active: bool) {
    VOICE.store(active, Ordering::SeqCst);
}

pub fn busy() -> bool {
    WORK.load(Ordering::SeqCst) > 0 || VOICE.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_and_voice_make_the_engine_busy_until_they_end() {
        // One test: the flags are process-wide.
        assert!(!busy());
        let guard = work();
        assert!(busy());
        drop(guard);
        assert!(!busy());
        set_voice_active(true);
        assert!(busy());
        set_voice_active(false);
        assert!(!busy());
    }
}
