//! Host CPU mimarisi ve mağaza platform etiketleri.
//!
//! Katalog `supported_platforms` ve `[[apps.artifacts]]` alanlarında
//! `linux-x64` / `linux-aarch64` (veya `x86_64` / `aarch64`) kullanır.
//! Kurulum ve canlı sürüm çözümü host mimarisine göre seçilir.

use serde::{Deserialize, Serialize};

/// Desteklenen CPU mimarileri (Linux mağaza hedefleri).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CpuArch {
    /// Intel/AMD 64-bit.
    X86_64,
    /// ARM 64-bit.
    Aarch64,
}

impl CpuArch {
    /// Katalog / manifest’te kullanılan kısa ad (`x86_64` / `aarch64`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
        }
    }

    /// `supported_platforms` için varsayılan Linux etiketi.
    pub fn platform_tag(self) -> &'static str {
        match self {
            Self::X86_64 => "linux-x64",
            Self::Aarch64 => "linux-aarch64",
        }
    }

    /// Esnek ayrıştırma (`amd64`, `arm64`, `linux-x64`, …).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "x86_64" | "amd64" | "x64" | "linux-x64" | "linux-amd64" | "linux_x86_64" => {
                Some(Self::X86_64)
            }
            "aarch64" | "arm64" | "linux-aarch64" | "linux-arm64" | "linux_arm64" => {
                Some(Self::Aarch64)
            }
            _ => None,
        }
    }

    /// Çalışan sürecin mimarisi (`std::env::consts::ARCH`).
    pub fn host() -> Option<Self> {
        Self::parse(std::env::consts::ARCH)
    }
}

impl std::fmt::Display for CpuArch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Platform etiketinin bu mimariye uyup uymadığı.
pub fn platform_matches(platform: &str, arch: CpuArch) -> bool {
    CpuArch::parse(platform) == Some(arch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_aliases() {
        assert_eq!(CpuArch::parse("linux-x64"), Some(CpuArch::X86_64));
        assert_eq!(CpuArch::parse("amd64"), Some(CpuArch::X86_64));
        assert_eq!(CpuArch::parse("linux-aarch64"), Some(CpuArch::Aarch64));
        assert_eq!(CpuArch::parse("arm64"), Some(CpuArch::Aarch64));
        assert!(CpuArch::parse("riscv64").is_none());
    }

    #[test]
    fn host_is_known_on_ci_linux() {
        let host = CpuArch::host();
        assert!(
            host == Some(CpuArch::X86_64) || host == Some(CpuArch::Aarch64),
            "beklenmeyen host mimarisi: {host:?} ({})",
            std::env::consts::ARCH
        );
    }
}
