use web_rwkv::context::{ContextBuilder, InstanceExt};
use web_rwkv::runtime::model::{ContextAutoLimits, ModelInfo};

#[tokio::main]
async fn main() {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .adapter(wgpu::PowerPreference::HighPerformance)
        .await
        .unwrap();
    let info = adapter.get_info();
    println!("Adapter: {} {:?}", info.name, info.backend);

    let model_info = ModelInfo {
        num_emb: 2560,
        num_hidden: 10240,
        num_vocab: 65536,
        num_layer: 32,
        num_head: 32,
        version: web_rwkv::runtime::model::ModelVersion::V7,
        custom: web_rwkv::runtime::model::ModelCustomInfo::None,
    };

    println!("head_buffer_size: {} MB", model_info.head_buffer_size() / (1024 * 1024));
    println!("max_non_head_buffer_size: {} MB", model_info.max_non_head_buffer_size() / (1024 * 1024));

    let context = ContextBuilder::new(adapter)
        .auto_limits(&model_info)
        .build()
        .await
        .unwrap();
    println!("Context created");
    println!("Device max_buffer_size: {} MB", 
        context.device.limits().max_buffer_size / (1024 * 1024));

    // Try allocating head.weight sized buffer
    let head_size = model_info.head_buffer_size();
    println!("Creating head buffer ({} MB)...", head_size / (1024 * 1024));
    let buf = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("head"),
        size: head_size as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    println!("Head buffer created");

    let data = vec![0u8; head_size];
    context.queue.write_buffer(&buf, 0, &data);
    let submission = context.queue.submit(None);
    println!("Submitted head data");

    let result = context.device.poll(wgpu::PollType::Wait {
        submission_index: Some(submission),
        timeout: Some(std::time::Duration::from_secs(10)),
    });
    println!("Head poll result: {:?}", result);
    
    // Per-layer matrix
    let layer_size = model_info.max_non_head_buffer_size();
    println!("\nCreating layer buffer ({} MB)...", layer_size / (1024 * 1024));
    let buf2 = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("layer"),
        size: layer_size as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    println!("Layer buffer created");
    
    let data2 = vec![0u8; layer_size];
    context.queue.write_buffer(&buf2, 0, &data2);
    let submission2 = context.queue.submit(None);
    println!("Submitted layer data");
    
    let result2 = context.device.poll(wgpu::PollType::Wait {
        submission_index: Some(submission2),
        timeout: Some(std::time::Duration::from_secs(10)),
    });
    println!("Layer poll result: {:?}", result2);

    println!("\nAll GPU buffer operations OK");
}
