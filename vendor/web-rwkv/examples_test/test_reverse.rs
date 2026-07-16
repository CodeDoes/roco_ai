use std::{collections::HashMap, env};
use web_rwkv::{
    context::ContextBuilder,
    runtime::{
        infer::{Rnn, RnnInput, RnnInputBatch, RnnOption},
        loader::Loader,
        model::{ModelBuilder, ModelInfo, Quant, ContextAutoLimits},
        softmax::softmax_one,
        v7,
        TokioRuntime,
    },
    tokenizer::Tokenizer,
};
use half::f16;
use safetensors::SafeTensors;

async fn create_context(info: &ModelInfo) -> anyhow::Result<web_rwkv::context::Context> {
    let instance = wgpu::Instance::default();
    let adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;
    let mut scored: Vec<_> = adapters.into_iter()
        .map(|a| {
            let i = a.get_info();
            let coop = a.features().contains(wgpu::Features::EXPERIMENTAL_COOPERATIVE_MATRIX);
            let type_score = match i.device_type {
                wgpu::DeviceType::DiscreteGpu => 30,
                wgpu::DeviceType::IntegratedGpu => 20,
                wgpu::DeviceType::VirtualGpu => 15,
                wgpu::DeviceType::Other => 10,
                wgpu::DeviceType::Cpu => 5,
            };
            let coop_bonus = if coop { 100 } else { 0 };
            (a, coop_bonus + type_score)
        })
        .collect();
    scored.sort_by_key(|&(_, s)| std::cmp::Reverse(s));
    
    let mut context = None;
    for (adapter, _) in scored {
        let ainfo = adapter.get_info();
        let cache_path = "/tmp/roco-pipeline-cache";
        let cached_pipelines = std::fs::read(cache_path).ok();
        let mut builder = ContextBuilder::new(adapter).auto_limits(info);
        if let Some(ref data) = cached_pipelines { builder = builder.with_pipeline_cache(data.clone()); }
        if let Ok(ctx) = builder.build().await {
            println!("Using GPU: {}", ainfo.name);
            context = Some(ctx);
            break;
        }
    }
    context.ok_or_else(|| anyhow::anyhow!("no GPU context"))
}

fn sample(probs: &[f32]) -> u32 {
    probs.iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, _)| i as u32)
        .unwrap_or(0)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let model_path = env::var("RWKV_MODEL").expect("RWKV_MODEL");
    let vocab_path = env::var("RWKV_VOCAB").unwrap_or_else(|_| "assets/vocab/rwkv_vocab_v20230424.json".to_string());
    
    let vocab_text = std::fs::read_to_string(&vocab_path)?;
    let tokenizer = Tokenizer::new(&vocab_text)?;
    
    // Load model using memmap for 'static lifetime
    let file = std::fs::File::open(&model_path)?;
    let data = unsafe { memmap2::Mmap::map(&file)? };
    let model = SafeTensors::deserialize(&data)?;
    let info = Loader::info(&model)?;
    println!("Model: {:?} layers={} vocab={} emb={}", info.version, info.num_layer, info.num_vocab, info.num_emb);
    
    let context = create_context(&info).await?;
    
    let mut quant_layers = HashMap::new();
    for l in 0..info.num_layer {
        quant_layers.insert(l, Quant::NF4);
    }
    
    let builder = ModelBuilder::new(&context, model).quant(quant_layers);
    let m = builder.build_v7().await?;
    let bundle = v7::Bundle::<f16>::new(m, 1);
    let runtime = TokioRuntime::<Rnn>::new(bundle).await;
    
    let prompts = vec![
        ("JSON", "System: You always output JSON.\n\nUser: List three colors in JSON format like this: {\"colors\": [\"red\", \"green\", \"blue\"]}\n\nAssistant:"),
        ("Steps", "System: You are a precise assistant.\n\nUser: Follow these steps exactly:\n1. Say 'Step 1 complete'\n2. Say 'Step 2 complete'\n3. Say 'All steps done'\n\nAssistant:"),
        ("List", "System: You are a list maker.\n\nUser: List 3 things you need for a picnic, numbered 1 to 3.\n\nAssistant:"),
        ("JSON2", "System: You are a data formatter. Always output valid JSON.\n\nUser: Output a JSON object with keys: name, age, city. Use example values.\n\nAssistant:"),
    ];
    
    for (name, prompt) in prompts {
        println!("
=== {} ===", name);
        println!("{}", prompt);
        
        let prompt_tokens = tokenizer.encode(prompt.as_bytes())?;
        let prompt_batch = RnnInputBatch::new(prompt_tokens, RnnOption::Last);
        let mut inference = RnnInput::new(vec![prompt_batch], 128);
        let mut generated = Vec::new();
        
        for _ in 0..80 {
            let input = inference.clone();
            let (input, output) = runtime.infer(input).await?;
            inference = input;
            
            let ot = output[0].0.clone();
            if ot.size() == 0 { continue; }
            
            let probs = softmax_one(&context, ot).await?;
            let token = sample(&probs.to_vec());
            
            if token == 0 { break; }
            
            let decoded = tokenizer.decode(&[token])?;
            let word = String::from_utf8_lossy(&decoded).to_string();
            
            if word == "\n\n" || word == "\nUser:" || word == "\nHuman:" { break; }
            
            generated.push(token);
            inference.batches[0] = RnnInputBatch::new(vec![token], RnnOption::Last);
        }
        
        let full_decoded = tokenizer.decode(&generated)?;
        let text = String::from_utf8_lossy(&full_decoded);
        println!("=== OUTPUT ===");
        println!("{}", text);
        
        if text.contains("reverse_length_6") || text.contains("end_reverse") {
            println!(">>> REVERSE TOKENS DETECTED <<<");
        }
    }
    
    Ok(())
}