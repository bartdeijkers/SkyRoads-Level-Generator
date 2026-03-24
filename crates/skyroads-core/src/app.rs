use skyroads_data::{DemoRecording, Level, LevelCell, ROAD_COLUMNS};

use crate::{sample_demo_input_for_ship, ControllerState, GameplayEvent, GameplaySession};

const TICKS_PER_SECOND: usize = 70;
const INTRO_SOUND_DELAY_TICKS: usize = TICKS_PER_SECOND / 2;
const INTRO_ANIM_START_TICKS: usize = TICKS_PER_SECOND * 2;
const INTRO_TITLE_HOLD_TICKS: usize = TICKS_PER_SECOND * 4;
const CREDIT_FRAME_TICKS: usize = TICKS_PER_SECOND * 4;
const MENU_IDLE_DEMO_TICKS: usize = TICKS_PER_SECOND * 5;
const RENDER_ROWS_BEHIND: usize = 3;
const RENDER_ROWS_AHEAD: usize = 7;
const MENU_SONG_INDEX: u8 = 1;
const GAMEPLAY_SONG_INDEX: u8 = 2;
const DEMO_SONG_INDEX: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Boot,
    Intro,
    MainMenu,
    HelpMenu,
    SettingsMenu,
    GoMenu,
    DemoPlayback,
    Gameplay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCursor {
    Start,
    Config,
    Help,
}

impl MenuCursor {
    pub fn index(self) -> usize {
        match self {
            Self::Start => 0,
            Self::Config => 1,
            Self::Help => 2,
        }
    }

