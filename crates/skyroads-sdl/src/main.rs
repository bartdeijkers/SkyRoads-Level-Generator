mod controller_manager;
mod display_preferences;
mod gamepad;
mod input_preferences;
mod sdl;

use std::env;
use std::path::{Path, PathBuf};
use std::process;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use controller_manager::{ControllerEventOutcome, ControllerManager, ControllerSample};
use gamepad::{GamepadLatch, GamepadSnapshot, MenuHeld};
use sdl::{
    scancode, AudioDevice, Color, DisplayInfo, DisplayModeInfo, Rect, Renderer, Sdl, Texture,
    Window,
};
use skyroads_audio_ref::{AttractAudioAssets, AudioMixer};
use skyroads_core::{
    AppInput, AppMode, AttractModeApp, AudioCommand, ControlMode, DisplayMode, DisplayModeCatalog,
    DisplaySettings, InputTuning, RenderScene, ShipState, VideoMode,
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
const INPUT_PREFERENCES_FILENAME: &str = "SKYROADS-RS-INPUT.CFG";

#[derive(Debug, Clone)]
struct LaunchConfig {
    source_root: PathBuf,
    automation: Option<AutomationMode>,
    display_mode_override: Option<DisplayMode>,
    controller_diagnostics: bool,
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
    menu_held: MenuHeld,
    menu_space_held: bool,
    suppress_keyboard_confirm: bool,
    debug_toggle: bool,
    quit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutomationMode {
    GameplaySmoke,
    GamepadSmoke,
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
struct GamepadSmokeAutomation {
    total_ticks: usize,
    previous_mode: Option<AppMode>,
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

#[derive(Debug, Clone, Copy, Default)]
struct MenuInputLatch {
    previous: MenuHeld,
    previous_keyboard_enter: bool,
    previous_keyboard_space: bool,
    previous_gamepad: MenuHeld,
    gamepad_confirm_seen_in_group: bool,
    current_confirm_action: Option<ConfirmAction>,
    pending_edges: AppInput,
    pending_keyboard_menu: PendingKeyboardMenuEdges,
    pending_keyboard_enter: PendingKeyboardEdge,
    pending_keyboard_space: PendingKeyboardEdge,
    gamepad_south_release_required: bool,
    toggle_fullscreen_requested: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmAction {
    Enter,
    Space,
    ToggleFullscreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PendingKeyboardEdge {
    #[default]
    None,
    CurrentGroup,
    // At least one accepted keyboard contribution belongs to an older or
    // completed group. A current contribution may share the same output bit,
    // but a controller lifecycle change must preserve the protected one.
    ProtectedGroup,
}

impl PendingKeyboardEdge {
    fn latch_current_group(&mut self) {
        if *self == Self::None {
            *self = Self::CurrentGroup;
        }
    }

    fn protect_current_group(&mut self) {
        if *self == Self::CurrentGroup {
            *self = Self::ProtectedGroup;
        }
    }

    fn keep_after_discontinuity(&mut self, matching_gamepad: bool) -> bool {
        if matching_gamepad {
            return self.remove_current_group();
        }
        *self != Self::None
    }

    fn remove_current_group(&mut self) -> bool {
        if *self == Self::CurrentGroup {
            *self = Self::None;
        }
        *self != Self::None
    }

    fn consume(&mut self) {
        *self = Self::None;
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PendingKeyboardMenuEdges {
    up: PendingKeyboardEdge,
    down: PendingKeyboardEdge,
    left: PendingKeyboardEdge,
    right: PendingKeyboardEdge,
    back: PendingKeyboardEdge,
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
            menu_held: MenuHeld {
                up: current.up,
                down: current.down,
                left: current.left,
                right: current.right,
                confirm: current.enter,
                back: current.escape,
            },
            menu_space_held: current.space,
            suppress_keyboard_confirm: shift_held && current.enter,
            debug_toggle,
            app: self.pending_app_input,
            quit,
        }
    }

    fn consume_app_edges(&mut self) {
        self.pending_app_input = held_only_input(self.pending_app_input);
    }
}

impl MenuInputLatch {
    fn sample(
        &mut self,
        keyboard: MenuHeld,
        gamepad: MenuHeld,
        keyboard_space_held: bool,
        suppress_keyboard_confirm: bool,
        app_mode: AppMode,
    ) -> AppInput {
        let current = keyboard.combined_with(gamepad);
        let keyboard_enter_pressed = take_edge(&mut self.previous_keyboard_enter, keyboard.confirm);
        let keyboard_space_pressed =
            take_edge(&mut self.previous_keyboard_space, keyboard_space_held);
        let gamepad_confirm_pressed =
            take_edge(&mut self.previous_gamepad.confirm, gamepad.confirm);
        let all_confirm_sources_released =
            !keyboard.confirm && !keyboard_space_held && !gamepad.confirm;
        if all_confirm_sources_released {
            self.current_confirm_action = None;
            self.gamepad_confirm_seen_in_group = false;
            self.pending_keyboard_enter.protect_current_group();
            self.pending_keyboard_space.protect_current_group();
        }
        self.gamepad_confirm_seen_in_group |= gamepad.confirm;

        let up_pressed = take_edge(&mut self.previous.up, current.up);
        let down_pressed = take_edge(&mut self.previous.down, current.down);
        let left_pressed = take_edge(&mut self.previous.left, current.left);
        let right_pressed = take_edge(&mut self.previous.right, current.right);
        let back_pressed = take_edge(&mut self.previous.back, current.back);
        latch_keyboard_menu_edge(
            &mut self.pending_edges.up,
            &mut self.pending_keyboard_menu.up,
            up_pressed,
            keyboard.up,
            current.up,
        );
        latch_keyboard_menu_edge(
            &mut self.pending_edges.down,
            &mut self.pending_keyboard_menu.down,
            down_pressed,
            keyboard.down,
            current.down,
        );
        latch_keyboard_menu_edge(
            &mut self.pending_edges.left,
            &mut self.pending_keyboard_menu.left,
            left_pressed,
            keyboard.left,
            current.left,
        );
        latch_keyboard_menu_edge(
            &mut self.pending_edges.right,
            &mut self.pending_keyboard_menu.right,
            right_pressed,
            keyboard.right,
            current.right,
        );
        latch_keyboard_menu_edge(
            &mut self.pending_edges.escape,
            &mut self.pending_keyboard_menu.back,
            back_pressed,
            keyboard.back,
            current.back,
        );
        self.previous_gamepad = gamepad;

        let keyboard_enter_selects = keyboard_enter_pressed && !suppress_keyboard_confirm;
        let fullscreen_chord_pressed = keyboard_enter_pressed && suppress_keyboard_confirm;
        self.latch_confirm_action(
            keyboard_enter_selects,
            gamepad_confirm_pressed,
            fullscreen_chord_pressed,
            keyboard_space_pressed,
            app_mode,
        );

        self.pending_edges
    }

    fn latch_confirm_action(
        &mut self,
        keyboard_enter_pressed: bool,
        gamepad_confirm_pressed: bool,
        fullscreen_chord_pressed: bool,
        keyboard_space_pressed: bool,
        app_mode: AppMode,
    ) {
        let select_source_pressed = keyboard_enter_pressed || gamepad_confirm_pressed;
        if self.current_confirm_action.is_none() {
            if select_source_pressed {
                self.pending_edges.enter = true;
                if keyboard_enter_pressed {
                    self.latch_keyboard_confirm_action(ConfirmAction::Enter);
                }
                self.current_confirm_action = Some(ConfirmAction::Enter);
            } else if fullscreen_chord_pressed {
                self.toggle_fullscreen_requested = true;
                self.current_confirm_action = Some(ConfirmAction::ToggleFullscreen);
            } else if keyboard_space_pressed {
                let action = space_confirm_action(app_mode);
                self.latch_keyboard_confirm_action(action);
                self.current_confirm_action = Some(action);
            }
            return;
        }

        let independent_keyboard_action = if self.gamepad_confirm_seen_in_group {
            None
        } else if keyboard_enter_pressed {
            Some(ConfirmAction::Enter)
        } else if fullscreen_chord_pressed {
            Some(ConfirmAction::ToggleFullscreen)
        } else if keyboard_space_pressed {
            Some(space_confirm_action(app_mode))
        } else {
            None
        };
        if let Some(action) = independent_keyboard_action {
            self.latch_independent_keyboard_action(action);
        }

        let space_activity_needs_main_menu_select = self.current_confirm_action
            == Some(ConfirmAction::Space)
            && app_mode == AppMode::MainMenu
            && select_source_pressed;
        if space_activity_needs_main_menu_select {
            self.pending_edges.space = self.pending_keyboard_space.remove_current_group();
            self.pending_edges.enter = true;
            if keyboard_enter_pressed {
                self.latch_keyboard_confirm_action(ConfirmAction::Enter);
            }
            self.current_confirm_action = Some(ConfirmAction::Enter);
        }
    }

    fn latch_independent_keyboard_action(&mut self, action: ConfirmAction) {
        // Any older keyboard action is independent of a delayed gamepad view
        // of this new action. Protect it before the new owner is recorded.
        self.pending_keyboard_enter.protect_current_group();
        self.pending_keyboard_space.protect_current_group();

        match action {
            ConfirmAction::ToggleFullscreen => self.toggle_fullscreen_requested = true,
            ConfirmAction::Enter | ConfirmAction::Space => {
                self.latch_keyboard_confirm_action(action);
            }
        }
        self.current_confirm_action = Some(action);
    }

    fn latch_keyboard_confirm_action(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::Enter => {
                self.pending_edges.enter = true;
                self.pending_keyboard_enter.latch_current_group();
            }
            ConfirmAction::Space => {
                self.pending_edges.space = true;
                self.pending_keyboard_space.latch_current_group();
            }
            ConfirmAction::ToggleFullscreen => {
                unreachable!("a keyboard confirm action cannot be the fullscreen chord")
            }
        }
    }

    fn consume_app_edges(&mut self) {
        self.pending_edges = AppInput::default();
        self.pending_keyboard_menu = PendingKeyboardMenuEdges::default();
        self.pending_keyboard_enter.consume();
        self.pending_keyboard_space.consume();
    }

    fn take_toggle_fullscreen_request(&mut self) -> bool {
        std::mem::take(&mut self.toggle_fullscreen_requested)
    }

    fn gameplay_snapshot(
        &mut self,
        snapshot: GamepadSnapshot,
        device_state_observed: bool,
    ) -> GamepadSnapshot {
        let fullscreen_owns_current_south = self.current_confirm_action
            == Some(ConfirmAction::ToggleFullscreen)
            && snapshot.south_pressed;
        if fullscreen_owns_current_south {
            self.gamepad_south_release_required = true;
        } else if device_state_observed && !snapshot.south_pressed {
            // Error and disconnect paths publish synthetic neutral input. Only
            // an actual device sample can prove the physical button released.
            self.gamepad_south_release_required = false;
        }

        GamepadSnapshot {
            south_pressed: snapshot.south_pressed && !self.gamepad_south_release_required,
            ..snapshot
        }
    }

    fn rebase_after_gamepad_discontinuity(
        &mut self,
        keyboard: MenuHeld,
        gamepad: MenuHeld,
        keyboard_space_held: bool,
        suppress_keyboard_confirm: bool,
        app_mode: AppMode,
    ) {
        let keyboard_up_pressed = keyboard.up && !self.previous.up;
        let keyboard_down_pressed = keyboard.down && !self.previous.down;
        let keyboard_left_pressed = keyboard.left && !self.previous.left;
        let keyboard_right_pressed = keyboard.right && !self.previous.right;
        let keyboard_back_pressed = keyboard.back && !self.previous.back;
        let keyboard_enter_pressed = keyboard.confirm && !self.previous_keyboard_enter;
        let keyboard_space_pressed = keyboard_space_held && !self.previous_keyboard_space;
        let fullscreen_chord_pressed = keyboard_enter_pressed && suppress_keyboard_confirm;
        let keyboard_with_space = MenuHeld {
            confirm: keyboard.confirm || keyboard_space_held,
            ..keyboard
        };
        let gamepad_seen_around_discontinuity = self.previous_gamepad.combined_with(gamepad);
        let matching_gamepad_view = gamepad_seen_around_discontinuity;
        let gamepad_matches_enter = matching_gamepad_view.confirm
            && self.current_confirm_action == Some(ConfirmAction::Enter);
        let gamepad_matches_space = matching_gamepad_view.confirm
            && self.current_confirm_action == Some(ConfirmAction::Space);
        let pending_keyboard_enter = self
            .pending_keyboard_enter
            .keep_after_discontinuity(gamepad_matches_enter);
        let pending_keyboard_space = self
            .pending_keyboard_space
            .keep_after_discontinuity(gamepad_matches_space);
        let mut pending_keyboard_up = self
            .pending_keyboard_menu
            .up
            .keep_after_discontinuity(matching_gamepad_view.up);
        let mut pending_keyboard_down = self
            .pending_keyboard_menu
            .down
            .keep_after_discontinuity(matching_gamepad_view.down);
        let mut pending_keyboard_left = self
            .pending_keyboard_menu
            .left
            .keep_after_discontinuity(matching_gamepad_view.left);
        let mut pending_keyboard_right = self
            .pending_keyboard_menu
            .right
            .keep_after_discontinuity(matching_gamepad_view.right);
        let mut pending_keyboard_back = self
            .pending_keyboard_menu
            .back
            .keep_after_discontinuity(matching_gamepad_view.back);
        let current = keyboard.combined_with(gamepad);
        latch_keyboard_menu_edge(
            &mut pending_keyboard_up,
            &mut self.pending_keyboard_menu.up,
            keyboard_up_pressed && !matching_gamepad_view.up,
            keyboard.up,
            current.up,
        );
        latch_keyboard_menu_edge(
            &mut pending_keyboard_down,
            &mut self.pending_keyboard_menu.down,
            keyboard_down_pressed && !matching_gamepad_view.down,
            keyboard.down,
            current.down,
        );
        latch_keyboard_menu_edge(
            &mut pending_keyboard_left,
            &mut self.pending_keyboard_menu.left,
            keyboard_left_pressed && !matching_gamepad_view.left,
            keyboard.left,
            current.left,
        );
        latch_keyboard_menu_edge(
            &mut pending_keyboard_right,
            &mut self.pending_keyboard_menu.right,
            keyboard_right_pressed && !matching_gamepad_view.right,
            keyboard.right,
            current.right,
        );
        latch_keyboard_menu_edge(
            &mut pending_keyboard_back,
            &mut self.pending_keyboard_menu.back,
            keyboard_back_pressed && !matching_gamepad_view.back,
            keyboard.back,
            current.back,
        );
        self.previous = keyboard_with_space.combined_with(gamepad);
        self.previous_keyboard_enter = keyboard.confirm;
        self.previous_keyboard_space = keyboard_space_held;
        self.previous_gamepad = gamepad;
        self.gamepad_confirm_seen_in_group |= matching_gamepad_view.confirm;
        self.pending_edges = AppInput {
            up: pending_keyboard_up,
            down: pending_keyboard_down,
            left: pending_keyboard_left,
            right: pending_keyboard_right,
            enter: pending_keyboard_enter,
            escape: pending_keyboard_back,
            space: pending_keyboard_space,
            ..AppInput::default()
        };

        // A discontinuity cannot change the owner of an in-flight press group.
        // Fresh keyboard actions still go through the normal arbitration below;
        // a held replacement control only establishes a release barrier.
        let confirm_is_held = keyboard.confirm || keyboard_space_held || gamepad.confirm;
        if !confirm_is_held {
            self.current_confirm_action = None;
            self.pending_keyboard_enter.protect_current_group();
            self.pending_keyboard_space.protect_current_group();
        }

        let keyboard_enter_selects =
            keyboard_enter_pressed && !suppress_keyboard_confirm && !matching_gamepad_view.confirm;
        let keyboard_space_selects = keyboard_space_pressed && !matching_gamepad_view.confirm;
        let keyboard_fullscreen_selects =
            fullscreen_chord_pressed && !self.gamepad_confirm_seen_in_group;
        self.latch_confirm_action(
            keyboard_enter_selects,
            false,
            keyboard_fullscreen_selects,
            keyboard_space_selects,
            app_mode,
        );

        if confirm_is_held && self.current_confirm_action.is_none() {
            self.current_confirm_action = Some(ConfirmAction::Enter);
        }
    }
}

fn latch_keyboard_menu_edge(
    pending_edge: &mut bool,
    pending_keyboard_edge: &mut PendingKeyboardEdge,
    edge_pressed: bool,
    keyboard_held: bool,
    combined_held: bool,
) {
    *pending_edge |= edge_pressed;
    if edge_pressed && keyboard_held {
        pending_keyboard_edge.latch_current_group();
    }
    if !combined_held {
        pending_keyboard_edge.protect_current_group();
    }
}

fn space_confirm_action(app_mode: AppMode) -> ConfirmAction {
    if app_mode == AppMode::MainMenu {
        ConfirmAction::Space
    } else {
        ConfirmAction::Enter
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

impl GamepadSmokeAutomation {
    fn next_snapshot(&mut self, mode: AppMode) -> GamepadSnapshot {
        self.total_ticks += 1;
        if self.previous_mode != Some(mode) {
            self.previous_mode = Some(mode);
            return GamepadSnapshot::default();
        }

        let press_south = match mode {
            AppMode::Intro => {
                !self.sent_intro_skip && self.total_ticks >= GAMEPLAY_SMOKE_INTRO_SKIP_TICKS
            }
            AppMode::MainMenu => self.sent_intro_skip && !self.sent_go_menu_open,
            AppMode::GoMenu => self.sent_go_menu_open && !self.sent_level_start,
            _ => false,
        };
        if press_south {
            match mode {
                AppMode::Intro => self.sent_intro_skip = true,
                AppMode::MainMenu => self.sent_go_menu_open = true,
                AppMode::GoMenu => self.sent_level_start = true,
                _ => {}
            }
            return GamepadSnapshot {
                south_pressed: true,
                ..GamepadSnapshot::default()
            };
        }

        if mode == AppMode::Gameplay {
            return GamepadSnapshot {
                right_trigger: i16::MAX as u16,
                ..GamepadSnapshot::default()
            };
        }

        GamepadSnapshot::default()
    }

    fn observe(&mut self, mode: AppMode, scene: &RenderScene) -> Result<Option<String>> {
        if mode == AppMode::Gameplay {
            let RenderScene::Gameplay(scene) = scene else {
                return Err(
                    "gamepad smoke entered gameplay without a gameplay render scene".to_string(),
                );
            };
            self.gameplay_ticks += 1;
            self.saw_throttle |= scene.ship.accel_input == 1;
            if scene.ship.state != ShipState::Alive {
                return Err(format!(
                    "gamepad smoke reached gameplay, but ship state became {:?}",
                    scene.ship.state
                ));
            }
            if self.gameplay_ticks >= GAMEPLAY_SMOKE_MIN_GAMEPLAY_TICKS {
                if !self.saw_throttle {
                    return Err(
                        "gamepad smoke reached gameplay, but trigger throttle never latched"
                            .to_string(),
                    );
                }
                return Ok(Some(format!(
                    "gamepad smoke ok: frame={} row={} z={:.6} accel={} state={:?}",
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
                "gamepad smoke timed out after {} ticks in mode {:?}",
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
    if config.controller_diagnostics {
        return run_controller_diagnostics();
    }

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
    let input_preferences_path = config.source_root.join(INPUT_PREFERENCES_FILENAME);
    let initial_input_tuning =
        initial_input_tuning(&input_preferences_path, config.automation.is_some());
    app.set_input_tuning(initial_input_tuning);

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

    if config.automation == Some(AutomationMode::GamepadSmoke) {
        println!("SkyRoads injected logical-gamepad smoke test");
        println!("assets: {}", config.source_root.display());
        return run_gamepad_smoke(GamepadSmokeRuntime {
            sdl: &sdl,
            window: &window,
            presenter: &presenter,
            renderer: &reference_renderer,
            app: &mut app,
            audio_mixer: &mut audio_mixer,
            audio_device: &audio_device,
            current_mode,
            current_scene,
        });
    }

    let mut texture = Texture::new_rgba_streaming(&presenter, 320, 200)?;
    print_controls(&config.source_root);
    let mut controller_manager = discover_controller_manager(&sdl);
    println!("{}", controller_manager.status_line());

    let timestep = Duration::from_nanos(1_000_000_000 / SIMULATION_HZ);
    let mut next_tick = Instant::now() + timestep;
    let mut latch = KeyLatch::default();
    let mut gamepad_latch = GamepadLatch::default();
    let mut menu_input_latch = MenuInputLatch::default();
    let mut last_input_tuning = initial_input_tuning;
    let mut last_controller_sample_error = None;
    let mut warned_missing_controller = false;

    'running: loop {
        let frame_started = Instant::now();
        let pending_events = sdl.poll_events();
        if pending_events.quit_requested {
            break;
        }
        if pending_events.renderer_reset {
            texture = Texture::new_rgba_streaming(&presenter, 320, 200)?;
        }
        report_input_event_errors(&pending_events.input_errors);
        let controller_event_discontinuity = process_controller_events(
            &mut controller_manager,
            &mut gamepad_latch,
            &pending_events.input_devices,
        );
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
        let controller_sample =
            sample_controller(&mut controller_manager, &mut last_controller_sample_error);
        if controller_sample.input_discontinuity {
            gamepad_latch.reset();
        }
        let controller_input_discontinuity =
            controller_event_discontinuity || controller_sample.input_discontinuity;
        let gamepad_snapshot = controller_sample.snapshot;
        let input_tuning = app.input_tuning();
        gamepad_latch.sample(gamepad_snapshot, input_tuning.controller_sensitivity());
        let mut input = latch.sample(sdl.keyboard_state());
        if controller_input_discontinuity {
            menu_input_latch.rebase_after_gamepad_discontinuity(
                input.menu_held,
                gamepad_latch.menu_held(),
                input.menu_space_held,
                input.suppress_keyboard_confirm,
                current_mode,
            );
        }
        let menu_edges = menu_input_latch.sample(
            input.menu_held,
            gamepad_latch.menu_held(),
            input.menu_space_held,
            input.suppress_keyboard_confirm,
            current_mode,
        );
        let toggle_fullscreen = menu_input_latch.take_toggle_fullscreen_request();
        apply_menu_edges(&mut input.app, menu_edges);
        let mouse = sdl.mouse_state();
        let (mouse_x, mouse_y) = framebuffer_mouse_position(mouse.x, mouse.y, input_rect);
        app.set_input_preview(gamepad::input_activation_preview(
            mouse_x,
            mouse_y,
            gamepad_snapshot,
            input_tuning,
        ));
        let control_mode = app.control_mode();
        if input.quit {
            break;
        }
        if toggle_fullscreen {
            app.toggle_fullscreen();
        }
        if input.debug_toggle {
            debug_view = debug_view.next();
            window.set_title(&window_title(current_mode, debug_view))?;
        }
        let gameplay_gamepad_snapshot = menu_input_latch
            .gameplay_snapshot(gamepad_snapshot, controller_sample.device_state_observed);
        input.app.gameplay_controls_override = normalized_gameplay_controls(
            current_mode,
            control_mode,
            mouse_x,
            mouse_y,
            mouse.buttons != 0,
            gameplay_gamepad_snapshot,
            input_tuning,
        );
        if control_mode == ControlMode::Mouse && current_mode == AppMode::Gameplay {
            recenter_dos_mouse_x(&window, mouse.y, input_rect);
        }
        if controller_manager.is_connected() {
            warned_missing_controller = false;
        } else if current_mode == AppMode::Gameplay
            && control_mode == ControlMode::Joystick
            && !warned_missing_controller
        {
            eprintln!(
                "warning: JOYSTICK gameplay mode is selected, but no controller is detected; input remains neutral"
            );
            warned_missing_controller = true;
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
                gamepad_latch.consume_app_edges();
                menu_input_latch.consume_app_edges();
                controller_manager.acknowledge_neutral_sample();
                input.app
            };
            let tick = app.tick(app_input);
            apply_audio_commands(&mut audio_mixer, &audio_device, &tick.audio_commands)?;
            sync_cfg_if_changed(&cfg_path, &mut last_saved_cfg, &app)?;
            if tick.quit_requested {
                break 'running;
            }
            if tick.mode != current_mode {
                current_mode = tick.mode;
                window.set_title(&window_title(current_mode, debug_view))?;
                if app.control_mode() == ControlMode::Mouse && current_mode == AppMode::Gameplay {
                    center_dos_mouse_for_gameplay(&window, input_rect);
                }
            }
            current_scene = tick.render_scene;
            let changed_input_tuning = app.input_tuning() != last_input_tuning;
            if changed_input_tuning {
                let updated_tuning = app.input_tuning();
                app.set_input_preview(gamepad::input_activation_preview(
                    mouse_x,
                    mouse_y,
                    gamepad_snapshot,
                    updated_tuning,
                ));
                current_scene = app.render_scene();
                save_input_preferences(&input_preferences_path, updated_tuning);
                last_input_tuning = updated_tuning;
            }
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

fn run_controller_diagnostics() -> Result<()> {
    let sdl = Sdl::init_controller_diagnostics()?;
    println!("SkyRoads controller diagnostics");
    println!("SDL: {}", sdl.version());

    let devices = sdl.input_device_diagnostics()?;
    if devices.is_empty() {
        println!("devices: none detected");
    } else {
        for device in &devices {
            println!(
                "device: index={} mapped={} name={:?}",
                device.info.device_index.value(),
                if device.info.mapped { "yes" } else { "no" },
                device.info.name
            );
            match &device.guid {
                Ok(guid) => println!("  guid: {guid}"),
                Err(error) => println!("  guid error: {error}"),
            }
            match &device.opened {
                Ok(opened) => println!(
                    "  joystick: instance_id={} axes={} buttons={}",
                    opened.instance_id.value(),
                    opened.axis_count,
                    opened.button_count
                ),
                Err(error) => println!("  joystick probe error: {error}"),
            }
            match &device.mapping {
                Ok(Some(mapping)) => println!("  mapping: {mapping}"),
                Ok(None) => println!("  mapping: none"),
                Err(error) => println!("  mapping error: {error}"),
            }
        }
    }

    let mut controller_manager = discover_controller_manager(&sdl);
    let mut gamepad_latch = GamepadLatch::default();
    let mut last_sample = None;
    let mut last_sample_error = None;
    println!("{}", controller_manager.status_line());
    println!("sampling normalized logical input; press Ctrl+C to stop");

    loop {
        let pending_events = sdl.poll_events();
        if pending_events.quit_requested {
            return Ok(());
        }
        report_input_event_errors(&pending_events.input_errors);
        process_controller_events(
            &mut controller_manager,
            &mut gamepad_latch,
            &pending_events.input_devices,
        );

        let controller_sample = sample_controller(&mut controller_manager, &mut last_sample_error);
        if controller_sample.input_discontinuity {
            gamepad_latch.reset();
        }
        let snapshot = controller_sample.snapshot;
        if last_sample != Some(snapshot) {
            println!("normalized: {snapshot:?}");
            last_sample = Some(snapshot);
        }
        controller_manager.acknowledge_neutral_sample();
        thread::sleep(Duration::from_millis(16));
    }
}

fn discover_controller_manager(sdl: &Sdl) -> ControllerManager<'_> {
    match ControllerManager::new(sdl) {
        Ok(controller_manager) => controller_manager,
        Err(error) => {
            eprintln!(
                "warning: {error}; starting without a controller and retrying on device events"
            );
            ControllerManager::without_active_controller(sdl)
        }
    }
}

fn report_input_event_errors(errors: &[String]) {
    for error in errors {
        eprintln!("warning: ignored invalid SDL input-device event: {error}");
    }
}

fn process_controller_events(
    controller_manager: &mut ControllerManager<'_>,
    gamepad_latch: &mut GamepadLatch,
    events: &[sdl::InputDeviceEvent],
) -> bool {
    let mut input_discontinuity = false;
    for event in events {
        match controller_manager.handle_event(*event) {
            Ok(ControllerEventOutcome::Ignored) => {}
            Ok(ControllerEventOutcome::SelectionRescanned) => {
                gamepad_latch.reset();
                input_discontinuity = true;
                println!("{}", controller_manager.status_line());
            }
            Ok(ControllerEventOutcome::MetadataRefreshed) => {
                gamepad_latch.reset();
                input_discontinuity = true;
                println!("{}", controller_manager.status_line());
            }
            Err(error) => {
                gamepad_latch.reset();
                input_discontinuity = true;
                eprintln!("warning: {error}; controller input remains neutral until recovery");
            }
        }
    }
    input_discontinuity
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ControllerFrameSample {
    snapshot: GamepadSnapshot,
    input_discontinuity: bool,
    device_state_observed: bool,
}

fn sample_controller(
    controller_manager: &mut ControllerManager<'_>,
    last_error: &mut Option<String>,
) -> ControllerFrameSample {
    resolve_controller_sample(controller_manager.sample(), last_error)
}

fn resolve_controller_sample(
    sample: Result<ControllerSample>,
    last_error: &mut Option<String>,
) -> ControllerFrameSample {
    match sample {
        Ok(sample) => {
            let recovered = last_error.take().is_some();
            if recovered {
                println!("controller: sampling recovered");
            }
            ControllerFrameSample {
                snapshot: sample.snapshot,
                input_discontinuity: sample.input_discontinuity || recovered,
                device_state_observed: sample.device_state_observed,
            }
        }
        Err(error) => {
            let entering_error_episode = last_error.is_none();
            if last_error.as_ref() != Some(&error) {
                eprintln!("warning: {error}; using neutral controller input");
            }
            *last_error = Some(error);
            ControllerFrameSample {
                snapshot: GamepadSnapshot::default(),
                input_discontinuity: entering_error_episode,
                device_state_observed: false,
            }
        }
    }
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

struct GamepadSmokeRuntime<'a, 'sdl, 'window> {
    sdl: &'a Sdl,
    window: &'a Window<'sdl>,
    presenter: &'a Renderer<'window>,
    renderer: &'a ReferenceRenderer,
    app: &'a mut AttractModeApp,
    audio_mixer: &'a mut AudioMixer,
    audio_device: &'a AudioDevice<'sdl>,
    current_mode: AppMode,
    current_scene: RenderScene,
}

fn run_gamepad_smoke(runtime: GamepadSmokeRuntime<'_, '_, '_>) -> Result<()> {
    let GamepadSmokeRuntime {
        sdl,
        window,
        presenter,
        renderer,
        app,
        audio_mixer,
        audio_device,
        mut current_mode,
        mut current_scene,
    } = runtime;

    let mut smoke_cfg = app.cfg_snapshot();
    smoke_cfg.control_mode = ControlMode::Joystick;
    app.apply_cfg(&smoke_cfg);

    let mut texture = Texture::new_rgba_streaming(presenter, 320, 200)?;
    let mut smoke = GamepadSmokeAutomation::default();
    let mut gamepad_latch = GamepadLatch::default();
    let mut menu_input_latch = MenuInputLatch::default();
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
            return Err("SDL quit requested before gamepad smoke completed".to_string());
        }
        if pending_events.renderer_reset {
            texture = Texture::new_rgba_streaming(presenter, 320, 200)?;
        }

        let snapshot = smoke.next_snapshot(current_mode);
        let mut input = AppInput::default();
        gamepad_latch.sample(snapshot, app.input_tuning().controller_sensitivity());
        let menu_edges = menu_input_latch.sample(
            MenuHeld::default(),
            gamepad_latch.menu_held(),
            false,
            false,
            current_mode,
        );
        apply_menu_edges(&mut input, menu_edges);
        if current_mode == AppMode::Gameplay {
            input.gameplay_controls_override = Some(gamepad::controller_state(
                snapshot,
                app.input_tuning().controller_sensitivity(),
            ));
        }

        let tick = app.tick(input);
        gamepad_latch.consume_app_edges();
        menu_input_latch.consume_app_edges();
        apply_audio_commands(audio_mixer, audio_device, &tick.audio_commands)?;
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
    let mut controller_diagnostics = false;

    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => return Err(usage().to_string()),
            "--smoke-gameplay" => {
                set_automation_mode(&mut automation, AutomationMode::GameplaySmoke)?;
            }
            "--smoke-gamepad" => {
                set_automation_mode(&mut automation, AutomationMode::GamepadSmoke)?;
            }
            "--controller-diagnostics" => {
                if controller_diagnostics {
                    return Err("--controller-diagnostics was specified more than once".to_string());
                }
                controller_diagnostics = true;
            }
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

    let diagnostics_has_conflicting_options = controller_diagnostics
        && (source_root.is_some() || automation.is_some() || display_mode_override.is_some());
    if diagnostics_has_conflicting_options {
        return Err(format!(
            "--controller-diagnostics does not load assets and cannot be combined with smoke, display, or source-root arguments\n{}",
            usage()
        ));
    }

    if automation.is_some() && display_mode_override.is_none() {
        display_mode_override = Some(DisplayMode::Windowed);
    }

    Ok(LaunchConfig {
        source_root: source_root.unwrap_or_else(|| PathBuf::from(".")),
        automation,
        display_mode_override,
        controller_diagnostics,
    })
}

fn set_automation_mode(
    selected: &mut Option<AutomationMode>,
    requested: AutomationMode,
) -> Result<()> {
    if let Some(existing) = selected {
        return Err(format!(
            "automation modes are mutually exclusive: selected {existing:?}, then {requested:?}"
        ));
    }
    *selected = Some(requested);
    Ok(())
}

fn usage() -> &'static str {
    "usage: cargo run -p skyroads-sdl -- [--smoke-gameplay|--smoke-gamepad] [--windowed|--fullscreen|--borderless|--exclusive-fullscreen] [source_root]\n       cargo run -p skyroads-sdl -- --controller-diagnostics"
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
    println!("  joystick   active mapped controller, with raw index-0 fallback");
    println!("  mouse      DOS-style mouse thresholds");
    println!("  input      tune mouse/controller sensitivity from 50% to 200%");
    println!("controller:");
    println!("  D-pad / left stick   navigate, steer, throttle/brake");
    println!("  south / Start        select; south also jumps in joystick mode");
    println!("  east / Back          return or exit gameplay");
    println!("  Quit + select        quit from the main menu");
    println!("  right / left trigger accelerate / brake");
    println!(
        "  after crash/win      release select, then press it again; no separate prompt is drawn"
    );
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
        AppMode::InputSettings => "Input Settings",
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

fn save_input_preferences(path: &Path, tuning: InputTuning) {
    if let Err(error) = input_preferences::save(path, tuning) {
        eprintln!("warning: {error}; continuing without persisted input preferences");
    }
}

fn initial_input_tuning(path: &Path, automation_enabled: bool) -> InputTuning {
    if automation_enabled {
        return InputTuning::default();
    }

    match input_preferences::load(path) {
        Ok(tuning) => tuning,
        Err(error) => {
            eprintln!("warning: {error}; using 100% mouse and controller sensitivity");
            InputTuning::default()
        }
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

fn apply_menu_edges(input: &mut AppInput, menu_edges: AppInput) {
    input.up = menu_edges.up;
    input.down = menu_edges.down;
    input.left = menu_edges.left;
    input.right = menu_edges.right;
    input.enter = menu_edges.enter;
    input.escape = menu_edges.escape;
    input.space = menu_edges.space;
}

fn normalized_gameplay_controls(
    app_mode: AppMode,
    control_mode: ControlMode,
    mouse_x: u16,
    mouse_y: u16,
    mouse_jump_pressed: bool,
    gamepad_snapshot: GamepadSnapshot,
    tuning: InputTuning,
) -> Option<skyroads_core::ControllerState> {
    match control_mode {
        ControlMode::Keyboard => None,
        ControlMode::Mouse if app_mode == AppMode::Gameplay => {
            Some(gamepad::mouse_controller_state(
                mouse_x,
                mouse_y,
                mouse_jump_pressed,
                tuning.mouse_sensitivity(),
            ))
        }
        ControlMode::Mouse => Some(skyroads_core::ControllerState::NEUTRAL),
        ControlMode::Joystick => Some(gamepad::controller_state(
            gamepad_snapshot,
            tuning.controller_sensitivity(),
        )),
    }
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
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        apply_menu_edges, fit_rect_with_aspect, held_only_input, initial_input_tuning,
        normalized_gameplay_controls, parse_args_from, resolve_controller_sample, scancode, sdl,
        AppInput, AppMode, AutomationMode, ControllerSample, DisplayMode, GamepadLatch,
        GamepadSnapshot, InputTuning, KeyLatch, MenuHeld, MenuInputLatch, Rect,
    };
    use skyroads_core::{ControllerState, SensitivityPercent};

    fn keyboard(keys: &[usize]) -> sdl::KeyboardState {
        sdl::KeyboardState::from_keys(keys)
    }

    #[test]
    fn shift_enter_toggles_fullscreen_without_triggering_enter() {
        let mut key_latch = KeyLatch::default();
        let mut menu_latch = MenuInputLatch::default();
        let mut sample = key_latch.sample(keyboard(&[scancode::LSHIFT, scancode::RETURN]));
        let menu_edges = menu_latch.sample(
            sample.menu_held,
            MenuHeld::default(),
            sample.menu_space_held,
            sample.suppress_keyboard_confirm,
            AppMode::MainMenu,
        );
        apply_menu_edges(&mut sample.app, menu_edges);

        assert!(menu_latch.take_toggle_fullscreen_request());
        assert!(!sample.app.enter);
        assert!(!sample.app.enter_held);

        menu_latch.consume_app_edges();
        key_latch.consume_app_edges();
        let mut shift_released = key_latch.sample(keyboard(&[scancode::RETURN]));
        let menu_edges = menu_latch.sample(
            shift_released.menu_held,
            MenuHeld::default(),
            shift_released.menu_space_held,
            shift_released.suppress_keyboard_confirm,
            AppMode::MainMenu,
        );
        apply_menu_edges(&mut shift_released.app, menu_edges);
        assert!(!menu_latch.take_toggle_fullscreen_request());
        assert!(
            !shift_released.app.enter,
            "releasing Shift while Return remains held must not select"
        );
    }

    #[test]
    fn shift_enter_on_a_controller_discontinuity_frame_still_toggles_fullscreen() {
        let mut key_latch = KeyLatch::default();
        let mut menu_latch = MenuInputLatch::default();
        let mut input = key_latch.sample(keyboard(&[scancode::LSHIFT, scancode::RETURN]));

        menu_latch.rebase_after_gamepad_discontinuity(
            input.menu_held,
            MenuHeld::default(),
            input.menu_space_held,
            input.suppress_keyboard_confirm,
            AppMode::MainMenu,
        );
        let menu_edges = menu_latch.sample(
            input.menu_held,
            MenuHeld::default(),
            input.menu_space_held,
            input.suppress_keyboard_confirm,
            AppMode::MainMenu,
        );
        apply_menu_edges(&mut input.app, menu_edges);

        assert!(menu_latch.take_toggle_fullscreen_request());
        assert!(!input.app.enter);
        assert!(!input.app.enter_held);
    }

    #[test]
    fn pending_keyboard_enter_survives_a_later_shift_enter_discontinuity() {
        let mut key_latch = KeyLatch::default();
        let mut menu_latch = MenuInputLatch::default();

        let pressed = key_latch.sample(keyboard(&[scancode::RETURN]));
        let first = menu_latch.sample(
            pressed.menu_held,
            MenuHeld::default(),
            pressed.menu_space_held,
            pressed.suppress_keyboard_confirm,
            AppMode::MainMenu,
        );
        assert!(first.enter);

        let released = key_latch.sample(keyboard(&[]));
        menu_latch.sample(
            released.menu_held,
            MenuHeld::default(),
            released.menu_space_held,
            released.suppress_keyboard_confirm,
            AppMode::MainMenu,
        );

        let chord = key_latch.sample(keyboard(&[scancode::LSHIFT, scancode::RETURN]));
        menu_latch.rebase_after_gamepad_discontinuity(
            chord.menu_held,
            MenuHeld::default(),
            chord.menu_space_held,
            chord.suppress_keyboard_confirm,
            AppMode::MainMenu,
        );
        let after_discontinuity = menu_latch.sample(
            chord.menu_held,
            MenuHeld::default(),
            chord.menu_space_held,
            chord.suppress_keyboard_confirm,
            AppMode::MainMenu,
        );

        assert!(menu_latch.take_toggle_fullscreen_request());
        assert!(after_discontinuity.enter);
        assert!(!after_discontinuity.space);
    }

    #[test]
    fn closed_keyboard_group_survives_a_later_gamepad_discontinuity() {
        let keyboard_enter = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        assert!(
            menu_latch
                .sample(
                    keyboard_enter,
                    MenuHeld::default(),
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .enter
        );
        menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::MainMenu,
        );
        menu_latch.sample(
            MenuHeld::default(),
            gamepad_confirm,
            false,
            false,
            AppMode::MainMenu,
        );

        menu_latch.rebase_after_gamepad_discontinuity(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::MainMenu,
        );
        let after_discontinuity = menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::MainMenu,
        );

        assert!(
            after_discontinuity.enter,
            "a completed keyboard press must survive a later gamepad-only group"
        );
    }

    #[test]
    fn closed_keyboard_navigation_survives_a_later_gamepad_discontinuity() {
        let keyboard_down = MenuHeld {
            down: true,
            ..MenuHeld::default()
        };
        let gamepad_down = MenuHeld {
            down: true,
            ..MenuHeld::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        assert!(
            menu_latch
                .sample(
                    keyboard_down,
                    MenuHeld::default(),
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .down
        );
        menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::MainMenu,
        );
        menu_latch.sample(
            MenuHeld::default(),
            gamepad_down,
            false,
            false,
            AppMode::MainMenu,
        );

        menu_latch.rebase_after_gamepad_discontinuity(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::MainMenu,
        );
        let after_discontinuity = menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::MainMenu,
        );

        assert!(
            after_discontinuity.down,
            "a completed keyboard move must survive a later gamepad-only group"
        );
    }

    #[test]
    fn discontinuity_does_not_restore_space_rejected_by_fullscreen_priority() {
        let keys = [scancode::LSHIFT, scancode::RETURN, scancode::SPACE];
        let mut key_latch = KeyLatch::default();
        let mut menu_latch = MenuInputLatch::default();

        let pressed = key_latch.sample(keyboard(&keys));
        let first = menu_latch.sample(
            pressed.menu_held,
            MenuHeld::default(),
            pressed.menu_space_held,
            pressed.suppress_keyboard_confirm,
            AppMode::HelpMenu,
        );
        assert!(menu_latch.take_toggle_fullscreen_request());
        assert!(!first.enter);
        assert!(!first.space);

        let held = key_latch.sample(keyboard(&keys));
        menu_latch.rebase_after_gamepad_discontinuity(
            held.menu_held,
            MenuHeld::default(),
            held.menu_space_held,
            held.suppress_keyboard_confirm,
            AppMode::HelpMenu,
        );
        let after_discontinuity = menu_latch.sample(
            held.menu_held,
            MenuHeld::default(),
            held.menu_space_held,
            held.suppress_keyboard_confirm,
            AppMode::HelpMenu,
        );

        assert!(!menu_latch.take_toggle_fullscreen_request());
        assert!(!after_discontinuity.enter);
        assert!(!after_discontinuity.space);
    }

    #[test]
    fn skewed_shift_enter_and_gamepad_views_choose_only_the_first_action() {
        let keyboard_enter = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };

        let mut chord_first = MenuInputLatch::default();
        let chord = chord_first.sample(
            keyboard_enter,
            MenuHeld::default(),
            false,
            true,
            AppMode::MainMenu,
        );
        assert!(!chord.enter);
        assert!(chord_first.take_toggle_fullscreen_request());
        chord_first.consume_app_edges();

        let joined = chord_first.sample(
            keyboard_enter,
            gamepad_confirm,
            false,
            true,
            AppMode::MainMenu,
        );
        assert!(!joined.enter);
        assert!(!chord_first.take_toggle_fullscreen_request());

        let mut gamepad_first = MenuInputLatch::default();
        let select = gamepad_first.sample(
            MenuHeld::default(),
            gamepad_confirm,
            false,
            false,
            AppMode::MainMenu,
        );
        assert!(select.enter);
        assert!(!gamepad_first.take_toggle_fullscreen_request());
        gamepad_first.consume_app_edges();

        let joined = gamepad_first.sample(
            keyboard_enter,
            gamepad_confirm,
            false,
            true,
            AppMode::MainMenu,
        );
        assert!(!joined.enter);
        assert!(!gamepad_first.take_toggle_fullscreen_request());
    }

    #[test]
    fn shift_enter_remains_independent_while_plain_keyboard_space_is_held() {
        let mut menu_latch = MenuInputLatch::default();
        let space = menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            true,
            false,
            AppMode::MainMenu,
        );
        assert!(space.space);
        menu_latch.consume_app_edges();

        let keyboard_enter = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let chord = menu_latch.sample(
            keyboard_enter,
            MenuHeld::default(),
            true,
            true,
            AppMode::MainMenu,
        );

        assert!(!chord.enter);
        assert!(menu_latch.take_toggle_fullscreen_request());
    }

    #[test]
    fn plain_space_remains_independent_while_shift_enter_is_held() {
        let keyboard_enter = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        let chord = menu_latch.sample(
            keyboard_enter,
            MenuHeld::default(),
            false,
            true,
            AppMode::HelpMenu,
        );
        assert!(!chord.enter);
        assert!(menu_latch.take_toggle_fullscreen_request());
        menu_latch.consume_app_edges();

        let space = menu_latch.sample(
            keyboard_enter,
            MenuHeld::default(),
            true,
            true,
            AppMode::HelpMenu,
        );
        assert!(space.enter);
        assert!(!space.space);
        assert!(!menu_latch.take_toggle_fullscreen_request());
    }

    #[test]
    fn latest_fullscreen_chord_owns_a_later_controller_mirror() {
        let keyboard_enter = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let pressed_snapshot = GamepadSnapshot {
            south_pressed: true,
            ..GamepadSnapshot::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        assert!(
            menu_latch
                .sample(
                    MenuHeld::default(),
                    MenuHeld::default(),
                    true,
                    false,
                    AppMode::MainMenu,
                )
                .space
        );
        let chord = menu_latch.sample(
            keyboard_enter,
            MenuHeld::default(),
            true,
            true,
            AppMode::MainMenu,
        );
        assert!(chord.space);
        assert!(menu_latch.take_toggle_fullscreen_request());

        let joined = menu_latch.sample(
            keyboard_enter,
            gamepad_confirm,
            true,
            true,
            AppMode::MainMenu,
        );
        assert!(joined.space, "the earlier keyboard action remains pending");
        assert!(!joined.enter, "the controller mirrors the newer chord");
        assert!(
            !menu_latch
                .gameplay_snapshot(pressed_snapshot, true)
                .south_pressed,
            "the fullscreen-owned controller press must not leak into gameplay"
        );
    }

    #[test]
    fn latest_space_press_owns_a_later_controller_mirror() {
        let keyboard_enter = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let pressed_snapshot = GamepadSnapshot {
            south_pressed: true,
            ..GamepadSnapshot::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        menu_latch.sample(
            keyboard_enter,
            MenuHeld::default(),
            false,
            true,
            AppMode::MainMenu,
        );
        assert!(menu_latch.take_toggle_fullscreen_request());

        let space = menu_latch.sample(
            keyboard_enter,
            MenuHeld::default(),
            true,
            true,
            AppMode::MainMenu,
        );
        assert!(space.space);

        let joined = menu_latch.sample(
            keyboard_enter,
            gamepad_confirm,
            true,
            true,
            AppMode::MainMenu,
        );
        assert!(
            joined.enter,
            "the controller upgrades the newer Space action"
        );
        assert!(!joined.space);
        assert!(
            menu_latch
                .gameplay_snapshot(pressed_snapshot, true)
                .south_pressed,
            "the controller no longer belongs to the older fullscreen chord"
        );
    }

    #[test]
    fn fullscreen_owned_controller_press_cannot_jump_through_the_raw_snapshot() {
        let keyboard_enter = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let pressed_snapshot = GamepadSnapshot {
            south_pressed: true,
            ..GamepadSnapshot::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        menu_latch.sample(
            keyboard_enter,
            MenuHeld::default(),
            false,
            true,
            AppMode::Gameplay,
        );
        assert!(menu_latch.take_toggle_fullscreen_request());
        menu_latch.sample(
            keyboard_enter,
            gamepad_confirm,
            false,
            true,
            AppMode::Gameplay,
        );

        let suppressed = menu_latch.gameplay_snapshot(pressed_snapshot, true);
        assert!(!suppressed.south_pressed);

        menu_latch.rebase_after_gamepad_discontinuity(
            keyboard_enter,
            gamepad_confirm,
            false,
            true,
            AppMode::Gameplay,
        );
        let suppressed_after_discontinuity = menu_latch.gameplay_snapshot(pressed_snapshot, true);
        assert!(
            !suppressed_after_discontinuity.south_pressed,
            "a lifecycle rebase must not turn the fullscreen-owned press into jump"
        );

        menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::Gameplay,
        );
        menu_latch.gameplay_snapshot(GamepadSnapshot::default(), true);
        menu_latch.sample(
            MenuHeld::default(),
            gamepad_confirm,
            false,
            false,
            AppMode::Gameplay,
        );
        let fresh_press = menu_latch.gameplay_snapshot(pressed_snapshot, true);
        assert!(fresh_press.south_pressed);
    }

    #[test]
    fn synthetic_neutral_cannot_release_a_fullscreen_owned_south_button() {
        let keyboard_enter = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let pressed_snapshot = GamepadSnapshot {
            south_pressed: true,
            ..GamepadSnapshot::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        menu_latch.sample(
            keyboard_enter,
            MenuHeld::default(),
            false,
            true,
            AppMode::Gameplay,
        );
        assert!(menu_latch.take_toggle_fullscreen_request());
        menu_latch.sample(
            keyboard_enter,
            gamepad_confirm,
            false,
            true,
            AppMode::Gameplay,
        );
        assert!(
            !menu_latch
                .gameplay_snapshot(pressed_snapshot, true)
                .south_pressed
        );

        menu_latch.sample(
            MenuHeld::default(),
            gamepad_confirm,
            false,
            false,
            AppMode::Gameplay,
        );
        menu_latch.rebase_after_gamepad_discontinuity(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::Gameplay,
        );
        menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::Gameplay,
        );
        menu_latch.gameplay_snapshot(GamepadSnapshot::default(), false);
        menu_latch.consume_app_edges();

        menu_latch.rebase_after_gamepad_discontinuity(
            MenuHeld::default(),
            gamepad_confirm,
            false,
            false,
            AppMode::Gameplay,
        );
        menu_latch.sample(
            MenuHeld::default(),
            gamepad_confirm,
            false,
            false,
            AppMode::Gameplay,
        );
        let held_after_recovery = menu_latch.gameplay_snapshot(pressed_snapshot, true);
        assert!(
            !held_after_recovery.south_pressed,
            "synthetic neutral input must not count as a physical South release"
        );

        menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::Gameplay,
        );
        menu_latch.gameplay_snapshot(GamepadSnapshot::default(), true);
        menu_latch.sample(
            MenuHeld::default(),
            gamepad_confirm,
            false,
            false,
            AppMode::Gameplay,
        );
        let fresh_press = menu_latch.gameplay_snapshot(pressed_snapshot, true);
        assert!(fresh_press.south_pressed);
    }

    #[test]
    fn plain_enter_still_maps_to_app_enter() {
        let mut key_latch = KeyLatch::default();
        let mut menu_latch = MenuInputLatch::default();
        let mut sample = key_latch.sample(keyboard(&[scancode::RETURN]));
        let menu_edges = menu_latch.sample(
            sample.menu_held,
            MenuHeld::default(),
            sample.menu_space_held,
            sample.suppress_keyboard_confirm,
            AppMode::MainMenu,
        );
        apply_menu_edges(&mut sample.app, menu_edges);

        assert!(!menu_latch.take_toggle_fullscreen_request());
        assert!(sample.app.enter);
        assert!(sample.app.enter_held);
    }

    #[test]
    fn plain_space_keeps_its_keyboard_specific_action() {
        let mut key_latch = KeyLatch::default();
        let mut menu_latch = MenuInputLatch::default();
        let mut sample = key_latch.sample(keyboard(&[scancode::SPACE]));
        let menu_edges = menu_latch.sample(
            sample.menu_held,
            MenuHeld::default(),
            sample.menu_space_held,
            sample.suppress_keyboard_confirm,
            AppMode::MainMenu,
        );
        apply_menu_edges(&mut sample.app, menu_edges);

        assert!(sample.app.space);
        assert!(sample.app.space_held);
        assert!(!sample.app.enter);
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
        assert!(!config.controller_diagnostics);
    }

    #[test]
    fn headless_smoke_defaults_to_windowed_but_respects_explicit_mode() {
        let smoke = parse_args_from(["--smoke-gameplay".to_string()]).unwrap();
        assert_eq!(smoke.automation, Some(AutomationMode::GameplaySmoke));
        assert_eq!(smoke.display_mode_override, Some(DisplayMode::Windowed));

        let gamepad = parse_args_from(["--smoke-gamepad".to_string()]).unwrap();
        assert_eq!(gamepad.automation, Some(AutomationMode::GamepadSmoke));
        assert_eq!(gamepad.display_mode_override, Some(DisplayMode::Windowed));

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
    fn controller_diagnostics_loads_no_assets_and_rejects_conflicting_modes() {
        let diagnostics = parse_args_from(["--controller-diagnostics".to_string()]).unwrap();
        assert!(diagnostics.controller_diagnostics);
        assert_eq!(diagnostics.automation, None);

        for conflicting in [
            vec!["--controller-diagnostics".to_string(), ".".to_string()],
            vec![
                "--controller-diagnostics".to_string(),
                "--windowed".to_string(),
            ],
            vec![
                "--controller-diagnostics".to_string(),
                "--smoke-gamepad".to_string(),
            ],
        ] {
            assert!(parse_args_from(conflicting).is_err());
        }
    }

    #[test]
    fn malformed_input_preferences_fall_back_atomically_to_both_defaults() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "skyroads-malformed-input-preferences-{}-{unique}.cfg",
            std::process::id()
        ));
        fs::write(
            &path,
            "mouse_sensitivity=150\ncontroller_sensitivity=invalid\n",
        )
        .unwrap();

        assert_eq!(initial_input_tuning(&path, false), InputTuning::default());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "mouse_sensitivity=150\ncontroller_sensitivity=invalid\n"
        );

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn combined_menu_edges_do_not_leak_gamepad_held_state_into_gameplay() {
        let mut input = AppInput {
            up_held: true,
            ..AppInput::default()
        };
        let keyboard = MenuHeld {
            down: true,
            ..MenuHeld::default()
        };
        let gamepad = MenuHeld {
            right: true,
            confirm: true,
            ..MenuHeld::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        let menu_edges = menu_latch.sample(keyboard, gamepad, false, false, AppMode::MainMenu);
        apply_menu_edges(&mut input, menu_edges);

        assert!(input.down);
        assert!(input.right);
        assert!(input.enter);
        assert!(input.up_held);
        assert!(!input.right_held);
        assert!(!input.enter_held);
    }

    #[test]
    fn skewed_keyboard_and_gamepad_views_of_one_press_emit_one_menu_edge() {
        let mut menu_latch = MenuInputLatch::default();
        let keyboard_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };

        assert!(
            menu_latch
                .sample(
                    keyboard_confirm,
                    MenuHeld::default(),
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .enter
        );
        menu_latch.consume_app_edges();

        assert!(
            !menu_latch
                .sample(
                    keyboard_confirm,
                    gamepad_confirm,
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .enter
        );
        menu_latch.consume_app_edges();

        assert!(
            !menu_latch
                .sample(
                    MenuHeld::default(),
                    gamepad_confirm,
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .enter
        );
        menu_latch.consume_app_edges();

        assert!(
            !menu_latch
                .sample(
                    MenuHeld::default(),
                    MenuHeld::default(),
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .enter
        );
        assert!(
            menu_latch
                .sample(
                    MenuHeld::default(),
                    gamepad_confirm,
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .enter,
            "a fresh press after both sources release must create a new edge"
        );
    }

    #[test]
    fn skewed_space_and_gamepad_views_of_one_press_emit_one_confirm_edge() {
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let mut gamepad_first = MenuInputLatch::default();

        let first = gamepad_first.sample(
            MenuHeld::default(),
            gamepad_confirm,
            false,
            false,
            AppMode::MainMenu,
        );
        assert!(first.enter);
        assert!(!first.space);
        gamepad_first.consume_app_edges();

        let joined = gamepad_first.sample(
            MenuHeld::default(),
            gamepad_confirm,
            true,
            false,
            AppMode::MainMenu,
        );
        assert!(!joined.enter);
        assert!(!joined.space);
        gamepad_first.consume_app_edges();

        let keyboard_lingers = gamepad_first.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            true,
            false,
            AppMode::MainMenu,
        );
        assert!(!keyboard_lingers.enter);
        assert!(!keyboard_lingers.space);

        let mut keyboard_first = MenuInputLatch::default();
        let first = keyboard_first.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            true,
            false,
            AppMode::MainMenu,
        );
        assert!(first.space);
        assert!(!first.enter);
        keyboard_first.consume_app_edges();

        let joined = keyboard_first.sample(
            MenuHeld::default(),
            gamepad_confirm,
            true,
            false,
            AppMode::MainMenu,
        );
        assert!(joined.enter);
        assert!(!joined.space);
        keyboard_first.consume_app_edges();

        let controller_lingers = keyboard_first.sample(
            MenuHeld::default(),
            gamepad_confirm,
            false,
            false,
            AppMode::MainMenu,
        );
        assert!(!controller_lingers.enter);
        assert!(!controller_lingers.space);

        let mut keyboard_pending = MenuInputLatch::default();
        assert!(
            keyboard_pending
                .sample(
                    MenuHeld::default(),
                    MenuHeld::default(),
                    true,
                    false,
                    AppMode::MainMenu,
                )
                .space
        );
        let upgraded_before_tick = keyboard_pending.sample(
            MenuHeld::default(),
            gamepad_confirm,
            true,
            false,
            AppMode::MainMenu,
        );
        assert!(upgraded_before_tick.enter);
        assert!(
            !upgraded_before_tick.space,
            "the pending keyboard activity must become one canonical select edge"
        );
    }

    #[test]
    fn protected_space_survives_a_later_controller_upgrade_and_discontinuity() {
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        assert!(
            menu_latch
                .sample(
                    MenuHeld::default(),
                    MenuHeld::default(),
                    true,
                    false,
                    AppMode::MainMenu,
                )
                .space
        );
        menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::MainMenu,
        );

        assert!(
            menu_latch
                .sample(
                    MenuHeld::default(),
                    MenuHeld::default(),
                    true,
                    false,
                    AppMode::MainMenu,
                )
                .space
        );
        let joined = menu_latch.sample(
            MenuHeld::default(),
            gamepad_confirm,
            true,
            false,
            AppMode::MainMenu,
        );
        assert!(joined.enter);
        assert!(joined.space, "the completed Space action remains protected");

        menu_latch.rebase_after_gamepad_discontinuity(
            MenuHeld::default(),
            MenuHeld::default(),
            true,
            false,
            AppMode::MainMenu,
        );
        let after_discontinuity = menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            true,
            false,
            AppMode::MainMenu,
        );

        assert!(!after_discontinuity.enter);
        assert!(after_discontinuity.space);
    }

    #[test]
    fn space_uses_enter_when_both_keys_have_the_same_app_semantics() {
        let mut menu_latch = MenuInputLatch::default();

        let help_confirm = menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            true,
            false,
            AppMode::HelpMenu,
        );

        assert!(help_confirm.enter);
        assert!(!help_confirm.space);
    }

    #[test]
    fn combined_menu_edge_remains_pending_until_a_simulation_tick_consumes_it() {
        let mut menu_latch = MenuInputLatch::default();
        let keyboard_down = MenuHeld {
            down: true,
            ..MenuHeld::default()
        };

        assert!(
            menu_latch
                .sample(
                    keyboard_down,
                    MenuHeld::default(),
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .down
        );
        assert!(
            menu_latch
                .sample(
                    MenuHeld::default(),
                    MenuHeld::default(),
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .down,
            "presentation frames must not consume a pending edge"
        );

        menu_latch.consume_app_edges();
        assert!(
            !menu_latch
                .sample(
                    MenuHeld::default(),
                    MenuHeld::default(),
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .down
        );
    }

    #[test]
    fn gamepad_discontinuity_discards_only_unconsumed_gamepad_menu_edges() {
        let mut menu_latch = MenuInputLatch::default();
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        assert!(
            menu_latch
                .sample(
                    MenuHeld::default(),
                    gamepad_confirm,
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .enter
        );

        let keyboard = MenuHeld {
            down: true,
            ..MenuHeld::default()
        };
        menu_latch.rebase_after_gamepad_discontinuity(
            keyboard,
            MenuHeld::default(),
            false,
            false,
            AppMode::MainMenu,
        );
        let after_disconnect = menu_latch.sample(
            keyboard,
            MenuHeld::default(),
            false,
            false,
            AppMode::MainMenu,
        );

        assert!(after_disconnect.down);
        assert!(!after_disconnect.enter);
    }

    #[test]
    fn recovered_held_gamepad_control_requires_release_before_a_new_edge() {
        let mut menu_latch = MenuInputLatch::default();
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };

        assert!(
            menu_latch
                .sample(
                    MenuHeld::default(),
                    gamepad_confirm,
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .enter
        );
        menu_latch.consume_app_edges();

        menu_latch.rebase_after_gamepad_discontinuity(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::MainMenu,
        );
        menu_latch.rebase_after_gamepad_discontinuity(
            MenuHeld::default(),
            gamepad_confirm,
            false,
            false,
            AppMode::MainMenu,
        );
        assert!(
            !menu_latch
                .sample(
                    MenuHeld::default(),
                    gamepad_confirm,
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .enter,
            "a button held through recovery must not become a second press"
        );

        menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::MainMenu,
        );
        assert!(
            menu_latch
                .sample(
                    MenuHeld::default(),
                    gamepad_confirm,
                    false,
                    false,
                    AppMode::MainMenu,
                )
                .enter,
            "release followed by a fresh press must still create an edge"
        );
    }

    #[test]
    fn replacement_live_sample_rebases_a_held_confirm_before_edge_detection() {
        let replacement_snapshot = GamepadSnapshot {
            south_pressed: true,
            ..GamepadSnapshot::default()
        };
        let mut last_error = None;
        let replacement_sample = resolve_controller_sample(
            Ok(ControllerSample {
                snapshot: replacement_snapshot,
                input_discontinuity: true,
                device_state_observed: true,
            }),
            &mut last_error,
        );
        assert!(replacement_sample.input_discontinuity);

        let mut gamepad_latch = GamepadLatch::default();
        if replacement_sample.input_discontinuity {
            gamepad_latch.reset();
        }
        gamepad_latch.sample(replacement_sample.snapshot, SensitivityPercent::DEFAULT);

        let mut menu_latch = MenuInputLatch::default();
        menu_latch.rebase_after_gamepad_discontinuity(
            MenuHeld::default(),
            gamepad_latch.menu_held(),
            false,
            false,
            AppMode::MainMenu,
        );
        let held = menu_latch.sample(
            MenuHeld::default(),
            gamepad_latch.menu_held(),
            false,
            false,
            AppMode::MainMenu,
        );
        assert!(
            !held.enter,
            "a button held on the replacement device must wait for release"
        );

        gamepad_latch.sample(GamepadSnapshot::default(), SensitivityPercent::DEFAULT);
        menu_latch.sample(
            MenuHeld::default(),
            gamepad_latch.menu_held(),
            false,
            false,
            AppMode::MainMenu,
        );
        gamepad_latch.sample(replacement_snapshot, SensitivityPercent::DEFAULT);
        let fresh_press = menu_latch.sample(
            MenuHeld::default(),
            gamepad_latch.menu_held(),
            false,
            false,
            AppMode::MainMenu,
        );
        assert!(fresh_press.enter);
    }

    #[test]
    fn replacement_rebase_drops_a_matching_steam_keyboard_mirror() {
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        menu_latch.rebase_after_gamepad_discontinuity(
            MenuHeld::default(),
            gamepad_confirm,
            true,
            false,
            AppMode::MainMenu,
        );
        let held = menu_latch.sample(
            MenuHeld::default(),
            gamepad_confirm,
            true,
            false,
            AppMode::MainMenu,
        );

        assert!(!held.enter);
        assert!(!held.space);

        menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::MainMenu,
        );
        let fresh_press = menu_latch.sample(
            MenuHeld::default(),
            gamepad_confirm,
            false,
            false,
            AppMode::MainMenu,
        );
        assert!(fresh_press.enter);
    }

    #[test]
    fn main_menu_space_rebased_on_a_discontinuity_can_still_upgrade_to_select() {
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        menu_latch.rebase_after_gamepad_discontinuity(
            MenuHeld::default(),
            MenuHeld::default(),
            true,
            false,
            AppMode::MainMenu,
        );
        let space = menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            true,
            false,
            AppMode::MainMenu,
        );
        assert!(space.space);
        assert!(!space.enter);
        menu_latch.consume_app_edges();

        let joined = menu_latch.sample(
            MenuHeld::default(),
            gamepad_confirm,
            true,
            false,
            AppMode::MainMenu,
        );
        assert!(joined.enter);
        assert!(!joined.space);
    }

    #[test]
    fn disconnect_neutral_rebase_drops_the_removed_gamepad_keyboard_mirror() {
        let gamepad_confirm = MenuHeld {
            confirm: true,
            ..MenuHeld::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        let duplicated_press = menu_latch.sample(
            MenuHeld::default(),
            gamepad_confirm,
            true,
            false,
            AppMode::HelpMenu,
        );
        assert!(duplicated_press.enter);

        menu_latch.rebase_after_gamepad_discontinuity(
            MenuHeld::default(),
            MenuHeld::default(),
            true,
            false,
            AppMode::HelpMenu,
        );
        let neutral_disconnect = menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            true,
            false,
            AppMode::HelpMenu,
        );

        assert!(!neutral_disconnect.enter);
        assert!(!neutral_disconnect.space);
    }

    #[test]
    fn repeated_neutral_rebases_do_not_restore_filtered_keyboard_mirrors() {
        let gamepad = MenuHeld {
            down: true,
            confirm: true,
            back: true,
            ..MenuHeld::default()
        };
        let keyboard = MenuHeld {
            down: true,
            back: true,
            ..MenuHeld::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        let duplicated_press = menu_latch.sample(keyboard, gamepad, true, false, AppMode::HelpMenu);
        assert!(duplicated_press.down);
        assert!(duplicated_press.escape);
        assert!(duplicated_press.enter);

        menu_latch.rebase_after_gamepad_discontinuity(
            keyboard,
            MenuHeld::default(),
            true,
            false,
            AppMode::HelpMenu,
        );
        menu_latch.rebase_after_gamepad_discontinuity(
            keyboard,
            MenuHeld::default(),
            true,
            false,
            AppMode::HelpMenu,
        );
        let after_second_discontinuity = menu_latch.sample(
            keyboard,
            MenuHeld::default(),
            true,
            false,
            AppMode::HelpMenu,
        );

        assert!(!after_second_discontinuity.down);
        assert!(!after_second_discontinuity.escape);
        assert!(!after_second_discontinuity.enter);
        assert!(!after_second_discontinuity.space);

        // The raw KeyLatch edge is consumed on this tick even though its
        // keyboard view is still held.
        menu_latch.consume_app_edges();
        menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::HelpMenu,
        );
        menu_latch.rebase_after_gamepad_discontinuity(
            keyboard,
            MenuHeld::default(),
            true,
            false,
            AppMode::HelpMenu,
        );
        let fresh_keyboard_press = menu_latch.sample(
            keyboard,
            MenuHeld::default(),
            true,
            false,
            AppMode::HelpMenu,
        );

        assert!(fresh_keyboard_press.down);
        assert!(fresh_keyboard_press.escape);
        assert!(fresh_keyboard_press.enter);
        assert!(!fresh_keyboard_press.space);
    }

    #[test]
    fn later_discontinuity_keeps_a_fresh_keyboard_group_before_the_tick() {
        let gamepad = MenuHeld {
            down: true,
            confirm: true,
            ..MenuHeld::default()
        };
        let keyboard = MenuHeld {
            down: true,
            confirm: true,
            ..MenuHeld::default()
        };
        let mut menu_latch = MenuInputLatch::default();

        let duplicated_press =
            menu_latch.sample(keyboard, gamepad, false, false, AppMode::HelpMenu);
        assert!(duplicated_press.down);
        assert!(duplicated_press.enter);

        menu_latch.rebase_after_gamepad_discontinuity(
            keyboard,
            MenuHeld::default(),
            false,
            false,
            AppMode::HelpMenu,
        );
        menu_latch.sample(
            MenuHeld::default(),
            MenuHeld::default(),
            false,
            false,
            AppMode::HelpMenu,
        );

        let fresh_keyboard_press = menu_latch.sample(
            keyboard,
            MenuHeld::default(),
            false,
            false,
            AppMode::HelpMenu,
        );
        assert!(fresh_keyboard_press.down);
        assert!(fresh_keyboard_press.enter);

        menu_latch.rebase_after_gamepad_discontinuity(
            keyboard,
            MenuHeld::default(),
            false,
            false,
            AppMode::HelpMenu,
        );
        let after_second_discontinuity = menu_latch.sample(
            keyboard,
            MenuHeld::default(),
            false,
            false,
            AppMode::HelpMenu,
        );

        assert!(after_second_discontinuity.down);
        assert!(after_second_discontinuity.enter);
    }

    #[test]
    fn gameplay_override_survives_catch_up_ticks_without_repeating_edges() {
        let controls = ControllerState::new(-1, 1, true);
        let first = AppInput {
            enter: true,
            gameplay_controls_override: Some(controls),
            ..AppInput::default()
        };

        let catch_up = held_only_input(first);

        assert!(!catch_up.enter);
        assert_eq!(catch_up.gameplay_controls_override, Some(controls));
    }

    #[test]
    fn menu_to_gameplay_catch_up_keeps_precomputed_joystick_movement() {
        let snapshot = GamepadSnapshot {
            left_stick_x: i16::MIN,
            right_trigger: i16::MAX as u16,
            ..GamepadSnapshot::default()
        };
        let first_tick = AppInput {
            enter: true,
            gameplay_controls_override: normalized_gameplay_controls(
                super::AppMode::GoMenu,
                skyroads_data::ControlMode::Joystick,
                160,
                100,
                false,
                snapshot,
                InputTuning::default(),
            ),
            ..AppInput::default()
        };

        let first_gameplay_tick = held_only_input(first_tick);

        assert_eq!(
            first_gameplay_tick.gameplay_controls_override,
            Some(ControllerState::new(-1, 1, false))
        );
    }

    #[test]
    fn menu_to_gameplay_catch_up_neutralizes_the_precenter_mouse_sample() {
        let first_tick = AppInput {
            enter: true,
            enter_held: true,
            gameplay_controls_override: normalized_gameplay_controls(
                super::AppMode::GoMenu,
                skyroads_data::ControlMode::Mouse,
                0,
                0,
                true,
                GamepadSnapshot::default(),
                InputTuning::default(),
            ),
            ..AppInput::default()
        };

        let first_gameplay_tick = held_only_input(first_tick);

        assert_eq!(
            first_gameplay_tick.gameplay_controls_override,
            Some(ControllerState::NEUTRAL)
        );
    }

    #[test]
    fn sampling_error_discards_a_pending_unconsumed_gamepad_edge() {
        let mut latch = GamepadLatch::default();
        let pressed = latch.sample(
            GamepadSnapshot {
                south_pressed: true,
                ..GamepadSnapshot::default()
            },
            SensitivityPercent::DEFAULT,
        );
        assert!(pressed.enter);

        let mut last_error = None;
        let failed =
            resolve_controller_sample(Err("device read failed".to_string()), &mut last_error);
        if failed.input_discontinuity {
            latch.reset();
        }
        let after_failure = latch.sample(failed.snapshot, SensitivityPercent::DEFAULT);

        assert!(failed.input_discontinuity);
        assert!(!failed.device_state_observed);
        assert_eq!(after_failure, AppInput::default());
        assert_eq!(last_error.as_deref(), Some("device read failed"));
    }

    #[test]
    fn repeated_sampling_failures_stay_inside_one_discontinuity_episode() {
        let mut last_error = None;

        let first =
            resolve_controller_sample(Err("device read failed".to_string()), &mut last_error);
        let repeated =
            resolve_controller_sample(Err("device read failed".to_string()), &mut last_error);
        let changed_message =
            resolve_controller_sample(Err("device detached".to_string()), &mut last_error);

        assert!(first.input_discontinuity);
        assert!(!repeated.input_discontinuity);
        assert!(!changed_message.input_discontinuity);
        assert!(!first.device_state_observed);
        assert!(!repeated.device_state_observed);
        assert!(!changed_message.device_state_observed);

        let recovered = resolve_controller_sample(
            Ok(ControllerSample {
                snapshot: GamepadSnapshot::default(),
                input_discontinuity: false,
                device_state_observed: true,
            }),
            &mut last_error,
        );
        assert!(recovered.input_discontinuity);
        assert!(recovered.device_state_observed);

        let next_episode =
            resolve_controller_sample(Err("device read failed".to_string()), &mut last_error);
        assert!(next_episode.input_discontinuity);
    }

    #[test]
    fn injected_trigger_throttle_matches_equivalent_keyboard_input() {
        let gamepad = crate::gamepad::controller_state(
            GamepadSnapshot {
                right_trigger: i16::MAX as u16,
                ..GamepadSnapshot::default()
            },
            SensitivityPercent::DEFAULT,
        );
        let keyboard = AppInput {
            up_held: true,
            ..AppInput::default()
        }
        .gameplay_controls();

        assert_eq!(gamepad, keyboard);
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
