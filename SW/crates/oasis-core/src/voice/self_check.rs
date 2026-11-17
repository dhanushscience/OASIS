//! What the check after the first download decides (`docs/VOICE_MODEL_SETUP.md` §2.3). Specifications
//! predict speed badly, so the app times the downloaded models on this PC and this module turns
//! the numbers into a verdict. Pure: the timing itself is `oasis_engine::self_check`.

use super::placement::ModelSet;

/// The speech model may take at most this many seconds to decode one second of speech (§2.3).
/// Raised from 0.30 on 2026-11-12: with speech decoded in segments while the button is held, the
/// wait after release depends on the last piece, not on how long was said, and the owner's own
/// laptop measured 0.15 to 0.23 depending on its mood (boost clocks, heat).
pub const STT_LIMIT: f64 = 0.40;
/// Within this share of the limit counts as borderline, and goes to the lighter set (V12).
pub const BORDERLINE: f64 = 0.15;
/// S1-mini must polish the test sentence in this long, or polish is switched off.
pub const POLISH_LIMIT_MS: u64 = 600;

/// What was measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Timings {
    /// Length of the test sentence.
    pub audio_s: f64,
    /// Median of three decodes (after a warm-up).
    pub decode_s: f64,
    /// S1-mini on the test sentence; `None` when it was not tried (not a QUALITY set) or could not run.
    pub polish_ms: Option<u64>,
    /// Why S1-mini could not run, when it was tried.
    pub polish_error: Option<String>,
}

/// What the check concluded.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// The set this PC should use. Lower than the one tested when the PC was too slow for it.
    pub set: ModelSet,
    /// Polish (S1-mini) works on this PC. Only ever true for QUALITY.
    pub polish: bool,
    /// One or two sentences for Settings, in plain words.
    pub summary: String,
    /// Even this set is slow here: worth saying so.
    pub slow: bool,
}

/// The better of two measurements of the same models: a busy moment (the app loading, a scan
/// running) only ever makes a measurement worse, so the faster of each number is the true one.
pub fn better(a: &Timings, b: &Timings) -> Timings {
    Timings {
        audio_s: a.audio_s,
        decode_s: a.decode_s.min(b.decode_s),
        polish_ms: match (a.polish_ms, b.polish_ms) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (x, y) => x.or(y),
        },
        polish_error: if a.polish_ms.or(b.polish_ms).is_some() { None } else { b.polish_error.clone().or_else(|| a.polish_error.clone()) },
    }
}

/// Seconds of decoding per second of speech.
pub fn ratio(t: &Timings) -> f64 {
    t.decode_s / t.audio_s.max(0.001)
}

