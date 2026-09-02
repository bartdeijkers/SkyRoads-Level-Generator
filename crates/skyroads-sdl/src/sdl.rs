use std::ffi::{c_void, CStr, CString};
use std::marker::PhantomData;
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::rc::Rc;
use std::slice;
use std::sync::atomic::{AtomicBool, Ordering};

type Result<T> = std::result::Result<T, String>;

pub mod scancode {
    pub const A: usize = 4;
    pub const D: usize = 7;
    pub const LSHIFT: usize = 225;
    pub const Q: usize = 20;
    pub const RSHIFT: usize = 229;
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

const SDL_INIT_AUDIO: u32 = 0x0000_0010;
const SDL_INIT_JOYSTICK: u32 = 0x0000_0200;
const SDL_INIT_GAMECONTROLLER: u32 = 0x0000_2000;
const SDL_INIT_VIDEO: u32 = 0x0000_0020;
const SDL_QUIT: u32 = 0x0100;
const SDL_DISPLAYEVENT: u32 = 0x0150;
const SDL_WINDOWEVENT: u32 = 0x0200;
const SDL_WINDOWEVENT_DISPLAY_CHANGED: u8 = 18;
const SDL_JOYDEVICEADDED: u32 = 0x0605;
const SDL_JOYDEVICEREMOVED: u32 = 0x0606;
const SDL_CONTROLLERDEVICEADDED: u32 = 0x0653;
const SDL_CONTROLLERDEVICEREMOVED: u32 = 0x0654;
const SDL_CONTROLLERDEVICEREMAPPED: u32 = 0x0655;
const SDL_RENDER_TARGETS_RESET: u32 = 0x2000;
const SDL_RENDER_DEVICE_RESET: u32 = 0x2001;
const SDL_WINDOWPOS_CENTERED: c_int = 0x2FFF0000u32 as c_int;
const SDL_WINDOW_FULLSCREEN: u32 = 0x0000_0001;
const SDL_WINDOW_HIDDEN: u32 = 0x0000_0008;
const SDL_WINDOW_FULLSCREEN_DESKTOP: u32 = 0x0000_1001;
const SDL_WINDOW_ALLOW_HIGHDPI: u32 = 0x0000_2000;
const SDL_RENDERER_SOFTWARE: u32 = 0x0000_0001;
const SDL_RENDERER_ACCELERATED: u32 = 0x0000_0002;
const SDL_RENDERER_PRESENTVSYNC: u32 = 0x0000_0004;
const SDL_TEXTUREACCESS_STREAMING: c_int = 1;
const SDL_PIXELFORMAT_RGBA32: u32 = 376_840_196;
const SDL_NUM_SCANCODES: usize = 512;
const SDL_HINT_OVERRIDE: c_int = 2;
const AUDIO_S16SYS: u16 = 32_784;
const SDL_CONTROLLER_AXIS_LEFTX: c_int = 0;
const SDL_CONTROLLER_AXIS_LEFTY: c_int = 1;
const SDL_CONTROLLER_AXIS_TRIGGERLEFT: c_int = 4;
const SDL_CONTROLLER_AXIS_TRIGGERRIGHT: c_int = 5;
const SDL_CONTROLLER_BUTTON_SOUTH: c_int = 0;
const SDL_CONTROLLER_BUTTON_EAST: c_int = 1;
const SDL_CONTROLLER_BUTTON_BACK: c_int = 4;
const SDL_CONTROLLER_BUTTON_START: c_int = 6;
const SDL_CONTROLLER_BUTTON_DPAD_UP: c_int = 11;
const SDL_CONTROLLER_BUTTON_DPAD_DOWN: c_int = 12;
const SDL_CONTROLLER_BUTTON_DPAD_LEFT: c_int = 13;
const SDL_CONTROLLER_BUTTON_DPAD_RIGHT: c_int = 14;
static SDL_ACTIVE: AtomicBool = AtomicBool::new(false);

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
struct SDL_GameController {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_JoystickGUID {
    data: [u8; 16],
}

const _: [(); 16] = [(); std::mem::size_of::<SDL_JoystickGUID>()];
const _: [(); 1] = [(); std::mem::align_of::<SDL_JoystickGUID>()];

#[repr(C)]
struct SDL_Rect {
    x: c_int,
    y: c_int,
    w: c_int,
    h: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_WindowEvent {
    type_: u32,
    timestamp: u32,
    window_id: u32,
    event: u8,
    padding1: u8,
    padding2: u8,
    padding3: u8,
    data1: i32,
    data2: i32,
}

const _: [(); 24] = [(); std::mem::size_of::<SDL_WindowEvent>()];

#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_JoyDeviceEvent {
    type_: u32,
    timestamp: u32,
    which: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_ControllerDeviceEvent {
    type_: u32,
    timestamp: u32,
    which: i32,
}

const _: [(); 12] = [(); std::mem::size_of::<SDL_JoyDeviceEvent>()];
const _: [(); 12] = [(); std::mem::size_of::<SDL_ControllerDeviceEvent>()];
const _: [(); std::mem::align_of::<u32>()] = [(); std::mem::align_of::<SDL_JoyDeviceEvent>()];
const _: [(); std::mem::align_of::<u32>()] =
    [(); std::mem::align_of::<SDL_ControllerDeviceEvent>()];

#[repr(C)]
union SDL_Event {
    type_: u32,
    window: SDL_WindowEvent,
    jdevice: SDL_JoyDeviceEvent,
    cdevice: SDL_ControllerDeviceEvent,
    padding: [u8; 56],
    align: *mut c_void,
}

const _: [(); 56] = [(); std::mem::size_of::<SDL_Event>()];
const _: [(); std::mem::align_of::<*mut c_void>()] = [(); std::mem::align_of::<SDL_Event>()];

#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_DisplayMode {
    format: u32,
    w: c_int,
    h: c_int,
    refresh_rate: c_int,
    driverdata: *mut c_void,
}

#[cfg(target_pointer_width = "64")]
const _: [(); 24] = [(); std::mem::size_of::<SDL_DisplayMode>()];
#[cfg(target_pointer_width = "32")]
const _: [(); 20] = [(); std::mem::size_of::<SDL_DisplayMode>()];

#[repr(C)]
struct SdlVersion {
    major: u8,
    minor: u8,
    patch: u8,
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
    fn SDL_ClearError();
    fn SDL_GetError() -> *const c_char;
    fn SDL_GetVersion(version: *mut SdlVersion);
    fn SDL_GetCurrentVideoDriver() -> *const c_char;
    #[cfg(target_os = "windows")]
    fn SDL_SetHint(name: *const c_char, value: *const c_char) -> c_int;
    fn SDL_SetHintWithPriority(name: *const c_char, value: *const c_char, priority: c_int)
        -> c_int;
    fn SDL_GetHint(name: *const c_char) -> *const c_char;
    fn SDL_CreateWindow(
        title: *const c_char,
        x: c_int,
        y: c_int,
        w: c_int,
        h: c_int,
        flags: u32,
    ) -> *mut SDL_Window;
    fn SDL_DestroyWindow(window: *mut SDL_Window);
    fn SDL_ShowWindow(window: *mut SDL_Window);
    fn SDL_GetWindowDisplayIndex(window: *mut SDL_Window) -> c_int;
    fn SDL_GetDisplayName(display_index: c_int) -> *const c_char;
    fn SDL_GetDesktopDisplayMode(display_index: c_int, mode: *mut SDL_DisplayMode) -> c_int;
    fn SDL_GetNumDisplayModes(display_index: c_int) -> c_int;
    fn SDL_GetDisplayMode(
        display_index: c_int,
        mode_index: c_int,
        mode: *mut SDL_DisplayMode,
    ) -> c_int;
    fn SDL_SetWindowDisplayMode(window: *mut SDL_Window, mode: *const SDL_DisplayMode) -> c_int;
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
    fn SDL_RenderSetVSync(renderer: *mut SDL_Renderer, vsync: c_int) -> c_int;
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
    fn SDL_GetRendererOutputSize(
        renderer: *mut SDL_Renderer,
        w: *mut c_int,
        h: *mut c_int,
    ) -> c_int;
    fn SDL_PollEvent(event: *mut SDL_Event) -> c_int;
    fn SDL_GetKeyboardState(numkeys: *mut c_int) -> *const u8;
    fn SDL_GetMouseState(x: *mut c_int, y: *mut c_int) -> u32;
    fn SDL_WarpMouseInWindow(window: *mut SDL_Window, x: c_int, y: c_int);
    fn SDL_NumJoysticks() -> c_int;
    fn SDL_JoystickNameForIndex(index: c_int) -> *const c_char;
    fn SDL_JoystickOpen(index: c_int) -> *mut SDL_Joystick;
    fn SDL_JoystickClose(joystick: *mut SDL_Joystick);
    fn SDL_JoystickName(joystick: *mut SDL_Joystick) -> *const c_char;
    fn SDL_JoystickGetDeviceInstanceID(device_index: c_int) -> i32;
    fn SDL_JoystickInstanceID(joystick: *mut SDL_Joystick) -> i32;
    fn SDL_JoystickGetDeviceGUID(device_index: c_int) -> SDL_JoystickGUID;
    fn SDL_JoystickNumAxes(joystick: *mut SDL_Joystick) -> c_int;
    fn SDL_JoystickNumButtons(joystick: *mut SDL_Joystick) -> c_int;
    fn SDL_JoystickGetAxis(joystick: *mut SDL_Joystick, axis: c_int) -> i16;
    fn SDL_JoystickGetButton(joystick: *mut SDL_Joystick, button: c_int) -> u8;
    fn SDL_IsGameController(index: c_int) -> c_int;
    fn SDL_GameControllerNameForIndex(index: c_int) -> *const c_char;
    fn SDL_GameControllerMappingForDeviceIndex(index: c_int) -> *mut c_char;
    fn SDL_GameControllerOpen(index: c_int) -> *mut SDL_GameController;
    fn SDL_GameControllerClose(controller: *mut SDL_GameController);
    fn SDL_GameControllerName(controller: *mut SDL_GameController) -> *const c_char;
    fn SDL_GameControllerGetAttached(controller: *mut SDL_GameController) -> c_int;
    fn SDL_GameControllerGetJoystick(controller: *mut SDL_GameController) -> *mut SDL_Joystick;
    fn SDL_GameControllerGetAxis(controller: *mut SDL_GameController, axis: c_int) -> i16;
    fn SDL_GameControllerGetButton(controller: *mut SDL_GameController, button: c_int) -> u8;
    fn SDL_free(memory: *mut c_void);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayModeInfo {
    pub width: u32,
    pub height: u32,
    pub refresh_rate_hz: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayInfo {
    pub name: String,
    pub desktop_mode: DisplayModeInfo,
    pub fullscreen_modes: Vec<DisplayModeInfo>,
    pub skipped_fullscreen_modes: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingEvents {
    pub quit_requested: bool,
    pub renderer_reset: bool,
    pub display_changed: bool,
    pub input_devices: Vec<InputDeviceEvent>,
    pub input_errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceIndex(c_int);

impl DeviceIndex {
    pub fn new(value: i32) -> Result<Self> {
        if value < 0 {
            return Err(format!(
                "SDL device index must be non-negative, got {value}"
            ));
        }
        Ok(Self(value))
    }

    pub fn value(self) -> i32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoystickInstanceId(i32);

impl JoystickInstanceId {
    fn new(value: i32) -> Result<Self> {
        if value < 0 {
            return Err(format!("SDL returned invalid joystick instance ID {value}"));
        }
        Ok(Self(value))
    }

    pub fn value(self) -> i32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDeviceEvent {
    MappedAdded(DeviceIndex),
    MappedRemoved(JoystickInstanceId),
    MappedRemapped(JoystickInstanceId),
    RawAdded(DeviceIndex),
    RawRemoved(JoystickInstanceId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeviceInfo {
    pub device_index: DeviceIndex,
    pub name: String,
    pub mapped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeviceDiagnostics {
    pub info: InputDeviceInfo,
    pub guid: std::result::Result<String, String>,
    pub mapping: std::result::Result<Option<String>, String>,
    pub opened: std::result::Result<OpenedJoystickDiagnostics, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenedJoystickDiagnostics {
    pub instance_id: JoystickInstanceId,
    pub axis_count: u32,
    pub button_count: u32,
}

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GameControllerState {
    pub left_x: i16,
    pub left_y: i16,
    pub left_trigger: u16,
    pub right_trigger: u16,
    pub dpad_up: bool,
    pub dpad_down: bool,
    pub dpad_left: bool,
    pub dpad_right: bool,
    pub south_pressed: bool,
    pub east_pressed: bool,
    pub start_pressed: bool,
    pub back_pressed: bool,
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

pub struct Sdl {
    _main_thread_only: PhantomData<Rc<()>>,
}

impl Sdl {
    pub fn init() -> Result<Self> {
        Self::init_subsystems(
            SDL_INIT_VIDEO | SDL_INIT_AUDIO | SDL_INIT_JOYSTICK | SDL_INIT_GAMECONTROLLER,
        )
    }

    pub fn init_controller_diagnostics() -> Result<Self> {
        Self::init_subsystems(SDL_INIT_JOYSTICK | SDL_INIT_GAMECONTROLLER)
    }

    fn init_subsystems(flags: u32) -> Result<Self> {
        if SDL_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("SDL is already initialized by this host".to_string());
        }

        // SDL's A/B enum values normally follow printed button labels. The
        // application contract is positional instead: south confirms and east
        // goes back on every controller layout.
        // SAFETY: Both arguments are static NUL-terminated strings. SDL accepts
        // this hint before the game-controller subsystem is initialized.
        let positional_button_hint = c"SDL_GAMECONTROLLER_USE_BUTTON_LABELS";
        unsafe {
            SDL_SetHintWithPriority(
                positional_button_hint.as_ptr(),
                c"0".as_ptr(),
                SDL_HINT_OVERRIDE,
            );
        }
        // SAFETY: The hint name is static and NUL-terminated. SDL owns the
        // nullable result, which is copied immediately.
        let effective_button_hint =
            c_string_lossy(unsafe { SDL_GetHint(positional_button_hint.as_ptr()) });
        if effective_button_hint.as_deref() != Some("0") {
            SDL_ACTIVE.store(false, Ordering::Release);
            return Err(format!(
                "SDL_GAMECONTROLLER_USE_BUTTON_LABELS must be 0 for positional controls, but its effective value is {:?}",
                effective_button_hint.as_deref().unwrap_or("unset")
            ));
        }

        #[cfg(target_os = "windows")]
        if flags & SDL_INIT_VIDEO != 0 {
            // SAFETY: Both arguments are static NUL-terminated strings, and SDL
            // accepts this hint before the video subsystem is initialized.
            unsafe {
                SDL_SetHint(
                    c"SDL_WINDOWS_DPI_AWARENESS".as_ptr(),
                    c"permonitorv2".as_ptr(),
                );
            }
        }
        // SAFETY: The flags are valid SDL subsystem bits. A successful call
        // establishes the process-wide SDL state borrowed by every wrapper.
        let result = unsafe { SDL_Init(flags) };
        if result != 0 {
            SDL_ACTIVE.store(false, Ordering::Release);
            return Err(last_error());
        }
        Ok(Self {
            _main_thread_only: PhantomData,
        })
    }

    pub fn version(&self) -> String {
        let mut version = SdlVersion {
            major: 0,
            minor: 0,
            patch: 0,
        };
        // SAFETY: `version` is writable and has SDL_version's exact C layout.
        unsafe { SDL_GetVersion(&mut version) };
        format!("{}.{}.{}", version.major, version.minor, version.patch)
    }

    pub fn video_driver(&self) -> String {
        // SAFETY: SDL is initialized for `self`; the returned pointer is only
        // read immediately and may validly be null.
        let driver = unsafe { SDL_GetCurrentVideoDriver() };
        c_string_lossy(driver).unwrap_or_else(|| "unknown".to_string())
    }

    pub fn input_devices(&self) -> Result<Vec<InputDeviceInfo>> {
        // SAFETY: `self` proves the joystick and game-controller subsystems are
        // initialized. SDL returns a negative value on enumeration failure.
        let device_count = unsafe { SDL_NumJoysticks() };
        if device_count < 0 {
            return Err(last_error());
        }

        let mut devices = Vec::with_capacity(device_count as usize);
        for raw_index in 0..device_count {
            let device_index = DeviceIndex::new(raw_index)?;
            devices.push(input_device_info(device_index));
        }
        Ok(devices)
    }

    pub fn input_device_diagnostics(&self) -> Result<Vec<InputDeviceDiagnostics>> {
        let expected_identities = input_device_identities()?;
        ensure_diagnostic_snapshot_unchanged(&expected_identities, &input_device_identities()?)?;

        let mut diagnostics = Vec::with_capacity(expected_identities.len());
        for (device_index, expected_instance_id) in &expected_identities {
            ensure_diagnostic_device_unchanged(*device_index, *expected_instance_id)?;
            let info = input_device_info(*device_index);
            ensure_diagnostic_device_unchanged(*device_index, *expected_instance_id)?;

            let guid = device_guid(*device_index);
            ensure_diagnostic_device_unchanged(*device_index, *expected_instance_id)?;

            let mapping = controller_mapping(*device_index, info.mapped);
            ensure_diagnostic_device_unchanged(*device_index, *expected_instance_id)?;

            let opened = match Joystick::open(self, *device_index) {
                Ok(joystick) if joystick.instance_id() != *expected_instance_id => {
                    return Err(diagnostic_device_changed_error(
                        *device_index,
                        *expected_instance_id,
                        joystick.instance_id(),
                    ));
                }
                Ok(joystick) => joystick.diagnostics(),
                Err(error) => Err(error),
            };
            ensure_diagnostic_device_unchanged(*device_index, *expected_instance_id)?;

            diagnostics.push(InputDeviceDiagnostics {
                info,
                guid,
                mapping,
                opened,
            });
        }

        ensure_diagnostic_snapshot_unchanged(&expected_identities, &input_device_identities()?)?;

        Ok(diagnostics)
    }

    pub fn poll_events(&self) -> PendingEvents {
        let mut pending = PendingEvents::default();
        let mut event = SDL_Event { padding: [0; 56] };
        loop {
            // SAFETY: `event` has SDL_Event's required size and alignment and
            // remains writable for the duration of the call.
            let ready = unsafe { SDL_PollEvent(&mut event) };
            if ready == 0 {
                break;
            }
            // SAFETY: A successful SDL_PollEvent initializes the event, and
            // every SDL event variant starts with the same Uint32 type field.
            let event_type = unsafe { event.type_ };
            match event_type {
                SDL_QUIT => pending.quit_requested = true,
                SDL_DISPLAYEVENT => pending.display_changed = true,
                SDL_WINDOWEVENT => {
                    // SAFETY: The checked event type selects SDL_Event.window,
                    // which SDL_PollEvent initialized using SDL_WindowEvent's C layout.
                    let window_event = unsafe { event.window };
                    pending.display_changed |=
                        window_event.event == SDL_WINDOWEVENT_DISPLAY_CHANGED;
                }
                SDL_RENDER_TARGETS_RESET | SDL_RENDER_DEVICE_RESET => {
                    pending.renderer_reset = true;
                }
                SDL_CONTROLLERDEVICEADDED
                | SDL_CONTROLLERDEVICEREMOVED
                | SDL_CONTROLLERDEVICEREMAPPED => {
                    // SAFETY: The checked type selects SDL_Event.cdevice, which
                    // SDL initialized with SDL_ControllerDeviceEvent's C layout.
                    let device_event = unsafe { event.cdevice };
                    match decode_input_device_event(event_type, device_event.which) {
                        Ok(Some(device_event)) => pending.input_devices.push(device_event),
                        Ok(None) => {}
                        Err(error) => pending.input_errors.push(error),
                    }
                }
                SDL_JOYDEVICEADDED | SDL_JOYDEVICEREMOVED => {
                    // SAFETY: The checked type selects SDL_Event.jdevice, which
                    // SDL initialized with SDL_JoyDeviceEvent's C layout.
                    let device_event = unsafe { event.jdevice };
                    match decode_input_device_event(event_type, device_event.which) {
                        Ok(Some(device_event)) => pending.input_devices.push(device_event),
                        Ok(None) => {}
                        Err(error) => pending.input_errors.push(error),
                    }
                }
                _ => {}
            }
        }
        pending
    }

    pub fn keyboard_state(&self) -> KeyboardState {
        let mut key_count = 0;
        // SAFETY: SDL is initialized and `key_count` is a valid out pointer.
        // A possibly-null result is checked before it is read.
        let key_ptr = unsafe { SDL_GetKeyboardState(&mut key_count) };
        let mut keys = [0; SDL_NUM_SCANCODES];
        if !key_ptr.is_null() && key_count > 0 {
            let copy_count = usize::try_from(key_count)
                .unwrap_or(0)
                .min(SDL_NUM_SCANCODES);
            // SAFETY: SDL owns an array of at least `key_count` bytes and keeps it
            // valid after this call. Copying immediately avoids exposing memory
            // that SDL mutates on later event-pump calls as a shared Rust slice.
            let source = unsafe { slice::from_raw_parts(key_ptr, copy_count) };
            keys[..copy_count].copy_from_slice(source);
        }
        KeyboardState { keys }
    }

    pub fn mouse_state(&self) -> MouseState {
        let mut x = 0;
        let mut y = 0;
        // SAFETY: SDL is initialized, and both coordinates are valid writable
        // out parameters for the duration of the call.
        let buttons = unsafe { SDL_GetMouseState(&mut x, &mut y) };
        MouseState { x, y, buttons }
    }
}

impl Drop for Sdl {
    fn drop(&mut self) {
        // SAFETY: Successful construction called SDL_Init. Resource wrappers
        // borrow `Sdl`, so Rust drops all of them before this call.
        unsafe { SDL_Quit() }
        SDL_ACTIVE.store(false, Ordering::Release);
    }
}

pub struct Window<'sdl> {
    raw: *mut SDL_Window,
    _sdl: PhantomData<&'sdl Sdl>,
}

impl<'sdl> Window<'sdl> {
    pub fn new(_sdl: &'sdl Sdl, title: &str, width: i32, height: i32) -> Result<Self> {
        if width <= 0 || height <= 0 {
            return Err("window dimensions must be positive".to_string());
        }
        let title = CString::new(title).map_err(|_| "window title contained NUL".to_string())?;
        // SAFETY: `title` is NUL-terminated, the dimensions were validated,
        // the flag combination is valid, and `_sdl` proves SDL is initialized.
        let raw = unsafe {
            SDL_CreateWindow(
                title.as_ptr(),
                SDL_WINDOWPOS_CENTERED,
                SDL_WINDOWPOS_CENTERED,
                width,
                height,
                SDL_WINDOW_HIDDEN | SDL_WINDOW_ALLOW_HIGHDPI,
            )
        };
        if raw.is_null() {
            return Err(last_error());
        }
        Ok(Self {
            raw,
            _sdl: PhantomData,
        })
    }

    pub fn set_title(&self, title: &str) -> Result<()> {
        let title = CString::new(title).map_err(|_| "window title contained NUL".to_string())?;
        // SAFETY: `self.raw` stays live for `self`, and SDL copies the valid
        // NUL-terminated title during this call.
        unsafe { SDL_SetWindowTitle(self.raw, title.as_ptr()) };
        Ok(())
    }

    pub fn show(&self) {
        // SAFETY: Successful construction guarantees a live SDL_Window.
        unsafe { SDL_ShowWindow(self.raw) }
    }

    pub fn display_info(&self) -> Result<DisplayInfo> {
        let display_index = self.display_index()?;
        let mut desktop_mode = empty_display_mode();
        // SAFETY: The index came from this live window, and `desktop_mode` is a
        // writable SDL_DisplayMode with the exact C layout.
        let result = unsafe { SDL_GetDesktopDisplayMode(display_index, &mut desktop_mode) };
        if result != 0 {
            return Err(last_error());
        }

        // SAFETY: The display index was returned by SDL for this live window.
        let mode_count = unsafe { SDL_GetNumDisplayModes(display_index) };
        if mode_count < 0 {
            return Err(last_error());
        }
        let mut fullscreen_modes = Vec::with_capacity(mode_count as usize);
        let mut skipped_fullscreen_modes = 0;
        for mode_index in 0..mode_count {
            match self.raw_display_mode(display_index, mode_index) {
                Ok(mode) => match display_mode_info(mode) {
                    Some(mode) => fullscreen_modes.push(mode),
                    None => skipped_fullscreen_modes += 1,
                },
                Err(_) => skipped_fullscreen_modes += 1,
            }
        }

        // SAFETY: The display index is valid. SDL owns the nullable string,
        // which `c_string_lossy` copies before any later SDL call can replace it.
        let name = c_string_lossy(unsafe { SDL_GetDisplayName(display_index) })
            .unwrap_or_else(|| format!("display {display_index}"));
        let desktop_mode = display_mode_info(desktop_mode)
            .ok_or_else(|| "SDL reported an invalid desktop display mode".to_string())?;
        Ok(DisplayInfo {
            name,
            desktop_mode,
            fullscreen_modes,
            skipped_fullscreen_modes,
        })
    }

    pub fn set_borderless_desktop(&self) -> Result<()> {
        self.set_fullscreen_flags(SDL_WINDOW_FULLSCREEN_DESKTOP)?;
        self.clear_exclusive_display_mode()
    }

    pub fn set_exclusive_fullscreen(&self, selected: DisplayModeInfo) -> Result<()> {
        let display_index = self.display_index()?;
        // SAFETY: The display index was returned by SDL for this live window.
        let mode_count = unsafe { SDL_GetNumDisplayModes(display_index) };
        if mode_count < 0 {
            return Err(last_error());
        }

        let mut exact_mode = None;
        for mode_index in 0..mode_count {
            let raw_mode = self.raw_display_mode(display_index, mode_index)?;
            if display_mode_info(raw_mode) == Some(selected) {
                exact_mode = Some(raw_mode);
                break;
            }
        }
        let exact_mode = exact_mode.ok_or_else(|| {
            format!(
                "display no longer exposes {}x{} {}",
                selected.width,
                selected.height,
                format_refresh_rate(selected.refresh_rate_hz)
            )
        })?;

        self.set_fullscreen_flags(0)?;
        // SAFETY: `exact_mode` is an unmodified mode returned by SDL for this
        // window's display, including its driver-owned private pointer.
        let result = unsafe { SDL_SetWindowDisplayMode(self.raw, &exact_mode) };
        if result != 0 {
            return Err(last_error());
        }
        self.set_fullscreen_flags(SDL_WINDOW_FULLSCREEN)
    }

    pub fn set_windowed(&self, width: i32, height: i32) -> Result<()> {
        self.set_fullscreen_flags(0)?;
        self.clear_exclusive_display_mode()?;
        self.set_bordered(true);
        self.set_size(width, height);
        self.center();
        Ok(())
    }

    pub fn set_bordered(&self, bordered: bool) {
        // SAFETY: Successful construction guarantees a live SDL_Window, and
        // SDL_bool is represented by the documented integer values 0 and 1.
        unsafe { SDL_SetWindowBordered(self.raw, if bordered { 1 } else { 0 }) }
    }

    pub fn set_size(&self, width: i32, height: i32) {
        // SAFETY: Successful construction guarantees a live SDL_Window. SDL
        // accepts signed dimensions and reports unsupported sizes itself.
        unsafe { SDL_SetWindowSize(self.raw, width, height) }
    }

    pub fn center(&self) {
        // SAFETY: The window pointer is live and both position sentinels are
        // the documented SDL_WINDOWPOS_CENTERED value.
        unsafe { SDL_SetWindowPosition(self.raw, SDL_WINDOWPOS_CENTERED, SDL_WINDOWPOS_CENTERED) }
    }

    pub fn size(&self) -> (i32, i32) {
        let mut width = 0;
        let mut height = 0;
        // SAFETY: The window is live, and both dimensions are writable out
        // parameters for the duration of the call.
        unsafe { SDL_GetWindowSize(self.raw, &mut width, &mut height) };
        (width, height)
    }

    pub fn warp_mouse(&self, x: i32, y: i32) {
        // SAFETY: Successful construction guarantees a live SDL_Window.
        unsafe { SDL_WarpMouseInWindow(self.raw, x, y) }
    }

    fn display_index(&self) -> Result<c_int> {
        // SAFETY: Successful construction guarantees a live SDL_Window.
        let display_index = unsafe { SDL_GetWindowDisplayIndex(self.raw) };
        if display_index < 0 {
            return Err(last_error());
        }
        Ok(display_index)
    }

    fn raw_display_mode(&self, display_index: c_int, mode_index: c_int) -> Result<SDL_DisplayMode> {
        let mut mode = empty_display_mode();
        // SAFETY: The caller obtains both indexes from SDL, and `mode` is a
        // writable value with SDL_DisplayMode's exact C layout.
        let result = unsafe { SDL_GetDisplayMode(display_index, mode_index, &mut mode) };
        if result != 0 {
            return Err(last_error());
        }
        Ok(mode)
    }

    fn set_fullscreen_flags(&self, flags: u32) -> Result<()> {
        // SAFETY: The window is live and callers pass only SDL's documented
        // windowed, desktop-fullscreen, or exclusive-fullscreen flag values.
        let result = unsafe { SDL_SetWindowFullscreen(self.raw, flags) };
        if result != 0 {
            return Err(last_error());
        }
        Ok(())
    }

    fn clear_exclusive_display_mode(&self) -> Result<()> {
        // SAFETY: The window is live; SDL documents a null mode as clearing an
        // explicit exclusive display mode.
        let result = unsafe { SDL_SetWindowDisplayMode(self.raw, ptr::null()) };
        if result != 0 {
            return Err(last_error());
        }
        Ok(())
    }
}

impl Drop for Window<'_> {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: This wrapper uniquely owns the live SDL_Window and drops
            // it once, while its `Sdl` borrow is still active.
            unsafe { SDL_DestroyWindow(self.raw) }
        }
    }
}

pub struct Joystick<'sdl> {
    raw: *mut SDL_Joystick,
    instance_id: JoystickInstanceId,
    name: String,
    _sdl: PhantomData<&'sdl Sdl>,
}

impl<'sdl> Joystick<'sdl> {
    pub fn open(_sdl: &'sdl Sdl, device_index: DeviceIndex) -> Result<Self> {
        // SAFETY: `_sdl` proves the joystick subsystem is initialized. The
        // caller obtained or validated this non-negative, transient index.
        let raw = unsafe { SDL_JoystickOpen(device_index.0) };
        if raw.is_null() {
            return Err(last_error());
        }

        let instance_id = match joystick_instance_id(raw) {
            Ok(instance_id) => instance_id,
            Err(error) => {
                // SAFETY: `raw` was opened successfully above and has not been
                // transferred anywhere, so this closes it exactly once.
                unsafe { SDL_JoystickClose(raw) };
                return Err(error);
            }
        };
        // SAFETY: `raw` is live. SDL owns the nullable name pointer, which is
        // copied before another SDL call can invalidate it.
        let name = c_string_lossy(unsafe { SDL_JoystickName(raw) })
            .unwrap_or_else(|| format!("unnamed SDL joystick {}", device_index.0));

        Ok(Self {
            raw,
            instance_id,
            name,
            _sdl: PhantomData,
        })
    }

    pub fn state(&self) -> Result<JoystickState> {
        clear_error();
        // SAFETY: Successful construction guarantees the joystick pointer is
        // live, and these accessors do not retain any Rust-provided memory.
        let state = JoystickState {
            x_axis: unsafe { SDL_JoystickGetAxis(self.raw, 0) },
            y_axis: unsafe { SDL_JoystickGetAxis(self.raw, 1) },
            jump_pressed: unsafe { SDL_JoystickGetButton(self.raw, 0) } != 0,
        };
        if let Some(error) = current_error() {
            return Err(format!("could not sample raw SDL joystick: {error}"));
        }
        Ok(state)
    }

    fn diagnostics(&self) -> Result<OpenedJoystickDiagnostics> {
        clear_error();
        // SAFETY: Successful construction guarantees a live joystick. The query
        // retains no Rust-owned memory and returns a negative value on failure.
        let axis_count = unsafe { SDL_JoystickNumAxes(self.raw) };
        if axis_count < 0 {
            return Err(format!(
                "could not query SDL joystick axis count: {}",
                last_error()
            ));
        }

        clear_error();
        // SAFETY: The same live joystick remains borrowed for this immediate
        // query, which returns a negative value on failure.
        let button_count = unsafe { SDL_JoystickNumButtons(self.raw) };
        if button_count < 0 {
            return Err(format!(
                "could not query SDL joystick button count: {}",
                last_error()
            ));
        }

        Ok(OpenedJoystickDiagnostics {
            instance_id: self.instance_id,
            axis_count: axis_count as u32,
            button_count: button_count as u32,
        })
    }

    pub fn instance_id(&self) -> JoystickInstanceId {
        self.instance_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for Joystick<'_> {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: This wrapper uniquely owns the live joystick handle and
            // drops it once while SDL remains initialized.
            unsafe { SDL_JoystickClose(self.raw) }
        }
    }
}

struct SdlAllocatedString(*mut c_char);

impl SdlAllocatedString {
    fn copy_lossy(&self) -> String {
        debug_assert!(!self.0.is_null());
        // SAFETY: This wrapper is constructed only from a non-null SDL-owned,
        // NUL-terminated mapping string and keeps it allocated through the copy.
        unsafe { CStr::from_ptr(self.0) }
            .to_string_lossy()
            .into_owned()
    }
}

impl Drop for SdlAllocatedString {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: SDL allocated this mapping string and transfers ownership
            // to the caller. SDL_free releases it exactly once.
            unsafe { SDL_free(self.0.cast()) }
        }
    }
}

pub struct GameController<'sdl> {
    raw: *mut SDL_GameController,
    instance_id: JoystickInstanceId,
    name: String,
    _sdl: PhantomData<&'sdl Sdl>,
}

impl<'sdl> GameController<'sdl> {
    pub fn open(_sdl: &'sdl Sdl, device_index: DeviceIndex) -> Result<Self> {
        // SAFETY: `_sdl` proves the controller subsystem is initialized. This
        // query does not open or transfer ownership of the indexed device.
        if unsafe { SDL_IsGameController(device_index.0) } == 0 {
            return Err(format!(
                "SDL input device {} has no game-controller mapping",
                device_index.0
            ));
        }

        // SAFETY: The validated index currently names a mapped controller and
        // SDL returns null instead of an invalid owned handle on failure.
        let raw = unsafe { SDL_GameControllerOpen(device_index.0) };
        if raw.is_null() {
            return Err(last_error());
        }

        // SAFETY: The controller is live. SDL owns this borrowed joystick and
        // keeps it valid until SDL_GameControllerClose; it must not be closed
        // independently.
        let joystick = unsafe { SDL_GameControllerGetJoystick(raw) };
        if joystick.is_null() {
            let error = last_error();
            // SAFETY: `raw` is uniquely owned and has not escaped this method.
            unsafe { SDL_GameControllerClose(raw) };
            return Err(error);
        }
        let instance_id = match joystick_instance_id(joystick) {
            Ok(instance_id) => instance_id,
            Err(error) => {
                // SAFETY: Closing the controller also releases its borrowed
                // joystick. Neither handle escaped this method.
                unsafe { SDL_GameControllerClose(raw) };
                return Err(error);
            }
        };
        // SAFETY: `raw` is live. The nullable SDL-owned name is copied now.
        let name = c_string_lossy(unsafe { SDL_GameControllerName(raw) })
            .unwrap_or_else(|| format!("unnamed SDL game controller {}", device_index.0));

        Ok(Self {
            raw,
            instance_id,
            name,
            _sdl: PhantomData,
        })
    }

