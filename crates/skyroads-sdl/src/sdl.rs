use std::ffi::{c_void, CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::slice;

type Result<T> = std::result::Result<T, String>;

pub mod scancode {
    pub const A: usize = 4;
    pub const D: usize = 7;
    pub const Q: usize = 20;
    pub const S: usize = 22;
    pub const W: usize = 26;
    pub const RETURN: usize = 40;
    pub const ESCAPE: usize = 41;
    pub const TAB: usize = 43;
    pub const SPACE: usize = 44;
    pub const RIGHT: usize = 79;
    pub const LEFT: usize = 80;
    pub const DOWN: usize = 81;
    pub const UP: usize = 82;
}

const SDL_EVENT_UNION_BYTES: usize = 56;
const SDL_INIT_AUDIO: u32 = 0x0000_0010;
const SDL_INIT_JOYSTICK: u32 = 0x0000_0200;
const SDL_INIT_VIDEO: u32 = 0x0000_0020;
const SDL_QUIT: u32 = 0x0100;
const SDL_WINDOWPOS_CENTERED: c_int = 0x2FFF0000u32 as c_int;
const SDL_WINDOW_SHOWN: u32 = 0x0000_0004;
const SDL_WINDOW_FULLSCREEN_DESKTOP: u32 = 0x0000_1001;
const SDL_RENDERER_SOFTWARE: u32 = 0x0000_0001;
const SDL_RENDERER_ACCELERATED: u32 = 0x0000_0002;
const SDL_TEXTUREACCESS_STREAMING: c_int = 1;
const SDL_PIXELFORMAT_RGBA32: u32 = 376_840_196;
const AUDIO_S16SYS: u16 = 32_784;

#[repr(C)]
struct SDL_Window {
    _private: [u8; 0],
}

#[repr(C)]
struct SDL_Renderer {
    _private: [u8; 0],
}

#[repr(C)]
struct SDL_Texture {
    _private: [u8; 0],
}

#[repr(C)]
struct SDL_Joystick {
    _private: [u8; 0],
}

#[repr(C)]
struct SDL_Rect {
    x: c_int,
    y: c_int,
    w: c_int,
    h: c_int,
}

#[repr(C)]
union SDL_Event {
    type_: u32,
    padding: [u8; SDL_EVENT_UNION_BYTES],
}

type SdlAudioCallback =
    Option<unsafe extern "C" fn(userdata: *mut c_void, stream: *mut u8, len: c_int)>;

#[repr(C)]
struct SDL_AudioSpec {
    freq: c_int,
    format: u16,
    channels: u8,
    silence: u8,
    samples: u16,
    padding: u16,
    size: u32,
    callback: SdlAudioCallback,
    userdata: *mut c_void,
}

extern "C" {
    fn SDL_Init(flags: u32) -> c_int;
    fn SDL_Quit();
    fn SDL_GetError() -> *const c_char;
    fn SDL_CreateWindow(
        title: *const c_char,
        x: c_int,
        y: c_int,
        w: c_int,
        h: c_int,
        flags: u32,
    ) -> *mut SDL_Window;
    fn SDL_DestroyWindow(window: *mut SDL_Window);
    fn SDL_CreateRenderer(window: *mut SDL_Window, index: c_int, flags: u32) -> *mut SDL_Renderer;
    fn SDL_DestroyRenderer(renderer: *mut SDL_Renderer);
    fn SDL_CreateTexture(
        renderer: *mut SDL_Renderer,
        format: u32,
        access: c_int,
        w: c_int,
        h: c_int,
    ) -> *mut SDL_Texture;
    fn SDL_DestroyTexture(texture: *mut SDL_Texture);
    fn SDL_UpdateTexture(
        texture: *mut SDL_Texture,
        rect: *const SDL_Rect,
        pixels: *const c_void,
        pitch: c_int,
    ) -> c_int;
    fn SDL_RenderCopy(
        renderer: *mut SDL_Renderer,
        texture: *mut SDL_Texture,
        src_rect: *const SDL_Rect,
        dst_rect: *const SDL_Rect,
    ) -> c_int;
    fn SDL_SetWindowFullscreen(window: *mut SDL_Window, flags: u32) -> c_int;
    fn SDL_SetWindowBordered(window: *mut SDL_Window, bordered: c_int);
    fn SDL_SetWindowPosition(window: *mut SDL_Window, x: c_int, y: c_int);
    fn SDL_SetWindowSize(window: *mut SDL_Window, w: c_int, h: c_int);
    fn SDL_GetWindowSize(window: *mut SDL_Window, w: *mut c_int, h: *mut c_int);
    fn SDL_SetWindowTitle(window: *mut SDL_Window, title: *const c_char);
    fn SDL_SetRenderDrawColor(renderer: *mut SDL_Renderer, r: u8, g: u8, b: u8, a: u8) -> c_int;
    fn SDL_RenderClear(renderer: *mut SDL_Renderer) -> c_int;
    fn SDL_RenderPresent(renderer: *mut SDL_Renderer);
    fn SDL_PollEvent(event: *mut SDL_Event) -> c_int;
    fn SDL_PumpEvents();
    fn SDL_GetKeyboardState(numkeys: *mut c_int) -> *const u8;
    fn SDL_GetMouseState(x: *mut c_int, y: *mut c_int) -> u32;
    fn SDL_WarpMouseInWindow(window: *mut SDL_Window, x: c_int, y: c_int);
    fn SDL_NumJoysticks() -> c_int;
    fn SDL_JoystickOpen(index: c_int) -> *mut SDL_Joystick;
    fn SDL_JoystickClose(joystick: *mut SDL_Joystick);
    fn SDL_JoystickGetAxis(joystick: *mut SDL_Joystick, axis: c_int) -> i16;
    fn SDL_JoystickGetButton(joystick: *mut SDL_Joystick, button: c_int) -> u8;
    fn SDL_OpenAudioDevice(
        device: *const c_char,
        iscapture: c_int,
        desired: *const SDL_AudioSpec,
        obtained: *mut SDL_AudioSpec,
        allowed_changes: c_int,
    ) -> u32;
    fn SDL_CloseAudioDevice(device: u32);
    fn SDL_PauseAudioDevice(device: u32, pause_on: c_int);
    fn SDL_QueueAudio(device: u32, data: *const c_void, len: u32) -> c_int;
    fn SDL_GetQueuedAudioSize(device: u32) -> u32;
    fn SDL_ClearQueuedAudio(device: u32);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub buttons: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoystickState {
    pub x_axis: i16,
    pub y_axis: i16,
    pub jump_pressed: bool,
}

impl From<Rect> for SDL_Rect {
    fn from(value: Rect) -> Self {
        Self {
            x: value.x,
            y: value.y,
            w: value.w,
            h: value.h,
        }
    }
}

pub struct Sdl;

impl Sdl {
    pub fn init() -> Result<Self> {
        let result = unsafe { SDL_Init(SDL_INIT_VIDEO | SDL_INIT_AUDIO | SDL_INIT_JOYSTICK) };
        if result != 0 {
            return Err(last_error());
        }
        Ok(Self)
    }

    pub fn poll_quit_requested(&self) -> bool {
        let mut event = SDL_Event {
            padding: [0; SDL_EVENT_UNION_BYTES],
        };
        loop {
            let ready = unsafe { SDL_PollEvent(&mut event) };
            if ready == 0 {
                break;
            }
            let event_type = unsafe { event.type_ };
            if event_type == SDL_QUIT {
                return true;
            }
        }
        false
    }

    pub fn pump_events(&self) {
        unsafe { SDL_PumpEvents() }
    }

    pub fn keyboard_state(&self) -> KeyboardState<'_> {
        let mut key_count = 0;
        let key_ptr = unsafe { SDL_GetKeyboardState(&mut key_count) };
        let keys = if key_ptr.is_null() || key_count <= 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(key_ptr, key_count as usize) }
        };
        KeyboardState { keys }
    }

    pub fn mouse_state(&self) -> MouseState {
        let mut x = 0;
        let mut y = 0;
        let buttons = unsafe { SDL_GetMouseState(&mut x, &mut y) };
        MouseState { x, y, buttons }
    }
}

