//! The on-device speech-to-text engine: sherpa-onnx's C API, loaded from its DLL at run time
//! (DECISIONS D8: sherpa-onnx is what we ship). Port of `SpeechRecognizer.LocalModel.cs` and
//! `SttModelCatalog.cs` for a transducer model such as NVIDIA Parakeet.
//!
//! Loading the DLL when it is needed, instead of linking it, means building OASIS needs no
//! native toolchain, and the app starts (without voice) when the DLL is absent. The structs
//! below mirror `c-api.h` of **sherpa-onnx 1.13.5**, field for field; the DLL must be that
//! version, because the C API passes these structs by pointer and a different layout would be
//! read as garbage. The tests prove the layout with a real model when one is present.

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::ptr;

use libloading::Library;
use oasis_models::{AudioClip, ModelError, SttEngine};

pub const DLL_NAME: &str = "sherpa-onnx-c-api.dll";
/// The DLL's own dependency; it must be in the same folder.
pub const ONNX_RUNTIME_DLL: &str = "onnxruntime.dll";

// ---- the C structs, in header order ------------------------------------------------------------

type Cstr = *const c_char;

#[repr(C)]
struct FeatureConfig {
    sample_rate: i32,
    feature_dim: i32,
}
#[repr(C)]
struct TransducerConfig {
    encoder: Cstr,
    decoder: Cstr,
    joiner: Cstr,
}
#[repr(C)]
struct OneModel {
    model: Cstr,
}
#[repr(C)]
struct WhisperConfig {
    encoder: Cstr,
    decoder: Cstr,
    language: Cstr,
    task: Cstr,
    tail_paddings: i32,
    enable_token_timestamps: i32,
    enable_segment_timestamps: i32,
}
#[repr(C)]
struct SenseVoiceConfig {
    model: Cstr,
    language: Cstr,
    use_itn: i32,
}
#[repr(C)]
struct MoonshineConfig {
    preprocessor: Cstr,
    encoder: Cstr,
    uncached_decoder: Cstr,
    cached_decoder: Cstr,
    merged_decoder: Cstr,
}
#[repr(C)]
struct FireRedAsrConfig {
    encoder: Cstr,
    decoder: Cstr,
}
#[repr(C)]
struct CanaryConfig {
    encoder: Cstr,
    decoder: Cstr,
    src_lang: Cstr,
    tgt_lang: Cstr,
    use_pnc: i32,
}
#[repr(C)]
struct FunAsrNanoConfig {
    encoder_adaptor: Cstr,
    llm: Cstr,
    embedding: Cstr,
    tokenizer: Cstr,
    system_prompt: Cstr,
    user_prompt: Cstr,
    max_new_tokens: i32,
    temperature: f32,
    top_p: f32,
    seed: i32,
    language: Cstr,
    itn: i32,
    hotwords: Cstr,
}
#[repr(C)]
struct Qwen3AsrConfig {
    conv_frontend: Cstr,
    encoder: Cstr,
    decoder: Cstr,
    tokenizer: Cstr,
    max_total_len: i32,
    max_new_tokens: i32,
    temperature: f32,
    top_p: f32,
    seed: i32,
    hotwords: Cstr,
}
#[repr(C)]
struct CohereConfig {
    encoder: Cstr,
    decoder: Cstr,
    language: Cstr,
    use_punct: i32,
    use_itn: i32,
}
#[repr(C)]
struct ModelConfig {
    transducer: TransducerConfig,
    paraformer: OneModel,
    nemo_ctc: OneModel,
    whisper: WhisperConfig,
    tdnn: OneModel,
    tokens: Cstr,
    num_threads: i32,
    debug: i32,
    provider: Cstr,
    model_type: Cstr,
    modeling_unit: Cstr,
    bpe_vocab: Cstr,
    telespeech_ctc: Cstr,
    sense_voice: SenseVoiceConfig,
    moonshine: MoonshineConfig,
    fire_red_asr: FireRedAsrConfig,
    dolphin: OneModel,
    zipformer_ctc: OneModel,
    canary: CanaryConfig,
    wenet_ctc: OneModel,
    omnilingual: OneModel,
    medasr: OneModel,
    funasr_nano: FunAsrNanoConfig,
    fire_red_asr_ctc: OneModel,
    qwen3_asr: Qwen3AsrConfig,
    cohere_transcribe: CohereConfig,
}
#[repr(C)]
struct LmConfig {
    model: Cstr,
    scale: f32,
}
#[repr(C)]
struct HomophoneReplacerConfig {
    dict_dir: Cstr,
    lexicon: Cstr,
    rule_fsts: Cstr,
}
#[repr(C)]
struct RecognizerConfig {
    feat_config: FeatureConfig,
    model_config: ModelConfig,
    lm_config: LmConfig,
    decoding_method: Cstr,
    max_active_paths: i32,
    hotwords_file: Cstr,
    hotwords_score: f32,
    rule_fsts: Cstr,
    rule_fars: Cstr,
    blank_penalty: f32,
    hr: HomophoneReplacerConfig,
}
/// Only the first field of the result is read.
#[repr(C)]
struct RecognizerResult {
    text: Cstr,
}

