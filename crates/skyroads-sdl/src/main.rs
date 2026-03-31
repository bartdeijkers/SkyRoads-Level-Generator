mod sdl;

use std::env;
use std::path::{Path, PathBuf};
use std::process;
use std::thread;
use std::time::{Duration, Instant};

use sdl::{scancode, AudioDevice, Color, Joystick, Rect, Renderer, Sdl, Texture, Window};
use skyroads_audio_ref::{AttractAudioAssets, AudioMixer};
use skyroads_core::{
    controller_state_from_dos_joystick, controller_state_from_dos_mouse, AppInput, AppMode,
    AttractModeApp, AudioCommand, ControlMode, ControllerState, DisplayMode, DisplaySettings,
    RenderScene, ShipState,
};
use skyroads_data::{
    levels_from_roads_archive, load_cfg_or_default, load_demo_rec_path, load_roads_lzs_path,
    save_cfg_path, SkyroadsCfg,
};
use skyroads_renderer_ref::{AttractModeAssets, DebugViewMode, ReferenceRenderer};

type Result<T> = std::result::Result<T, String>;

const WINDOW_WIDTH: i32 = 1280;
const WINDOW_HEIGHT: i32 = 960;
const SIMULATION_HZ: u64 = 70;
const MAX_CATCH_UP_STEPS: usize = 4;
const AUDIO_DEVICE_BUFFER_SAMPLES: u16 = 1024;
const AUDIO_QUEUE_LOW_WATER_SAMPLES: usize = 2048;
const AUDIO_QUEUE_TARGET_SAMPLES: usize = 4096;
const FRAMEBUFFER_WIDTH: i32 = 320;
const FRAMEBUFFER_HEIGHT: i32 = 200;
const GAMEPLAY_SMOKE_INTRO_SKIP_TICKS: usize = 40;
const GAMEPLAY_SMOKE_MIN_GAMEPLAY_TICKS: usize = 8;
const GAMEPLAY_SMOKE_TIMEOUT_TICKS: usize = 180;
const DOS_MOUSE_RECENTER_X: i32 = FRAMEBUFFER_WIDTH / 2;
const DOS_MOUSE_CENTER_Y: i32 = FRAMEBUFFER_HEIGHT / 2;

#[derive(Debug, Clone)]
struct LaunchConfig {
    source_root: PathBuf,
    automation: Option<AutomationMode>,
    display_settings: DisplaySettings,
}

