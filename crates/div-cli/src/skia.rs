//! The Skia build profile apps must use to render like the qualified engine.
//!
//! Div's pixels depend on how Skia is compiled. CI builds it from source
//! with these variables (`.github/workflows/ci.yml`); a test below fails if
//! the two drift apart. Apps get them through `.cargo/config.toml`, so plain
//! `cargo build` produces the same Skia as `div dev` and `div build`.

macro_rules! gn_args {
    ($extra:literal) => {
        concat!(
            r#"extra_cflags += [ "-DSK_SUPPORT_LEGACY_CONIC_CHOP", "#,
            r#""-DSK_ENABLE_LEGACY_SHADERCONTEXT", "-USK_ENABLE_AVX512_OPTS", "#,
            r#""-DSK_USE_FREETYPE_EMBOLDEN", "-DSK_GAMMA_EXPONENT=1.2", "#,
            r#""-DSK_GAMMA_CONTRAST=0.2", "-USK_GAMMA_APPLY_TO_A8", "-fno-strict-aliasing", "#,
            r#""-msse3""#,
            $extra,
            r#" ]"#,
        )
    };
}

/// Compile-time Skia options, as in CI's `SKIA_GN_ARGS` on Linux.
///
/// The last flag, `-O2`, is the optimization level Chromium compiles Skia
/// with. Off Windows, rust-skia passes Cargo's `OPT_LEVEL` to the compiler as
/// `-O<level>` after Skia's own `-O3`, so a dev build would compile Skia at
/// `-O0`; this later flag wins in every profile.
pub const GN_ARGS: &str = gn_args!(r#", "-O2""#);

/// Windows' Skia options: the same options without `-O2`, plus Chromium's
/// `/clang:-ffp-contract=off` (`build/config/compiler/BUILD.gn` at
/// 147.0.7727.50).
///
/// rust-skia passes no optimization level on Windows, where Skia's release
/// configuration already compiles with `/O2`. rust-skia compiles Skia with
/// clang-cl, which otherwise fuses multiply-adds in Skia's AVX2 code; Skia's
/// own GN turns contraction off for clang on other platforms only. clang on
/// Linux would read the `/clang:` flag as a file name, so it is Windows' alone.
pub const WINDOWS_GN_ARGS: &str = gn_args!(r#", "/clang:-ffp-contract=off""#);

/// Variables every platform sets.
pub const ENV: &[(&str, &str)] = &[
    ("FORCE_SKIA_BUILD", "1"),
    ("SKIA_NINJA_COMMAND", "ninja"),
    // Deep layouts need more than libtest's and std's 2 MiB thread stacks.
    ("RUST_MIN_STACK", "4194304"),
];

/// Linux CI compiles Skia with clang rather than the system C compiler.
pub const LINUX_ENV: &[(&str, &str)] = &[
    ("SKIA_GN_ARGS", GN_ARGS),
    ("CC", "clang"),
    ("CXX", "clang++"),
    ("CLANGCC", "clang"),
    ("CLANGCXX", "clang++"),
];

/// Windows CI's Skia options.
pub const WINDOWS_ENV: &[(&str, &str)] = &[("SKIA_GN_ARGS", WINDOWS_GN_ARGS)];

/// Other platforms (macOS): the Linux options, without Linux's compilers.
const OTHER_ENV: &[(&str, &str)] = &[("SKIA_GN_ARGS", GN_ARGS)];

/// `.cargo/config.toml` for an app created on `os` (`std::env::consts::OS`).
pub fn cargo_config(os: &str) -> String {
    let mut config = String::from(
        "# Written by `div new`. Div's pixels depend on how Skia is\n\
         # compiled; these match the framework's CI. Variables already set in\n\
         # the environment take precedence.\n\
         [env]\n",
    );
    let platform = match os {
        "linux" => LINUX_ENV,
        "windows" => WINDOWS_ENV,
        _ => OTHER_ENV,
    };
    for (name, value) in ENV.iter().chain(platform) {
        // TOML literal strings keep the quotes in GN_ARGS as they are.
        debug_assert!(!value.contains('\''));
        config.push_str(&format!("{name} = '{value}'\n"));
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    const CI: &str = include_str!("../../../.github/workflows/ci.yml");

    /// The `env:` block of a CI job, as `(name, value)` pairs, with folded
    /// (`>-`) values joined by spaces as YAML does.
    fn job_env(job: &str) -> Vec<(String, String)> {
        let start = CI
            .find(&format!("\n  {job}:\n"))
            .unwrap_or_else(|| panic!("CI job {job} not found"));
        let lines: Vec<&str> = CI[start..].lines().skip(2).collect();
        let env_at = lines
            .iter()
            .position(|line| *line == "    env:")
            .expect("job has an env block");
        let mut env = Vec::new();
        let mut index = env_at + 1;
        while let Some(line) = lines.get(index) {
            if !line.starts_with("      ") {
                break;
            }
            index += 1;
            let entry = line.trim();
            if entry.starts_with('#') {
                continue;
            }
            let (name, value) = entry.split_once(':').expect("env entry");
            let value = value.trim();
            let value = if value == ">-" {
                let mut folded = Vec::new();
                while let Some(next) = lines.get(index) {
                    if !next.starts_with("        ") {
                        break;
                    }
                    folded.push(next.trim());
                    index += 1;
                }
                folded.join(" ")
            } else {
                value.trim_matches('"').to_owned()
            };
            env.push((name.to_owned(), value));
        }
        env
    }

    fn assert_job_sets(job: &str, expected: &[(&str, &str)]) {
        let env = job_env(job);
        for (name, value) in expected {
            let actual = env
                .iter()
                .find(|(key, _)| key == name)
                .unwrap_or_else(|| panic!("CI job {job} does not set {name}"));
            assert_eq!(actual.1, *value, "{name} differs from CI job {job}");
        }
    }

    #[test]
    fn profile_matches_both_ci_platforms() {
        assert_job_sets("rust-parity", ENV);
        assert_job_sets("rust-parity", LINUX_ENV);
        assert_job_sets("windows-msvc", ENV);
        assert_job_sets("windows-msvc", WINDOWS_ENV);
    }

    #[test]
    fn platforms_differ_only_in_their_last_flag() {
        let shared = GN_ARGS
            .strip_suffix(r#", "-O2" ]"#)
            .expect("the Linux options end with -O2");
        assert!(shared.ends_with(r#""-fno-strict-aliasing", "-msse3""#));
        assert_eq!(
            WINDOWS_GN_ARGS,
            format!(r#"{shared}, "/clang:-ffp-contract=off" ]"#)
        );
    }

    #[test]
    fn cargo_config_sets_the_profile_as_literal_strings() {
        let windows = cargo_config("windows");
        assert!(windows.starts_with("# Written by `div new`"));
        assert!(windows.contains(&format!("SKIA_GN_ARGS = '{WINDOWS_GN_ARGS}'\n")));
        assert!(!windows.contains("CC = "));
        let linux = cargo_config("linux");
        assert!(linux.contains(&format!("SKIA_GN_ARGS = '{GN_ARGS}'\n")));
        assert!(linux.contains("CC = 'clang'\nCXX = 'clang++'\n"));
        let macos = cargo_config("macos");
        assert!(macos.contains(&format!("SKIA_GN_ARGS = '{GN_ARGS}'\n")));
        assert!(!macos.contains("CC = "));
    }
}