type CreateRecognizer = unsafe extern "C" fn(*const RecognizerConfig) -> *const c_void;
type DestroyRecognizer = unsafe extern "C" fn(*const c_void);
type CreateStream = unsafe extern "C" fn(*const c_void) -> *const c_void;
type DestroyStream = unsafe extern "C" fn(*const c_void);
type AcceptWaveform = unsafe extern "C" fn(*const c_void, i32, *const f32, i32);
type Decode = unsafe extern "C" fn(*const c_void, *const c_void);
type GetResult = unsafe extern "C" fn(*const c_void) -> *const RecognizerResult;
type DestroyResult = unsafe extern "C" fn(*const RecognizerResult);

// ---- which files make a model ------------------------------------------------------------------

/// The files of a transducer model folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransducerLayout {
    pub tokens: PathBuf,
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: PathBuf,
}

/// Find the model files in `folder`. A folder may hold several builds of the model: int8 is the
/// one for a CPU (quantised ops do not run on the CUDA provider), fp16 for a GPU, and a plain
/// build as the middle choice.
pub fn detect_transducer(folder: &Path, prefer_gpu: bool) -> Option<TransducerLayout> {
    let tokens = folder.join("tokens.txt");
    if !tokens.is_file() {
        return None;
    }
    let builds: [&str; 3] = if prefer_gpu { ["fp16", "", "int8"] } else { ["int8", "", "fp16"] };
    builds.iter().find_map(|build| {
        let suffix = if build.is_empty() { ".onnx".to_string() } else { format!(".{build}.onnx") };
        let (encoder, decoder, joiner) = (
            folder.join(format!("encoder{suffix}")),
            folder.join(format!("decoder{suffix}")),
            folder.join(format!("joiner{suffix}")),
        );
        (encoder.is_file() && decoder.is_file() && joiner.is_file())
            .then(|| TransducerLayout { tokens: tokens.clone(), encoder, decoder, joiner })
    })
}

/// Folders where the native DLLs may be, nearest first: an explicit `OASIS_SHERPA_DIR`, beside the
/// program, and a `native` folder beside it.
pub fn dll_search_folders(exe_dir: Option<&Path>, override_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    folders.extend(override_dir.map(Path::to_path_buf));
    if let Some(dir) = exe_dir {
        folders.push(dir.to_path_buf());
        folders.push(dir.join("native"));
    }
    folders
}

/// The first folder that holds both DLLs.
pub fn find_dll_folder(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|f| f.join(DLL_NAME).is_file() && f.join(ONNX_RUNTIME_DLL).is_file()).cloned()
}

/// Any model this wrapper can load: a transducer (Parakeet TDT), a NeMo CTC model (Parakeet
/// TDT-CTC 110M), or Moonshine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelLayout {
    Transducer(TransducerLayout),
    NemoCtc {
        tokens: PathBuf,
        model: PathBuf,
    },
    Moonshine {
        tokens: PathBuf,
        preprocessor: PathBuf,
        encoder: PathBuf,
        uncached_decoder: PathBuf,
        cached_decoder: PathBuf,
    },
}

/// The model in `folder`, of whichever kind: a transducer first, then a single-file CTC model
/// (`model.int8.onnx` / `model.onnx`), then Moonshine (`preprocess.onnx` and three `.int8.onnx`).
pub fn detect_model(folder: &Path, prefer_gpu: bool) -> Option<ModelLayout> {
    if let Some(t) = detect_transducer(folder, prefer_gpu) {
        return Some(ModelLayout::Transducer(t));
    }
    let tokens = folder.join("tokens.txt");
    if !tokens.is_file() {
        return None;
    }
    if let Some(model) = ["model.int8.onnx", "model.onnx"]
        .iter()
        .map(|n| folder.join(n))
        .find(|p| p.is_file())
    {
        return Some(ModelLayout::NemoCtc { tokens, model });
    }
    let file = |n: &str| Some(folder.join(n)).filter(|p| p.is_file());
    Some(ModelLayout::Moonshine {
        preprocessor: file("preprocess.onnx")?,
        encoder: file("encode.int8.onnx")?,
        uncached_decoder: file("uncached_decode.int8.onnx")?,
        cached_decoder: file("cached_decode.int8.onnx")?,
        tokens,
    })
}

