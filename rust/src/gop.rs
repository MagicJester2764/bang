use uefi::boot;
use uefi::println;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};

/// Framebuffer info extracted from GOP, passed to Multiboot2 builder.
#[derive(Debug, Clone, Copy)]
pub struct FbInfo {
    pub addr: u64,
    pub pitch: u32,
    pub width: u32,
    pub height: u32,
    pub bpp: u8,
    pub fb_type: u8,
    pub red_pos: u8,
    pub red_size: u8,
    pub green_pos: u8,
    pub green_size: u8,
    pub blue_pos: u8,
    pub blue_size: u8,
}

/// Query GOP for framebuffer information.
/// Must be called before ExitBootServices.
pub fn query_gop() -> Option<FbInfo> {
    let handle = boot::get_handle_for_protocol::<GraphicsOutput>().ok()?;
    let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(handle).ok()?;

    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    let fb_base = gop.frame_buffer().as_mut_ptr() as u64;

    let (bpp, fb_type, red_pos, red_size, green_pos, green_size, blue_pos, blue_size) =
        match mode.pixel_format() {
            PixelFormat::Rgb => (32, 1, 0, 8, 8, 8, 16, 8),
            PixelFormat::Bgr => (32, 1, 16, 8, 8, 8, 0, 8),
            _ => (32, 1, 0, 8, 8, 8, 16, 8),
        };

    let pitch = mode.stride() as u32 * (bpp / 8) as u32;

    let fb = FbInfo {
        addr: fb_base,
        pitch,
        width: width as u32,
        height: height as u32,
        bpp,
        fb_type,
        red_pos,
        red_size,
        green_pos,
        green_size,
        blue_pos,
        blue_size,
    };

    println!(
        "[+] GOP: {}x{} @ {:#x}, pitch={}",
        fb.width, fb.height, fb.addr, fb.pitch
    );

    Some(fb)
}