    pub fn state(&self) -> Result<GameControllerState> {
        clear_error();
        // SAFETY: Successful construction guarantees a live controller handle.
        // These polling functions retain no Rust memory. SDL defines triggers
        // as non-negative; a negative value is treated as neutral defensively.
        let state = unsafe {
            GameControllerState {
                left_x: SDL_GameControllerGetAxis(self.raw, SDL_CONTROLLER_AXIS_LEFTX),
                left_y: SDL_GameControllerGetAxis(self.raw, SDL_CONTROLLER_AXIS_LEFTY),
                left_trigger: non_negative_axis(SDL_GameControllerGetAxis(
                    self.raw,
                    SDL_CONTROLLER_AXIS_TRIGGERLEFT,
                )),
                right_trigger: non_negative_axis(SDL_GameControllerGetAxis(
                    self.raw,
                    SDL_CONTROLLER_AXIS_TRIGGERRIGHT,
                )),
                dpad_up: controller_button(self.raw, SDL_CONTROLLER_BUTTON_DPAD_UP),
                dpad_down: controller_button(self.raw, SDL_CONTROLLER_BUTTON_DPAD_DOWN),
                dpad_left: controller_button(self.raw, SDL_CONTROLLER_BUTTON_DPAD_LEFT),
                dpad_right: controller_button(self.raw, SDL_CONTROLLER_BUTTON_DPAD_RIGHT),
                south_pressed: controller_button(self.raw, SDL_CONTROLLER_BUTTON_SOUTH),
                east_pressed: controller_button(self.raw, SDL_CONTROLLER_BUTTON_EAST),
                start_pressed: controller_button(self.raw, SDL_CONTROLLER_BUTTON_START),
                back_pressed: controller_button(self.raw, SDL_CONTROLLER_BUTTON_BACK),
            }
        };
        if let Some(error) = current_error() {
            return Err(format!("could not sample mapped SDL controller: {error}"));
        }
        Ok(state)
    }