// ---- the engine --------------------------------------------------------------------------------

struct Api {
    // Keeps the DLL loaded for as long as the function pointers are in use.
    _library: Library,
    create_recognizer: CreateRecognizer,
    destroy_recognizer: DestroyRecognizer,
    create_stream: CreateStream,
    destroy_stream: DestroyStream,
    accept_waveform: AcceptWaveform,
    decode: Decode,
    get_result: GetResult,
    destroy_result: DestroyResult,
}

pub struct SherpaTransducer {
    api: Api,
    recognizer: *const c_void,
}

// SAFETY: the recognizer is only used through `&mut self`, so one thread uses it at a time, and
// sherpa-onnx recognizers have no thread affinity.
#[allow(unsafe_code)]
unsafe impl Send for SherpaTransducer {}

impl SherpaTransducer {
    /// Load the DLL from `dll_folder` and the model from `layout`. `num_threads` is how many CPU
    /// threads decoding may use; `provider` is `"cpu"` (or `"cuda"` with the CUDA build).
    pub fn load(
        dll_folder: &Path,
        layout: &TransducerLayout,
        num_threads: i32,
        provider: &str,
    ) -> Result<SherpaTransducer, ModelError> {
        SherpaTransducer::load_model(
            dll_folder,
            &ModelLayout::Transducer(layout.clone()),
            num_threads,
            provider,
        )
    }

    /// [`SherpaTransducer::load`] for any [`ModelLayout`].
    #[allow(unsafe_code)]
    pub fn load_model(
        dll_folder: &Path,
        layout: &ModelLayout,
        num_threads: i32,
        provider: &str,
    ) -> Result<SherpaTransducer, ModelError> {
        let fail = |why: String| ModelError::Failed(why);
        let dll = dll_folder.join(DLL_NAME);
        // SAFETY: loading a DLL runs its initialisation; this is the sherpa-onnx DLL the user
        // placed next to the program. ALTERED_SEARCH_PATH makes Windows look for onnxruntime.dll
        // in the same folder rather than only in the program's.
        let library = unsafe {
            libloading::os::windows::Library::load_with_flags(&dll, 0x0000_0008)
                .map(Library::from)
                .map_err(|e| fail(format!("The speech engine ({}) could not be loaded: {e}", dll.display())))?
        };
        macro_rules! symbol {
            ($name:literal, $ty:ty) => {
                // SAFETY: the symbol name and type are those of sherpa-onnx's c-api.h.
                *unsafe { library.get::<$ty>(concat!($name, "\0").as_bytes()) }
                    .map_err(|e| fail(format!("The speech engine is missing {}: {e}", $name)))?
            };
        }
        let create_recognizer: CreateRecognizer = symbol!("SherpaOnnxCreateOfflineRecognizer", CreateRecognizer);
        let destroy_recognizer: DestroyRecognizer = symbol!("SherpaOnnxDestroyOfflineRecognizer", DestroyRecognizer);
        let create_stream: CreateStream = symbol!("SherpaOnnxCreateOfflineStream", CreateStream);
        let destroy_stream: DestroyStream = symbol!("SherpaOnnxDestroyOfflineStream", DestroyStream);
        let accept_waveform: AcceptWaveform = symbol!("SherpaOnnxAcceptWaveformOffline", AcceptWaveform);
        let decode: Decode = symbol!("SherpaOnnxDecodeOfflineStream", Decode);
        let get_result: GetResult = symbol!("SherpaOnnxGetOfflineStreamResult", GetResult);
        let destroy_result: DestroyResult = symbol!("SherpaOnnxDestroyOfflineRecognizerResult", DestroyResult);

        let text = |p: &Path| {
            CString::new(p.to_string_lossy().as_bytes())
                .map_err(|_| fail("A model path has an invalid character.".into()))
        };
        // The model's files, in the order the config below reads them; the strings must outlive the create call.
        let (tokens, files): (CString, Vec<CString>) = match layout {
            ModelLayout::Transducer(t) => (
                text(&t.tokens)?,
                vec![text(&t.encoder)?, text(&t.decoder)?, text(&t.joiner)?],
            ),
            ModelLayout::NemoCtc { tokens, model } => (text(tokens)?, vec![text(model)?]),
            ModelLayout::Moonshine {
                tokens,
                preprocessor,
                encoder,
                uncached_decoder,
                cached_decoder,
            } => (
                text(tokens)?,
                vec![
                    text(preprocessor)?,
                    text(encoder)?,
                    text(uncached_decoder)?,
                    text(cached_decoder)?,
                ],
            ),
        };
        let provider = CString::new(provider).map_err(|_| fail("Invalid provider name.".into()))?;
        let greedy = CString::new("greedy_search").expect("no NUL");

        // SAFETY: every field of these structs is an integer, a float or a pointer, for which
        // all-zero is the documented "not set" value.
        let mut config: RecognizerConfig = unsafe { std::mem::zeroed() };
        config.feat_config = FeatureConfig {
            sample_rate: 16_000,
            feature_dim: 80,
        };
        let f = |i: usize| files[i].as_ptr();
        match layout {
            ModelLayout::Transducer(_) => {
                config.model_config.transducer = TransducerConfig {
                    encoder: f(0),
                    decoder: f(1),
                    joiner: f(2),
                }
            }
            ModelLayout::NemoCtc { .. } => config.model_config.nemo_ctc = OneModel { model: f(0) },
            ModelLayout::Moonshine { .. } => {
                config.model_config.moonshine = MoonshineConfig {
                    preprocessor: f(0),
                    encoder: f(1),
                    uncached_decoder: f(2),
                    cached_decoder: f(3),
                    merged_decoder: ptr::null(),
                };
            }
        }
        config.model_config.tokens = tokens.as_ptr();
        config.model_config.num_threads = num_threads.max(1);
        config.model_config.provider = provider.as_ptr();
        config.decoding_method = greedy.as_ptr();
        config.max_active_paths = 4;

        // SAFETY: `config` is fully initialised and every string it points to outlives the call.
        let recognizer = unsafe { create_recognizer(&config) };
        if recognizer.is_null() {
            return Err(fail("The speech model could not be loaded (its files may be damaged or not a supported model).".into()));
        }
        Ok(SherpaTransducer {
            api: Api {
                _library: library,
                create_recognizer,
                destroy_recognizer,
                create_stream,
                destroy_stream,
                accept_waveform,
                decode,
                get_result,
                destroy_result,
            },
            recognizer,
        })
    }
}