impl Drop for Sdl {
    fn drop(&mut self) {
        unsafe { SDL_Quit() }
    }
}

pub struct Window {
    raw: *mut SDL_Window,
}

impl Window {
    pub fn new(title: &str, width: i32, height: i32) -> Result<Self> {
        let title = CString::new(title).map_err(|_| "window title contained NUL".to_string())?;
        let raw = unsafe {
            SDL_CreateWindow(
                title.as_ptr(),
                SDL_WINDOWPOS_CENTERED,
                SDL_WINDOWPOS_CENTERED,
                width,
                height,
                SDL_WINDOW_SHOWN,
            )
        };
        if raw.is_null() {
            return Err(last_error());
        }
        Ok(Self { raw })
    }

    pub fn set_title(&self, title: &str) -> Result<()> {
        let title = CString::new(title).map_err(|_| "window title contained NUL".to_string())?;
        unsafe { SDL_SetWindowTitle(self.raw, title.as_ptr()) };
        Ok(())
    }

    pub fn set_fullscreen_desktop(&self, enabled: bool) -> Result<()> {
        let flags = if enabled {
            SDL_WINDOW_FULLSCREEN_DESKTOP
        } else {
            0
        };
        let result = unsafe { SDL_SetWindowFullscreen(self.raw, flags) };
        if result != 0 {
            return Err(last_error());
        }
        Ok(())
    }