#[derive(Debug, Clone, Copy, Default)]
struct KeyEdges {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    debug_toggle: bool,
    enter: bool,
    escape: bool,
    space: bool,
    quit: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct HostInput {
    app: AppInput,
    debug_toggle: bool,
    toggle_fullscreen: bool,
    quit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutomationMode {
    GameplaySmoke,
}

#[derive(Debug, Clone, Copy, Default)]
struct GameplaySmokeAutomation {
    total_ticks: usize,
    sent_intro_skip: bool,
    sent_go_menu_open: bool,
    sent_level_start: bool,
    gameplay_ticks: usize,
    saw_throttle: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct KeyLatch {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    debug_toggle: bool,
    enter: bool,
    escape: bool,
    space: bool,
    quit: bool,
}

impl KeyLatch {
    fn sample(&mut self, keyboard: sdl::KeyboardState<'_>) -> HostInput {
        let shift_held =
            keyboard.is_pressed(scancode::LSHIFT) || keyboard.is_pressed(scancode::RSHIFT);
        let raw_enter = keyboard.is_pressed(scancode::RETURN);
        let current = KeyEdges {
            up: keyboard.is_pressed(scancode::UP) || keyboard.is_pressed(scancode::W),
            down: keyboard.is_pressed(scancode::DOWN) || keyboard.is_pressed(scancode::S),
            left: keyboard.is_pressed(scancode::LEFT) || keyboard.is_pressed(scancode::A),
            right: keyboard.is_pressed(scancode::RIGHT) || keyboard.is_pressed(scancode::D),
            debug_toggle: keyboard.is_pressed(scancode::TAB),
            enter: raw_enter,
            escape: keyboard.is_pressed(scancode::ESCAPE),
            space: keyboard.is_pressed(scancode::SPACE),
            quit: keyboard.is_pressed(scancode::Q),
        };
        let up = take_edge(&mut self.up, current.up);
        let down = take_edge(&mut self.down, current.down);
        let left = take_edge(&mut self.left, current.left);
        let right = take_edge(&mut self.right, current.right);
        let debug_toggle = take_edge(&mut self.debug_toggle, current.debug_toggle);
        let enter_edge = take_edge(&mut self.enter, current.enter);
        let escape = take_edge(&mut self.escape, current.escape);
        let space = take_edge(&mut self.space, current.space);
        let quit = take_edge(&mut self.quit, current.quit);
        let toggle_fullscreen = enter_edge && shift_held;
        let enter = enter_edge && !shift_held;

        HostInput {
            debug_toggle,
            toggle_fullscreen,
            app: AppInput {
                up,
                down,
                left,
                right,
                enter,
                escape,
                space,
                up_held: current.up,
                down_held: current.down,
                left_held: current.left,
                right_held: current.right,
                enter_held: current.enter && !shift_held,
                space_held: current.space,
                gameplay_controls_override: None,
            },
            quit,
        }
    }
}

impl GameplaySmokeAutomation {
    fn next_input(&mut self, mode: AppMode) -> AppInput {
        self.total_ticks += 1;
        match mode {
            AppMode::Intro
                if !self.sent_intro_skip && self.total_ticks >= GAMEPLAY_SMOKE_INTRO_SKIP_TICKS =>
            {
                self.sent_intro_skip = true;
                AppInput {
                    space: true,
                    ..AppInput::default()
                }
            }
            AppMode::MainMenu if self.sent_intro_skip && !self.sent_go_menu_open => {
                self.sent_go_menu_open = true;
                AppInput {
                    enter: true,
                    ..AppInput::default()
                }
            }
            AppMode::GoMenu if self.sent_go_menu_open && !self.sent_level_start => {
                self.sent_level_start = true;
                AppInput {
                    enter: true,
                    ..AppInput::default()
                }
            }
            AppMode::Gameplay => AppInput {
                up_held: true,
                ..AppInput::default()
            },
            _ => AppInput::default(),
        }
    }

    fn observe(&mut self, mode: AppMode, scene: &RenderScene) -> Result<Option<String>> {
        if mode == AppMode::Gameplay {
            let RenderScene::Gameplay(scene) = scene else {
                return Err("app entered gameplay mode without a gameplay render scene".to_string());
            };
            self.gameplay_ticks += 1;
            self.saw_throttle |= scene.ship.accel_input == 1;
            if scene.ship.state != ShipState::Alive {
                return Err(format!(
                    "gameplay smoke test reached gameplay, but ship state became {:?}",
                    scene.ship.state
                ));
            }
            if self.gameplay_ticks >= GAMEPLAY_SMOKE_MIN_GAMEPLAY_TICKS {
                if !self.saw_throttle {
                    return Err(
                        "gameplay smoke test reached gameplay, but throttle never latched"
                            .to_string(),
                    );
                }
                return Ok(Some(format!(
                    "gameplay smoke ok: frame={} row={} z={:.6} accel={} state={:?}",
                    scene.frame_index,
                    scene.current_row,
                    scene.snapshot.z_position,
                    scene.ship.accel_input,
                    scene.ship.state
                )));
            }
        }

        if self.total_ticks >= GAMEPLAY_SMOKE_TIMEOUT_TICKS {
            return Err(format!(
                "gameplay smoke test timed out after {} ticks in mode {:?}",
                self.total_ticks, mode
            ));
        }

        Ok(None)
    }
}

fn take_edge(previous: &mut bool, current: bool) -> bool {
    let edge = current && !*previous;
    *previous = current;
    edge
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let config = parse_args()?;
    let roads = load_roads_lzs_path(config.source_root.join("ROADS.LZS"))
        .map_err(|error| error.to_string())?;
    let demo = load_demo_rec_path(config.source_root.join("DEMO.REC"))
        .map_err(|error| error.to_string())?;
    let levels = levels_from_roads_archive(&roads);
    if levels.is_empty() {
        return Err("ROADS.LZS did not contain any playable levels".to_string());
    }

    let renderer_assets = AttractModeAssets::load_from_root(&config.source_root)
        .map_err(|error| error.to_string())?;
    let reference_renderer = ReferenceRenderer::new(renderer_assets);
    let audio_assets = AttractAudioAssets::load_from_root(&config.source_root)
        .map_err(|error| error.to_string())?;
    let mut audio_mixer = AudioMixer::new(audio_assets);
    let mut app = AttractModeApp::new(levels, demo);
    let cfg_path = config.source_root.join("SKYROADS.CFG");
    let mut last_saved_cfg = load_cfg_or_default(&cfg_path).map_err(|error| error.to_string())?;
    app.apply_cfg(&last_saved_cfg);
    app.set_display_settings(config.display_settings);

    let sdl = Sdl::init()?;
    let window = Window::new("SkyRoads Native", WINDOW_WIDTH, WINDOW_HEIGHT)?;
    apply_window_mode(&window, app.display_settings())?;
    let presenter = Renderer::new(&window)?;
    let texture = Texture::new_rgba_streaming(&presenter, 320, 200)?;
    let joystick = Joystick::open_first()?;
    let audio_device = AudioDevice::open_queue_playback_mono(
        audio_mixer.output_sample_rate(),
        AUDIO_DEVICE_BUFFER_SAMPLES,
    )?;

    let initial = app.tick(AppInput::default());
    apply_audio_commands(&mut audio_mixer, &audio_device, &initial.audio_commands)?;
    fill_audio_queue(&audio_device, &mut audio_mixer)?;
    audio_device.resume();

    let mut current_mode = initial.mode;
    let mut current_scene = initial.render_scene;
    let mut debug_view = DebugViewMode::Off;
    let mut applied_display_settings = app.display_settings();
    window.set_title(&window_title(current_mode, debug_view))?;

    if config.automation == Some(AutomationMode::GameplaySmoke) {
        println!("SkyRoads automated gameplay smoke test");
        println!("assets: {}", config.source_root.display());
        return run_gameplay_smoke(
            &sdl,
            &window,
            &presenter,
            &texture,
            &reference_renderer,
            &mut app,
            &cfg_path,
            &mut last_saved_cfg,
            &mut audio_mixer,
            &audio_device,
            current_mode,
            current_scene,
        );
    }

    print_controls(&config.source_root);

    let timestep = Duration::from_nanos(1_000_000_000 / SIMULATION_HZ);
    let mut next_tick = Instant::now() + timestep;
    let mut latch = KeyLatch::default();

    loop {
        if sdl.poll_quit_requested() {
            break;
        }
        sdl.pump_events();
        let mut display_rect = window_display_rect(&window);
        let mut input = latch.sample(sdl.keyboard_state());
        let control_mode = app.control_mode();
        if input.quit {
            break;
        }
        if input.toggle_fullscreen {
            let mut display_settings = app.display_settings();
            display_settings.fullscreen = !display_settings.fullscreen;
            app.set_display_settings(display_settings);
        }
        if input.debug_toggle {
            debug_view = debug_view.next();
            window.set_title(&window_title(current_mode, debug_view))?;
        }
        if current_mode == AppMode::Gameplay {
            match control_mode {
                ControlMode::Keyboard => {}
                ControlMode::Mouse => {
                    let mouse = sdl.mouse_state();
                    input.app.gameplay_controls_override = Some(dos_mouse_controls(
                        mouse.x,
                        mouse.y,
                        mouse.buttons,
                        display_rect,
                    ));
                    recenter_dos_mouse_x(&window, mouse.y, display_rect);
                }
                ControlMode::Joystick => {
                    if let Some(joystick) = joystick.as_ref() {
                        let state = joystick.state();
                        input.app.gameplay_controls_override = Some(dos_joystick_controls(state));
                    }
                }
            }
        }

        let mut step_count = 0usize;
        let mut consumed_input = false;
        let now = Instant::now();
        while now >= next_tick && step_count < MAX_CATCH_UP_STEPS {
            let app_input = if consumed_input {
                held_only_input(input.app)
            } else {
                consumed_input = true;
                input.app
            };
            let tick = app.tick(app_input);
            apply_audio_commands(&mut audio_mixer, &audio_device, &tick.audio_commands)?;
            sync_cfg_if_changed(&cfg_path, &mut last_saved_cfg, &app)?;
            if tick.mode != current_mode {
                current_mode = tick.mode;
                window.set_title(&window_title(current_mode, debug_view))?;
                if app.control_mode() == ControlMode::Mouse && current_mode == AppMode::Gameplay {
                    center_dos_mouse_for_gameplay(&window, display_rect);
                }
            }
            current_scene = tick.render_scene;
            let display_settings = app.display_settings();
            if display_settings != applied_display_settings {
                apply_window_mode(&window, display_settings)?;
                applied_display_settings = display_settings;
                display_rect = window_display_rect(&window);
                if app.control_mode() == ControlMode::Mouse && current_mode == AppMode::Gameplay {
                    center_dos_mouse_for_gameplay(&window, display_rect);
                }
            }
            next_tick += timestep;
            step_count += 1;
        }
        if now > next_tick + timestep {
            next_tick = now + timestep;
        }

        display_rect = window_display_rect(&window);
        fill_audio_queue(&audio_device, &mut audio_mixer)?;
        present_scene(
            &presenter,
            &texture,
            &reference_renderer,
            &current_scene,
            debug_view,
            display_rect,
        )?;

        let sleep_for = next_tick
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(2));
        if !sleep_for.is_zero() {
            thread::sleep(sleep_for);
        }
    }

    Ok(())
}

fn run_gameplay_smoke(
    sdl: &Sdl,
    window: &Window,
    presenter: &Renderer,
    texture: &Texture,
    renderer: &ReferenceRenderer,
    app: &mut AttractModeApp,
    cfg_path: &Path,
    last_saved_cfg: &mut SkyroadsCfg,
    audio_mixer: &mut AudioMixer,
    audio_device: &AudioDevice,
    mut current_mode: AppMode,
    mut current_scene: RenderScene,
) -> Result<()> {
    let mut smoke = GameplaySmokeAutomation::default();
    let mut display_rect = window_display_rect(window);
    present_scene(
        presenter,
        texture,
        renderer,
        &current_scene,
        DebugViewMode::Off,
        display_rect,
    )?;

    loop {
        if sdl.poll_quit_requested() {
            return Err("SDL quit requested before gameplay smoke test completed".to_string());
        }
        sdl.pump_events();

        let input = smoke.next_input(current_mode);
        let tick = app.tick(input);
        apply_audio_commands(audio_mixer, audio_device, &tick.audio_commands)?;
        sync_cfg_if_changed(cfg_path, last_saved_cfg, app)?;
        if tick.mode != current_mode {
            current_mode = tick.mode;
            window.set_title(&window_title(current_mode, DebugViewMode::Off))?;
        }
        current_scene = tick.render_scene;
        display_rect = window_display_rect(window);
        present_scene(
            presenter,
            texture,
            renderer,
            &current_scene,
            DebugViewMode::Off,
            display_rect,
        )?;

        if let Some(summary) = smoke.observe(current_mode, &current_scene)? {
            println!("{summary}");
            return Ok(());
        }
    }
}

fn present_scene(
    presenter: &Renderer,
    texture: &Texture,
    renderer: &ReferenceRenderer,
    scene: &RenderScene,
    debug_view: DebugViewMode,
    display_rect: Rect,
) -> Result<()> {
    let frame = renderer.render_scene_with_debug(scene, debug_view);
    texture.update_rgba(&frame.pixels_rgba, usize::from(frame.width) * 4)?;
    presenter.set_draw_color(Color::rgb(0, 0, 0))?;
    presenter.clear()?;
    presenter.copy_texture(texture, display_rect)?;
    presenter.present();
    Ok(())
}

fn apply_audio_commands(
    mixer: &mut AudioMixer,
    audio_device: &AudioDevice,
    commands: &[AudioCommand],
) -> Result<()> {
    if commands.is_empty() {
        return Ok(());
    }
    mixer.apply_commands(commands);
    if commands_require_flush(commands) {
        audio_device.clear();
    }
    fill_audio_queue(audio_device, mixer)
}

fn sync_cfg_if_changed(
    cfg_path: &Path,
    last_saved_cfg: &mut SkyroadsCfg,
    app: &AttractModeApp,
) -> Result<()> {
    let cfg = app.cfg_snapshot();
    if cfg == *last_saved_cfg {
        return Ok(());
    }
    save_cfg_path(cfg_path, &cfg).map_err(|error| error.to_string())?;
    *last_saved_cfg = cfg;
    Ok(())
}

fn fill_audio_queue(audio_device: &AudioDevice, mixer: &mut AudioMixer) -> Result<()> {
    let queued = audio_device.queued_samples();
    if queued >= AUDIO_QUEUE_LOW_WATER_SAMPLES {
        return Ok(());
    }
    let needed = AUDIO_QUEUE_TARGET_SAMPLES.saturating_sub(queued);
    if needed == 0 {
        return Ok(());
    }
    let samples = mixer.render_i16(needed);
    audio_device.queue_i16(&samples)
}

fn parse_args() -> Result<LaunchConfig> {
    let mut source_root = None;
    let mut automation = None;
    let mut display_settings = DisplaySettings::default();

    for arg in env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => return Err(usage().to_string()),
            "--smoke-gameplay" => automation = Some(AutomationMode::GameplaySmoke),
            "--fullscreen" => display_settings.fullscreen = true,
            "--borderless" => display_settings.borderless = true,
            _ if arg.starts_with('-') => {
                return Err(format!("unknown option: {arg}\n{}", usage()));
            }
            _ => {
                if source_root.replace(PathBuf::from(arg)).is_some() {
                    return Err(usage().to_string());
                }
            }
        }
    }

