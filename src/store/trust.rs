//! Mağaza güven seviyeleri ve upstream imza üstverisi.
//!
//! UI rozetleri üç seviyeyi gösterir:
//! - `hash+sig` — SHA-256 + upstream imza (minisign/cosign/GPG/attestation)
//! - `hash` — yalnızca gömülü / upstream SHA-256
//! - `manual` — Nova indirme doğrulamaz (`manual` veya paket yöneticisi)

use serde::{Deserialize, Serialize};

use crate::store::{InstallMethod, StoreEntry};

/// Kullanıcıya gösterilen güven rozeti.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustLevel {
    /// Arşiv: SHA-256 + katalogda tanımlı upstream imza.
    HashAndSignature,
    /// Arşiv: yalnızca SHA-256.
    HashOnly,
    /// Manuel talimat veya host paket yöneticisi — Nova özet doğrulamaz.
    Manual,
}

impl TrustLevel {
    /// Kısa rozet metni (`hash+sig` / `hash` / `manual`).
    pub fn badge(self) -> &'static str {
        match self {
            Self::HashAndSignature => "hash+sig",
            Self::HashOnly => "hash",
            Self::Manual => "manual",
        }
    }

    /// Detay panelinde gösterilecek açıklama.
    pub fn label(self) -> &'static str {
        match self {
            Self::HashAndSignature => "SHA-256 + upstream imza",
            Self::HashOnly => "yalnızca SHA-256",
            Self::Manual => "Nova indirme doğrulamaz",
        }
    }
}

/// Upstream imza türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureKind {
    /// `minisign` CLI.
    Minisign,
    /// `cosign verify-blob`.
    Cosign,
    /// `gpgv` / OpenPGP ayrılmış imza.
    Gpg,
    /// `cosign verify-blob-attestation` (SLSA / in-toto).
    Attestation,
}

impl SignatureKind {
    /// Yerel doğrulayıcı program adı (PATH'te aranır).
    pub fn tool_name(self) -> &'static str {
        match self {
            Self::Minisign => "minisign",
            Self::Cosign | Self::Attestation => "cosign",
            Self::Gpg => "gpgv",
        }
    }

    /// Kısa etiket.
    pub fn label(self) -> &'static str {
        match self {
            Self::Minisign => "minisign",
            Self::Cosign => "cosign",
            Self::Gpg => "gpg",
            Self::Attestation => "attestation",
        }
    }
}

/// Katalogda tanımlı upstream imza kaynağı.
///
/// İndirme hâlâ SHA-256 ile zorunlu doğrulanır; imza **mümkünse** ek
/// katmandır. `signature_url` içinde `{version}` yer tutucusu olabilir.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignatureSpec {
    /// İmza algoritması / araç ailesi.
    pub kind: SignatureKind,
    /// İmza (veya attestation) dosyasının `https://` adresi.
    pub signature_url: String,
    /// minisign genel anahtarı veya armored public key (gömülü).
    #[serde(default)]
    pub public_key: Option<String>,
    /// GPG anahtar kimliği / parmak izi (bilgi amaçlı).
    #[serde(default)]
    pub key_id: Option<String>,
    /// cosign / attestation sertifika kimliği (ör. GitHub Actions).
    #[serde(default)]
    pub identity: Option<String>,
    /// cosign OIDC issuer (ör. `https://token.actions.githubusercontent.com`).
    #[serde(default)]
    pub issuer: Option<String>,
}

impl SignatureSpec {
    /// Sürüm yer tutucusunu doldurur.
    pub fn url_for_version(&self, version: &str) -> String {
        self.signature_url.replace("{version}", version)
    }