    pub fn set_bordered(&self, bordered: bool) {
        unsafe { SDL_SetWindowBordered(self.raw, if bordered { 1 } else { 0 }) }
    }

    pub fn set_size(&self, width: i32, height: i32) {
        unsafe { SDL_SetWindowSize(self.raw, width, height) }
    }

    pub fn center(&self) {
        unsafe { SDL_SetWindowPosition(self.raw, SDL_WINDOWPOS_CENTERED, SDL_WINDOWPOS_CENTERED) }
    }

    pub fn size(&self) -> (i32, i32) {
        let mut width = 0;
        let mut height = 0;
        unsafe { SDL_GetWindowSize(self.raw, &mut width, &mut height) };
        (width, height)
    }

    pub fn warp_mouse(&self, x: i32, y: i32) {
        unsafe { SDL_WarpMouseInWindow(self.raw, x, y) }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { SDL_DestroyWindow(self.raw) }
        }
    }
}

pub struct Joystick {
    raw: *mut SDL_Joystick,
}

impl Joystick {
    pub fn open_first() -> Result<Option<Self>> {
        if unsafe { SDL_NumJoysticks() } <= 0 {
            return Ok(None);
        }
        let raw = unsafe { SDL_JoystickOpen(0) };
        if raw.is_null() {
            return Err(last_error());
        }
        Ok(Some(Self { raw }))
    }

    pub fn state(&self) -> JoystickState {
        JoystickState {
            x_axis: unsafe { SDL_JoystickGetAxis(self.raw, 0) },
            y_axis: unsafe { SDL_JoystickGetAxis(self.raw, 1) },
            jump_pressed: unsafe { SDL_JoystickGetButton(self.raw, 0) } != 0,
        }
    }
}

impl Drop for Joystick {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { SDL_JoystickClose(self.raw) }
        }
    }
}

pub struct Renderer {
    raw: *mut SDL_Renderer,
}

impl Renderer {
    pub fn new(window: &Window) -> Result<Self> {
        let raw = unsafe { SDL_CreateRenderer(window.raw, -1, SDL_RENDERER_ACCELERATED) };
        let raw = if raw.is_null() {
            unsafe { SDL_CreateRenderer(window.raw, -1, SDL_RENDERER_SOFTWARE) }
        } else {
            raw
        };
        if raw.is_null() {
            return Err(last_error());
        }
        Ok(Self { raw })
    }

    pub fn set_draw_color(&self, color: Color) -> Result<()> {
        let result =
            unsafe { SDL_SetRenderDrawColor(self.raw, color.r, color.g, color.b, color.a) };
        if result != 0 {
            return Err(last_error());
        }
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        let result = unsafe { SDL_RenderClear(self.raw) };
        if result != 0 {
            return Err(last_error());
        }
        Ok(())
    }

    pub fn present(&self) {
        unsafe { SDL_RenderPresent(self.raw) }
    }

    pub fn copy_texture(&self, texture: &Texture, destination: Rect) -> Result<()> {
        let destination = SDL_Rect::from(destination);
        let result = unsafe { SDL_RenderCopy(self.raw, texture.raw, ptr::null(), &destination) };
        if result != 0 {
            return Err(last_error());
        }
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { SDL_DestroyRenderer(self.raw) }
        }
    }
}