impl SttEngine for SherpaTransducer {
    #[allow(unsafe_code)]
    fn transcribe(&mut self, audio: &AudioClip) -> Result<String, ModelError> {
        if audio.samples.is_empty() {
            return Ok(String::new());
        }
        let count = i32::try_from(audio.samples.len()).map_err(|_| ModelError::Failed("The recording is too long.".into()))?;
        let rate = i32::try_from(audio.sample_rate).map_err(|_| ModelError::Failed("Invalid sample rate.".into()))?;
        // SAFETY: the recognizer is alive (owned by self); the stream is created, used and
        // destroyed here; the sample buffer outlives the calls; the result is read before it is
        // destroyed and its text is copied out.
        unsafe {
            let stream = (self.api.create_stream)(self.recognizer);
            if stream.is_null() {
                return Err(ModelError::Failed("The speech engine could not start a recognition.".into()));
            }
            (self.api.accept_waveform)(stream, rate, audio.samples.as_ptr(), count);
            (self.api.decode)(self.recognizer, stream);
            let result = (self.api.get_result)(stream);
            let text = if result.is_null() || (*result).text.is_null() {
                String::new()
            } else {
                CStr::from_ptr((*result).text).to_string_lossy().trim().to_string()
            };
            if !result.is_null() {
                (self.api.destroy_result)(result);
            }
            (self.api.destroy_stream)(stream);
            Ok(text)
        }
    }
}

impl Drop for SherpaTransducer {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        if !self.recognizer.is_null() {
            // SAFETY: created by `create_recognizer`, destroyed once, before the DLL unloads
            // (the library field drops after this).
            unsafe { (self.api.destroy_recognizer)(self.recognizer) };
            self.recognizer = ptr::null();
        }
        let _ = self.api.create_recognizer; // the pointers die with the library
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oasis-sherpa-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(dir: &Path, names: &[&str]) {
        for n in names {
            fs::write(dir.join(n), "x").unwrap();
        }
    }

    #[test]
    fn a_transducer_folder_is_recognised_and_int8_is_preferred_on_a_cpu() {
        let dir = scratch("int8");
        touch(&dir, &["tokens.txt", "encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx", "encoder.fp16.onnx", "decoder.fp16.onnx", "joiner.fp16.onnx"]);
        let cpu = detect_transducer(&dir, false).unwrap();
        assert!(cpu.encoder.ends_with("encoder.int8.onnx"));
        assert!(detect_transducer(&dir, true).unwrap().encoder.ends_with("encoder.fp16.onnx"));
    }

