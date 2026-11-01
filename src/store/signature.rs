//! Upstream imza doğrulama (mümkün olduğunda).
//!
//! SHA-256 zorunlu kapıdır. Katalogda [`crate::store::trust::SignatureSpec`]
//! varsa ve ilgili CLI PATH'teyse Nova imzayı da doğrular:
//! `minisign`, `cosign` (blob / attestation), `gpgv`.
//!
//! Araç yoksa kurulum **özetten sonra** devam eder; sonuç özetine atlama
//! notu eklenir. Araç varken doğrulama başarısızsa kurulum kesilir.

use crate::store::install::{Downloader, InstallError};
use crate::store::trust::{SignatureKind, SignatureSpec};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

/// İmza denetiminin sonucu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureCheck {
    /// Yerel araç imzayı doğruladı.
    Verified,
    /// Katalogda imza yok; atlandı.
    NotRequested,
    /// İmza isteniyor ama doğrulayıcı PATH'te yok (`when possible`).
    SkippedMissingTool(&'static str),
}

/// `kind` için doğrulayıcı binary PATH'te mi?
pub fn signature_tool_available(kind: SignatureKind) -> bool {
    which::which(kind.tool_name()).is_ok()
}

/// İndirilmiş artefakt için imza doğrular (mümkünse).
///
/// 1. `spec` yoksa [`SignatureCheck::NotRequested`].
/// 2. Araç yoksa [`SignatureCheck::SkippedMissingTool`].
/// 3. İmza dosyası indirilir, araç argv ile (kabuksuz) çalıştırılır.
pub fn verify_signature_when_possible(
    artifact: &Path,
    spec: Option<&SignatureSpec>,
    version: &str,
    work_dir: &Path,
    downloader: &dyn Downloader,
) -> Result<SignatureCheck, InstallError> {
    let Some(spec) = spec else {
        return Ok(SignatureCheck::NotRequested);
    };
    spec.validate()
        .map_err(|e| InstallError::Security(format!("imza üstverisi geçersiz: {e}")))?;

    if !signature_tool_available(spec.kind) {
        return Ok(SignatureCheck::SkippedMissingTool(spec.kind.tool_name()));
    }

    let sig_url = spec.url_for_version(version);
    if !sig_url.starts_with("https://") {
        return Err(InstallError::Security(format!(
            "imza adresi yalnızca https:// olabilir: '{sig_url}'"
        )));
    }

    let sig_path = work_dir.join("upstream.sig");
    downloader.download(&sig_url, &sig_path)?;

    match spec.kind {
        SignatureKind::Minisign => verify_minisign(artifact, &sig_path, spec)?,
        SignatureKind::Gpg => verify_gpg(artifact, &sig_path, spec, work_dir)?,
        SignatureKind::Cosign => verify_cosign_blob(artifact, &sig_path, spec)?,
        SignatureKind::Attestation => verify_attestation(artifact, &sig_path, spec)?,
    }

    Ok(SignatureCheck::Verified)
}

fn verify_minisign(artifact: &Path, sig: &Path, spec: &SignatureSpec) -> Result<(), InstallError> {
    let pubkey = spec
        .public_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| InstallError::Security("minisign public_key eksik".into()))?;
    run_verifier(
        "minisign",
        &[
            "-Vm".as_ref(),
            artifact.as_os_str(),
            "-x".as_ref(),
            sig.as_os_str(),
            "-P".as_ref(),
            pubkey.as_ref(),
        ],
    )
}

fn verify_gpg(
    artifact: &Path,
    sig: &Path,
    spec: &SignatureSpec,
    work_dir: &Path,
) -> Result<(), InstallError> {
    let armored = spec
        .public_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| InstallError::Security("gpg public_key eksik".into()))?;
    let keyring = work_dir.join("upstream-keyring.gpg");
    write_temp(&keyring, armored.as_bytes())?;
    run_verifier(
        "gpgv",
        &[
            "--keyring".as_ref(),
            keyring.as_os_str(),
            sig.as_os_str(),
            artifact.as_os_str(),
        ],
    )
}

fn verify_cosign_blob(
    artifact: &Path,
    sig: &Path,
    spec: &SignatureSpec,
) -> Result<(), InstallError> {
    let mut args: Vec<std::ffi::OsString> = vec![
        "verify-blob".into(),
        "--signature".into(),
        sig.as_os_str().to_owned(),
    ];
    if let Some(key) = spec
        .public_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        args.push("--key".into());
        args.push(key.into());
    } else {
        let identity = spec
            .identity
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| InstallError::Security("cosign identity eksik".into()))?;
        let issuer = spec
            .issuer
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| InstallError::Security("cosign issuer eksik".into()))?;
        args.push("--certificate-identity".into());
        args.push(identity.into());
        args.push("--certificate-oidc-issuer".into());
        args.push(issuer.into());
    }
    args.push(artifact.as_os_str().to_owned());
    let argv: Vec<&std::ffi::OsStr> = args.iter().map(AsRef::as_ref).collect();
    run_verifier("cosign", &argv)
}

