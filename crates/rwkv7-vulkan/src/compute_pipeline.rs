//! Vulkan compute pipeline management

use ash::{vk, Device};
use std::ffi::CString;
use std::sync::Arc;
use std::time::Instant;

use crate::vulkan_context::VulkanContext;

/// Compute pipeline wrapper
pub struct ComputePipeline {
    ctx: Arc<VulkanContext>,
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
}

impl ComputePipeline {
    /// Create a new compute pipeline from GLSL source
    pub fn new(
        ctx: Arc<VulkanContext>,
        shader_source: &str,
        entry_point: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Compile shader to SPIR-V using glslangValidator
        let spirv = compile_shader_glslang(shader_source, entry_point)?;
        Self::from_spirv(ctx, &spirv, entry_point)
    }
    
    /// Create a compute pipeline from pre-compiled SPIR-V
    pub fn from_spirv(
        ctx: Arc<VulkanContext>,
        spirv: &[u32],
        entry_point: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        unsafe {
            // Create shader module
            let shader_module = create_shader_module(&ctx.device, spirv)?;
            
            // Create pipeline layout
            let pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::builder();
            
            let pipeline_layout = ctx
                .device
                .create_pipeline_layout(&pipeline_layout_create_info, None)?;
            
            // Create compute pipeline
            let entry_point_cstr = CString::new(entry_point)?;
            
            let shader_stage_create_info = vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(shader_module)
                .name(&entry_point_cstr)
                .build();
            
            let compute_pipeline_create_info = vk::ComputePipelineCreateInfo::builder()
                .stage(shader_stage_create_info)
                .layout(pipeline_layout);
            
            let pipelines = ctx.device.create_compute_pipelines(
                vk::PipelineCache::null(),
                std::slice::from_ref(&compute_pipeline_create_info),
                None,
            ).map_err(|e| format!("Failed to create compute pipeline: {:?}", e))?;
            
            let pipeline = pipelines[0];
            
            ctx.device.destroy_shader_module(shader_module, None);
            
            let descriptor_set_layout = vk::DescriptorSetLayout::default();
            
            Ok(Self {
                ctx,
                pipeline,
                pipeline_layout,
                descriptor_set_layout,
            })
        }
    }
    
    /// Dispatch the compute pipeline
    pub fn dispatch(
        &self,
        command_buffer: vk::CommandBuffer,
        groups_x: u32,
        groups_y: u32,
        groups_z: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            self.ctx.device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline,
            );
            
            self.ctx.device.cmd_dispatch(
                command_buffer,
                groups_x,
                groups_y,
                groups_z,
            );
        }
        
        Ok(())
    }
    
    /// Get the pipeline handle
    pub fn handle(&self) -> vk::Pipeline {
        self.pipeline
    }
    
    /// Get the pipeline layout handle
    pub fn layout(&self) -> vk::PipelineLayout {
        self.pipeline_layout
    }
}

impl Drop for ComputePipeline {
    fn drop(&mut self) {
        unsafe {
            self.ctx
                .device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.ctx.device.destroy_pipeline(self.pipeline, None);
            self.ctx
                .device
                .destroy_pipeline_layout(self.pipeline_layout, None);
        }
    }
}

/// Compile GLSL to SPIR-V using glslangValidator
fn compile_shader_glslang(source: &str, entry_point: &str) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
    let t0 = Instant::now();
    
    // Create temporary files
    let tmp_dir = std::env::temp_dir();
    let shader_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    
    let glsl_path = tmp_dir.join(format!("rwkv7_shader_{}.glsl", shader_id));
    let spirv_path = tmp_dir.join(format!("rwkv7_shader_{}.spv", shader_id));
    
    // Write GLSL source
    std::fs::write(&glsl_path, source)?;
    
    // Compile with glslangValidator
    let output = std::process::Command::new("glslangValidator")
        .args(&[
            "-V",
            glsl_path.to_str().unwrap(),
            "-o",
            spirv_path.to_str().unwrap(),
            "-e",
            entry_point,
        ])
        .output()?;
    
    // Cleanup GLSL file
    let _ = std::fs::remove_file(&glsl_path);
    
    if !output.status.success() {
        let _ = std::fs::remove_file(&spirv_path);
        return Err(format!(
            "Shader compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ).into());
    }
    
    // Read SPIR-V
    let spirv_bytes = std::fs::read(&spirv_path)?;
    let _ = std::fs::remove_file(&spirv_path);
    
    // Convert bytes to u32 words
    let spirv: Vec<u32> = spirv_bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    
    log::debug!("Shader compiled in {:?}", t0.elapsed());
    
    Ok(spirv)
}

/// Create a Vulkan shader module from SPIR-V
unsafe fn create_shader_module(
    device: &Device,
    spirv: &[u32],
) -> Result<vk::ShaderModule, Box<dyn std::error::Error>> {
    let create_info = vk::ShaderModuleCreateInfo::builder()
        .code(spirv);
    
    let shader_module = device.create_shader_module(&create_info, None)?;
    
    Ok(shader_module)
}

/// Helper to create descriptor set layout
pub fn create_descriptor_set_layout(
    device: &Device,
    bindings: &[(u32, vk::DescriptorType, u32, vk::ShaderStageFlags)],
) -> Result<vk::DescriptorSetLayout, Box<dyn std::error::Error>> {
    unsafe {
        let mut layout_bindings = Vec::new();
        
        for &(binding, descriptor_type, count, stage_flags) in bindings {
            let layout_binding = vk::DescriptorSetLayoutBinding::builder()
                .binding(binding)
                .descriptor_type(descriptor_type)
                .descriptor_count(count)
                .stage_flags(stage_flags)
                .build();
            
            layout_bindings.push(layout_binding);
        }
        
        let create_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&layout_bindings);
        
        let layout = device.create_descriptor_set_layout(&create_info, None)?;
        
        Ok(layout)
    }
}
