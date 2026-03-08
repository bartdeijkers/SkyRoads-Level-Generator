mod sdl;

use std::env;
use std::path::{Path, PathBuf};
use std::process;
use std::thread;
use std::time::{Duration, Instant};

use sdl::{scancode, AudioDevice, Color, Rect, Renderer, Sdl, Texture, Window};
use skyroads_audio_ref::{AttractAudioAssets, AudioMixer};
use skyroads_core::{AppInput, AppMode, AttractModeApp, AudioCommand, RenderScene};
use skyroads_data::{levels_from_roads_archive, load_demo_rec_path, load_roads_lzs_path};
use skyroads_renderer_ref::{AttractModeAssets, DebugViewMode, ReferenceRenderer};

type Result<T> = std::result::Result<T, String>;

const WINDOW_WIDTH: i32 = 1280;
const WINDOW_HEIGHT: i32 = 960;
const SIMULATION_HZ: u64 = 70;
const MAX_CATCH_UP_STEPS: usize = 4;
const AUDIO_DEVICE_BUFFER_SAMPLES: u16 = 1024;
const AUDIO_QUEUE_LOW_WATER_SAMPLES: usize = 2048;
const AUDIO_QUEUE_TARGET_SAMPLES: usize = 4096;

#[derive(Debug, Clone)]
struct LaunchConfig {
    source_root: PathBuf,
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
    quit: bool,
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
        let current = KeyEdges {
            up: keyboard.is_pressed(scancode::UP) || keyboard.is_pressed(scancode::W),
            down: keyboard.is_pressed(scancode::DOWN) || keyboard.is_pressed(scancode::S),
            left: keyboard.is_pressed(scancode::LEFT) || keyboard.is_pressed(scancode::A),
            right: keyboard.is_pressed(scancode::RIGHT) || keyboard.is_pressed(scancode::D),
            debug_toggle: keyboard.is_pressed(scancode::TAB),
            enter: keyboard.is_pressed(scancode::RETURN),
            escape: keyboard.is_pressed(scancode::ESCAPE),
            space: keyboard.is_pressed(scancode::SPACE),
            quit: keyboard.is_pressed(scancode::Q),
        };
        let up = take_edge(&mut self.up, current.up);
        let down = take_edge(&mut self.down, current.down);
        let left = take_edge(&mut self.left, current.left);
        let right = take_edge(&mut self.right, current.right);
        let debug_toggle = take_edge(&mut self.debug_toggle, current.debug_toggle);
        let enter = take_edge(&mut self.enter, current.enter);
        let escape = take_edge(&mut self.escape, current.escape);
        let space = take_edge(&mut self.space, current.space);
        let quit = take_edge(&mut self.quit, current.quit);

        HostInput {
            debug_toggle,
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
                enter_held: current.enter,
                space_held: current.space,
            },
            quit,
        }
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

    let sdl = Sdl::init()?;
    let window = Window::new("SkyRoads Native", WINDOW_WIDTH, WINDOW_HEIGHT)?;
    let presenter = Renderer::new(&window)?;
    let texture = Texture::new_rgba_streaming(&presenter, 320, 200)?;
    let audio_device = AudioDevice::open_queue_playback_mono(
        audio_mixer.output_sample_rate(),
        AUDIO_DEVICE_BUFFER_SAMPLES,
    )?;

    print_controls(&config.source_root);

    let initial = app.tick(AppInput::default());
    apply_audio_commands(&mut audio_mixer, &audio_device, &initial.audio_commands)?;
    fill_audio_queue(&audio_device, &mut audio_mixer)?;
    audio_device.resume();

    let mut current_mode = initial.mode;
    let mut current_scene = initial.render_scene;
    let mut debug_view = DebugViewMode::Off;
    window.set_title(&window_title(current_mode, debug_view))?;

    let display_rect = Rect {
        x: 0,
        y: 0,
        w: WINDOW_WIDTH,
        h: WINDOW_HEIGHT,
    };
    let timestep = Duration::from_nanos(1_000_000_000 / SIMULATION_HZ);
    let mut next_tick = Instant::now() + timestep;
    let mut latch = KeyLatch::default();

    loop {
        if sdl.poll_quit_requested() {
            break;
        }
        sdl.pump_events();
        let input = latch.sample(sdl.keyboard_state());
        if input.quit {
            break;
        }
        if input.debug_toggle {
            debug_view = debug_view.next();
            window.set_title(&window_title(current_mode, debug_view))?;
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
            if tick.mode != current_mode {
                current_mode = tick.mode;
                window.set_title(&window_title(current_mode, debug_view))?;
            }
            current_scene = tick.render_scene;
            next_tick += timestep;
            step_count += 1;
        }
        if now > next_tick + timestep {
            next_tick = now + timestep;
        }

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
    let mut args = env::args().skip(1);
    let Some(first) = args.next() else {
        return Ok(LaunchConfig {
            source_root: PathBuf::from("."),
        });
    };
    if first == "-h" || first == "--help" {
        return Err("usage: cargo run -p skyroads-sdl -- [source_root]".to_string());
    }
    if args.next().is_some() {
        return Err("usage: cargo run -p skyroads-sdl -- [source_root]".to_string());
    }
    Ok(LaunchConfig {
        source_root: PathBuf::from(first),
    })
}

fn print_controls(source_root: &Path) {
    println!("SkyRoads native attract-mode demo");
    println!("assets: {}", source_root.display());
    println!("controls:");
    println!("  Up / Down  menu navigation, throttle, brake");
    println!("  Left / Right  steer");
    println!("  Enter      select, restart after crash/win");
    println!("  Space      skip intro, jump, restart after crash/win");
    println!("  Tab        cycle debug views");
    println!("  Escape     back to menu");
    println!("  Q          quit");
}

fn window_title(mode: AppMode, debug_view: DebugViewMode) -> String {
    let label = match mode {
        AppMode::Intro => "Intro",
        AppMode::MainMenu => "Main Menu",
        AppMode::HelpMenu => "Help",
        AppMode::SettingsMenu => "Settings",
        AppMode::DemoPlayback => "Demo",
        AppMode::Boot => "Boot",
        AppMode::GoMenu => "Go",
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

fn held_only_input(input: AppInput) -> AppInput {
    AppInput {
        up_held: input.up_held,
        down_held: input.down_held,
        left_held: input.left_held,
        right_held: input.right_held,
        enter_held: input.enter_held,
        space_held: input.space_held,
        ..AppInput::default()
    }
}