    pub fn instance_id(&self) -> JoystickInstanceId {
        self.instance_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn refresh_name(&mut self) {
        // SAFETY: The controller remains live and owns the nullable string. We
        // copy it immediately, retaining the last known name if SDL has none.
        if let Some(name) = c_string_lossy(unsafe { SDL_GameControllerName(self.raw) }) {
            self.name = name;
        }
    }

    pub fn is_attached(&self) -> bool {
        // SAFETY: `self.raw` remains owned by this wrapper. SDL permits this
        // query after a removal event so callers can replace the active device.
        unsafe { SDL_GameControllerGetAttached(self.raw) != 0 }
    }
}

impl Drop for GameController<'_> {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: This wrapper uniquely owns the controller and closes it
            // exactly once. SDL also releases the borrowed joystick here.
            unsafe { SDL_GameControllerClose(self.raw) }
        }
    }
}

fn joystick_instance_id(raw: *mut SDL_Joystick) -> Result<JoystickInstanceId> {
    if raw.is_null() {
        return Err("cannot query the instance ID of a null SDL joystick".to_string());
    }
    // SAFETY: Null was rejected and every caller supplies a live SDL-owned
    // joystick for the duration of this immediate query.
    JoystickInstanceId::new(unsafe { SDL_JoystickInstanceID(raw) })
}

fn input_device_info(device_index: DeviceIndex) -> InputDeviceInfo {
    // SAFETY: The caller obtained this transient index from the current SDL
    // enumeration. Neither query retains Rust-owned memory.
    let mapped = unsafe { SDL_IsGameController(device_index.0) } != 0;
    let name_ptr = if mapped {
        // SAFETY: The current index names a mapped device for this immediate
        // query. SDL owns the nullable returned string.
        unsafe { SDL_GameControllerNameForIndex(device_index.0) }
    } else {
        // SAFETY: The current index names a joystick for this immediate query.
        // SDL owns the nullable returned string.
        unsafe { SDL_JoystickNameForIndex(device_index.0) }
    };
    let name = c_string_lossy(name_ptr)
        .unwrap_or_else(|| format!("unnamed SDL input device {}", device_index.0));

    InputDeviceInfo {
        device_index,
        name,
        mapped,
    }
}

fn input_device_identities() -> Result<Vec<(DeviceIndex, JoystickInstanceId)>> {
    clear_error();
    // SAFETY: Diagnostics initialize the joystick subsystem before calling this
    // helper. SDL reports enumeration failure with a negative value.
    let device_count = unsafe { SDL_NumJoysticks() };
    if device_count < 0 {
        return Err(format!(
            "could not enumerate SDL input devices: {}",
            last_error()
        ));
    }

    let mut identities = Vec::with_capacity(device_count as usize);
    for raw_index in 0..device_count {
        let device_index = DeviceIndex::new(raw_index)?;
        identities.push((device_index, input_device_instance_id(device_index)?));
    }
    Ok(identities)
}

fn input_device_instance_id(device_index: DeviceIndex) -> Result<JoystickInstanceId> {
    clear_error();
    // SAFETY: The caller obtained this transient index from the current SDL
    // enumeration. SDL returns a negative ID if the index became invalid.
    let raw_instance_id = unsafe { SDL_JoystickGetDeviceInstanceID(device_index.0) };
    if raw_instance_id < 0 {
        let detail =
            current_error().unwrap_or_else(|| "the device index is no longer valid".to_string());
        return Err(format!(
            "could not identify SDL device index {}: {detail}",
            device_index.0
        ));
    }
    JoystickInstanceId::new(raw_instance_id)
}

fn ensure_diagnostic_device_unchanged(
    device_index: DeviceIndex,
    expected: JoystickInstanceId,
) -> Result<()> {
    let observed = input_device_instance_id(device_index).map_err(|error| {
        format!("SDL input devices changed during diagnostics: {error}; run diagnostics again")
    })?;
    ensure_diagnostic_identity_matches(device_index, expected, observed)
}

fn ensure_diagnostic_identity_matches(
    device_index: DeviceIndex,
    expected: JoystickInstanceId,
    observed: JoystickInstanceId,
) -> Result<()> {
    if expected == observed {
        return Ok(());
    }

    Err(diagnostic_device_changed_error(
        device_index,
        expected,
        observed,
    ))
}

fn diagnostic_device_changed_error(
    device_index: DeviceIndex,
    expected: JoystickInstanceId,
    observed: JoystickInstanceId,
) -> String {
    format!(
        "SDL input devices changed during diagnostics: device index {} moved from instance {} to {}; run diagnostics again",
        device_index.0, expected.0, observed.0
    )
}

fn ensure_diagnostic_snapshot_unchanged(
    expected: &[(DeviceIndex, JoystickInstanceId)],
    observed: &[(DeviceIndex, JoystickInstanceId)],
) -> Result<()> {
    if expected == observed {
        return Ok(());
    }

    Err(
        "SDL input device list changed during diagnostics; run diagnostics again to collect one coherent snapshot"
            .to_string(),
    )
}

fn device_guid(device_index: DeviceIndex) -> Result<String> {
    clear_error();
    // SAFETY: The caller obtained this transient index from the current SDL
    // enumeration. The returned GUID is a 16-byte value with the asserted C ABI.
    let guid = unsafe { SDL_JoystickGetDeviceGUID(device_index.0) };
    if guid.data == [0; 16] {
        let detail = current_error().unwrap_or_else(|| {
            "SDL returned a zero GUID; the device may have disconnected".to_string()
        });
        return Err(format!(
            "could not query GUID for SDL device index {}: {detail}",
            device_index.0
        ));
    }

    Ok(guid_bytes_to_string(guid.data))
}

fn guid_bytes_to_string(bytes: [u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut guid = String::with_capacity(32);
    for byte in bytes {
        guid.push(char::from(HEX[usize::from(byte >> 4)]));
        guid.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    guid
}

fn controller_mapping(device_index: DeviceIndex, mapped: bool) -> Result<Option<String>> {
    clear_error();
    // SAFETY: The current enumeration produced this transient index. SDL either
    // returns null or transfers an allocated NUL-terminated string to the caller.
    let raw = unsafe { SDL_GameControllerMappingForDeviceIndex(device_index.0) };
    if raw.is_null() {
        return missing_controller_mapping(device_index, mapped, current_error());
    }

    let owned = SdlAllocatedString(raw);
    Ok(Some(owned.copy_lossy()))
}

fn missing_controller_mapping(
    device_index: DeviceIndex,
    mapped: bool,
    error: Option<String>,
) -> Result<Option<String>> {
    if let Some(detail) = error {
        return Err(format!(
            "could not query mapping for SDL device index {}: {detail}",
            device_index.0
        ));
    }
    if mapped {
        return Err(format!(
            "could not query mapping for SDL device index {}: SDL reported a mapped device without mapping data",
            device_index.0
        ));
    }
    Ok(None)
}

fn controller_button(raw: *mut SDL_GameController, button: c_int) -> bool {
    debug_assert!(!raw.is_null());
    // SAFETY: Every caller supplies a live controller owned by its wrapper and
    // one of the declared SDL_GameControllerButton discriminants.
    unsafe { SDL_GameControllerGetButton(raw, button) != 0 }
}

fn non_negative_axis(value: i16) -> u16 {
    u16::try_from(value).unwrap_or(0)
}

fn decode_input_device_event(event_type: u32, which: i32) -> Result<Option<InputDeviceEvent>> {
    let event = match event_type {
        SDL_CONTROLLERDEVICEADDED => InputDeviceEvent::MappedAdded(DeviceIndex::new(which)?),
        SDL_CONTROLLERDEVICEREMOVED => {
            InputDeviceEvent::MappedRemoved(JoystickInstanceId::new(which)?)
        }
        SDL_CONTROLLERDEVICEREMAPPED => {
            InputDeviceEvent::MappedRemapped(JoystickInstanceId::new(which)?)
        }
        SDL_JOYDEVICEADDED => InputDeviceEvent::RawAdded(DeviceIndex::new(which)?),
        SDL_JOYDEVICEREMOVED => InputDeviceEvent::RawRemoved(JoystickInstanceId::new(which)?),
        _ => return Ok(None),
    };
    Ok(Some(event))
}

pub struct Renderer<'window> {
    raw: *mut SDL_Renderer,
    vsync_enabled: bool,
    _window: PhantomData<&'window SDL_Window>,
}

impl Renderer<'_> {
    pub fn new<'window>(window: &'window Window<'_>) -> Result<Renderer<'window>> {
        // SAFETY: `window.raw` is live for `'window`. The renderer flags are a
        // documented combination; SDL returns null rather than an invalid handle.
        let raw = unsafe {
            SDL_CreateRenderer(
                window.raw,
                -1,
                SDL_RENDERER_ACCELERATED | SDL_RENDERER_PRESENTVSYNC,
            )
        };
        let raw = if raw.is_null() {
            // SAFETY: The same live window is retried without the optional
            // present-vsync flag after the first attempt returned null.
            unsafe { SDL_CreateRenderer(window.raw, -1, SDL_RENDERER_ACCELERATED) }
        } else {
            raw
        };
        let raw = if raw.is_null() {
            // SAFETY: The same live window is retried with SDL's software flag
            // after both accelerated attempts returned null.
            unsafe { SDL_CreateRenderer(window.raw, -1, SDL_RENDERER_SOFTWARE) }
        } else {
            raw
        };
        if raw.is_null() {
            return Err(last_error());
        }
        // SAFETY: `raw` is a live renderer and 1 is SDL's documented value for
        // enabling vsync. The return value reports whether it was applied.
        let vsync_enabled = unsafe { SDL_RenderSetVSync(raw, 1) } == 0;
        Ok(Renderer {
            raw,
            vsync_enabled,
            _window: PhantomData,
        })
    }

    pub fn vsync_enabled(&self) -> bool {
        self.vsync_enabled
    }

    pub fn output_size(&self) -> Result<(i32, i32)> {
        let mut width = 0;
        let mut height = 0;
        // SAFETY: The renderer is live, and both dimensions are writable out
        // parameters. This returns pixel size even for high-DPI windows.
        let result = unsafe { SDL_GetRendererOutputSize(self.raw, &mut width, &mut height) };
        if result != 0 {
            return Err(last_error());
        }
        Ok((width, height))
    }

    pub fn set_draw_color(&self, color: Color) -> Result<()> {
        // SAFETY: The renderer is live and each color component is an SDL Uint8.
        let result =
            unsafe { SDL_SetRenderDrawColor(self.raw, color.r, color.g, color.b, color.a) };
        if result != 0 {
            return Err(last_error());
        }
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        // SAFETY: Successful construction guarantees a live SDL_Renderer.
        let result = unsafe { SDL_RenderClear(self.raw) };
        if result != 0 {
            return Err(last_error());
        }
        Ok(())
    }

    pub fn present(&self) {
        // SAFETY: Successful construction guarantees a live SDL_Renderer.
        unsafe { SDL_RenderPresent(self.raw) }
    }

    pub fn copy_texture(&self, texture: &Texture, destination: Rect) -> Result<()> {
        let destination = SDL_Rect::from(destination);
        // SAFETY: Both handles are live, `texture` borrows a renderer for its
        // lifetime, null selects the full source, and `destination` is in scope.
        let result = unsafe { SDL_RenderCopy(self.raw, texture.raw, ptr::null(), &destination) };
        if result != 0 {
            return Err(last_error());
        }
        Ok(())
    }
}

fn empty_display_mode() -> SDL_DisplayMode {
    SDL_DisplayMode {
        format: 0,
        w: 0,
        h: 0,
        refresh_rate: 0,
        driverdata: ptr::null_mut(),
    }
}

fn display_mode_info(mode: SDL_DisplayMode) -> Option<DisplayModeInfo> {
    let width = u32::try_from(mode.w).ok().filter(|value| *value > 0)?;
    let height = u32::try_from(mode.h).ok().filter(|value| *value > 0)?;
    let refresh_rate_hz = u32::try_from(mode.refresh_rate)
        .ok()
        .filter(|value| *value > 0);
    Some(DisplayModeInfo {
        width,
        height,
        refresh_rate_hz,
    })
}

fn c_string_lossy(value: *const c_char) -> Option<String> {
    if value.is_null() {
        return None;
    }
    Some(
        // SAFETY: All callers pass nullable NUL-terminated strings owned by
        // SDL. Null was rejected above and the value is copied immediately.
        unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned(),
    )
}

fn format_refresh_rate(refresh_rate_hz: Option<u32>) -> String {
    refresh_rate_hz
        .map(|refresh_rate_hz| format!("at {refresh_rate_hz} Hz"))
        .unwrap_or_else(|| "at the default refresh rate".to_string())
}

impl Drop for Renderer<'_> {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: This wrapper uniquely owns the live renderer and the
            // lifetime marker keeps its window alive until after this drop.
            unsafe { SDL_DestroyRenderer(self.raw) }
        }
    }
}