    Ok(LaunchConfig {
        source_root: source_root.unwrap_or_else(|| PathBuf::from(".")),
        automation,
        display_settings,
    })
}

fn usage() -> &'static str {
    "usage: cargo run -p skyroads-sdl -- [--smoke-gameplay] [--fullscreen] [--borderless] [source_root]"
}

fn print_controls(source_root: &Path) {
    println!("SkyRoads native attract-mode demo");
    println!("assets: {}", source_root.display());
    println!("controls:");
    println!("  Up / Down  menu navigation, level select, settings menu, keyboard throttle/brake");
    println!("  Left / Right  steer, level select, settings menu");
    println!("  Enter      select, start level, retry after crash, return after win");
    println!("  Space      skip intro, start level, jump, retry after crash, return after win");
    println!("  Shift+Enter toggle fullscreen on/off");
    println!("  Tab        cycle debug views");
    println!("  Escape     back to previous menu, exit gameplay to level select");
    println!("  Q          quit");
    println!("settings menu modes:");
    println!("  keyboard   arrow keys + enter/space");
    println!("  joystick   first SDL joystick/gamepad axis 0/1 + button 0");
    println!("  mouse      DOS-style mouse thresholds");
    println!("display modes:");
    println!("  fullscreen  desktop fullscreen");
    println!("  borderless  centered 1280x960 borderless window");
    println!("mouse mode:");
    println!("  move mouse left/right  steer");
    println!("  move mouse up/down     throttle/brake");
    println!("  any mouse button       jump");
}