    fn move_by(self, delta: i8) -> Self {
        match (self.index() as i8 + delta).clamp(0, 2) {
            0 => Self::Start,
            1 => Self::Config,
            _ => Self::Help,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlMode {
    Keyboard,
    Joystick,
    Mouse,
}

impl ControlMode {
    pub fn dos_value(self) -> u16 {
        match self {
            Self::Keyboard => 0,
            Self::Joystick => 1,
            Self::Mouse => 2,
        }
    }

    fn from_dos_value(value: u16) -> Self {
        match value {
            1 => Self::Joystick,
            2 => Self::Mouse,
            _ => Self::Keyboard,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsMenuCursor {
    Keyboard,
    Joystick,
    Mouse,
    SoundFx,
    Music,
}

impl SettingsMenuCursor {
    pub fn overlay_frame_index(self) -> usize {
        match self {
            Self::Keyboard => 1,
            Self::Joystick => 2,
            Self::Mouse => 3,
            Self::SoundFx => 4,
            Self::Music => 5,
        }
    }

    fn from_control_mode(mode: ControlMode) -> Self {
        match mode {
            ControlMode::Keyboard => Self::Keyboard,
            ControlMode::Joystick => Self::Joystick,
            ControlMode::Mouse => Self::Mouse,
        }
    }

    fn selected_control_mode(self) -> Option<ControlMode> {
        match self {
            Self::Keyboard => Some(ControlMode::Keyboard),
            Self::Joystick => Some(ControlMode::Joystick),
            Self::Mouse => Some(ControlMode::Mouse),
            Self::SoundFx | Self::Music => None,
        }
    }

    fn x_center(self) -> i32 {
        match self {
            Self::Keyboard => 84,
            Self::Joystick => 166,
            Self::Mouse => 250,
            Self::SoundFx => 120,
            Self::Music => 208,
        }
    }

    fn row_index(self) -> usize {
        match self {
            Self::Keyboard | Self::Joystick | Self::Mouse => 0,
            Self::SoundFx | Self::Music => 1,
        }
    }

    fn row_items(row: usize) -> &'static [Self] {
        match row {
            0 => &[Self::Keyboard, Self::Joystick, Self::Mouse],
            1 => &[Self::SoundFx, Self::Music],
            _ => &[],
        }
    }

    fn move_in_direction(self, direction: SettingsNavDirection) -> Self {
        let row = Self::row_items(self.row_index());
        let index = row
            .iter()
            .position(|candidate| *candidate == self)
            .expect("settings cursor must belong to its recovered SETMENU row");
        match direction {
            SettingsNavDirection::Left => index
                .checked_sub(1)
                .and_then(|next| row.get(next))
                .copied()
                .unwrap_or(self),
            SettingsNavDirection::Right => row.get(index + 1).copied().unwrap_or(self),
            SettingsNavDirection::Up => self.move_between_rows(-1),
            SettingsNavDirection::Down => self.move_between_rows(1),
        }
    }

    fn move_between_rows(self, delta: isize) -> Self {
        let target_row = self.row_index() as isize + delta;
        if target_row < 0 {
            return self;
        }
        Self::row_items(target_row as usize)
            .iter()
            .copied()
            .min_by_key(|candidate| ((candidate.x_center() - self.x_center()).abs(), candidate.x_center()))
            .unwrap_or(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsNavDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AppInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub enter: bool,
    pub escape: bool,
    pub space: bool,
    pub up_held: bool,
    pub down_held: bool,
    pub left_held: bool,
    pub right_held: bool,
    pub enter_held: bool,
    pub space_held: bool,
    pub gameplay_controls_override: Option<ControllerState>,
}

impl AppInput {
    pub fn skip_requested(self) -> bool {
        self.enter || self.space || self.escape
    }

    pub fn gameplay_controls(self) -> ControllerState {
        if let Some(controls) = self.gameplay_controls_override {
            return controls;
        }
        ControllerState::new(
            axis(self.left_held, self.right_held),
            axis(self.down_held, self.up_held),
            self.enter_held || self.space_held,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCommand {
    PlaySong(u8),
    StopSong,
    PlayIntroSample,
    PlaySfx(u8),
    StopAllSamples,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntroSequenceState {
    pub tick: usize,
    pub background_brightness: f32,
    pub title_progress: f32,
    pub anim_frame_index: Option<usize>,
    pub credit_frame_index: Option<usize>,
    pub credit_alpha: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShipRenderState {
    pub x_position: f64,
    pub y_position: f64,
    pub z_position: f64,
    pub y_velocity: f64,
    pub z_velocity: f64,
    pub state: crate::ShipState,
    pub is_on_ground: bool,
    pub is_going_up: bool,
    pub turn_input: i8,
    pub accel_input: i8,
    pub jump_input: bool,
    pub death_frame_index: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoadRenderRow {
    pub row_index: usize,
    pub cells: [LevelCell; ROAD_COLUMNS],
}

#[derive(Debug, Clone, PartialEq)]
pub struct DemoPlaybackState {
    pub world_index: usize,
    pub gravity: u16,
    pub level_length: usize,
    pub frame_index: usize,
    pub current_row: usize,
    pub fractional_z: f64,
    pub rows: Vec<RoadRenderRow>,
    pub did_win: bool,
    pub is_demo: bool,
    pub craft_state: crate::ShipState,
    pub snapshot: crate::GameSnapshot,
    pub ship: ShipRenderState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MainMenuScene {
    pub selected: MenuCursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelpMenuScene {
    pub page_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsMenuScene {
    pub cursor: SettingsMenuCursor,
    pub control_mode: ControlMode,
    pub sound_fx_enabled: bool,
    pub music_enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RenderScene {
    Intro(IntroSequenceState),
    MainMenu(MainMenuScene),
    HelpMenu(HelpMenuScene),
    SettingsMenu(SettingsMenuScene),
    DemoPlayback(DemoPlaybackState),
    Gameplay(DemoPlaybackState),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppTickResult {
    pub mode: AppMode,
    pub render_scene: RenderScene,
    pub audio_commands: Vec<AudioCommand>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttractModeApp {
    levels: Vec<Level>,
    mode: AppMode,
    current_level_index: usize,
    demo_level_index: usize,
    demo_recording: DemoRecording,
    demo_session: GameplaySession,
    gameplay_session: GameplaySession,
    intro_tick: usize,
    menu_idle_tick: usize,
    main_menu_cursor: MenuCursor,
    help_page: usize,
    intro_song_started: bool,
    intro_sample_started: bool,
    menu_song_started: bool,
    control_mode: ControlMode,
    settings_cursor: SettingsMenuCursor,
    sound_fx_enabled: bool,
    music_enabled: bool,
}

impl AttractModeApp {
    pub fn new(levels: Vec<Level>, demo_recording: DemoRecording) -> Self {
        assert!(
            !levels.is_empty(),
            "AttractModeApp requires at least one level"
        );
        let demo_level_index = 0usize;
        let demo_session = GameplaySession::new(levels[demo_level_index].clone());
        let gameplay_session = GameplaySession::new(levels[0].clone());
        Self {
            levels,
            mode: AppMode::Intro,
            current_level_index: 0,
            demo_level_index,
            demo_recording,
            demo_session,
            gameplay_session,
            intro_tick: 0,
            menu_idle_tick: 0,
            main_menu_cursor: MenuCursor::Start,
            help_page: 0,
            intro_song_started: false,
            intro_sample_started: false,
            menu_song_started: false,
            control_mode: ControlMode::Keyboard,
            settings_cursor: SettingsMenuCursor::Keyboard,
            sound_fx_enabled: true,
            music_enabled: true,
        }
    }

    pub fn mode(&self) -> AppMode {
        self.mode
    }

    pub fn control_mode(&self) -> ControlMode {
        self.control_mode
    }

    pub fn tick(&mut self, input: AppInput) -> AppTickResult {
        let mut audio_commands = Vec::new();
        match self.mode {
            AppMode::Intro => self.tick_intro(input, &mut audio_commands),
            AppMode::MainMenu => self.tick_main_menu(input, &mut audio_commands),
            AppMode::HelpMenu => self.tick_help_menu(input, &mut audio_commands),
            AppMode::SettingsMenu => self.tick_settings_menu(input, &mut audio_commands),
            AppMode::DemoPlayback => self.tick_demo(input, &mut audio_commands),
            AppMode::Gameplay => self.tick_gameplay(input, &mut audio_commands),
            AppMode::Boot | AppMode::GoMenu => {
                self.mode = AppMode::MainMenu;
            }
        }
        self.filter_audio_commands(&mut audio_commands);

        AppTickResult {
            mode: self.mode,
            render_scene: self.current_render_scene(),
            audio_commands,
        }
    }

    fn tick_intro(&mut self, input: AppInput, audio_commands: &mut Vec<AudioCommand>) {
        if !self.intro_song_started {
            audio_commands.push(AudioCommand::PlaySong(0));
            self.intro_song_started = true;
        }
        if !self.intro_sample_started && self.intro_tick >= INTRO_SOUND_DELAY_TICKS {
            audio_commands.push(AudioCommand::PlayIntroSample);
            self.intro_sample_started = true;
        }
        if self.intro_tick >= INTRO_SOUND_DELAY_TICKS && input.skip_requested() {
            self.enter_main_menu(audio_commands);
            return;
        }

        let final_credit_end = self.final_credit_end_tick();
        if self.intro_tick >= final_credit_end {
            self.enter_main_menu(audio_commands);
            return;
        }

        self.intro_tick += 1;
    }

    fn tick_main_menu(&mut self, input: AppInput, audio_commands: &mut Vec<AudioCommand>) {
        if !self.menu_song_started {
            audio_commands.push(AudioCommand::PlaySong(MENU_SONG_INDEX));
            self.menu_song_started = true;
        }

        let mut navigated = false;
        if input.up {
            self.main_menu_cursor = self.main_menu_cursor.move_by(-1);
            navigated = true;
        }
        if input.down {
            self.main_menu_cursor = self.main_menu_cursor.move_by(1);
            navigated = true;
        }

        if input.enter {
            self.menu_idle_tick = 0;
            match self.main_menu_cursor {
                MenuCursor::Start => self.start_gameplay(audio_commands),
                MenuCursor::Config => self.enter_settings_menu(),
                MenuCursor::Help => {
                    self.help_page = 0;
                    self.mode = AppMode::HelpMenu;
                }
            }
            return;
        }

        if navigated || input.escape || input.space {
            self.menu_idle_tick = 0;
        } else {
            self.menu_idle_tick += 1;
        }

        if self.menu_idle_tick >= MENU_IDLE_DEMO_TICKS {
            self.start_demo(audio_commands);
        }
    }

    fn tick_help_menu(&mut self, input: AppInput, _audio_commands: &mut Vec<AudioCommand>) {
        if input.escape {
            self.mode = AppMode::MainMenu;
            self.menu_idle_tick = 0;
            return;
        }
        if input.enter || input.space {
            self.help_page += 1;
            if self.help_page >= 3 {
                self.help_page = 0;
                self.mode = AppMode::MainMenu;
            }
            self.menu_idle_tick = 0;
        }
    }

    fn tick_settings_menu(&mut self, input: AppInput, audio_commands: &mut Vec<AudioCommand>) {
        if input.escape {
            self.mode = AppMode::MainMenu;
            self.menu_idle_tick = 0;
            return;
        }
        if input.left {
            self.settings_cursor = self
                .settings_cursor
                .move_in_direction(SettingsNavDirection::Left);
        }
        if input.right {
            self.settings_cursor = self
                .settings_cursor
                .move_in_direction(SettingsNavDirection::Right);
        }
        if input.up {
            self.settings_cursor = self.settings_cursor.move_in_direction(SettingsNavDirection::Up);
        }
        if input.down {
            self.settings_cursor = self
                .settings_cursor
                .move_in_direction(SettingsNavDirection::Down);
        }
        if input.enter || input.space {
            self.apply_settings_selection(audio_commands);
            self.menu_idle_tick = 0;
        }
    }

    fn tick_demo(&mut self, input: AppInput, audio_commands: &mut Vec<AudioCommand>) {
        if input.escape || input.enter || input.space {
            self.return_to_menu(audio_commands);
            return;
        }
        if sample_demo_input_for_ship(&self.demo_recording, self.demo_session.ship).is_none() {
            self.return_to_menu(audio_commands);
            return;
        }
        self.demo_session.run_demo_frame(&self.demo_recording);
    }

    fn tick_gameplay(&mut self, input: AppInput, audio_commands: &mut Vec<AudioCommand>) {
        if input.escape {
            self.return_to_menu(audio_commands);
            return;
        }

        if self.gameplay_session.did_win {
            if input.enter || input.space {
                self.current_level_index = (self.current_level_index + 1) % self.levels.len();
                self.start_gameplay(audio_commands);
            }
            return;
        }

        if self.gameplay_session.ship.state != crate::ShipState::Alive {
            if input.enter || input.space {
                self.start_gameplay(audio_commands);
            }
            return;
        }

        let result = self.gameplay_session.run_frame(input.gameplay_controls());
        emit_sfx_for_events(&result.events, audio_commands);
    }

    fn start_demo(&mut self, audio_commands: &mut Vec<AudioCommand>) {
        self.mode = AppMode::DemoPlayback;
        self.menu_idle_tick = 0;
        self.demo_session = GameplaySession::new(self.levels[self.demo_level_index].clone());
        audio_commands.push(AudioCommand::PlaySong(DEMO_SONG_INDEX));
    }

    fn start_gameplay(&mut self, audio_commands: &mut Vec<AudioCommand>) {
        self.mode = AppMode::Gameplay;
        self.menu_idle_tick = 0;
        self.gameplay_session = GameplaySession::new(self.levels[self.current_level_index].clone());
        audio_commands.push(AudioCommand::PlaySong(GAMEPLAY_SONG_INDEX));
    }

    fn enter_main_menu(&mut self, audio_commands: &mut Vec<AudioCommand>) {
        self.mode = AppMode::MainMenu;
        self.menu_idle_tick = 0;
        self.main_menu_cursor = MenuCursor::Start;
        self.menu_song_started = false;
        audio_commands.push(AudioCommand::PlaySong(MENU_SONG_INDEX));
        self.menu_song_started = true;
    }

    fn return_to_menu(&mut self, audio_commands: &mut Vec<AudioCommand>) {
        self.mode = AppMode::MainMenu;
        self.menu_idle_tick = 0;
        self.main_menu_cursor = MenuCursor::Start;
        self.menu_song_started = false;
        audio_commands.push(AudioCommand::PlaySong(MENU_SONG_INDEX));
        self.menu_song_started = true;
    }

    fn current_render_scene(&self) -> RenderScene {
        match self.mode {
            AppMode::Intro => RenderScene::Intro(self.current_intro_scene()),
            AppMode::MainMenu => RenderScene::MainMenu(MainMenuScene {
                selected: self.main_menu_cursor,
            }),
            AppMode::HelpMenu => RenderScene::HelpMenu(HelpMenuScene {
                page_index: self.help_page,
            }),
            AppMode::SettingsMenu => RenderScene::SettingsMenu(SettingsMenuScene {
                cursor: self.settings_cursor,
                control_mode: self.control_mode,
                sound_fx_enabled: self.sound_fx_enabled,
                music_enabled: self.music_enabled,
            }),
            AppMode::DemoPlayback => RenderScene::DemoPlayback(self.current_demo_scene()),
            AppMode::Gameplay => RenderScene::Gameplay(self.current_gameplay_scene()),
            AppMode::Boot | AppMode::GoMenu => RenderScene::MainMenu(MainMenuScene {
                selected: self.main_menu_cursor,
            }),
        }
    }

    fn enter_settings_menu(&mut self) {
        self.mode = AppMode::SettingsMenu;
        self.settings_cursor = SettingsMenuCursor::from_control_mode(self.control_mode);
    }

    fn apply_settings_selection(&mut self, audio_commands: &mut Vec<AudioCommand>) {
        if let Some(mode) = self.settings_cursor.selected_control_mode() {
            self.control_mode = ControlMode::from_dos_value(mode.dos_value());
            return;
        }
        match self.settings_cursor {
            SettingsMenuCursor::SoundFx => {
                self.sound_fx_enabled = !self.sound_fx_enabled;
                if !self.sound_fx_enabled {
                    audio_commands.push(AudioCommand::StopAllSamples);
                }
            }
            SettingsMenuCursor::Music => {
                self.music_enabled = !self.music_enabled;
                if self.music_enabled {
                    audio_commands.push(AudioCommand::PlaySong(MENU_SONG_INDEX));
                    self.menu_song_started = true;
                } else {
                    audio_commands.push(AudioCommand::StopSong);
                }
            }
            SettingsMenuCursor::Keyboard
            | SettingsMenuCursor::Joystick
            | SettingsMenuCursor::Mouse => {}
        }
    }

    fn filter_audio_commands(&self, audio_commands: &mut Vec<AudioCommand>) {
        audio_commands.retain(|command| match command {
            AudioCommand::PlaySong(_) => self.music_enabled,
            AudioCommand::PlayIntroSample | AudioCommand::PlaySfx(_) => self.sound_fx_enabled,
            AudioCommand::StopSong | AudioCommand::StopAllSamples => true,
        });
    }

    fn current_intro_scene(&self) -> IntroSequenceState {
        let anim_frame_count = 100usize;
        let credit_frame_count = 8usize;
        let title_start = INTRO_ANIM_START_TICKS + anim_frame_count;
        let credits_start = title_start + INTRO_TITLE_HOLD_TICKS;

        let background_brightness = (self.intro_tick as f32 / TICKS_PER_SECOND as f32).min(1.0);
        let anim_frame_index = self
            .intro_tick
            .checked_sub(INTRO_ANIM_START_TICKS)
            .filter(|index| *index < anim_frame_count);
        let title_progress = self
            .intro_tick
            .checked_sub(title_start)
            .map(|ticks| (ticks as f32 / (TICKS_PER_SECOND as f32 * 3.5)).min(1.0))
            .unwrap_or(0.0);

        let credit_ticks = self.intro_tick.saturating_sub(credits_start);
        let credit_frame_index = if self.intro_tick >= credits_start {
            Some((credit_ticks / CREDIT_FRAME_TICKS).min(credit_frame_count.saturating_sub(1)))
        } else {
            None
        };
        let credit_alpha = if self.intro_tick < credits_start {
            0.0
        } else {
            let seq = credit_ticks % CREDIT_FRAME_TICKS;
            if seq < TICKS_PER_SECOND {
                seq as f32 / TICKS_PER_SECOND as f32
            } else if seq > TICKS_PER_SECOND * 3 {
                (CREDIT_FRAME_TICKS - seq) as f32 / TICKS_PER_SECOND as f32
            } else {
                1.0
            }
        };

        IntroSequenceState {
            tick: self.intro_tick,
            background_brightness,
            title_progress,
            anim_frame_index,
            credit_frame_index,
            credit_alpha,
        }
    }

    fn current_demo_scene(&self) -> DemoPlaybackState {
        self.build_play_scene(&self.demo_session, true)
    }

    fn current_gameplay_scene(&self) -> DemoPlaybackState {
        self.build_play_scene(&self.gameplay_session, false)
    }

    fn build_play_scene(&self, session: &GameplaySession, is_demo: bool) -> DemoPlaybackState {
        let current_row = (session.ship.z_position * 8.0).floor().max(0.0) as usize;
        let current_group = current_row >> 3;
        let start_row = current_group.saturating_sub(RENDER_ROWS_BEHIND);
        let end_row = (current_group + RENDER_ROWS_AHEAD + 1).min(session.level.length());
        let rows = (start_row..end_row)
            .filter_map(|row_index| {
                session
                    .level
                    .row(row_index)
                    .copied()
                    .map(|cells| RoadRenderRow { row_index, cells })
            })
            .collect::<Vec<_>>();

        DemoPlaybackState {
            world_index: world_index_for_level(session.level.road_index),
            gravity: session.level.gravity,
            level_length: session.level.length(),
            frame_index: session.frame_index(),
            current_row,
            fractional_z: session.ship.z_position - (current_row as f64 / 8.0),
            rows,
            did_win: session.did_win,
            is_demo,
            craft_state: session.ship.state,
            snapshot: crate::GameSnapshot {
                x_position: session.ship.x_position,
                y_position: session.ship.y_position,
                z_position: session.ship.z_position,
                z_velocity: session.ship.z_velocity + session.ship.jump_o_master_velocity_delta,
                craft_state: session.ship.state,
                oxygen_percent: session.ship.oxygen_remaining / 0x7530 as f64,
                fuel_percent: session.ship.fuel_remaining / 0x7530 as f64,
                jump_o_master_in_use: session.ship.jump_o_master_in_use,
                jump_o_master_velocity_delta: session.ship.jump_o_master_velocity_delta,
            },
            ship: build_ship_render_state(session),
        }
    }

    fn final_credit_end_tick(&self) -> usize {
        let anim_frame_count = 100usize;
        let credit_frame_count = 8usize;
        INTRO_ANIM_START_TICKS
            + anim_frame_count
            + INTRO_TITLE_HOLD_TICKS
            + CREDIT_FRAME_TICKS * credit_frame_count
    }
}

fn world_index_for_level(level_index: usize) -> usize {
    if level_index == 0 {
        0
    } else {
        (level_index - 1) / 3
    }
}

fn axis(negative: bool, positive: bool) -> i8 {
    match (negative, positive) {
        (true, false) => -1,
        (false, true) => 1,
        _ => 0,
    }
}

fn emit_sfx_for_events(events: &[GameplayEvent], audio_commands: &mut Vec<AudioCommand>) {
    for event in events {
        let sfx = match event {
            GameplayEvent::ShipBumpedWall => Some(0),
            GameplayEvent::ShipExploded => Some(1),
            GameplayEvent::ShipBounced => Some(3),
            GameplayEvent::ShipRefilled => Some(4),
        };
        if let Some(sfx) = sfx {
            audio_commands.push(AudioCommand::PlaySfx(sfx));
        }
    }
}

fn build_ship_render_state(session: &GameplaySession) -> ShipRenderState {
    ShipRenderState {
        x_position: session.ship.x_position,
        y_position: session.ship.y_position,
        z_position: session.ship.z_position,
        y_velocity: session.ship.y_velocity,
        z_velocity: session.ship.z_velocity + session.ship.jump_o_master_velocity_delta,
        state: session.ship.state,
        is_on_ground: session.ship.is_on_ground,
        is_going_up: session.ship.is_going_up,
        turn_input: session.last_controls.turn_input,
        accel_input: session.last_controls.accel_input,
        jump_input: session.last_controls.jump_input,
        death_frame_index: session.death_frame_index,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use skyroads_data::{level_from_road_entry, load_demo_rec_path, load_roads_lzs_path};

    use super::{
        AppInput, AppMode, AttractModeApp, AudioCommand, ControlMode, MenuCursor, RenderScene,
        SettingsMenuCursor, GAMEPLAY_SONG_INDEX, RENDER_ROWS_BEHIND,
    };

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn make_app() -> AttractModeApp {
        let roads = load_roads_lzs_path(repo_root().join("ROADS.LZS")).unwrap();
        let demo = load_demo_rec_path(repo_root().join("DEMO.REC")).unwrap();
        let levels = roads
            .roads
            .iter()
            .map(level_from_road_entry)
            .collect::<Vec<_>>();
        AttractModeApp::new(levels, demo)
    }

    #[test]
    fn intro_starts_with_intro_scene_and_song() {
        let mut app = make_app();
        let tick = app.tick(AppInput::default());
        assert_eq!(tick.mode, AppMode::Intro);
        assert_eq!(tick.audio_commands, vec![AudioCommand::PlaySong(0)]);
        assert!(matches!(tick.render_scene, RenderScene::Intro(_)));
    }

    #[test]
    fn skip_exits_intro_to_main_menu() {
        let mut app = make_app();
        for _ in 0..35 {
            app.tick(AppInput::default());
        }
        let tick = app.tick(AppInput {
            space: true,
            ..AppInput::default()
        });
        assert_eq!(tick.mode, AppMode::MainMenu);
        assert!(matches!(tick.render_scene, RenderScene::MainMenu(_)));
        assert_eq!(
            tick.audio_commands,
            vec![AudioCommand::PlayIntroSample, AudioCommand::PlaySong(1)]
        );
    }

    #[test]
    fn idle_menu_enters_demo_playback() {
        let mut app = make_app();
        for _ in 0..35 {
            app.tick(AppInput::default());
        }
        app.tick(AppInput {
            space: true,
            ..AppInput::default()
        });
        for _ in 0..(70 * 5) {
            app.tick(AppInput::default());
        }
        let tick = app.tick(AppInput::default());
        assert_eq!(tick.mode, AppMode::DemoPlayback);
        assert!(matches!(tick.render_scene, RenderScene::DemoPlayback(_)));
    }

    #[test]
    fn start_menu_entry_launches_gameplay() {
        let mut app = make_app();
        for _ in 0..35 {
            app.tick(AppInput::default());
        }
        app.tick(AppInput {
            space: true,
            ..AppInput::default()
        });
        let tick = app.tick(AppInput {
            enter: true,
            ..AppInput::default()
        });
        assert_eq!(tick.mode, AppMode::Gameplay);
        assert!(matches!(tick.render_scene, RenderScene::Gameplay(_)));
        assert_eq!(
            tick.audio_commands,
            vec![AudioCommand::PlaySong(GAMEPLAY_SONG_INDEX)]
        );
    }

    #[test]
    fn help_menu_cycles_back_to_main_menu() {
        let mut app = make_app();
        for _ in 0..35 {
            app.tick(AppInput::default());
        }
        app.tick(AppInput {
            space: true,
            ..AppInput::default()
        });
        app.tick(AppInput {
            down: true,
            ..AppInput::default()
        });
        let main_menu = app.tick(AppInput {
            down: true,
            ..AppInput::default()
        });
        match main_menu.render_scene {
            RenderScene::MainMenu(scene) => assert_eq!(scene.selected, MenuCursor::Help),
            other => panic!("unexpected render scene: {other:?}"),
        }
        assert_eq!(app.mode(), AppMode::MainMenu);

        let help = app.tick(AppInput {
            enter: true,
            ..AppInput::default()
        });
        assert_eq!(help.mode, AppMode::HelpMenu);

        app.tick(AppInput {
            enter: true,
            ..AppInput::default()
        });
        app.tick(AppInput {
            enter: true,
            ..AppInput::default()
        });
        let back = app.tick(AppInput {
            enter: true,
            ..AppInput::default()
        });
        assert_eq!(back.mode, AppMode::MainMenu);
    }

    #[test]
    fn gameplay_scene_tracks_absolute_rows_and_ship_bank() {
        let mut app = make_app();
        for _ in 0..35 {
            app.tick(AppInput::default());
        }
        app.tick(AppInput {
            space: true,
            ..AppInput::default()
        });
        app.tick(AppInput {
            enter: true,
            ..AppInput::default()
        });
        let tick = app.tick(AppInput {
            up_held: true,
            right_held: true,
            ..AppInput::default()
        });
        match tick.render_scene {
            RenderScene::Gameplay(scene) => {
                assert_eq!(
                    scene.rows.first().unwrap().row_index,
                    (scene.current_row >> 3).saturating_sub(RENDER_ROWS_BEHIND)
                );
                assert_eq!(scene.ship.turn_input, 1);
                assert_eq!(scene.ship.accel_input, 1);
            }
            other => panic!("unexpected render scene: {other:?}"),
        }
    }

    #[test]
    fn gameplay_controls_override_takes_precedence_over_keyboard_holds() {
        let input = AppInput {
            up_held: true,
            right_held: true,
            gameplay_controls_override: Some(crate::ControllerState::new(-1, -1, true)),
            ..AppInput::default()
        };
        assert_eq!(
            input.gameplay_controls(),
            crate::ControllerState::new(-1, -1, true)
        );
    }

    #[test]
    fn settings_menu_reflects_current_runtime_settings() {
        let mut app = make_app();
        for _ in 0..35 {
            app.tick(AppInput::default());
        }
        app.tick(AppInput {
            space: true,
            ..AppInput::default()
        });
        app.tick(AppInput {
            down: true,
            ..AppInput::default()
        });
        let settings = app.tick(AppInput {
            enter: true,
            ..AppInput::default()
        });
        match settings.render_scene {
            RenderScene::SettingsMenu(scene) => {
                assert_eq!(scene.cursor, SettingsMenuCursor::Keyboard);
                assert_eq!(scene.control_mode, ControlMode::Keyboard);
                assert!(scene.sound_fx_enabled);
                assert!(scene.music_enabled);
            }
            other => panic!("unexpected render scene: {other:?}"),
        }
    }

    #[test]
    fn settings_menu_can_switch_control_mode_and_toggle_music() {
        let mut app = make_app();
        for _ in 0..35 {
            app.tick(AppInput::default());
        }
        app.tick(AppInput {
            space: true,
            ..AppInput::default()
        });
        app.tick(AppInput {
            down: true,
            ..AppInput::default()
        });
        app.tick(AppInput {
            enter: true,
            ..AppInput::default()
        });

        app.tick(AppInput {
            right: true,
            ..AppInput::default()
        });
        let joystick_hover = app.tick(AppInput::default());
        match joystick_hover.render_scene {
            RenderScene::SettingsMenu(scene) => {
                assert_eq!(scene.cursor, SettingsMenuCursor::Joystick);
                assert_eq!(scene.control_mode, ControlMode::Keyboard);
            }
            other => panic!("unexpected render scene: {other:?}"),
        }
        let mouse_select = app.tick(AppInput {
            right: true,
            enter: true,
            ..AppInput::default()
        });
        assert!(mouse_select.audio_commands.is_empty());
        assert_eq!(app.control_mode(), ControlMode::Mouse);
        match mouse_select.render_scene {
            RenderScene::SettingsMenu(scene) => {
                assert_eq!(scene.cursor, SettingsMenuCursor::Mouse);
                assert_eq!(scene.control_mode, ControlMode::Mouse);
            }
            other => panic!("unexpected render scene: {other:?}"),
        }

        let music_toggle = app.tick(AppInput {
            down: true,
            enter: true,
            ..AppInput::default()
        });
        assert_eq!(music_toggle.audio_commands, vec![AudioCommand::StopSong]);
        match music_toggle.render_scene {
            RenderScene::SettingsMenu(scene) => {
                assert_eq!(scene.cursor, SettingsMenuCursor::Music);
                assert!(!scene.music_enabled);
                assert!(scene.sound_fx_enabled);
            }
            other => panic!("unexpected render scene: {other:?}"),
        }
    }

    #[test]
    fn settings_menu_can_toggle_sound_fx() {
        let mut app = make_app();
        for _ in 0..35 {
            app.tick(AppInput::default());
        }
        app.tick(AppInput {
            space: true,
            ..AppInput::default()
        });
        app.tick(AppInput {
            down: true,
            ..AppInput::default()
        });
        app.tick(AppInput {
            enter: true,
            ..AppInput::default()
        });

        let sound_toggle = app.tick(AppInput {
            down: true,
            enter: true,
            ..AppInput::default()
        });
        assert_eq!(sound_toggle.audio_commands, vec![AudioCommand::StopAllSamples]);
        match sound_toggle.render_scene {
            RenderScene::SettingsMenu(scene) => {
                assert_eq!(scene.cursor, SettingsMenuCursor::SoundFx);
                assert!(!scene.sound_fx_enabled);
                assert!(scene.music_enabled);
            }
            other => panic!("unexpected render scene: {other:?}"),
        }
    }

    #[test]
    fn exploding_ship_render_state_tracks_death_frame() {
        let mut app = make_app();
        app.gameplay_session.ship.state = crate::ShipState::Exploded;
        app.gameplay_session.death_frame_index = Some(5);
        app.gameplay_session.frame_index = 14;
        let scene = app.current_gameplay_scene();
        assert_eq!(scene.ship.state, crate::ShipState::Exploded);
        assert_eq!(scene.ship.death_frame_index, Some(5));
    }

    #[test]
    fn gameplay_scene_current_row_uses_dos_eighth_tile_units() {
        let mut app = make_app();
        app.gameplay_session.ship.z_position = 3.375;
        let scene = app.current_gameplay_scene();
        assert_eq!(scene.current_row, 27);
        assert!((scene.fractional_z - 0.0).abs() < f64::EPSILON);
        assert_eq!(
            scene.rows.first().unwrap().row_index,
            (scene.current_row >> 3).saturating_sub(RENDER_ROWS_BEHIND)
        );
    }
}
