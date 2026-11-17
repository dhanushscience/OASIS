//! The PC check for the voice models (`docs/VOICE_MODEL_SETUP.md` §2.2): this PC's facts, read
//! through `oasis_win::hardware`, in the form `oasis_core::voice::placement` decides on.

use std::path::Path;

use oasis_core::voice::placement::{Gpu, PcFacts};

/// This PC's facts; free disk is for the drive of `models_folder`.
pub fn pc_facts(models_folder: &Path) -> PcFacts {
    let hw = oasis_win::hardware::read(models_folder);
    PcFacts {
        processor: hw.processor,
        avx2: hw.avx2,
        cores: hw.cores,
        threads: hw.threads,
        memory_bytes: hw.memory_bytes,
        free_disk_bytes: hw.free_disk_bytes,
        gpus: hw.gpus.into_iter().map(|(name, memory_bytes)| Gpu { name, memory_bytes }).collect(),
    }
}