pub struct KeyboardState {
    keys: [u8; SDL_NUM_SCANCODES],
}

impl KeyboardState {
    pub fn is_pressed(&self, scancode: usize) -> bool {
        self.keys.get(scancode).copied().unwrap_or(0) != 0
    }

    #[cfg(test)]
    pub(crate) fn from_keys(pressed_keys: &[usize]) -> Self {
        let mut keys = [0; SDL_NUM_SCANCODES];
        for key in pressed_keys.iter().copied() {
            if key < SDL_NUM_SCANCODES {
                keys[key] = 1;
            }
        }
        Self { keys }
    }
}

pub struct Texture<'renderer> {
    raw: *mut SDL_Texture,
    width: usize,
    height: usize,
    _renderer: PhantomData<&'renderer SDL_Renderer>,
}

impl Texture<'_> {
    pub fn new_rgba_streaming<'renderer>(
        renderer: &'renderer Renderer<'_>,
        width: i32,
        height: i32,
    ) -> Result<Texture<'renderer>> {
        let stored_width = usize::try_from(width)
            .ok()
            .filter(|width| *width > 0)
            .ok_or_else(|| "texture width must be positive".to_string())?;
        let stored_height = usize::try_from(height)
            .ok()
            .filter(|height| *height > 0)
            .ok_or_else(|| "texture height must be positive".to_string())?;
        // SAFETY: The renderer is live, dimensions are positive SDL integers,
        // and format/access are documented constants.
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
        Ok(Texture {
            raw,
            width: stored_width,
            height: stored_height,
            _renderer: PhantomData,
        })
    }

    pub fn update_rgba(&self, pixels: &[u8], pitch: usize) -> Result<()> {
        let row_bytes = self
            .width
            .checked_mul(4)
            .ok_or_else(|| "RGBA row size overflowed usize".to_string())?;
        if pitch < row_bytes {
            return Err(format!(
                "RGBA pitch {pitch} is smaller than the {row_bytes}-byte row"
            ));
        }
        let required_bytes = pitch
            .checked_mul(self.height)
            .ok_or_else(|| "RGBA buffer size overflowed usize".to_string())?;
        if pixels.len() < required_bytes {
            return Err(format!(
                "RGBA buffer has {} bytes but {required_bytes} are required",
                pixels.len()
            ));
        }
        let pitch = c_int::try_from(pitch)
            .map_err(|_| "RGBA pitch does not fit SDL's integer range".to_string())?;
        // SAFETY: The buffer checks above prove SDL can read `height` rows of
        // `pitch` bytes. SDL_UpdateTexture copies them before returning.
        let result = unsafe {
            SDL_UpdateTexture(
                self.raw,
                ptr::null(),
                pixels.as_ptr().cast::<c_void>(),
                pitch,
            )
        };
        if result != 0 {
            return Err(last_error());
        }
        Ok(())
    }
}