fn window_title(mode: AppMode, debug_view: DebugViewMode) -> String {
    let label = match mode {
        AppMode::Intro => "Intro",
        AppMode::MainMenu => "Main Menu",
        AppMode::HelpMenu => "Help",
        AppMode::SettingsMenu => "Settings",
        AppMode::DemoPlayback => "Demo",
        AppMode::Boot => "Boot",
        AppMode::GoMenu => "Level Select",
        AppMode::Gameplay => "Gameplay",
    };
    if debug_view == DebugViewMode::Off {
        format!("SkyRoads Native | {label}")
    } else {
        format!("SkyRoads Native | {label} | Debug {}", debug_view.label())
    }
}

fn commands_require_flush(commands: &[AudioCommand]) -> bool {
    commands.iter().any(|command| {
        matches!(
            command,
            AudioCommand::PlaySong(_)
                | AudioCommand::StopSong
                | AudioCommand::PlayIntroSample
                | AudioCommand::StopAllSamples
        )
    })
}

fn apply_window_mode(window: &Window, settings: DisplaySettings) -> Result<()> {
    match settings.active_mode() {
        DisplayMode::Fullscreen => window.set_fullscreen_desktop(true)?,
        DisplayMode::Windowed | DisplayMode::Borderless => {
            window.set_fullscreen_desktop(false)?;
            window.set_bordered(settings.active_mode() == DisplayMode::Windowed);
            window.set_size(WINDOW_WIDTH, WINDOW_HEIGHT);
            window.center();
        }
    }
    Ok(())
}