fn verify_attestation(
    artifact: &Path,
    sig: &Path,
    spec: &SignatureSpec,
) -> Result<(), InstallError> {
    let identity = spec
        .identity
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| InstallError::Security("attestation identity eksik".into()))?;
    let issuer = spec
        .issuer
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| InstallError::Security("attestation issuer eksik".into()))?;
    run_verifier(
        "cosign",
        &[
            "verify-blob-attestation".as_ref(),
            "--signature".as_ref(),
            sig.as_os_str(),
            "--certificate-identity".as_ref(),
            identity.as_ref(),
            "--certificate-oidc-issuer".as_ref(),
            issuer.as_ref(),
            artifact.as_os_str(),
        ],
    )
}

fn run_verifier(program: &str, args: &[&std::ffi::OsStr]) -> Result<(), InstallError> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| InstallError::Security(format!("'{program}' çalıştırılamadı: {e}")))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else {
            stdout.trim().to_string()
        };
        Err(InstallError::Security(format!(
            "upstream imza doğrulanamadı ({program}): {detail}"
        )))
    }
}

fn write_temp(path: &Path, bytes: &[u8]) -> Result<(), InstallError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            InstallError::Permission(format!("'{}' oluşturulamadı: {e}", parent.display()))
        })?;
    }
    let mut file = fs::File::create(path)
        .map_err(|e| InstallError::Permission(format!("'{}' yazılamadı: {e}", path.display())))?;
    file.write_all(bytes)
        .map_err(|e| InstallError::Permission(format!("'{}' yazılamadı: {e}", path.display())))?;
    Ok(())
}

/// Testlerde sahte doğrulayıcı.
#[cfg(test)]
pub mod test_support {
    use super::*;

    /// Başarılı doğrulama simülasyonu (ağ/CLI yok).
    pub fn pretend_verified() -> SignatureCheck {
        SignatureCheck::Verified
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::trust::SignatureKind;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    type FileMap = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

    struct MemDownloader {
        files: FileMap,
    }

    impl Downloader for MemDownloader {
        fn download(&self, url: &str, destination: &Path) -> Result<PathBuf, InstallError> {
            let files = self.files.lock().unwrap();
            let bytes = files
                .iter()
                .find(|(u, _)| u == url)
                .map(|(_, b)| b.clone())
                .ok_or_else(|| InstallError::Network(format!("yok: {url}")))?;
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(destination, bytes).unwrap();
            Ok(destination.to_path_buf())
        }
    }

    #[test]
    fn missing_spec_is_not_requested() {
        let dir = std::env::temp_dir().join("nova-sig-none");
        let _ = fs::create_dir_all(&dir);
        let artifact = dir.join("a.bin");
        fs::write(&artifact, b"x").unwrap();
        let dl = MemDownloader {
            files: Arc::new(Mutex::new(vec![])),
        };
        let check = verify_signature_when_possible(&artifact, None, "1.0.0", &dir, &dl).unwrap();
        assert_eq!(check, SignatureCheck::NotRequested);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_tool_skips_when_possible() {
        let dir = std::env::temp_dir().join("nova-sig-skip");
        let _ = fs::create_dir_all(&dir);
        let artifact = dir.join("a.bin");
        fs::write(&artifact, b"x").unwrap();
        // minisign bu ortamda yok sayılır; varsa da skip dışı yola girer —
        // bu test yalnızca "araç yok" dalını kilitler.
        if which::which("minisign").is_ok() {
            return;
        }
        let spec = SignatureSpec {
            kind: SignatureKind::Minisign,
            signature_url: "https://example.org/a.minisig".into(),
            public_key: Some("RWQtestkey".into()),
            key_id: None,
            identity: None,
            issuer: None,
        };
        let dl = MemDownloader {
            files: Arc::new(Mutex::new(vec![])),
        };
        let check =
            verify_signature_when_possible(&artifact, Some(&spec), "1.0.0", &dir, &dl).unwrap();
        assert_eq!(check, SignatureCheck::SkippedMissingTool("minisign"));
        let _ = fs::remove_dir_all(&dir);
    }
}
