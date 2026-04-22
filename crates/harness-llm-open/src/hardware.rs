// IMPLEMENTS: D-441
use crate::OpenBackend;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareProfile {
    pub os: &'static str,
    pub arch: &'static str,
    pub apple_silicon: bool,
    pub cuda_likely: bool,
}

#[must_use]
pub fn detect_hardware() -> HardwareProfile {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let apple_silicon = os == "macos" && arch == "aarch64";
    // We can't probe CUDA without spawning a process; treat any non-mac
    // unix box on x86_64/aarch64 as a candidate.
    let cuda_likely = matches!(os, "linux" | "windows") && matches!(arch, "x86_64" | "aarch64");
    HardwareProfile {
        os,
        arch,
        apple_silicon,
        cuda_likely,
    }
}

/// Pick a sensible local backend given the hardware. Mirrors the
/// recommendation table in D-441's research note (R57) — Apple Silicon
/// → MLX, CUDA-likely → vLLM, otherwise → Ollama as the safest default.
#[must_use]
pub fn recommended_backend(profile: &HardwareProfile) -> OpenBackend {
    if profile.apple_silicon {
        OpenBackend::Mlx
    } else if profile.cuda_likely {
        OpenBackend::Vllm
    } else {
        OpenBackend::Ollama
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_reports_consts() {
        let p = detect_hardware();
        assert_eq!(p.os, std::env::consts::OS);
        assert_eq!(p.arch, std::env::consts::ARCH);
    }

    #[test]
    fn recommendation_matches_apple_silicon() {
        let p = HardwareProfile {
            os: "macos",
            arch: "aarch64",
            apple_silicon: true,
            cuda_likely: false,
        };
        assert_eq!(recommended_backend(&p), OpenBackend::Mlx);
    }

    #[test]
    fn recommendation_matches_cuda_box() {
        let p = HardwareProfile {
            os: "linux",
            arch: "x86_64",
            apple_silicon: false,
            cuda_likely: true,
        };
        assert_eq!(recommended_backend(&p), OpenBackend::Vllm);
    }

    #[test]
    fn fallback_is_ollama() {
        let p = HardwareProfile {
            os: "freebsd",
            arch: "riscv64",
            apple_silicon: false,
            cuda_likely: false,
        };
        assert_eq!(recommended_backend(&p), OpenBackend::Ollama);
    }
}