impl Drop for Texture<'_> {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: This wrapper uniquely owns the texture and its lifetime
            // marker keeps the renderer alive until after this drop.
            unsafe { SDL_DestroyTexture(self.raw) }
        }
    }
}

pub struct AudioDevice<'sdl> {
    id: u32,
    channels: u8,
    _sdl: PhantomData<&'sdl Sdl>,
}

impl<'sdl> AudioDevice<'sdl> {
    pub fn open_queue_playback_mono(
        _sdl: &'sdl Sdl,
        sample_rate: u32,
        sample_buffer: u16,
    ) -> Result<Self> {
        let sample_rate = c_int::try_from(sample_rate)
            .ok()
            .filter(|sample_rate| *sample_rate > 0)
            .ok_or_else(|| "audio sample rate must fit a positive SDL integer".to_string())?;
        let desired = SDL_AudioSpec {
            freq: sample_rate,
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

        // SAFETY: Both specs have SDL_AudioSpec's exact C layout and live for
        // the call; null selects the default playback device and no callback is used.
        let id = unsafe { SDL_OpenAudioDevice(ptr::null(), 0, &desired, &mut obtained, 0) };
        if id == 0 {
            return Err(last_error());
        }
        if obtained.format != AUDIO_S16SYS || obtained.channels != 1 || obtained.freq != sample_rate
        {
            // SAFETY: A nonzero id is a live device uniquely owned here. It is
            // closed before returning the format mismatch.
            unsafe { SDL_CloseAudioDevice(id) };
            return Err(format!(
                "SDL audio format mismatch: requested {sample_rate}Hz mono s16, got {}Hz {}ch format {}",
                obtained.freq, obtained.channels, obtained.format
            ));
        }

        Ok(Self {
            id,
            channels: obtained.channels,
            _sdl: PhantomData,
        })
    }

    pub fn resume(&self) {
        // SAFETY: Successful construction guarantees a live audio device id;
        // zero is SDL's documented value for unpausing.
        unsafe { SDL_PauseAudioDevice(self.id, 0) }
    }

    pub fn queue_i16(&self, samples: &[i16]) -> Result<()> {
        // SAFETY: Every initialized i16 consists of initialized bytes, u8 has
        // weaker alignment, and size_of_val supplies the exact byte length.
        let bytes = unsafe {
            slice::from_raw_parts(
                samples.as_ptr().cast::<u8>(),
                std::mem::size_of_val(samples),
            )
        };
        let byte_count = u32::try_from(bytes.len())
            .map_err(|_| "audio queue is larger than SDL can accept".to_string())?;
        // SAFETY: The device is live, `byte_count` matches the valid slice, and
        // SDL_QueueAudio copies the bytes before this call returns.
        let result =
            unsafe { SDL_QueueAudio(self.id, bytes.as_ptr().cast::<c_void>(), byte_count) };
        if result != 0 {
            return Err(last_error());
        }
        Ok(())
    }

    pub fn queued_samples(&self) -> usize {
        self.queued_bytes() / std::mem::size_of::<i16>() / usize::from(self.channels)
    }

    pub fn clear(&self) {
        // SAFETY: Successful construction guarantees a live audio device id.
        unsafe { SDL_ClearQueuedAudio(self.id) }
    }

    fn queued_bytes(&self) -> usize {
        // SAFETY: Successful construction guarantees a live audio device id.
        unsafe { SDL_GetQueuedAudioSize(self.id) as usize }
    }
}

impl Drop for AudioDevice<'_> {
    fn drop(&mut self) {
        if self.id != 0 {
            // SAFETY: This wrapper uniquely owns the live audio device and its
            // Sdl lifetime marker keeps SDL initialized through this drop.
            unsafe { SDL_CloseAudioDevice(self.id) }
        }
    }
}