    #[test]
    fn a_plain_build_is_used_when_there_is_no_other() {
        let dir = scratch("plain");
        touch(&dir, &["tokens.txt", "encoder.onnx", "decoder.onnx", "joiner.onnx"]);
        assert!(detect_transducer(&dir, false).unwrap().joiner.ends_with("joiner.onnx"));
    }

    #[test]
    fn an_incomplete_folder_is_not_a_model() {
        let dir = scratch("partial");
        touch(&dir, &["tokens.txt", "encoder.int8.onnx", "decoder.int8.onnx"]);
        assert_eq!(detect_transducer(&dir, false), None, "no joiner");
        let no_tokens = scratch("notokens");
        touch(&no_tokens, &["encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx"]);
        assert_eq!(detect_transducer(&no_tokens, false), None);
        assert_eq!(detect_transducer(Path::new(r"C:\no\such\folder"), false), None);
    }

    #[test]
    fn a_ctc_or_moonshine_folder_is_recognised_and_a_transducer_wins() {
        let ctc = scratch("ctc");
        touch(&ctc, &["tokens.txt", "model.int8.onnx"]);
        assert!(
            matches!(detect_model(&ctc, false), Some(ModelLayout::NemoCtc { model, .. }) if model.ends_with("model.int8.onnx"))
        );
        let moon = scratch("moon");
        touch(
            &moon,
            &[
                "tokens.txt",
                "preprocess.onnx",
                "encode.int8.onnx",
                "uncached_decode.int8.onnx",
                "cached_decode.int8.onnx",
            ],
        );
        assert!(matches!(
            detect_model(&moon, false),
            Some(ModelLayout::Moonshine { .. })
        ));
        touch(
            &moon,
            &["encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx"],
        );
        assert!(matches!(
            detect_model(&moon, false),
            Some(ModelLayout::Transducer(_))
        ));
        let half = scratch("halfmoon");
        touch(
            &half,
            &["tokens.txt", "preprocess.onnx", "encode.int8.onnx"],
        );
        assert_eq!(detect_model(&half, false), None);
    }

    #[test]
    fn the_dll_folder_needs_both_files() {
        let (both, one) = (scratch("both"), scratch("one"));
        touch(&both, &[DLL_NAME, ONNX_RUNTIME_DLL]);
        touch(&one, &[DLL_NAME]);
        assert_eq!(find_dll_folder(&[one.clone(), both.clone()]), Some(both));
        assert_eq!(find_dll_folder(&[one]), None);
    }

    #[test]
    fn the_search_order_is_override_then_beside_the_program() {
        let found = dll_search_folders(Some(Path::new(r"C:\app")), Some(Path::new(r"C:\mine")));
        assert_eq!(found, [PathBuf::from(r"C:\mine"), PathBuf::from(r"C:\app"), PathBuf::from(r"C:\app\native")]);
    }

    #[test]
    fn a_missing_dll_is_a_sentence_not_a_crash() {
        let dir = scratch("nodll");
        let layout = TransducerLayout { tokens: dir.join("t"), encoder: dir.join("e"), decoder: dir.join("d"), joiner: dir.join("j") };
        let error = SherpaTransducer::load(&dir, &layout, 2, "cpu").err().expect("fails");
        assert!(error.to_string().contains("could not be loaded"), "{error}");
    }

    /// With a real model and the DLLs (set OASIS_SHERPA_DIR and OASIS_STT_MODEL_DIR), the engine
    /// loads and decodes. Proves the struct layout above against the real DLL.
    #[test]
    fn a_real_model_loads_and_silence_gives_no_words() {
        let (Some(dlls), Some(model)) = (std::env::var_os("OASIS_SHERPA_DIR"), std::env::var_os("OASIS_STT_MODEL_DIR")) else {
            eprintln!("skipped: OASIS_SHERPA_DIR and OASIS_STT_MODEL_DIR are not set");
            return;
        };
        let layout = detect_transducer(Path::new(&model), false).expect("a model folder");
        let mut engine = SherpaTransducer::load(Path::new(&dlls), &layout, 2, "cpu").expect("the engine loads");
        let silence = AudioClip { samples: vec![0.0; 16_000], sample_rate: 16_000 };
        assert_eq!(engine.transcribe(&silence).unwrap(), "");
    }
}
