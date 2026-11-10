//! Test-only süreç çevresi kilidi.
//!
//! Birden fazla modül (`store::path`, `store::install`, `native_cli`,
//! `update`, …) `HOME` / `PATH` değiştirir. Ayrı `Mutex`’ler paralel
//! testlerde yarışır; tek paylaşılan kilit CI flake’ini önler.

#![cfg(test)]

use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// `HOME`/`PATH`/`CARGO_HOME` değiştiren testler bunu almalı.
///
/// Zehirlenmiş kilitte panik yerine içeriği alır (önceki flake sonrası
/// tüm suite’in PoisonError ile düşmesini engeller).
pub fn lock() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}