    /// Katalog doğrulaması: HTTPS adres ve türe göre zorunlu alanlar.
    pub fn validate(&self) -> Result<(), String> {
        let url = self.signature_url.trim();
        if url.is_empty() {
            return Err("signature_url boş olamaz".into());
        }
        if !url.starts_with("https://") {
            return Err(format!("signature_url yalnızca https:// olabilir: '{url}'"));
        }
        match self.kind {
            SignatureKind::Minisign => {
                let key = self.public_key.as_deref().unwrap_or("").trim();
                if key.is_empty() {
                    return Err("minisign için public_key zorunludur".into());
                }
            }
            SignatureKind::Gpg => {
                let key = self.public_key.as_deref().unwrap_or("").trim();
                if key.is_empty() {
                    return Err("gpg için public_key (armored keyring) zorunludur".into());
                }
            }
            SignatureKind::Cosign | SignatureKind::Attestation => {
                let identity = self.identity.as_deref().unwrap_or("").trim();
                let issuer = self.issuer.as_deref().unwrap_or("").trim();
                let key = self.public_key.as_deref().unwrap_or("").trim();
                if key.is_empty() && (identity.is_empty() || issuer.is_empty()) {
                    return Err(
                        "cosign/attestation için public_key veya identity+issuer zorunludur".into(),
                    );
                }
            }
        }
        Ok(())
    }
}

impl StoreEntry {
    /// Bu kayıt için UI güven rozeti.
    pub fn trust_level(&self) -> TrustLevel {
        match self.install_method {
            InstallMethod::DownloadArchive if self.signature.is_some() => {
                TrustLevel::HashAndSignature
            }
            InstallMethod::DownloadArchive => TrustLevel::HashOnly,
            _ => TrustLevel::Manual,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InstallMethod;

    fn archive_entry(with_sig: bool) -> StoreEntry {
        StoreEntry {
            id: "a".into(),
            name: "a".into(),
            description: "d".into(),
            category: "Günlük".into(),
            source: "https://example.org/".into(),
            license: "MIT".into(),
            executable: "a".into(),
            install_method: InstallMethod::DownloadArchive,
            supported_platforms: vec!["linux-x64".into()],
            checksum: Some("a".repeat(64)),
            checksum_type: Some("sha256".into()),
            version: Some("1.0.0".into()),
            download_url: Some("https://example.org/a.tar.gz".into()),
            install_command: None,
            install_notes: None,
            release: None,
            signature: with_sig.then(|| SignatureSpec {
                kind: SignatureKind::Minisign,
                signature_url: "https://example.org/a.tar.gz.minisig".into(),
                public_key: Some("RWQaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
                key_id: None,
                identity: None,
                issuer: None,
            }),
        }
    }

    #[test]
    fn trust_badges_cover_three_levels() {
        let hash_sig = archive_entry(true);
        assert_eq!(hash_sig.trust_level(), TrustLevel::HashAndSignature);
        assert_eq!(hash_sig.trust_level().badge(), "hash+sig");

        let hash_only = archive_entry(false);
        assert_eq!(hash_only.trust_level(), TrustLevel::HashOnly);
        assert_eq!(hash_only.trust_level().badge(), "hash");

        let mut manual = archive_entry(false);
        manual.install_method = InstallMethod::Manual;
        manual.download_url = None;
        manual.checksum = None;
        manual.signature = None;
        assert_eq!(manual.trust_level(), TrustLevel::Manual);
        assert_eq!(manual.trust_level().badge(), "manual");

        let mut pm = archive_entry(false);
        pm.install_method = InstallMethod::Pacman;
        pm.download_url = None;
        pm.checksum = None;
        pm.signature = None;
        pm.install_command = Some(vec![
            "sudo".into(),
            "pacman".into(),
            "-S".into(),
            "--noconfirm".into(),
            "a".into(),
        ]);
        assert_eq!(pm.trust_level(), TrustLevel::Manual);
    }

    #[test]
    fn signature_spec_requires_https_and_kind_fields() {
        let mut spec = SignatureSpec {
            kind: SignatureKind::Minisign,
            signature_url: "http://bad".into(),
            public_key: Some("RWQ…".into()),
            key_id: None,
            identity: None,
            issuer: None,
        };
        assert!(spec.validate().unwrap_err().contains("https"));
        spec.signature_url = "https://example.org/a.minisig".into();
        spec.public_key = None;
        assert!(spec.validate().unwrap_err().contains("public_key"));
        spec.public_key = Some("RWQkey".into());
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn signature_url_substitutes_version() {
        let spec = SignatureSpec {
            kind: SignatureKind::Cosign,
            signature_url: "https://example.org/a-{version}.sig".into(),
            public_key: Some("x".into()),
            key_id: None,
            identity: None,
            issuer: None,
        };
        assert_eq!(
            spec.url_for_version("1.2.3"),
            "https://example.org/a-1.2.3.sig"
        );
    }
}
