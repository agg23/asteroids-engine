use objc2_io_surface::IOSurfaceRef;
use objc2_metal::{
    MTLDevice, MTLPixelFormat, MTLResourceOptions, MTLStorageMode, MTLTextureDescriptor,
    MTLTextureType, MTLTextureUsage,
};
use wgpu::{
    Device, Extent3d, Texture, TextureDescriptor, TextureDimension, TextureUsages, TextureUses,
    hal::{CopyExtent, api::Metal, metal},
};

use crate::OUTPUT_TEXTURE_FORMAT;

// Requires the texture to be `resolution` * `resolution` in size and use 64 bit RGBA half float
pub unsafe fn texture_from_io_surface(
    device: &Device,
    ref_address: usize,
    resolution: u32,
) -> Result<Texture, String> {
    let io_surface =
        unsafe { (ref_address as *const IOSurfaceRef).as_ref() }.ok_or("null IOSurface address")?;

    let descriptor = MTLTextureDescriptor::new();
    descriptor.setTextureType(MTLTextureType::Type2D);
    descriptor.setPixelFormat(MTLPixelFormat::RGBA16Float);

    unsafe {
        descriptor.setWidth(resolution as usize);
        descriptor.setHeight(resolution as usize);
        descriptor.setMipmapLevelCount(1);
    }

    descriptor.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);

    // IOSurface backed textures are shared between the CPU and GPU
    descriptor.setStorageMode(MTLStorageMode::Shared);
    descriptor.setResourceOptions(MTLResourceOptions::StorageModeShared);

    let raw_texture = {
        let hal_device =
            unsafe { device.as_hal::<Metal>() }.ok_or("wgpu is not running on Metal")?;

        hal_device
            .raw_device()
            .newTextureWithDescriptor_iosurface_plane(&descriptor, io_surface, 0)
            .ok_or("Metal refused to back a texture with the IOSurface")?
    };

    let hal_texture = unsafe {
        metal::Device::texture_from_raw(
            raw_texture,
            OUTPUT_TEXTURE_FORMAT,
            MTLTextureType::Type2D,
            1,
            1,
            CopyExtent {
                width: resolution,
                height: resolution,
                depth: 1,
            },
            None,
        )
    };

    let texture_descriptor = TextureDescriptor {
        label: Some("Host IOSurface"),
        size: Extent3d {
            width: resolution,
            height: resolution,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: OUTPUT_TEXTURE_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    };

    Ok(unsafe {
        device.create_texture_from_hal::<Metal>(
            hal_texture,
            &texture_descriptor,
            TextureUses::UNINITIALIZED,
        )
    })
}
