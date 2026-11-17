//! Which voice model set a PC gets (`docs/VOICE_MODEL_SETUP.md` §2.2, decisions V12 to V30). Pure:
//! the facts come from the OS layer, the answer goes to the downloader and to Settings.
//!
//! The QUALITY placement is a candidate only: the graphics card must still start the Vulkan
//! runtime and polish a test sentence in time (§2.3, the self-check), or the PC is BALANCED.

const GB: u64 = 1_000_000_000;
const MB: u64 = 1_000_000;

/// What the PC check found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PcFacts {
    pub processor: String,
    pub avx2: bool,
    pub cores: u32,
    pub threads: u32,
    pub memory_bytes: u64,
    /// Free space on the drive the models go to.
    pub free_disk_bytes: u64,
    pub gpus: Vec<Gpu>,
}

/// A graphics card and the memory of its own (0 for one that shares the main memory).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gpu {
    pub name: String,
    pub memory_bytes: u64,
}

/// The model sets of §2.1, lightest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModelSet {
    Light,
    Balanced,
    Quality,
}

impl ModelSet {
    pub fn name(self) -> &'static str {
        match self {
            // what people read: "Basic", so it is not taken for the normal choice (2026-11-17)
            ModelSet::Light => "BASIC",
            ModelSet::Balanced => "BALANCED",
            ModelSet::Quality => "QUALITY",
        }
    }

    /// What it downloads, from the manifest.
    pub fn download_bytes(self) -> u64 {
        super::models::files_for(self).map(|f| f.bytes).sum()
    }

    pub fn lighter(self) -> Option<ModelSet> {
        match self {
            ModelSet::Light => None,
            ModelSet::Balanced => Some(ModelSet::Light),
            ModelSet::Quality => Some(ModelSet::Balanced),
        }
    }
}

/// Room left on the disk after a download.
pub const DISK_SPARE_BYTES: u64 = GB;
/// A graphics card needs this much memory of its own for the polish model (QUALITY).
pub const QUALITY_GPU_BYTES: u64 = 4 * GB;
/// Main memory for BALANCED and the floor; Windows reports a little under the installed amount,
/// so "6 GB" means 5.5 GB seen and "4 GB" 3.5 GB. BALANCED's speech model peaks at 1.36 GB against
/// 0.66 GB for LIGHT (VOICE_BENCH.md §3), so 6 GB is room enough; was 7.5 GB until 2026-11-17.
/// A PC that turns out too slow is still moved down by the self-check.
const BALANCED_MEMORY_BYTES: u64 = 5_500 * MB;
const FLOOR_MEMORY_BYTES: u64 = 3_500 * MB;
const BALANCED_CORES: u32 = 4;
const FLOOR_THREADS: u32 = 4;

/// Where the PC check put this PC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    /// Download `set`. `wanted` is the set the PC qualified for when the disk forced a lighter one.
    Set { set: ModelSet, wanted: ModelSet, reason: String },
    /// Under the minimum for any local model: offer the cloud set instead.
    BelowFloor { reason: String },
    /// Not even the lightest set fits on the disk.
    NoDisk { wanted: ModelSet, reason: String },
}

/// Place a PC (§2.2): the heaviest set whose requirements all hold, then the heaviest of those
/// that fits on the disk with [`DISK_SPARE_BYTES`] to spare.
pub fn place(f: &PcFacts) -> Placement {
    let memory = gigabytes(f.memory_bytes);
    let mut missing = Vec::new();
    if !f.avx2 {
        missing.push("a processor with AVX2".to_string());
    }
    if f.threads < FLOOR_THREADS {
        missing.push(format!("at least {FLOOR_THREADS} processor threads (it has {})", f.threads));
    }
    if f.memory_bytes < FLOOR_MEMORY_BYTES {
        missing.push(format!("at least 4 GB of memory (it has {memory})"));
    }
    if !missing.is_empty() {
        return Placement::BelowFloor {
            reason: format!("This PC is below the minimum for the voice models on the PC: it needs {}. The cloud set (Groq) still works.", join_and(&missing)),
        };
    }

    let best_gpu = f.gpus.iter().max_by_key(|g| g.memory_bytes);
    let balanced = f.cores >= BALANCED_CORES && f.memory_bytes >= BALANCED_MEMORY_BYTES;
    let gpu_ok = best_gpu.is_some_and(|g| g.memory_bytes >= QUALITY_GPU_BYTES);
    let wanted = match (balanced, gpu_ok) {
        (true, true) => ModelSet::Quality,
        (true, false) => ModelSet::Balanced,
        _ => ModelSet::Light,
    };
    let mut about = format!("This PC has a {}-core processor and {memory} of memory", f.cores);
    if let Some(g) = best_gpu.filter(|_| wanted == ModelSet::Quality) {
        about.push_str(&format!(", and a graphics card with {} of its own ({})", gigabytes(g.memory_bytes), g.name));
    }

    let mut set = Some(wanted);
    while let Some(s) = set.filter(|s| f.free_disk_bytes < s.download_bytes() + DISK_SPARE_BYTES) {
        set = s.lighter();
    }
    let free = gigabytes(f.free_disk_bytes);
    match set {
        Some(set) if set == wanted => Placement::Set { set, wanted, reason: format!("{about}.") },
        Some(set) => Placement::Set {
            set,
            wanted,
            reason: format!("{about}, so it suits {}, but only {free} of disk is free; {} fits.", wanted.name(), set.name()),
        },
        None => Placement::NoDisk {
            wanted,
            reason: format!("{about}, but only {free} of disk is free. Free up {} to download the voice models.", gigabytes(ModelSet::Light.download_bytes() + DISK_SPARE_BYTES)),
        },
    }
}

