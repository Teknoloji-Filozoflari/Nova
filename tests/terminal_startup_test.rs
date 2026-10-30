use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn non_tty_startup_failure_does_not_write_terminal_control_sequences() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX epoch")
        .as_nanos();
    let isolated_home =
        std::env::temp_dir().join(format!("nova-non-tty-test-{}-{nonce}", std::process::id()));
    let isolated_xdg = isolated_home.join("xdg-config");

    let output = Command::new(env!("CARGO_BIN_EXE_nova"))
        .env("HOME", &isolated_home)
        .env("XDG_CONFIG_HOME", &isolated_xdg)
        .stdin(Stdio::null())
        .output()
        .expect("Nova binary should start for the non-TTY check");

    assert!(
        !output.status.success(),
        "Nova must reject a non-TTY session"
    );
    assert!(
        output.stdout.is_empty(),
        "startup failure wrote terminal controls to stdout: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("terminal ham moda alınamadı"),
        "startup failure should explain the missing terminal: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = std::fs::remove_dir_all(isolated_home);
}

#[test]
fn broken_xdg_config_fails_with_the_actual_path_before_terminal_setup() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX epoch")
        .as_nanos();
    let isolated_home = std::env::temp_dir().join(format!(
        "nova-broken-config-test-{}-{nonce}",
        std::process::id()
    ));
    let isolated_xdg = isolated_home.join("xdg-config");
    let config_path = isolated_xdg.join("nova").join("config.toml");
    std::fs::create_dir_all(config_path.parent().expect("config has parent"))
        .expect("isolated config directory should be created");
    std::fs::write(&config_path, "[[tools]\nid = [\n")
        .expect("broken config fixture should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_nova"))
        .env("HOME", &isolated_home)
        .env("XDG_CONFIG_HOME", &isolated_xdg)
        .stdin(Stdio::null())
        .output()
        .expect("Nova binary should start for the invalid-config check");

    assert!(!output.status.success(), "broken config must stop startup");
    assert!(
        output.stdout.is_empty(),
        "config error wrote terminal controls to stdout: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&config_path.display().to_string()));
    assert!(stderr.contains("geçerli TOML değil"));
    assert!(
        stderr.contains("Araç kataloğu yüklenemedi"),
        "startup should explain the invalid catalog: {stderr}"
    );

    let _ = std::fs::remove_dir_all(isolated_home);
}
