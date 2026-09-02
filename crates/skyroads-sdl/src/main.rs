mod display_preferences;
mod sdl;

use std::env;
use std::path::{Path, PathBuf};
use std::process;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sdl::{
    scancode, AudioDevice, Color, DisplayInfo, DisplayModeInfo, Joystick, Rect, Renderer, Sdl,
    Texture, Window,
};
use skyroads_audio_ref::{AttractAudioAssets, AudioMixer};
use skyroads_core::{
    controller_state_from_dos_joystick, controller_state_from_dos_mouse, AppInput, AppMode,
    AttractModeApp, AudioCommand, ControlMode, ControllerState, DisplayMode, DisplayModeCatalog,
    DisplaySettings, RenderScene, ShipState, VideoMode,
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
const DISPLAY_PREFERENCES_FILENAME: &str = "SKYROADS-RS-DISPLAY.CFG";

#[derive(Debug, Clone)]
struct LaunchConfig {
    source_root: PathBuf,
    automation: Option<AutomationMode>,
    display_mode_override: Option<DisplayMode>,
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
    pending_app_input: AppInput,
}

impl KeyLatch {
    fn sample(&mut self, keyboard: sdl::KeyboardState) -> HostInput {
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

        self.pending_app_input.up |= up;
        self.pending_app_input.down |= down;
        self.pending_app_input.left |= left;
        self.pending_app_input.right |= right;
        self.pending_app_input.enter |= enter;
        self.pending_app_input.escape |= escape;
        self.pending_app_input.space |= space;
        self.pending_app_input.up_held = current.up;
        self.pending_app_input.down_held = current.down;
        self.pending_app_input.left_held = current.left;
        self.pending_app_input.right_held = current.right;
        self.pending_app_input.enter_held = current.enter && !shift_held;
        self.pending_app_input.space_held = current.space;

        HostInput {
            debug_toggle,
            toggle_fullscreen,
            app: self.pending_app_input,
            quit,
        }
    }

    fn consume_app_edges(&mut self) {
        self.pending_app_input = held_only_input(self.pending_app_input);
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

fn music_random_seed() -> u32 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    elapsed.as_secs() as u32 ^ elapsed.subsec_nanos()
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
    if config.automation.is_none() {
        app.set_music_random_seed(music_random_seed());
    }
    let cfg_path = config.source_root.join("SKYROADS.CFG");
    let mut last_saved_cfg = load_cfg_or_default(&cfg_path).map_err(|error| error.to_string())?;
    app.apply_cfg(&last_saved_cfg);

    let sdl = Sdl::init()?;
    let window = Window::new(&sdl, "SkyRoads Native", WINDOW_WIDTH, WINDOW_HEIGHT)?;
    let mut display_info = configure_display_modes(&window, &mut app);
    let display_preferences_path = config.source_root.join(DISPLAY_PREFERENCES_FILENAME);
    let saved_display_settings =
        if config.automation.is_none() && config.display_mode_override.is_none() {
            match display_preferences::load(&display_preferences_path) {
                Ok(settings) => settings,
                Err(error) => {
                    eprintln!("warning: {error}; using borderless desktop");
                    None
                }
            }
        } else {
            None
        };
    if let Some(mode) = config.display_mode_override {
        select_initial_display_mode(&mut app, mode);
    } else if let Some(settings) = saved_display_settings {
        if !app.set_display_settings(settings) {
            eprintln!(
                "warning: saved exclusive display mode is no longer available; using borderless desktop"
            );
        }
    }
    window.show();
    let mut applied_display_settings = apply_window_mode_with_fallback(&window, &mut app, None)?;
    if config.automation.is_none()
        && config.display_mode_override.is_none()
        && saved_display_settings != Some(applied_display_settings)
    {
        save_display_preferences(&display_preferences_path, applied_display_settings);
    }
    let mut last_saved_display_settings = applied_display_settings;
    let presenter = Renderer::new(&window)?;
    let joystick = Joystick::open_first(&sdl)?;
    let audio_device = AudioDevice::open_queue_playback_mono(
        &sdl,
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
    window.set_title(&window_title(current_mode, debug_view))?;
    print_display_diagnostics(&sdl, display_info.as_ref(), &app, &presenter);

    if config.automation == Some(AutomationMode::GameplaySmoke) {
        println!("SkyRoads automated gameplay smoke test");
        println!("assets: {}", config.source_root.display());
        return run_gameplay_smoke(GameplaySmokeRuntime {
            sdl: &sdl,
            window: &window,
            presenter: &presenter,
            renderer: &reference_renderer,
            app: &mut app,
            cfg_path: &cfg_path,
            last_saved_cfg: &mut last_saved_cfg,
            audio_mixer: &mut audio_mixer,
            audio_device: &audio_device,
            current_mode,
            current_scene,
        });
    }

    let mut texture = Texture::new_rgba_streaming(&presenter, 320, 200)?;
    print_controls(&config.source_root);

    let timestep = Duration::from_nanos(1_000_000_000 / SIMULATION_HZ);
    let mut next_tick = Instant::now() + timestep;
    let mut latch = KeyLatch::default();

    loop {
        let frame_started = Instant::now();
        let pending_events = sdl.poll_events();
        if pending_events.quit_requested {
            break;
        }
        if pending_events.renderer_reset {
            texture = Texture::new_rgba_streaming(&presenter, 320, 200)?;
        }
        let mut input_rect = window_input_rect(&window);
        if pending_events.display_changed {
            display_info = configure_display_modes(&window, &mut app);
            print_display_diagnostics(&sdl, display_info.as_ref(), &app, &presenter);
            if app.display_settings() != applied_display_settings {
                applied_display_settings = apply_window_mode_with_fallback(
                    &window,
                    &mut app,
                    Some(applied_display_settings),
                )?;
                texture = Texture::new_rgba_streaming(&presenter, 320, 200)?;
                current_scene = app.render_scene();
                input_rect = window_input_rect(&window);
            }
        }
        let mut input = latch.sample(sdl.keyboard_state());
        let control_mode = app.control_mode();
        if input.quit {
            break;
        }
        if input.toggle_fullscreen {
            app.toggle_fullscreen();
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
                        input_rect,
                    ));
                    recenter_dos_mouse_x(&window, mouse.y, input_rect);
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
                latch.consume_app_edges();
                input.app
            };
            let tick = app.tick(app_input);
            apply_audio_commands(&mut audio_mixer, &audio_device, &tick.audio_commands)?;
            sync_cfg_if_changed(&cfg_path, &mut last_saved_cfg, &app)?;
            if tick.mode != current_mode {
                current_mode = tick.mode;
                window.set_title(&window_title(current_mode, debug_view))?;
                if app.control_mode() == ControlMode::Mouse && current_mode == AppMode::Gameplay {
                    center_dos_mouse_for_gameplay(&window, input_rect);
                }
            }
            current_scene = tick.render_scene;
            let display_settings = app.display_settings();
            if display_settings != applied_display_settings {
                applied_display_settings = apply_window_mode_with_fallback(
                    &window,
                    &mut app,
                    Some(applied_display_settings),
                )?;
                texture = Texture::new_rgba_streaming(&presenter, 320, 200)?;
                current_scene = app.render_scene();
                input_rect = window_input_rect(&window);
                if app.control_mode() == ControlMode::Mouse && current_mode == AppMode::Gameplay {
                    center_dos_mouse_for_gameplay(&window, input_rect);
                }
            }
            if config.automation.is_none()
                && applied_display_settings != last_saved_display_settings
            {
                save_display_preferences(&display_preferences_path, applied_display_settings);
                last_saved_display_settings = applied_display_settings;
            }
            next_tick += timestep;
            step_count += 1;
        }
        if now > next_tick + timestep {
            next_tick = now + timestep;
        }

        let display_rect = renderer_display_rect(&presenter)?;
        fill_audio_queue(&audio_device, &mut audio_mixer)?;
        present_scene(
            &presenter,
            &texture,
            &reference_renderer,
            &current_scene,
            debug_view,
            display_rect,
        )?;

        if !presenter.vsync_enabled() {
            let sleep_for = presentation_interval(&app).saturating_sub(frame_started.elapsed());
            if !sleep_for.is_zero() {
                thread::sleep(sleep_for);
            }
        }
    }

    Ok(())
}

