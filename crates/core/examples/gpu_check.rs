//! Query GPU hardware capabilities for RWKV inference.
//! Tells you what quantization the GPU supports and whether the model will fit.

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
            println!("  [{i}] {} ({:?})", info.name, info.device_type);
            println!("      vendor: {:?}", info.vendor);
            println!("      driver: {} {}", info.driver, info.driver_info);
            println!("      backend: {:?}", info.backend);
            println!("      limits:");
            println!(
                "        max_buffer_size: {} MB",
                limits.max_buffer_size / 1048576
            );
            println!(
                "      max_texture_dimension_1d: {}",
                limits.max_texture_dimension_1d
            );
            println!(
                "        max_compute_workgroup_size_x: {}",
                limits.max_compute_workgroup_size_x
            );
            println!(
                "        max_compute_workgroups_per_dimension: {}",
                limits.max_compute_workgroups_per_dimension
            );

            // Check for cooperative matrix support (needed for NF4/Int8 quantization)
            let features = adapter.features();
            let has_coop_matrix =
                features.contains(wgpu::Features::EXPERIMENTAL_COOPERATIVE_MATRIX);
            println!(
                "      cooperative_matrix (experimental): {}",
                if has_coop_matrix { "YES" } else { "NO" }
            );

            // VRAM estimate
            let vram_mb = limits.max_buffer_size as f64 / 1048576.0;
            println!("      max_buffer (approx VRAM): {:.0} MB", vram_mb);

            // Model size estimates
            let model_raw_mb = 5500; // 5.5GB raw
            let nf4_mb = model_raw_mb / 4; // ~1.4GB
            let int8_mb = model_raw_mb / 2; // ~2.75GB

            println!("\n      Model fit check (rwkv7 2.9B, 5.5GB raw):");
            println!(
                "        no quant (FP16 resident ~700MB): {}",
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

            // Recommendation
            println!("\n      Recommendation:");
            if has_coop_matrix && vram_mb > 1400.0 {
                println!("        → Use NF4 quantization: RWKV_QUANT=nf4=32");
            } else if vram_mb > 2750.0 {
                println!("        → Use Int8 quantization: RWKV_QUANT=32");
                println!("        → Or no quant (streaming): RWKV_QUANT=none");
            } else if vram_mb > 700.0 {
                println!("        → No quantization (streaming, ~700MB resident): RWKV_QUANT=none");
            } else {
                println!("        → GPU too small. Use CPU backend (LocalRwkvBackend).");
            }
            println!();
        }

        // Pick best adapter
        let discrete = all
            .iter()
            .find(|a| a.get_info().device_type == wgpu::DeviceType::DiscreteGpu);
        let best = discrete.or(all.first()).unwrap();
        let info = best.get_info();
        let features = best.features();
        let has_coop_matrix = features.contains(wgpu::Features::EXPERIMENTAL_COOPERATIVE_MATRIX);

        println!("Selected: {} ({:?})", info.name, info.device_type);
        println!(
            "Cooperative matrix: {}",
            if has_coop_matrix { "YES" } else { "NO" }
        );

        if has_coop_matrix {
            println!("\n✅ GPU supports cooperative matrices — NF4/Int8 quantization available.");
            println!("   Set RWKV_QUANT=nf4=32 for best performance.");
        } else {
            println!("\n⚠ GPU does NOT support cooperative matrices.");
            println!("   NF4/Int8 quantization will NOT work.");
            println!("   Use RWKV_QUANT=none (streaming, ~700MB resident) or CPU backend.");
        }
    });

    println!("\n=== Done ({:.2}s) ===\n", t0.elapsed().as_secs_f64());
}