fn window_display_rect(window: &Window) -> Rect {
    let (window_width, window_height) = window.size();
    fit_rect_with_aspect(window_width, window_height, WINDOW_WIDTH, WINDOW_HEIGHT)
}

fn fit_rect_with_aspect(
    window_width: i32,
    window_height: i32,
    content_width: i32,
    content_height: i32,
) -> Rect {
    if window_width <= 0 || window_height <= 0 || content_width <= 0 || content_height <= 0 {
        return Rect {
            x: 0,
            y: 0,
            w: content_width.max(1),
            h: content_height.max(1),
        };
    }

    let width_from_height =
        i64::from(window_height) * i64::from(content_width) / i64::from(content_height);
    let (display_width, display_height) = if width_from_height <= i64::from(window_width) {
        (width_from_height as i32, window_height)
    } else {
        let height_from_width =
            i64::from(window_width) * i64::from(content_height) / i64::from(content_width);
        (window_width, height_from_width as i32)
    };

    Rect {
        x: (window_width - display_width) / 2,
        y: (window_height - display_height) / 2,
        w: display_width.max(1),
        h: display_height.max(1),
    }
}

fn held_only_input(input: AppInput) -> AppInput {
    AppInput {
        up_held: input.up_held,
        down_held: input.down_held,
        left_held: input.left_held,
        right_held: input.right_held,
        enter_held: input.enter_held,
        space_held: input.space_held,
        gameplay_controls_override: input.gameplay_controls_override,
        ..AppInput::default()
    }
}