fn last_error() -> String {
    // SAFETY: SDL_GetError returns a nullable pointer to SDL-owned storage.
    let ptr = unsafe { SDL_GetError() };
    if ptr.is_null() {
        return "SDL reported an unknown error".to_string();
    }
    // SAFETY: Null was rejected above. SDL guarantees a NUL-terminated error
    // string, which is copied before any subsequent SDL call can replace it.
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .unwrap_or("SDL returned non-UTF-8 error text")
        .to_string()
}

fn clear_error() {
    // SAFETY: Clearing SDL's thread-local error state has no preconditions and
    // lets callers distinguish a legitimate zero-valued sample from failure.
    unsafe { SDL_ClearError() }
}

fn current_error() -> Option<String> {
    // SAFETY: SDL_GetError returns a nullable pointer to SDL-owned storage.
    let ptr = unsafe { SDL_GetError() };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: Null was rejected. SDL guarantees a NUL-terminated error string,
    // which is copied before any later SDL call can replace it.
    let error = unsafe { CStr::from_ptr(ptr) }.to_string_lossy();
    if error.is_empty() {
        None
    } else {
        Some(error.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;
    use std::mem::{align_of, size_of};
    use std::ptr;

    use super::{
        decode_input_device_event, display_mode_info, ensure_diagnostic_identity_matches,
        ensure_diagnostic_snapshot_unchanged, guid_bytes_to_string, missing_controller_mapping,
        non_negative_axis, DeviceIndex, InputDeviceEvent, JoystickInstanceId,
        SDL_ControllerDeviceEvent, SDL_DisplayMode, SDL_Event, SDL_JoyDeviceEvent,
        SDL_CONTROLLERDEVICEADDED, SDL_CONTROLLERDEVICEREMAPPED, SDL_CONTROLLERDEVICEREMOVED,
        SDL_JOYDEVICEADDED, SDL_JOYDEVICEREMOVED,
    };

    #[test]
    fn event_union_matches_sdl2_size_and_pointer_alignment() {
        assert_eq!(size_of::<SDL_Event>(), 56);
        assert_eq!(align_of::<SDL_Event>(), align_of::<*mut c_void>());
        assert_eq!(size_of::<SDL_ControllerDeviceEvent>(), 12);
        assert_eq!(size_of::<SDL_JoyDeviceEvent>(), 12);
        assert_eq!(align_of::<SDL_ControllerDeviceEvent>(), align_of::<u32>());
        assert_eq!(align_of::<SDL_JoyDeviceEvent>(), align_of::<u32>());
    }

    #[test]
    fn joystick_guid_bytes_use_sdl_lowercase_hex_notation() {
        assert_eq!(
            guid_bytes_to_string([
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
                0x0e, 0x0f,
            ]),
            "000102030405060708090a0b0c0d0e0f"
        );
    }

    #[test]
    fn diagnostics_reject_device_identity_changes_instead_of_mixing_fields() {
        let device_index = DeviceIndex::new(0).unwrap();
        let original = JoystickInstanceId::new(17).unwrap();
        let replacement = JoystickInstanceId::new(18).unwrap();

        let error =
            ensure_diagnostic_identity_matches(device_index, original, replacement).unwrap_err();

        assert!(error.contains("moved from instance 17 to 18"));
        assert!(ensure_diagnostic_snapshot_unchanged(
            &[(device_index, original)],
            &[(device_index, replacement)]
        )
        .is_err());
    }

    #[test]
    fn mapping_query_errors_are_not_reported_as_an_unmapped_device() {
        let device_index = DeviceIndex::new(3).unwrap();

        let error = missing_controller_mapping(
            device_index,
            false,
            Some("device disconnected".to_string()),
        )
        .unwrap_err();

        assert!(error.contains("device disconnected"));
        assert_eq!(
            missing_controller_mapping(device_index, false, None).unwrap(),
            None
        );
    }

    #[test]
    fn typed_device_events_keep_transient_indices_and_stable_ids_distinct() {
        let cases = [
            (
                SDL_CONTROLLERDEVICEADDED,
                InputDeviceEvent::MappedAdded(DeviceIndex::new(7).unwrap()),
            ),
            (
                SDL_CONTROLLERDEVICEREMOVED,
                InputDeviceEvent::MappedRemoved(super::JoystickInstanceId::new(7).unwrap()),
            ),
            (
                SDL_CONTROLLERDEVICEREMAPPED,
                InputDeviceEvent::MappedRemapped(super::JoystickInstanceId::new(7).unwrap()),
            ),
            (
                SDL_JOYDEVICEADDED,
                InputDeviceEvent::RawAdded(DeviceIndex::new(7).unwrap()),
            ),
            (
                SDL_JOYDEVICEREMOVED,
                InputDeviceEvent::RawRemoved(super::JoystickInstanceId::new(7).unwrap()),
            ),
        ];

        for (event_type, expected) in cases {
            assert_eq!(
                decode_input_device_event(event_type, 7).unwrap(),
                Some(expected)
            );
        }
    }

    #[test]
    fn negative_device_identifiers_are_rejected_at_the_safe_boundary() {
        assert!(DeviceIndex::new(-1).is_err());
        assert!(decode_input_device_event(SDL_CONTROLLERDEVICEADDED, -1).is_err());
        assert!(decode_input_device_event(SDL_CONTROLLERDEVICEREMOVED, -1).is_err());
    }

    #[test]
    fn mapped_trigger_samples_never_expose_negative_values() {
        assert_eq!(non_negative_axis(-1), 0);
        assert_eq!(non_negative_axis(0), 0);
        assert_eq!(non_negative_axis(i16::MAX), i16::MAX as u16);
    }

    #[test]
    fn display_mode_conversion_preserves_unspecified_refresh_rate() {
        let mode = display_mode_info(SDL_DisplayMode {
            format: 0,
            w: 3840,
            h: 2160,
            refresh_rate: 0,
            driverdata: ptr::null_mut(),
        })
        .unwrap();

        assert_eq!(mode.width, 3840);
        assert_eq!(mode.height, 2160);
        assert_eq!(mode.refresh_rate_hz, None);
    }

    #[test]
    fn display_mode_conversion_rejects_non_positive_dimensions() {
        let mode = display_mode_info(SDL_DisplayMode {
            format: 0,
            w: 0,
            h: 2160,
            refresh_rate: 144,
            driverdata: ptr::null_mut(),
        });

        assert_eq!(mode, None);
    }
}