struct GameplaySmokeRuntime<'a, 'sdl, 'window> {
    sdl: &'a Sdl,
    window: &'a Window<'sdl>,
    presenter: &'a Renderer<'window>,
    renderer: &'a ReferenceRenderer,
    app: &'a mut AttractModeApp,
    cfg_path: &'a Path,
    last_saved_cfg: &'a mut SkyroadsCfg,
    audio_mixer: &'a mut AudioMixer,
    audio_device: &'a AudioDevice<'sdl>,
    current_mode: AppMode,
    current_scene: RenderScene,
}

fn run_gameplay_smoke(runtime: GameplaySmokeRuntime<'_, '_, '_>) -> Result<()> {
    let GameplaySmokeRuntime {
        sdl,
        window,
        presenter,
        renderer,
        app,
        cfg_path,
        last_saved_cfg,
        audio_mixer,
        audio_device,
        mut current_mode,
        mut current_scene,
    } = runtime;
    let mut texture = Texture::new_rgba_streaming(presenter, 320, 200)?;
    let mut smoke = GameplaySmokeAutomation::default();
    let mut display_rect = renderer_display_rect(presenter)?;
    present_scene(
        presenter,
        &texture,
        renderer,
        &current_scene,
        DebugViewMode::Off,
        display_rect,
    )?;

    loop {
        let pending_events = sdl.poll_events();
        if pending_events.quit_requested {
            return Err("SDL quit requested before gameplay smoke test completed".to_string());
        }
        if pending_events.renderer_reset {
            texture = Texture::new_rgba_streaming(presenter, 320, 200)?;
        }

        let input = smoke.next_input(current_mode);
        let tick = app.tick(input);
        apply_audio_commands(audio_mixer, audio_device, &tick.audio_commands)?;
        sync_cfg_if_changed(cfg_path, last_saved_cfg, app)?;
        if tick.mode != current_mode {
            current_mode = tick.mode;
            window.set_title(&window_title(current_mode, DebugViewMode::Off))?;
        }
        current_scene = tick.render_scene;
        display_rect = renderer_display_rect(presenter)?;
        present_scene(
            presenter,
            &texture,
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
    let pixels_rgba = frame.to_rgba();
    texture.update_rgba(&pixels_rgba, usize::from(frame.width) * 4)?;
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
    parse_args_from(env::args().skip(1))
}

fn parse_args_from(args: impl IntoIterator<Item = String>) -> Result<LaunchConfig> {
    let mut source_root = None;
    let mut automation = None;
    let mut display_mode_override = None;

    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => return Err(usage().to_string()),
            "--smoke-gameplay" => automation = Some(AutomationMode::GameplaySmoke),
            "--fullscreen" | "--borderless" => {
                display_mode_override = Some(DisplayMode::BorderlessDesktop);
            }
            "--exclusive-fullscreen" => {
                display_mode_override = Some(DisplayMode::ExclusiveFullscreen);
            }
            "--windowed" => {
                display_mode_override = Some(DisplayMode::Windowed);
            }
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

    if automation.is_some() && display_mode_override.is_none() {
        display_mode_override = Some(DisplayMode::Windowed);
    }

    Ok(LaunchConfig {
        source_root: source_root.unwrap_or_else(|| PathBuf::from(".")),
        automation,
        display_mode_override,
    })
}

fn usage() -> &'static str {
    "usage: cargo run -p skyroads-sdl -- [--smoke-gameplay] [--windowed|--fullscreen|--borderless|--exclusive-fullscreen] [source_root]"
}

fn print_controls(source_root: &Path) {
    println!("SkyRoads native attract-mode demo");
    println!("assets: {}", source_root.display());
    println!("controls:");
    println!("  Up / Down  menu navigation, level select, settings menu, keyboard throttle/brake");
    println!("  Left / Right  steer, level select, settings menu");
    println!("  Enter      select, start level, retry after crash, return after win");
    println!("  Space      skip intro, start level, jump, retry after crash, return after win");
    println!("  Shift+Enter toggle the last fullscreen mode/windowed");
    println!("  Tab        cycle debug views");
    println!("  Escape     back to previous menu, exit gameplay to level select");
    println!("  Q          quit");
    println!("settings menu modes:");
    println!("  keyboard   arrow keys + enter/space");
    println!("  joystick   first SDL joystick/gamepad axis 0/1 + button 0");
    println!("  mouse      DOS-style mouse thresholds");
    println!("display modes:");
    println!("  borderless  current desktop resolution (default)");
    println!("  exclusive   selected SDL resolution and refresh rate");
    println!("  windowed    centered 1280x960 window");
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

fn display_mode_catalog(display_info: &DisplayInfo) -> Result<DisplayModeCatalog> {
    let desktop_mode = video_mode(display_info.desktop_mode)?;
    let fullscreen_modes = display_info
        .fullscreen_modes
        .iter()
        .copied()
        .map(video_mode)
        .collect::<Result<Vec<_>>>()?;
    Ok(DisplayModeCatalog::new(desktop_mode, fullscreen_modes))
}

fn configure_display_modes(window: &Window, app: &mut AttractModeApp) -> Option<DisplayInfo> {
    let display_info = match window.display_info() {
        Ok(display_info) => display_info,
        Err(error) => {
            eprintln!(
                "warning: could not discover exclusive display modes: {error}; borderless and windowed modes remain available"
            );
            app.configure_display_modes(DisplayModeCatalog::default());
            return None;
        }
    };
    let catalog = match display_mode_catalog(&display_info) {
        Ok(catalog) => catalog,
        Err(error) => {
            eprintln!(
                "warning: could not use discovered exclusive display modes: {error}; borderless and windowed modes remain available"
            );
            app.configure_display_modes(DisplayModeCatalog::default());
            return None;
        }
    };
    app.configure_display_modes(catalog);
    Some(display_info)
}

fn save_display_preferences(path: &Path, settings: DisplaySettings) {
    if let Err(error) = display_preferences::save(path, settings) {
        eprintln!("warning: {error}; continuing without persisted display preferences");
    }
}

fn video_mode(mode: DisplayModeInfo) -> Result<VideoMode> {
    VideoMode::new(mode.width, mode.height, mode.refresh_rate_hz).ok_or_else(|| {
        format!(
            "SDL reported invalid display mode {}x{} {:?} Hz",
            mode.width, mode.height, mode.refresh_rate_hz
        )
    })
}

fn sdl_display_mode(mode: VideoMode) -> DisplayModeInfo {
    DisplayModeInfo {
        width: mode.width(),
        height: mode.height(),
        refresh_rate_hz: mode.refresh_hz(),
    }
}

fn select_initial_display_mode(app: &mut AttractModeApp, mode: DisplayMode) {
    let settings = match mode {
        DisplayMode::Windowed => DisplaySettings::Windowed,
        DisplayMode::BorderlessDesktop => DisplaySettings::BorderlessDesktop,
        DisplayMode::ExclusiveFullscreen => app
            .selected_video_mode()
            .map(DisplaySettings::ExclusiveFullscreen)
            .unwrap_or(DisplaySettings::BorderlessDesktop),
    };
    let _ = app.set_display_settings(settings);
}

fn apply_window_mode_with_fallback(
    window: &Window,
    app: &mut AttractModeApp,
    previous: Option<DisplaySettings>,
) -> Result<DisplaySettings> {
    let requested = app.display_settings();
    let mut candidates = vec![requested];
    if let Some(previous) = previous {
        candidates.push(previous);
    }
    candidates.push(DisplaySettings::BorderlessDesktop);
    candidates.push(DisplaySettings::Windowed);

    let mut attempted = Vec::new();
    let mut last_error = None;
    for candidate in candidates {
        if attempted.contains(&candidate) {
            continue;
        }
        attempted.push(candidate);

        match apply_window_mode(window, candidate) {
            Ok(()) => {
                let accepted = app.set_display_settings(candidate);
                if accepted {
                    return Ok(candidate);
                }
                last_error = Some("core rejected a previously validated display mode".to_string());
            }
            Err(error) => {
                eprintln!("warning: could not apply {candidate:?}: {error}");
                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "no display mode could be applied".to_string()))
}

fn apply_window_mode(window: &Window, settings: DisplaySettings) -> Result<()> {
    match settings {
        DisplaySettings::Windowed => window.set_windowed(WINDOW_WIDTH, WINDOW_HEIGHT),
        DisplaySettings::BorderlessDesktop => window.set_borderless_desktop(),
        DisplaySettings::ExclusiveFullscreen(mode) => {
            window.set_exclusive_fullscreen(sdl_display_mode(mode))
        }
    }
}

fn window_input_rect(window: &Window) -> Rect {
    let (window_width, window_height) = window.size();
    fit_rect_with_aspect(window_width, window_height, WINDOW_WIDTH, WINDOW_HEIGHT)
}

fn renderer_display_rect(renderer: &Renderer) -> Result<Rect> {
    let (output_width, output_height) = renderer.output_size()?;
    Ok(fit_rect_with_aspect(
        output_width,
        output_height,
        WINDOW_WIDTH,
        WINDOW_HEIGHT,
    ))
}

fn presentation_interval(app: &AttractModeApp) -> Duration {
    let active_refresh_hz = app
        .display_settings()
        .video_mode()
        .and_then(VideoMode::refresh_hz);
    let desktop_refresh_hz = app
        .display_mode_catalog()
        .desktop_mode()
        .and_then(VideoMode::refresh_hz);
    let refresh_hz = active_refresh_hz
        .or(desktop_refresh_hz)
        .unwrap_or(60)
        .max(1);
    Duration::from_nanos(1_000_000_000 / u64::from(refresh_hz))
}

fn print_display_diagnostics(
    sdl: &Sdl,
    display_info: Option<&DisplayInfo>,
    app: &AttractModeApp,
    renderer: &Renderer,
) {
    println!("SDL: {} ({})", sdl.version(), sdl.video_driver());
    if let Some(display_info) = display_info {
        println!(
            "display: {} desktop {}",
            display_info.name,
            format_display_mode(display_info.desktop_mode)
        );
        println!(
            "exclusive modes: {} suitable of {} readable ({} skipped); selected {}",
            app.display_mode_catalog().modes().len(),
            display_info.fullscreen_modes.len(),
            display_info.skipped_fullscreen_modes,
            app.selected_video_mode()
                .map(format_video_mode)
                .unwrap_or_else(|| "unavailable".to_string())
        );
    } else {
        println!("display: exclusive mode discovery unavailable");
    }
    println!(
        "presentation: {}",
        if renderer.vsync_enabled() {
            "vsync"
        } else {
            "manual refresh-rate limiter"
        }
    );
}

fn format_display_mode(mode: DisplayModeInfo) -> String {
    format!(
        "{}x{} {}",
        mode.width,
        mode.height,
        mode.refresh_rate_hz
            .map(|refresh_hz| format!("{refresh_hz}Hz"))
            .unwrap_or_else(|| "default Hz".to_string())
    )
}

fn format_video_mode(mode: VideoMode) -> String {
    format_display_mode(sdl_display_mode(mode))
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
    use super::{
        fit_rect_with_aspect, parse_args_from, scancode, sdl, AutomationMode, DisplayMode,
        KeyLatch, Rect,
    };

    fn keyboard(keys: &[usize]) -> sdl::KeyboardState {
        sdl::KeyboardState::from_keys(keys)
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

    #[test]
    fn menu_edge_remains_pending_until_a_simulation_tick_consumes_it() {
        let mut latch = KeyLatch::default();

        let pressed = latch.sample(keyboard(&[scancode::DOWN]));
        assert!(pressed.app.down);

        let released_before_tick = latch.sample(keyboard(&[]));
        assert!(released_before_tick.app.down);
        assert!(!released_before_tick.app.down_held);

        latch.consume_app_edges();
        let after_tick = latch.sample(keyboard(&[]));
        assert!(!after_tick.app.down);
    }

    #[test]
    fn consumed_menu_edge_does_not_repeat_while_the_key_stays_held() {
        let mut latch = KeyLatch::default();

        assert!(latch.sample(keyboard(&[scancode::DOWN])).app.down);
        latch.consume_app_edges();

        let still_held = latch.sample(keyboard(&[scancode::DOWN]));
        assert!(!still_held.app.down);
        assert!(still_held.app.down_held);
    }

    #[test]
    fn interactive_launch_defaults_to_borderless_desktop() {
        let config = parse_args_from(Vec::<String>::new()).unwrap();

        assert_eq!(config.display_mode_override, None);
        assert_eq!(config.automation, None);
    }

    #[test]
    fn headless_smoke_defaults_to_windowed_but_respects_explicit_mode() {
        let smoke = parse_args_from(["--smoke-gameplay".to_string()]).unwrap();
        assert_eq!(smoke.automation, Some(AutomationMode::GameplaySmoke));
        assert_eq!(smoke.display_mode_override, Some(DisplayMode::Windowed));

        let exclusive = parse_args_from([
            "--smoke-gameplay".to_string(),
            "--exclusive-fullscreen".to_string(),
        ])
        .unwrap();
        assert_eq!(
            exclusive.display_mode_override,
            Some(DisplayMode::ExclusiveFullscreen)
        );
    }

    #[test]
    fn legacy_fullscreen_flag_selects_borderless_desktop() {
        let config = parse_args_from(["--fullscreen".to_string()]).unwrap();

        assert_eq!(
            config.display_mode_override,
            Some(DisplayMode::BorderlessDesktop)
        );
    }

    #[test]
    fn modern_widescreen_outputs_keep_the_four_by_three_presentation_centered() {
        for (width, height, expected) in [
            (
                1920,
                1080,
                Rect {
                    x: 240,
                    y: 0,
                    w: 1440,
                    h: 1080,
                },
            ),
            (
                2560,
                1440,
                Rect {
                    x: 320,
                    y: 0,
                    w: 1920,
                    h: 1440,
                },
            ),
            (
                3840,
                2160,
                Rect {
                    x: 480,
                    y: 0,
                    w: 2880,
                    h: 2160,
                },
            ),
        ] {
            assert_eq!(fit_rect_with_aspect(width, height, 1280, 960), expected);
        }
    }
}
