//! Analyze RWKV model weight distributions for quantization planning.
//!
//! Loads a SafeTensors model, computes the RWKVQuant coarse (P_c) and
//! fine (P_f) proxies for every weight tensor, and reports which tensors
//! are safe for scalar quantization (NF4/Int8) vs which need FP16.
//!
//! ```bash
//! RWKV_MODEL=/path/to/model.st cargo run -p roco-core --features local-rwkv \
//!   --example quant_analyze --release
//! ```
//!
//! For very large tensors the proxy uses a representative sample (up to
//! 65 536 elements) to keep memory bounded.

use std::env;

use roco_core::rwkv_quant_proxy::analyze_model_streaming;

fn find_model() -> anyhow::Result<String> {
    if let Ok(p) = env::var("RWKV_MODEL") {
        return Ok(p);
    }
    // Scan models/ directory
    let dir = std::env::current_dir()?;
    for candidate in ["models", "../models"] {
        let p = dir.join(candidate);
        if p.is_dir() {
            for e in std::fs::read_dir(p)? {
                let entry = e?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("rwkv7") && name.ends_with(".st") {
                    return Ok(entry.path().to_string_lossy().to_string());
                }
            }
        }
    }
    anyhow::bail!("no model found — set $RWKV_MODEL or put rwkv7-*.st in models/")
}

fn main() -> anyhow::Result<()> {
    let model_path = find_model()?;
    println!("Model: {model_path}");
    let meta = std::fs::metadata(&model_path)?;
    println!("Size: {} MB", meta.len() / (1024 * 1024));

    let analysis = analyze_model_streaming(&model_path)?;
    analysis.print();

    // Layer-level summary
    println!("Layer-level summary:");
    println!("{:-<80}", "");
    println!(
        "{:<10} {:>8} {:>8} {:>10} {:>10}  {:>6}",
        "layer", "tensors", "sq", "fp16", "sq_pct", "rec"
    );
    println!("{:-<80}", "");

    // Extract unique layer indices
    let mut layers: Vec<usize> = analysis
        .tensors
        .iter()
        .filter_map(|t| {
            let parts: Vec<&str> = t.name.split('.').collect();
            for (i, part) in parts.iter().enumerate() {
                if (*part == "blk" || *part == "blocks") && i + 1 < parts.len() {
                    return parts[i + 1].parse().ok();
                }
            }
            None
        })
        .collect();
    layers.sort();
    layers.dedup();

    let mut total_sq = 0;
    let mut total_fp16 = 0;

    for layer in &layers {
        let layer_tensors: Vec<_> = analysis
            .tensors
            .iter()
            .filter(|t| {
                t.name.starts_with(&format!("blk.{layer}."))
                    || t.name.starts_with(&format!("blocks.{layer}."))
            })
            .collect();
        let sq = layer_tensors.iter().filter(|t| {
            matches!(
                t.recommendation,
                roco_core::rwkv_quant_proxy::QuantRecommendation::ScalarQuant
            )
        }).count();
        let fp16 = layer_tensors.len() - sq;
        let pct = if layer_tensors.is_empty() {
            0.0
        } else {
            100.0 * sq as f64 / layer_tensors.len() as f64
        };
        let rec = if fp16 == 0 { "SQ    " } else { "HYBRID" };
        println!(
            "{:<10} {:>8} {:>8} {:>10} {:>9.1}%  {:>6}",
            layer,
            layer_tensors.len(),
            sq,
            fp16,
            pct,
            rec
        );
        total_sq += sq;
        total_fp16 += fp16;
    }

    println!("{:-<80}", "");
    let total = total_sq + total_fp16;
    let pct = if total == 0 { 0.0 } else { 100.0 * total_sq as f64 / total as f64 };
    println!(
        "  Total:  SQ={} ({:.1}%)  FP16={} ({:.1}%)  tensors={}",
        total_sq, pct, total_fp16, 100.0 - pct, total
    );

    // Estimated memory savings
    // SQ = NF4 ≈ 0.5 bytes/param, FP16 = 2 bytes/param
    let sq_bytes = analysis.sq_numels * 2; // NF4 is ~0.5x FP16
    let fp16_bytes = analysis.fp16_numels * 2;
    let total_original = (analysis.sq_numels + analysis.fp16_numels) * 2;
    let total_quantized = sq_bytes / 4 + fp16_bytes; // NF4 = 4-bit = 1/4 of FP16
    let saving = if total_original > 0 {
        100.0 * (1.0 - total_quantized as f64 / total_original as f64)
    } else {
        0.0
    };
    println!(
        "  Est. memory: {:.1} MB (original {:.1} MB, saving {:.1}%)",
        total_quantized as f64 / (1024.0 * 1024.0),
        total_original as f64 / (1024.0 * 1024.0),
        saving
    );
    println!();

    Ok(())
}
