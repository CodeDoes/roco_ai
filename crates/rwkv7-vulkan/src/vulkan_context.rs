//! Vulkan device and instance management

use ash::{vk, Device, Entry, Instance};
use std::ffi::CString;

/// Vulkan context holding device and instance
pub struct VulkanContext {
    pub entry: Entry,
    pub instance: Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: Device,
    pub queue: vk::Queue,
    pub queue_family_index: u32,
    pub command_pool: vk::CommandPool,
}

impl VulkanContext {
    /// Create a new Vulkan context
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        unsafe {
            // Load Vulkan library
            let entry = Entry::load()?;
            
            // Create instance
            let app_name = CString::new("RWKV7-Vulkan")?;
            let app_info = vk::ApplicationInfo::builder()
                .application_name(&app_name)
                .application_version(vk::make_api_version(0, 1, 0, 0))
                .api_version(vk::make_api_version(0, 1, 3, 0));
            
            let instance_create_info = vk::InstanceCreateInfo::builder()
                .application_info(&app_info);
            
            let instance = entry.create_instance(&instance_create_info, None)?;
            
            // Find a suitable physical device
            let physical_devices = instance.enumerate_physical_devices()?;
            let physical_device = physical_devices
                .into_iter()
                .find(|&pd| {
                    let props = instance.get_physical_device_properties(pd);
                    props.device_type == vk::PhysicalDeviceType::DISCRETE_GPU
                        || props.device_type == vk::PhysicalDeviceType::INTEGRATED_GPU
                })
                .ok_or("No suitable GPU found")?;
            
            let props = instance.get_physical_device_properties(physical_device);
            log::info!(
                "Using GPU: {}",
                std::ffi::CStr::from_ptr(props.device_name.as_ptr())
                    .to_str()
                    .unwrap_or("Unknown")
            );
            
            // Find a compute queue family
            let queue_family_properties = instance.get_physical_device_queue_family_properties(physical_device);
            let queue_family_index = queue_family_properties
                .iter()
                .enumerate()
                .find(|(_, props)| props.queue_flags.contains(vk::QueueFlags::COMPUTE))
                .map(|(i, _)| i as u32)
                .ok_or("No compute queue family found")?;
            
            // Create logical device
            let queue_priorities = [1.0];
            let queue_create_info = vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(queue_family_index)
                .queue_priorities(&queue_priorities);
            
            let device_create_info = vk::DeviceCreateInfo::builder()
                .queue_create_infos(std::slice::from_ref(&queue_create_info));
            
            let device = instance.create_device(physical_device, &device_create_info, None)?;
            
            let queue = device.get_device_queue(queue_family_index, 0);
            
            // Create command pool
            let command_pool_create_info = vk::CommandPoolCreateInfo::builder()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_family_index);
            
            let command_pool = device.create_command_pool(&command_pool_create_info, None)?;
            
            Ok(Self {
                entry,
                instance,
                physical_device,
                device,
                queue,
                queue_family_index,
                command_pool,
            })
        }
    }
    
    /// Get memory properties for the physical device
    pub fn memory_properties(&self) -> vk::PhysicalDeviceMemoryProperties {
        unsafe {
            self.instance
                .get_physical_device_memory_properties(self.physical_device)
        }
    }
    
    /// Find a suitable memory type
    pub fn find_memory_type(
        &self,
        type_filter: u32,
        properties: vk::MemoryPropertyFlags,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        let mem_properties = self.memory_properties();
        
        for i in 0..mem_properties.memory_type_count {
            if (type_filter & (1 << i)) != 0
                && mem_properties.memory_types[i as usize]
                    .property_flags
                    .contains(properties)
            {
                return Ok(i);
            }
        }
        
        Err("Failed to find suitable memory type".into())
    }
    
    /// Begin a one-time command buffer
    pub fn begin_one_time_command(&self) -> Result<vk::CommandBuffer, Box<dyn std::error::Error>> {
        unsafe {
            let alloc_info = vk::CommandBufferAllocateInfo::builder()
                .command_pool(self.command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            
            let command_buffer = self
                .device
                .allocate_command_buffers(&alloc_info)?[0];
            
            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            
            self.device
                .begin_command_buffer(command_buffer, &begin_info)?;
            
            Ok(command_buffer)
        }
    }
    
    /// End and submit a one-time command buffer
    pub fn end_one_time_command(&self, command_buffer: vk::CommandBuffer) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            self.device.end_command_buffer(command_buffer)?;
            
            let submit_info = vk::SubmitInfo::builder()
                .command_buffers(std::slice::from_ref(&command_buffer));
            
            self.device.queue_submit(
                self.queue,
                std::slice::from_ref(&submit_info),
                vk::Fence::null(),
            )?;
            
            self.device.queue_wait_idle(self.queue)?;
            
            self.device
                .free_command_buffers(self.command_pool, std::slice::from_ref(&command_buffer));
            
            Ok(())
        }
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}
