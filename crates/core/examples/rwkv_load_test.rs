//! Stage-by-stage RWKV model loading validation.
//!
//! Runs with explicit timeouts so we can see exactly which stage hangs.
//! Each stage has a 30s timeout — if it exceeds, we know where to look.
//!
//! ```bash
//! cargo run -p roco-core --features local-rwkv --example rwkv_load_test
//! ```

use std::collections::HashMap;
use std::time::Instant;

use half::f16;
use memmap2::Mmap;
use safetensors::SafeTensors;
use tokio::time::timeout;
use tracing::info;
use wgpu::PowerPreference;
use web_rwkv::context::{ContextBuilder, InstanceExt};
use web_rwkv::runtime::loader::Loader;
use web_rwkv::runtime::model::{ContextAutoLimits, ModelBuilder, Quant};
use web_rwkv::runtime::v7;

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn ts() -> String {
    format!("{:.1}s", now().as_secs_f64())
}
fn now() -> std::time::Duration {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
}

fn main() {
    eprintln!("[rwkv_load_test] starting main()");
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    eprintln!("[rwkv_load_test] tracing initialized");

    let t0 = Instant::now();
    eprintln!("[rwkv_load_test] t0 captured");
    println!("\n=== RWKV Model Load Validation ===\n");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let result = rt.block_on(run());

    match result {
        Ok(msg) => println!("\n✅ {msg}  (total {})", ts()),
        Err(e) => println!("\n❌ {e}  (total {})", ts()),
    }
    println!("Total elapsed: {:.1}s", t0.elapsed().as_secs_f64());
}

async fn run() -> Result<String, String> {
    // Stage 1: File access
    let model_path = std::env::var("RWKV_MODEL").unwrap_or_else(|_| {
        "models/rwkv7-g1g-2.9b-20260526-ctx8192-converted.st".into()
    });
    let vocab_path = std::env::var("RWKV_VOCAB").unwrap_or_else(|_| {
        "assets/vocab/rwkv_vocab_v20230424.json".into()
    });

    let model_meta = tokio::fs::metadata(&model_path).await
        .map_err(|e| format!("1. Model file: {e}"))?;
    info!("1. Model file OK — {:.0} MB", model_meta.len() as f64 / 1048576.0);

    let vocab_meta = tokio::fs::metadata(&vocab_path).await
        .map_err(|e| format!("1. Vocab file: {e}"))?;
    info!("2. Vocab file OK — {:.0} KB", vocab_meta.len() as f64 / 1024.0);

    // Stage 3: SafeTensors + Loader::info
    let file = tokio::fs::File::open(&model_path).await
        .map_err(|e| format!("3. File open: {e}"))?;
    let mmap = unsafe { Mmap::map(&file) }.map_err(|e| format!("3. Mmap: {e}"))?;
    info!("3. Mmap OK");

    let model = SafeTensors::deserialize(&mmap).map_err(|e| format!("3. Deserialize: {e}"))?;
    info!("3. SafeTensors OK");

    let info = Loader::info(&model).map_err(|e| format!("3. Loader::info: {e}"))?;
    info!("3. Loader::info OK — version={:?} layers={} vocab={} emb={}",
        info.version, info.num_layer, info.num_vocab, info.num_emb);

    // Stage 4: WebGPU adapter
    let instance = wgpu::Instance::default();
    let all = instance.enumerate_adapters(wgpu::Backends::all()).await;
    info!("4. Found {} adapter(s)", all.len());
    for a in &all {
        let i = a.get_info();
        info!("4.   - {} ({:?})", i.name, i.device_type);
    }

    let adapter = instance.adapter(PowerPreference::HighPerformance).await
        .map_err(|e| format!("4. Adapter: {e}"))?;
    info!("4. Adapter OK — {}", adapter.get_info().name);

    // Stage 5: Context
    let ctx = timeout(TIMEOUT, ContextBuilder::new(adapter).auto_limits(&info).build()).await
        .map_err(|_| "5. Context: TIMEOUT")?
        .map_err(|e| format!("5. Context: {e}"))?;
    info!("5. Context OK — {}", ctx.adapter.get_info().name);

    // Stage 6: Quantization setup + ModelBuilder
    let quant_spec = std::env::var("RWKV_QUANT").unwrap_or_else(|_| "nf4=32".into());
    let quant_layers: HashMap<usize, Quant> = if quant_spec == "none" {
        HashMap::new()
    } else if let Some(n) = quant_spec.strip_prefix("nf4=") {
        let n = n.parse::<usize>().unwrap_or(32);
        (0..n).map(|l| (l, Quant::NF4)).collect()
    } else if let Ok(n) = quant_spec.parse::<usize>() {
        (0..n).map(|l| (l, Quant::Int8)).collect()
    } else {
        (0..32).map(|l| (l, Quant::NF4)).collect()
    };
    info!("6. Quant: {} layers ({})", quant_layers.len(), quant_spec);

    let builder = ModelBuilder::new(&ctx, model).quant(quant_layers);
    info!("6. ModelBuilder OK");

    // Stage 7: build_v7 — THIS IS WHERE IT HANGS
    info!("7. Starting build_v7 (model weights → VRAM)...");
    let t7 = Instant::now();

    let build_result = timeout(TIMEOUT, builder.build_v7()).await;

    info!("7. build_v7 returned after {:.1}s", t7.elapsed().as_secs_f64());

    match build_result {
        Ok(Ok(m)) => {
            info!("7. build_v7 OK!");
            let bundle = v7::Bundle::<f16>::new(m, 1);
            info!("7. Bundle created");
            drop(bundle);
            info!("7. Bundle dropped");

            Ok(format!("Model loaded into VRAM successfully"))
        }
        Ok(Err(e)) => {
            Err(format!("7. build_v7 failed: {e}"))
        }
        Err(_) => {
            Err(format!("7. build_v7 TIMEOUT after {:.1}s — model weights NOT loading to VRAM", t7.elapsed().as_secs_f64()))
        }
    }
}