pub struct KeyboardState<'a> {
    keys: &'a [u8],
}

impl<'a> KeyboardState<'a> {
    pub fn is_pressed(&self, scancode: usize) -> bool {
        self.keys.get(scancode).copied().unwrap_or(0) != 0
    }
}

pub struct Texture {
    raw: *mut SDL_Texture,
}

impl Texture {
    pub fn new_rgba_streaming(renderer: &Renderer, width: i32, height: i32) -> Result<Self> {
        let raw = unsafe {
            SDL_CreateTexture(
                renderer.raw,
                SDL_PIXELFORMAT_RGBA32,
                SDL_TEXTUREACCESS_STREAMING,
                width,
                height,
            )
        };
        if raw.is_null() {
            return Err(last_error());
        }
        Ok(Self { raw })
    }

    pub fn update_rgba(&self, pixels: &[u8], pitch: usize) -> Result<()> {
        let result = unsafe {
            SDL_UpdateTexture(
                self.raw,
                ptr::null(),
                pixels.as_ptr().cast::<c_void>(),
                pitch as c_int,
            )
        };
        if result != 0 {
            return Err(last_error());
        }
        Ok(())
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { SDL_DestroyTexture(self.raw) }
        }
    }
}

pub struct AudioDevice {
    id: u32,
    channels: u8,
}

impl AudioDevice {
    pub fn open_queue_playback_mono(sample_rate: u32, sample_buffer: u16) -> Result<Self> {
        let desired = SDL_AudioSpec {
            freq: sample_rate as c_int,
            format: AUDIO_S16SYS,
            channels: 1,
            silence: 0,
            samples: sample_buffer,
            padding: 0,
            size: 0,
            callback: None,
            userdata: ptr::null_mut(),
        };
        let mut obtained = SDL_AudioSpec {
            freq: 0,
            format: 0,
            channels: 0,
            silence: 0,
            samples: 0,
            padding: 0,
            size: 0,
            callback: None,
            userdata: ptr::null_mut(),
        };

        let id = unsafe { SDL_OpenAudioDevice(ptr::null(), 0, &desired, &mut obtained, 0) };
        if id == 0 {
            return Err(last_error());
        }
        if obtained.format != AUDIO_S16SYS
            || obtained.channels != 1
            || obtained.freq != sample_rate as c_int
        {
            unsafe { SDL_CloseAudioDevice(id) };
            return Err(format!(
                "SDL audio format mismatch: requested {sample_rate}Hz mono s16, got {}Hz {}ch format {}",
                obtained.freq, obtained.channels, obtained.format
            ));
        }

        Ok(Self {
            id,
            channels: obtained.channels,
        })
    }

    pub fn resume(&self) {
        unsafe { SDL_PauseAudioDevice(self.id, 0) }
    }

    pub fn queue_i16(&self, samples: &[i16]) -> Result<()> {
        let bytes = unsafe {
            slice::from_raw_parts(
                samples.as_ptr().cast::<u8>(),
                std::mem::size_of_val(samples),
            )
        };
        let result =
            unsafe { SDL_QueueAudio(self.id, bytes.as_ptr().cast::<c_void>(), bytes.len() as u32) };
        if result != 0 {
            return Err(last_error());
        }
        Ok(())
    }

    pub fn queued_samples(&self) -> usize {
        self.queued_bytes() / std::mem::size_of::<i16>() / usize::from(self.channels)
    }

    pub fn clear(&self) {
        unsafe { SDL_ClearQueuedAudio(self.id) }
    }

    fn queued_bytes(&self) -> usize {
        unsafe { SDL_GetQueuedAudioSize(self.id) as usize }
    }
}

impl Drop for AudioDevice {
    fn drop(&mut self) {
        if self.id != 0 {
            unsafe { SDL_CloseAudioDevice(self.id) }
        }
    }
}

fn last_error() -> String {
    let ptr = unsafe { SDL_GetError() };
    if ptr.is_null() {
        return "SDL reported an unknown error".to_string();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .unwrap_or("SDL returned non-UTF-8 error text")
        .to_string()
}
