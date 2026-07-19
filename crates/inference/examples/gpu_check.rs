//! Query GPU hardware capabilities for RWKV inference.

use std::time::Instant;

fn main() {
    let t0 = Instant::now();
    println!("\n=== GPU Capability Check for RWKV ===\n");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async {
        let instance = wgpu::Instance::default();
        let all = instance.enumerate_adapters(wgpu::Backends::all()).await;

        if all.is_empty() {
            println!("No GPU adapters found. Will need CPU fallback.\n");
            return;
        }

        println!("Found {} adapter(s):\n", all.len());

        for (i, adapter) in all.iter().enumerate() {
            let info = adapter.get_info();
            let limits = adapter.limits();
            let features = adapter.features();
            let has_coop = features.contains(wgpu::Features::EXPERIMENTAL_COOPERATIVE_MATRIX);
            let vram_mb = limits.max_buffer_size as f64 / 1048576.0;

            println!("  [{i}] {} ({:?})", info.name, info.device_type);
            println!("      vendor: {:?}", info.vendor);
            println!("      backend: {:?}", info.backend);
            println!(
                "      cooperative_matrix: {}",
                if has_coop { "YES" } else { "NO" }
            );
            println!("      max_buffer: {:.0} MB", vram_mb);

            println!("\n      Model fit check (rwkv7 2.9B, 5.5GB raw):");
            println!(
                "        no quant (FP16 ~700MB): {}",
                if vram_mb > 700.0 {
                    "✓ fits"
                } else {
                    "✗ too small"
                }
            );
            println!(
                "        Int8 quant (~2.75GB): {}",
                if vram_mb > 2750.0 {
                    "✓ fits"
                } else {
                    "✗ too small"
                }
            );
            println!(
                "        NF4 quant (~1.4GB): {}",
                if vram_mb > 1400.0 {
                    "✓ fits"
                } else {
                    "✗ too small"
                }
            );
        }

        let discrete = all
            .iter()
            .find(|a| a.get_info().device_type == wgpu::DeviceType::DiscreteGpu);
        let best = discrete.or(all.first()).unwrap();
        let info = best.get_info();
        let features = best.features();
        let has_coop = features.contains(wgpu::Features::EXPERIMENTAL_COOPERATIVE_MATRIX);

        println!("Selected: {} ({:?})", info.name, info.device_type);
        println!(
            "Cooperative matrix: {}",
            if has_coop { "YES" } else { "NO" }
        );
        if has_coop {
            println!("\n✅ GPU supports cooperative matrices — NF4/Int8 available.");
        } else {
            println!("\n⚠ GPU does NOT support cooperative matrices.");
            println!("   NF4/Int8 quantization will NOT work.");
        }
    });

    println!("\n=== Done ({:.2}s) ===\n", t0.elapsed().as_secs_f64());
}
