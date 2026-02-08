use criterion::{Criterion, criterion_group, criterion_main};
use exoclaw::sandbox::PluginHost;
use std::path::PathBuf;

fn echo_wasm_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/echo-plugin/target/wasm32-unknown-unknown/release/echo_plugin.wasm")
}

fn bench_sandbox_call(c: &mut Criterion) {
    let wasm_path = echo_wasm_path();
    if !wasm_path.exists() {
        eprintln!(
            "skipping sandbox benchmark (missing wasm): {}",
            wasm_path.display()
        );
        return;
    }

    let mut host = PluginHost::new();
    if let Err(e) = host.register("echo", wasm_path.to_str().unwrap_or_default(), vec![]) {
        eprintln!("skipping sandbox benchmark (register failed): {e}");
        return;
    }

    let input = serde_json::json!({ "message": "benchmark" });
    c.bench_function("sandbox_call_fresh_instance", |b| {
        b.iter(|| {
            let result = host.call_tool("echo", &input);
            criterion::black_box(result.content);
            criterion::black_box(result.is_error);
        });
    });
}

criterion_group!(benches, bench_sandbox_call);
criterion_main!(benches);
