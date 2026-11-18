// Written by scripts/publish-models.ps1; do not edit by hand. The files of release models-2026.11 in dhanushscience/OASIS,
// with the SHA-256 each download must match (docs/VOICE_MODEL_SETUP.md section 4.3).

/// The release the files below come from.
pub const RELEASE: &str = "models-2026.11";
/// Where they are downloaded from: this, then the asset name.
pub const BASE_URL: &str = "https://github.com/dhanushscience/OASIS/releases/download/models-2026.11/";

/// Every file, in the order they are downloaded.
pub const FILES: &[ModelFile] = &[
    ModelFile { asset: "parakeet-tdt-0.6b-v2.encoder.int8.onnx", path: "stt/parakeet-tdt-0.6b-v2/encoder.int8.onnx", bytes: 652184296, sha256: "a32b12d17bbbc309d0686fbbcc2987b5e9b8333a7da83fa6b089f0a2acd651ab", group: Group::SpeechBalanced, unzip: false },
    ModelFile { asset: "parakeet-tdt-0.6b-v2.decoder.int8.onnx", path: "stt/parakeet-tdt-0.6b-v2/decoder.int8.onnx", bytes: 7257753, sha256: "b6bb64963457237b900e496ee9994b59294526439fbcc1fecf705b31a15c6b4e", group: Group::SpeechBalanced, unzip: false },
    ModelFile { asset: "parakeet-tdt-0.6b-v2.joiner.int8.onnx", path: "stt/parakeet-tdt-0.6b-v2/joiner.int8.onnx", bytes: 1739080, sha256: "7946164367946e7f9f29a122407c3252b680dbae9a51343eb2488d057c3c43d2", group: Group::SpeechBalanced, unzip: false },
    ModelFile { asset: "parakeet-tdt-0.6b-v2.tokens.txt", path: "stt/parakeet-tdt-0.6b-v2/tokens.txt", bytes: 9384, sha256: "ec182b70dd42113aff6c5372c75cac58c952443eb22322f57bbd7f53977d497d", group: Group::SpeechBalanced, unzip: false },
    ModelFile { asset: "parakeet-tdt-0.6b-v2.NOTICE.txt", path: "stt/parakeet-tdt-0.6b-v2/NOTICE.txt", bytes: 437, sha256: "6f105a0ff40facae52c6fe85a11b364fcc62f114b69a146b95c9005230e1c29c", group: Group::SpeechBalanced, unzip: false },
    ModelFile { asset: "parakeet-tdt-ctc-110m.model.int8.onnx", path: "stt/parakeet-tdt-ctc-110m/model.int8.onnx", bytes: 131652171, sha256: "9177a9146cf32ee0cc8152276ef95116f312018d316be37ccf57f7efea81fc1a", group: Group::SpeechLight, unzip: false },
    ModelFile { asset: "parakeet-tdt-ctc-110m.tokens.txt", path: "stt/parakeet-tdt-ctc-110m/tokens.txt", bytes: 9953, sha256: "450e56bd2f036fe5b6aa821865838cc5aa9d8b0106134ce9a9ba0664abe6cd10", group: Group::SpeechLight, unzip: false },
    ModelFile { asset: "parakeet-tdt-ctc-110m.NOTICE.txt", path: "stt/parakeet-tdt-ctc-110m/NOTICE.txt", bytes: 437, sha256: "6f105a0ff40facae52c6fe85a11b364fcc62f114b69a146b95c9005230e1c29c", group: Group::SpeechLight, unzip: false },
    ModelFile { asset: "s1-mini-oasis.q4_k_m.gguf", path: "llm/s1-mini/s1-mini-q4_k_m.gguf", bytes: 484219808, sha256: "12e5fd1509309a6cc4126752631dcefbfbb68f960eebd6d4d0d933d20a93b1c2", group: Group::Polish, unzip: false },
    ModelFile { asset: "s1-mini.LICENSE", path: "llm/s1-mini/LICENSE", bytes: 12033, sha256: "f715982df6ce767ae64864d74b95644e5d658aab54717dbceeb252eb3cbcb421", group: Group::Polish, unzip: false },
    ModelFile { asset: "s1-mini-oasis.NOTICE", path: "llm/s1-mini/NOTICE", bytes: 1177, sha256: "7cd37d9117670a97aaddb5e5cc006b79e2a637fe3570c9cdd57606d11f2d3e99", group: Group::Polish, unzip: false },
    ModelFile { asset: "llama-vulkan-b12304.zip", path: "runtime/llama-vulkan", bytes: 38045220, sha256: "d8b990953aca480412581521045a52f7263db84c5f2cbb89ada9aa47413ce6b1", group: Group::Polish, unzip: true },
    ModelFile { asset: "llama.cpp.LICENSE.txt", path: "runtime/llama-vulkan/LICENSE-llama.cpp.txt", bytes: 1078, sha256: "94f29bbed6a22c35b992c5c6ebf0e7c92f13b836b90f36f461c9cf2f0f1d010d", group: Group::Polish, unzip: false },
];
