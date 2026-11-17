//! Checks the installed voice models still work: decodes the built-in test clip with each speech
//! model found and times one polish with S1-mini. Before a release:
//!   cargo build --release -p oasis-engine --example self_check
//!   OASIS_SHERPA_DIR=target/release/native target/release/examples/self_check.exe
fn main() {
    let models = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join("ProjectOASIS").join("Models");
    let Ok(folders) = std::fs::read_dir(models.join("stt")) else {
        eprintln!("no models under {}", models.display());
        std::process::exit(1);
    };
    let mut failed = false;
    for (i, folder) in folders.flatten().filter(|f| f.path().is_dir()).enumerate() {
        let name = folder.file_name().to_string_lossy().into_owned();
        match oasis_engine::self_check::measure(&models, &name, i == 0) {
            Ok(t) => println!("{name}: {:.1} s of speech in {:.2} s; polish {:?} ms {}", t.audio_s, t.decode_s, t.polish_ms, t.polish_error.unwrap_or_default()),
            Err(why) => {
                failed = true;
                println!("{name}: FAILED: {why}");
            }
        }
    }
    std::process::exit(i32::from(failed));
}
