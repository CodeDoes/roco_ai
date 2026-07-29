//! Vulkan buffer management

use ash::vk;
use std::sync::Arc;

use crate::vulkan_context::VulkanContext;

/// Buffer usage flags
#[derive(Debug, Clone, Copy)]
pub enum BufferUsage {
    StorageBuffer,
    UniformBuffer,
    StagingBuffer,
}

impl BufferUsage {
    fn to_vk(self) -> vk::BufferUsageFlags {
        match self {
            BufferUsage::StorageBuffer => vk::BufferUsageFlags::STORAGE_BUFFER,
            BufferUsage::UniformBuffer => vk::BufferUsageFlags::UNIFORM_BUFFER,
            BufferUsage::StagingBuffer => {
                vk::BufferUsageFlags::TRANSFER_SRC | vk::BufferUsageFlags::TRANSFER_DST
            }
        }
    }
}

/// Vulkan buffer wrapper
pub struct Buffer {
    ctx: Arc<VulkanContext>,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: u64,
}

impl Buffer {
    /// Create a new buffer
    pub fn new(
        ctx: Arc<VulkanContext>,
        size: usize,
        usage: BufferUsage,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        unsafe {
            let buffer_create_info = vk::BufferCreateInfo::builder()
                .size(size as u64)
                .usage(usage.to_vk())
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            
            let buffer = ctx.device.create_buffer(&buffer_create_info, None)?;
            
            let mem_requirements = ctx.device.get_buffer_memory_requirements(buffer);
            
            let memory_type_index = ctx.find_memory_type(
                mem_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            
            let alloc_info = vk::MemoryAllocateInfo::builder()
                .allocation_size(mem_requirements.size)
                .memory_type_index(memory_type_index);
            
            let memory = ctx.device.allocate_memory(&alloc_info, None)?;
            
            ctx.device.bind_buffer_memory(buffer, memory, 0)?;
            
            Ok(Self {
                ctx,
                buffer,
                memory,
                size: size as u64,
            })
        }
    }
    
    /// Create a buffer from existing data
    pub fn from_data(
        ctx: Arc<VulkanContext>,
        data: &[u8],
        usage: BufferUsage,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut buffer = Self::new(ctx, data.len(), usage)?;
        buffer.write_data(data)?;
        Ok(buffer)
    }
    
    /// Write data to the buffer
    pub fn write_data(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        assert!(data.len() as u64 <= self.size, "Data too large for buffer");
        
        unsafe {
            let ptr = self.ctx.device.map_memory(
                self.memory,
                0,
                self.size,
                vk::MemoryMapFlags::empty(),
            )? as *mut u8;
            
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            
            self.ctx.device.unmap_memory(self.memory);
        }
        
        Ok(())
    }
    
    /// Read data from the buffer
    pub fn read_data(&self, size: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        assert!(size as u64 <= self.size, "Read size too large");
        
        let mut data = vec![0u8; size];
        
        unsafe {
            let ptr = self.ctx.device.map_memory(
                self.memory,
                0,
                self.size,
                vk::MemoryMapFlags::empty(),
            )? as *const u8;
            
            std::ptr::copy_nonoverlapping(ptr, data.as_mut_ptr(), size);
            
            self.ctx.device.unmap_memory(self.memory);
        }
        
        Ok(data)
    }
    
    /// Get the Vulkan buffer handle
    pub fn handle(&self) -> vk::Buffer {
        self.buffer
    }
    
    /// Get the buffer size
    pub fn size(&self) -> u64 {
        self.size
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            self.ctx.device.free_memory(self.memory, None);
            self.ctx.device.destroy_buffer(self.buffer, None);
        }
    }
}

/// Helper to convert slices to bytes
pub trait AsBytes {
    fn as_bytes(&self) -> &[u8];
}

impl AsBytes for [f32] {
    fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.as_ptr() as *const u8,
                self.len() * std::mem::size_of::<f32>(),
            )
        }
    }
}

impl AsBytes for [F16] {
    fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.as_ptr() as *const u8,
                self.len() * std::mem::size_of::<F16>(),
            )
        }
    }
}

/// F16 type (half precision)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct F16(u16);

impl F16 {
    pub fn from_f32(val: f32) -> Self {
        // Simple f32 to f16 conversion
        let bits = val.to_bits();
        let sign = (bits >> 16) as u16 & 0x8000;
        let exponent = ((bits >> 23) as i32 - 127 + 15) as u16;
        let mantissa = (bits >> 13) as u16 & 0x3FF;
        
        if exponent == 0 {
            Self(sign)
        } else if exponent >= 31 {
            Self(sign | 0x7C00)
        } else {
            Self(sign | (exponent << 10) | mantissa)
        }
    }
    
    pub fn to_f32(self) -> f32 {
        let bits = self.0;
        let sign = ((bits >> 15) as u32) << 31;
        let exponent = ((bits >> 10) & 0x1F) as i32 - 15 + 127;
        let mantissa = ((bits & 0x3FF) as u32) << 13;
        
        if exponent == 127 - 15 {
            // Denormalized
            if mantissa == 0 {
                f32::from_bits(sign)
            } else {
                f32::from_bits(sign | mantissa)
            }
        } else if exponent >= 255 {
            f32::from_bits(sign | 0x7F800000)
        } else {
            f32::from_bits(sign | ((exponent as u32) << 23) | mantissa)
        }
    }
}

impl From<f32> for F16 {
    fn from(val: f32) -> Self {
        Self::from_f32(val)
    }
}

impl From<F16> for f32 {
    fn from(val: F16) -> Self {
        val.to_f32()
    }
}