/// Judge the timings of `tested`.
pub fn decide(tested: ModelSet, t: &Timings) -> Outcome {
    let r = ratio(t);
    let said = format!("this PC turns {:.1} s of speech into text in {:.1} s", t.audio_s, t.decode_s);
    let too_slow = r > STT_LIMIT;
    let borderline = r > STT_LIMIT * (1.0 - BORDERLINE);

    if tested == ModelSet::Light {
        let slow = too_slow;
        let summary = if slow {
            format!("BASIC: {said}. That is slow even for the smallest set; Groq (online) in Voice settings would be faster.")
        } else {
            format!("BASIC: {said}.")
        };
        return Outcome { set: ModelSet::Light, polish: false, summary, slow };
    }

    // BALANCED and QUALITY run the same speech model, so slow speech moves both to LIGHT.
    if too_slow || borderline {
        let why = if too_slow { "too slow" } else { "close to too slow" };
        return Outcome {
            set: ModelSet::Light,
            polish: false,
            summary: format!("{}: {said}, {why} for it, so this PC uses BASIC, the faster model that makes more mistakes.", tested.name()),
            slow: false,
        };
    }

    if tested == ModelSet::Quality {
        return match (t.polish_ms, &t.polish_error) {
            (Some(ms), _) if ms <= POLISH_LIMIT_MS => Outcome {
                set: ModelSet::Quality,
                polish: true,
                summary: format!("QUALITY: {said}; S1-mini polished a sentence in {:.2} s.", ms as f64 / 1000.0),
                slow: false,
            },
            (Some(ms), _) => Outcome {
                set: ModelSet::Balanced,
                polish: false,
                summary: format!(
                    "BALANCED: {said}. S1-mini took {:.2} s on the graphics card (limit {:.1} s), so polish is off.",
                    ms as f64 / 1000.0,
                    POLISH_LIMIT_MS as f64 / 1000.0
                ),
                slow: false,
            },
            (None, why) => Outcome {
                set: ModelSet::Balanced,
                polish: false,
                summary: format!(
                    "BALANCED: {said}. S1-mini could not run on the graphics card{}, so polish is off.",
                    why.as_deref().map_or(String::new(), |w| format!(" ({w})"))
                ),
                slow: false,
            },
        };
    }

    Outcome { set: ModelSet::Balanced, polish: false, summary: format!("BALANCED: {said}."), slow: false }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timings(decode_s: f64, polish_ms: Option<u64>) -> Timings {
        Timings { audio_s: 10.0, decode_s, polish_ms, polish_error: None }
    }

    #[test]
    fn the_better_of_two_measurements_wins() {
        let busy = timings(2.8, Some(900));
        let calm = timings(1.6, Some(250));
        let b = better(&busy, &calm);
        assert_eq!((b.decode_s, b.polish_ms), (1.6, Some(250)));
        // A polish that failed once and worked once counts as working.
        let failed = Timings { polish_ms: None, polish_error: Some("busy".into()), ..timings(2.0, None) };
        assert_eq!(better(&failed, &calm).polish_ms, Some(250));
        assert_eq!(better(&failed, &failed).polish_error.as_deref(), Some("busy"));
        // Two busy tests are still judged slow: the PC really is slow.
        assert_eq!(decide(ModelSet::Balanced, &better(&timings(4.5, None), &timings(4.2, None))).set, ModelSet::Light);
    }

    #[test]
    fn a_fast_pc_keeps_its_set() {
        // The owner's laptop measured about 0.16 s per second of speech.
        let o = decide(ModelSet::Balanced, &timings(1.6, None));
        assert_eq!((o.set, o.polish, o.slow), (ModelSet::Balanced, false, false));
        assert_eq!(o.summary, "BALANCED: this PC turns 10.0 s of speech into text in 1.6 s.");
        let q = decide(ModelSet::Quality, &timings(1.6, Some(250)));
        assert_eq!((q.set, q.polish), (ModelSet::Quality, true));
        assert!(q.summary.contains("polished a sentence in 0.25 s"), "{}", q.summary);
    }

    #[test]
    fn too_slow_or_borderline_goes_to_light() {
        for decode in [4.5, 3.5] {
            let o = decide(ModelSet::Balanced, &timings(decode, None));
            assert_eq!(o.set, ModelSet::Light, "{decode} s");
        }
        assert_eq!(decide(ModelSet::Balanced, &timings(3.3, None)).set, ModelSet::Balanced, "0.33 is under the borderline (0.34)");
        assert_eq!(decide(ModelSet::Balanced, &timings(2.4, None)).set, ModelSet::Balanced, "the owner's laptop on a warm day");
        let q = decide(ModelSet::Quality, &timings(4.5, Some(100)));
        assert_eq!((q.set, q.polish), (ModelSet::Light, false), "slow speech moves QUALITY straight to LIGHT");
        assert!(q.summary.contains("too slow for it, so this PC uses BASIC, the faster model that makes more mistakes"), "{}", q.summary);
    }

    #[test]
    fn slow_or_missing_polish_turns_polish_off_and_keeps_balanced() {
        let slow = decide(ModelSet::Quality, &timings(1.0, Some(900)));
        assert_eq!((slow.set, slow.polish), (ModelSet::Balanced, false));
        assert!(slow.summary.contains("took 0.90 s") && slow.summary.contains("limit 0.6 s"), "{}", slow.summary);
        let mut none = timings(1.0, None);
        none.polish_error = Some("no Vulkan driver".into());
        let o = decide(ModelSet::Quality, &none);
        assert_eq!((o.set, o.polish), (ModelSet::Balanced, false));
        assert!(o.summary.contains("(no Vulkan driver)"), "{}", o.summary);
    }

    #[test]
    fn light_stays_light_and_says_when_it_is_slow() {
        assert!(!decide(ModelSet::Light, &timings(0.4, None)).slow);
        let slow = decide(ModelSet::Light, &timings(5.0, None));
        assert_eq!((slow.set, slow.slow), (ModelSet::Light, true));
        assert!(slow.summary.starts_with("BASIC:") && slow.summary.contains("Groq"), "{}", slow.summary);
    }
}