fn dos_mouse_controls(
    mouse_x: i32,
    mouse_y: i32,
    buttons: u32,
    display_rect: Rect,
) -> ControllerState {
    let (framebuffer_x, framebuffer_y) = framebuffer_mouse_position(mouse_x, mouse_y, display_rect);
    controller_state_from_dos_mouse(framebuffer_x, framebuffer_y, buttons as u16)
}

fn dos_joystick_controls(state: sdl::JoystickState) -> ControllerState {
    let raw_x = (i32::from(state.x_axis) + 32_768).clamp(0, 65_535) as u16;
    let raw_y = (i32::from(state.y_axis) + 32_768).clamp(0, 65_535) as u16;
    controller_state_from_dos_joystick(raw_x, raw_y, state.jump_pressed)
}

fn framebuffer_mouse_position(mouse_x: i32, mouse_y: i32, display_rect: Rect) -> (u16, u16) {
    let local_x = (mouse_x - display_rect.x).clamp(0, display_rect.w.saturating_sub(1));
    let local_y = (mouse_y - display_rect.y).clamp(0, display_rect.h.saturating_sub(1));
    let framebuffer_x = (local_x * FRAMEBUFFER_WIDTH / display_rect.w.max(1)) as u16;
    let framebuffer_y = (local_y * FRAMEBUFFER_HEIGHT / display_rect.h.max(1)) as u16;
    (framebuffer_x, framebuffer_y)
}

fn recenter_dos_mouse_x(window: &Window, mouse_y: i32, display_rect: Rect) {
    let clamped_y = mouse_y.clamp(
        display_rect.y,
        display_rect.y + display_rect.h.saturating_sub(1),
    );
    let center_x = display_rect.x + display_rect.w / 2;
    window.warp_mouse(center_x, clamped_y);
}

fn center_dos_mouse_for_gameplay(window: &Window, display_rect: Rect) {
    let center_x = display_rect.x + DOS_MOUSE_RECENTER_X * display_rect.w / FRAMEBUFFER_WIDTH;
    let center_y = display_rect.y + DOS_MOUSE_CENTER_Y * display_rect.h / FRAMEBUFFER_HEIGHT;
    window.warp_mouse(center_x, center_y);
}

#[cfg(test)]
mod tests {
    use super::{scancode, sdl, KeyLatch};

    fn keyboard(keys: &[usize]) -> sdl::KeyboardState<'static> {
        let mut state = vec![0u8; scancode::RSHIFT + 1];
        for key in keys {
            state[*key] = 1;
        }
        let leaked = Box::leak(state.into_boxed_slice());
        sdl::KeyboardState::from_keys(leaked)
    }

    #[test]
    fn shift_enter_toggles_fullscreen_without_triggering_enter() {
        let mut latch = KeyLatch::default();
        let sample = latch.sample(keyboard(&[scancode::LSHIFT, scancode::RETURN]));

        assert!(sample.toggle_fullscreen);
        assert!(!sample.app.enter);
        assert!(!sample.app.enter_held);
    }

    #[test]
    fn plain_enter_still_maps_to_app_enter() {
        let mut latch = KeyLatch::default();
        let sample = latch.sample(keyboard(&[scancode::RETURN]));

        assert!(!sample.toggle_fullscreen);
        assert!(sample.app.enter);
        assert!(sample.app.enter_held);
    }
}
