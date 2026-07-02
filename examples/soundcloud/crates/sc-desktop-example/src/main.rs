//! How SoundCloud itself runs the Desktop-Runtime engine: a thin binary
//! that reuses `rn-linux::run` as an ordinary library consumer, plugging in
//! `sc-desktop-ops`'s host functions and a real `sc-rn` init through
//! `RunConfig::before_bundle_eval` — the engine itself never mentions
//! `sc-rn`/`@sc/ui` anywhere. See Desktop-Runtime/CLAUDE.md, "Спайк 8".

use std::path::PathBuf;

fn main() {
    let bundle_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("RN_LINUX_BUNDLE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../js/dist/bundle.js")));

    rn_linux::run(rn_linux::RunConfig {
        bundle_path,
        window_title: "SoundCloud demo — Desktop-Runtime".to_string(),
        before_bundle_eval: Some(Box::new(|rt| {
            sc_desktop_ops::install(rt).map_err(|e| e.to_string())?;

            // sc-rn needs a data/cache dir before any JS calls into it
            // (examples/soundcloud/js/src/live-data.ts's `initCore`) —
            // resolving *which* paths is the shell's job (see Core/shared/
            // crates/sc-rn/src/runtime.rs), not JS's.
            let data_dir = std::env::temp_dir().join("sc-desktop-runtime/data");
            let cache_dir = std::env::temp_dir().join("sc-desktop-runtime/cache");
            std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;
            let init_err = rt
                .eval(&format!(
                    "__scInitCore({:?}, {:?}, false)",
                    data_dir.to_str().expect("data dir should be valid utf8"),
                    cache_dir.to_str().expect("cache dir should be valid utf8"),
                ))
                .map_err(|e| e.to_string())?
                .into_string()
                .expect("init_core returns a string")
                .to_rust_string()
                .expect("valid utf8");
            if !init_err.is_empty() {
                return Err(format!("sc-rn init_runtime failed: {init_err}"));
            }
            Ok(())
        })),
        ..Default::default()
    });
}