/// "16 GB", "3.5 GB", "512 MB", in the binary units Windows shows (so 16 GiB of memory reads
/// "16 GB", as on the box).
fn gigabytes(bytes: u64) -> String {
    const GIB: u64 = 1 << 30;
    if bytes < GIB {
        return format!("{} MB", bytes >> 20);
    }
    let tenths = (bytes + GIB / 20) / (GIB / 10);
    if tenths % 10 == 0 { format!("{} GB", tenths / 10) } else { format!("{}.{} GB", tenths / 10, tenths % 10) }
}

fn join_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pc(cores: u32, threads: u32, memory_gb: f64, disk_gb: f64, gpu_gb: Option<f64>) -> PcFacts {
        let bytes = |gb: f64| (gb * (1u64 << 30) as f64) as u64;
        PcFacts {
            processor: "Test CPU".into(),
            avx2: true,
            cores,
            threads,
            memory_bytes: bytes(memory_gb),
            free_disk_bytes: bytes(disk_gb),
            gpus: gpu_gb.map(|g| vec![Gpu { name: "Test GPU".into(), memory_bytes: bytes(g) }]).unwrap_or_default(),
        }
    }

    fn set_of(p: &Placement) -> Option<ModelSet> {
        match p {
            Placement::Set { set, .. } => Some(*set),
            _ => None,
        }
    }

    #[test]
    fn each_kind_of_pc_gets_its_set() {
        // The owner's laptop: i5-9300H, 4 cores, 16 GB, GTX 1650 Max-Q with 4 GB.
        assert_eq!(set_of(&place(&pc(4, 8, 15.8, 200.0, Some(4.0)))), Some(ModelSet::Quality));
        assert_eq!(set_of(&place(&pc(4, 8, 15.8, 200.0, Some(2.0)))), Some(ModelSet::Balanced), "a 2 GB card is too small");
        assert_eq!(set_of(&place(&pc(4, 8, 15.8, 200.0, None))), Some(ModelSet::Balanced));
        assert_eq!(set_of(&place(&pc(2, 4, 7.8, 200.0, Some(8.0)))), Some(ModelSet::Light), "2 cores: light, whatever the card");
        assert_eq!(set_of(&place(&pc(4, 4, 3.9, 200.0, None))), Some(ModelSet::Light), "4 GB of memory: light");
        assert_eq!(set_of(&place(&pc(4, 8, 5.8, 200.0, None))), Some(ModelSet::Balanced), "6 GB of memory: balanced since 2026-11-17");
        assert_eq!(set_of(&place(&pc(4, 8, 7.8, 200.0, Some(4.0)))), Some(ModelSet::Quality), "8 GB and a 4 GB card");
    }

    #[test]
    fn the_balanced_memory_bar_is_5_5_gb() {
        let at = |bytes: u64| set_of(&place(&PcFacts { memory_bytes: bytes, ..pc(4, 8, 0.0, 200.0, None) }));
        assert_eq!(at(5_500_000_000), Some(ModelSet::Balanced));
        assert_eq!(at(5_499_999_999), Some(ModelSet::Light));
    }

    #[test]
    fn light_is_called_basic() {
        assert_eq!(ModelSet::Light.name(), "BASIC");
    }

    #[test]
    fn under_the_floor_gets_no_local_model_and_says_why() {
        let mut old = pc(4, 4, 8.0, 200.0, None);
        old.avx2 = false;
        let Placement::BelowFloor { reason } = place(&old) else { panic!("below the floor") };
        assert!(reason.contains("AVX2") && reason.contains("cloud"), "{reason}");
        let Placement::BelowFloor { reason } = place(&pc(1, 2, 2.0, 200.0, None)) else { panic!("below the floor") };
        assert!(reason.contains("4 processor threads (it has 2)") && reason.contains("4 GB of memory (it has 2 GB)"), "{reason}");
    }

    #[test]
    fn a_full_disk_moves_to_a_lighter_set_or_none() {
        let p = place(&pc(4, 8, 16.0, 1.8, Some(6.0)));
        assert_eq!(set_of(&p), Some(ModelSet::Balanced), "1.18 GB + 1 GB spare does not fit; 0.66 + 1 does");
        let Placement::Set { wanted, reason, .. } = p else { unreachable!() };
        assert_eq!(wanted, ModelSet::Quality);
        assert!(reason.contains("only 1.8 GB of disk is free"), "{reason}");
        assert_eq!(set_of(&place(&pc(4, 8, 16.0, 1.2, None))), Some(ModelSet::Light));
        assert!(matches!(place(&pc(4, 8, 16.0, 0.9, None)), Placement::NoDisk { wanted: ModelSet::Balanced, .. }));
    }

    #[test]
    fn the_reason_names_the_hardware() {
        let Placement::Set { reason, .. } = place(&pc(4, 8, 16.0, 200.0, Some(4.0))) else { panic!() };
        assert_eq!(reason, "This PC has a 4-core processor and 16 GB of memory, and a graphics card with 4 GB of its own (Test GPU).");
        assert_eq!(gigabytes(512 << 20), "512 MB");
        assert_eq!(gigabytes(17_014_000_000), "15.8 GB", "16 GB of memory as Windows reports it");
    }
}
