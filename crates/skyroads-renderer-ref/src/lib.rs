use std::path::Path;

use skyroads_core::{
    renderer_row_state, ControlMode, DemoPlaybackState, HelpMenuScene, IntroSequenceState,
    MainMenuScene, RenderScene, RoadRenderRow, SettingsMenuCursor, SettingsMenuScene,
};
use skyroads_data::{
    load_dashboard_dat_path, load_image_archive_path, load_skyroads_exe_path,
    load_trekdat_lzs_path, ExeShipRuntimeTables, HudFragmentPack, ImageArchive, ImageFrame,
    LevelCell, Result, RgbColor, TouchEffect, TrekdatArchive, TrekdatCellPointers, TrekdatRecord,
    TrekdatShape, DASHBOARD_COLORS, DOS_SHIP_LANE_COUNT, DOS_SHIP_SHADOW_MASK_HEIGHT,
    DOS_SHIP_SHADOW_MASK_WIDTH, GROUND_Y, LEVEL_CENTER_X, LEVEL_MAX_X, LEVEL_MIN_X,
    LEVEL_TILE_STRIDE_X, ROAD_COLUMNS, SCREEN_HEIGHT, SCREEN_WIDTH,
};

const FRAMEBUFFER_WIDTH: usize = SCREEN_WIDTH as usize;
const FRAMEBUFFER_HEIGHT: usize = SCREEN_HEIGHT as usize;
const DASHBOARD_TOP: usize = 138;
const HORIZON_Y: usize = 24;
const VIEW_BOTTOM_Y: usize = DASHBOARD_TOP;
const SHIP_SCALE: usize = 1;
const DOS_EXPLOSION_ANIMATION_TICKS: usize = 0x2A;
const DOS_NON_ALIVE_ANIMATION_TICKS: usize = 0x6C;
const DOS_SHIP_SPRITE_WIDTH: usize = 29;
const DOS_SHIP_CLIP_MASK_HEIGHT: usize = 33;
const DOS_SHIP_CLIP_MASK_BYTES: usize = DOS_SHIP_SPRITE_WIDTH * DOS_SHIP_CLIP_MASK_HEIGHT;
const DOS_SHIP_MASK_ROW_STRIDE: usize = DOS_SHIP_SPRITE_WIDTH;
const DOS_SHIP_SHADOW_VARIANTS: usize = 5;
const DOS_EXACT_SHIP_FRAME_START: usize = 18;
const DOS_EXACT_SHIP_FRAME_COUNT: usize = 63;
const DOS_SHIP_X_BASE_OFFSET: i32 = 0x6E;
const DOS_SHIP_CENTER_X_OFFSET: i32 = 96;
const DOS_SHIP_TOP_BASE: i32 = 0x9D;
const DOS_SHIP_CENTER_Y_OFFSET: i32 = 12;
const DOS_SHIP_SHADOW_TOP_OFFSET: i32 = 16;
const DEBUG_PANEL_X: i32 = 8;
const DEBUG_PANEL_Y: i32 = 8;
const DEBUG_PANEL_W: i32 = 124;
const DEBUG_PANEL_H: i32 = 42;
const DEBUG_TOPDOWN_INSET_X: i32 = 206;
const DEBUG_TOPDOWN_INSET_Y: i32 = 28;
const DEBUG_TOPDOWN_INSET_W: i32 = 104;
const DEBUG_TOPDOWN_INSET_H: i32 = 84;
const SETTINGS_WIDGET_TOP: i32 = 146;
const SETTINGS_WIDGET_WIDTH: i32 = 86;
const SETTINGS_WIDGET_HEIGHT: i32 = 22;
const SETTINGS_WIDGET_STATUS_WIDTH: i32 = 28;
const SETTINGS_WIDGET_STATUS_HEIGHT: i32 = 10;

const SETTINGS_CURSOR_WHITE: RgbColor = RgbColor::new(218, 218, 218);
const SETTINGS_SELECTED_ORANGE: RgbColor = RgbColor::new(255, 80, 0);
const SETTINGS_WIDGET_BG: RgbColor = RgbColor::new(10, 16, 34);
const SETTINGS_WIDGET_OUTLINE: RgbColor = RgbColor::new(76, 86, 120);
const SETTINGS_WIDGET_TEXT_DIM: RgbColor = RgbColor::new(168, 176, 190);
const TEXT_SHADOW: RgbColor = RgbColor::new(0, 0, 0);

const DOS_LEFT_CELL_COLUMNS: [usize; 4] = [0, 1, 2, 3];
const DOS_RIGHT_CELL_COLUMNS: [usize; 4] = [6, 5, 4, 3];

#[derive(Debug, Clone, PartialEq, Eq)]
struct RoadSpan {
    start_column: usize,
    end_column_exclusive: usize,
    sample_cell: LevelCell,
}

#[derive(Debug, Clone, PartialEq)]
struct ProjectedRoadSpan {
    top_start: f32,
    top_end: f32,
    bottom_start: f32,
    bottom_end: f32,
    sample_cell: LevelCell,
}

#[derive(Debug, Clone, PartialEq)]
struct ProjectedObstacle {
    column_start: f32,
    column_end: f32,
    height_factor: f32,
    color: RgbColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RoadCellBytes {
    byte0: u8,
    byte1: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DosRenderSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PrimitiveCursor {
    next_offset: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TrekdatProjectionKey {
    depth_index: usize,
    road_row_group: usize,
    trekdat_slot: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct ProjectedRoadSlice {
    trekdat_key: TrekdatProjectionKey,
    top_y: usize,
    bottom_y: usize,
    center_top: f32,
    center_bottom: f32,
    width_top: f32,
    width_bottom: f32,
    spans: Vec<ProjectedRoadSpan>,
    obstacles: Vec<ProjectedObstacle>,
    tunnel_span: Option<(f32, f32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CarAtlas {
    explosion_frames: Vec<ImageFrame>,
    exact_ship_frames_raw: Vec<ImageFrame>,
    exact_ship_frames: Vec<ImageFrame>,
    alive_left: Vec<ImageFrame>,
    alive_center: Vec<ImageFrame>,
    alive_right: Vec<ImageFrame>,
    jump_left: Vec<ImageFrame>,
    jump_center: Vec<ImageFrame>,
    jump_right: Vec<ImageFrame>,
    destroyed: ImageFrame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShipSpriteKind {
    Alive,
    Exploding,
    Destroyed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShipBank {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DerivedShipVisualState {
    sprite_kind: ShipSpriteKind,
    bank: ShipBank,
    jumping: bool,
    explosion_frame: usize,
    exact_ship_frame_index: Option<usize>,
    on_surface: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShipScreenPlacement {
    sprite_left: i32,
    sprite_top: i32,
    sprite_center_x: i32,
    sprite_center_y: i32,
    shadow_left: i32,
    shadow_top: i32,
    shadow_center_x: i32,
    shadow_center_y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DosShipPipeline {
    placement: ShipScreenPlacement,
    clip_mask: [u8; DOS_SHIP_CLIP_MASK_BYTES],
    shadow_surface_mask: [u8; DOS_SHIP_SHADOW_MASK_HEIGHT * DOS_SHIP_SHADOW_MASK_WIDTH],
    shadow_variant: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameBuffer320x200 {
    pub width: u16,
    pub height: u16,
    pub pixels_rgba: Vec<u8>,
}

impl FrameBuffer320x200 {
    pub fn new() -> Self {
        Self {
            width: SCREEN_WIDTH,
            height: SCREEN_HEIGHT,
            pixels_rgba: vec![0; FRAMEBUFFER_WIDTH * FRAMEBUFFER_HEIGHT * 4],
        }
    }

    pub fn clear(&mut self, color: RgbColor) {
        for pixel in self.pixels_rgba.chunks_exact_mut(4) {
            pixel[0] = color.r;
            pixel[1] = color.g;
            pixel[2] = color.b;
            pixel[3] = 255;
        }
    }

    fn fill_rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: RgbColor) {
        let x0 = x.max(0) as usize;
        let y0 = y.max(0) as usize;
        let x1 = (x + width).min(FRAMEBUFFER_WIDTH as i32).max(0) as usize;
        let y1 = (y + height).min(FRAMEBUFFER_HEIGHT as i32).max(0) as usize;
        for yy in y0..y1 {
            for xx in x0..x1 {
                self.set_pixel(xx, yy, color);
            }
        }
    }

    fn stroke_rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: RgbColor) {
        if width <= 0 || height <= 0 {
            return;
        }
        self.fill_rect(x, y, width, 1, color);
        self.fill_rect(x, y + height - 1, width, 1, color);
        if height > 2 {
            self.fill_rect(x, y + 1, 1, height - 2, color);
            self.fill_rect(x + width - 1, y + 1, 1, height - 2, color);
        }
    }

    fn set_pixel(&mut self, x: usize, y: usize, color: RgbColor) {
        if x >= FRAMEBUFFER_WIDTH || y >= FRAMEBUFFER_HEIGHT {
            return;
        }
        let offset = (y * FRAMEBUFFER_WIDTH + x) * 4;
        self.pixels_rgba[offset] = color.r;
        self.pixels_rgba[offset + 1] = color.g;
        self.pixels_rgba[offset + 2] = color.b;
        self.pixels_rgba[offset + 3] = 255;
    }

    fn blend_pixel(&mut self, x: usize, y: usize, color: RgbColor, alpha: f32) {
        if x >= FRAMEBUFFER_WIDTH || y >= FRAMEBUFFER_HEIGHT {
            return;
        }
        let alpha = alpha.clamp(0.0, 1.0);
        let offset = (y * FRAMEBUFFER_WIDTH + x) * 4;
        let dr = self.pixels_rgba[offset] as f32;
        let dg = self.pixels_rgba[offset + 1] as f32;
        let db = self.pixels_rgba[offset + 2] as f32;
        self.pixels_rgba[offset] = (dr * (1.0 - alpha) + color.r as f32 * alpha).round() as u8;
        self.pixels_rgba[offset + 1] = (dg * (1.0 - alpha) + color.g as f32 * alpha).round() as u8;
        self.pixels_rgba[offset + 2] = (db * (1.0 - alpha) + color.b as f32 * alpha).round() as u8;
        self.pixels_rgba[offset + 3] = 255;
    }

    fn darken_pixel(&mut self, x: i32, y: i32, factor: f32) {
        if x < 0 || y < 0 {
            return;
        }
        let x = x as usize;
        let y = y as usize;
        if x >= FRAMEBUFFER_WIDTH || y >= FRAMEBUFFER_HEIGHT {
            return;
        }
        let factor = factor.clamp(0.0, 1.0);
        let offset = (y * FRAMEBUFFER_WIDTH + x) * 4;
        self.pixels_rgba[offset] = (self.pixels_rgba[offset] as f32 * factor).round() as u8;
        self.pixels_rgba[offset + 1] = (self.pixels_rgba[offset + 1] as f32 * factor).round() as u8;
        self.pixels_rgba[offset + 2] = (self.pixels_rgba[offset + 2] as f32 * factor).round() as u8;
        self.pixels_rgba[offset + 3] = 255;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttractModeAssets {
    pub intro: ImageArchive,
    pub anim: ImageArchive,
    pub main_menu: ImageArchive,
    pub help_menu: ImageArchive,
    pub settings_menu: ImageArchive,
    pub go_menu: ImageArchive,
    pub cars: ImageArchive,
    pub worlds: Vec<ImageArchive>,
    pub dashboard: ImageArchive,
    pub trekdat: TrekdatArchive,
    pub oxygen_gauge: HudFragmentPack,
    pub fuel_gauge: HudFragmentPack,
    pub speed_gauge: HudFragmentPack,
    pub dos_ship_tables: ExeShipRuntimeTables,
}

impl AttractModeAssets {
    pub fn load_from_root(source_root: impl AsRef<Path>) -> Result<Self> {
        let source_root = source_root.as_ref();
        let worlds = (0..=9)
            .map(|index| load_image_archive_path(source_root.join(format!("WORLD{index}.LZS"))))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            intro: load_image_archive_path(source_root.join("INTRO.LZS"))?,
            anim: load_image_archive_path(source_root.join("ANIM.LZS"))?,
            main_menu: load_image_archive_path(source_root.join("MAINMENU.LZS"))?,
            help_menu: load_image_archive_path(source_root.join("HELPMENU.LZS"))?,
            settings_menu: load_image_archive_path(source_root.join("SETMENU.LZS"))?,
            go_menu: load_image_archive_path(source_root.join("GOMENU.LZS"))?,
            cars: load_image_archive_path(source_root.join("CARS.LZS"))?,
            worlds,
            dashboard: load_image_archive_path(source_root.join("DASHBRD.LZS"))?,
            trekdat: load_trekdat_lzs_path(source_root.join("TREKDAT.LZS"))?,
            oxygen_gauge: load_dashboard_dat_path(source_root.join("OXY_DISP.DAT"))?,
            fuel_gauge: load_dashboard_dat_path(source_root.join("FUL_DISP.DAT"))?,
            speed_gauge: load_dashboard_dat_path(source_root.join("SPEED.DAT"))?,
            dos_ship_tables: load_skyroads_exe_path(source_root.join("SKYROADS.EXE"))?
                .runtime_tables
                .ship,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceRenderer {
    assets: AttractModeAssets,
    car_atlas: Option<CarAtlas>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugViewMode {
    Off,
    Overlay,
    Geometry,
    TopDown,
}

impl DebugViewMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Overlay,
            Self::Overlay => Self::Geometry,
            Self::Geometry => Self::TopDown,
            Self::TopDown => Self::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Normal",
            Self::Overlay => "Overlay",
            Self::Geometry => "Geometry",
            Self::TopDown => "TopDown",
        }
    }
}

impl ReferenceRenderer {
    pub fn new(assets: AttractModeAssets) -> Self {
        let car_atlas = CarAtlas::from_archive(&assets.cars);
        Self { assets, car_atlas }
    }

    pub fn assets(&self) -> &AttractModeAssets {
        &self.assets
    }

    pub fn render_scene(&self, scene: &RenderScene) -> FrameBuffer320x200 {
        self.render_scene_with_debug(scene, DebugViewMode::Off)
    }

    pub fn render_scene_with_debug(
        &self,
        scene: &RenderScene,
        debug_view: DebugViewMode,
    ) -> FrameBuffer320x200 {
        let mut frame = FrameBuffer320x200::new();
        match scene {
            RenderScene::Intro(scene) => self.render_intro(&mut frame, scene),
            RenderScene::MainMenu(scene) => self.render_main_menu(&mut frame, scene),
            RenderScene::HelpMenu(scene) => self.render_help_menu(&mut frame, scene),
            RenderScene::SettingsMenu(scene) => self.render_settings_menu(&mut frame, scene),
            RenderScene::DemoPlayback(scene) => {
                self.render_play_scene_with_debug(&mut frame, scene, debug_view)
            }
            RenderScene::Gameplay(scene) => {
                self.render_play_scene_with_debug(&mut frame, scene, debug_view)
            }
        }
        frame
    }

    fn render_intro(&self, frame: &mut FrameBuffer320x200, scene: &IntroSequenceState) {
        frame.clear(RgbColor::new(0, 0, 0));
        self.draw_archive_frame(
            frame,
            &self.assets.intro,
            0,
            1.0,
            scene.background_brightness,
        );
        if let Some(anim_frame_index) = scene.anim_frame_index {
            let frame_index = anim_frame_index.min(self.assets.anim.frames.len().saturating_sub(1));
            self.draw_archive_frame(frame, &self.assets.anim, frame_index, 1.0, 1.0);
        }
        if let Some(credit_index) = scene.credit_frame_index {
            let intro_index =
                (credit_index + 2).min(self.assets.intro.frames.len().saturating_sub(1));
            self.draw_archive_frame(
                frame,
                &self.assets.intro,
                intro_index,
                scene.credit_alpha,
                1.0,
            );
        }
        if scene.title_progress > 0.0 {
            self.draw_archive_frame_reveal(frame, &self.assets.intro, 1, scene.title_progress, 1.0);
        }
        if scene.title_progress >= 0.98 && scene.credit_frame_index.is_none() {
            self.draw_branding(frame, 186, 1, 0.8);
        }
    }

    fn render_main_menu(&self, frame: &mut FrameBuffer320x200, scene: &MainMenuScene) {
        frame.clear(RgbColor::new(0, 0, 0));
        self.draw_archive_frame(frame, &self.assets.intro, 0, 1.0, 1.0);
        self.draw_archive_frame(frame, &self.assets.intro, 1, 1.0, 1.0);
        self.draw_archive_frame(
            frame,
            &self.assets.main_menu,
            scene.selected.index(),
            1.0,
            1.0,
        );
        self.draw_branding(frame, 184, 2, 1.0);
    }

    fn render_help_menu(&self, frame: &mut FrameBuffer320x200, scene: &HelpMenuScene) {
        frame.clear(RgbColor::new(0, 0, 0));
        let page_index = scene
            .page_index
            .min(self.assets.help_menu.frames.len().saturating_sub(1));
        self.draw_archive_frame(frame, &self.assets.help_menu, page_index, 1.0, 1.0);
    }

    fn render_settings_menu(&self, frame: &mut FrameBuffer320x200, scene: &SettingsMenuScene) {
        frame.clear(RgbColor::new(0, 0, 0));
        self.draw_archive_frame(frame, &self.assets.settings_menu, 0, 1.0, 1.0);
        self.draw_archive_frame(
            frame,
            &self.assets.settings_menu,
            settings_menu_selected_control_overlay(scene.control_mode),
            1.0,
            1.0,
        );
        if scene.sound_fx_enabled {
            self.draw_archive_frame(frame, &self.assets.settings_menu, 9, 1.0, 1.0);
        }
        if scene.music_enabled {
            self.draw_archive_frame(frame, &self.assets.settings_menu, 10, 1.0, 1.0);
        }
        if let Some(frame_index) = scene.cursor.setmenu_overlay_frame_index() {
            self.draw_archive_frame(frame, &self.assets.settings_menu, frame_index, 1.0, 1.0);
        }
        self.draw_settings_display_toggle(
            frame,
            "FULLSCREEN",
            120,
            scene.display_settings.fullscreen,
            scene.cursor == SettingsMenuCursor::Fullscreen,
        );
        self.draw_settings_display_toggle(
            frame,
            "BORDERLESS",
            208,
            scene.display_settings.borderless,
            scene.cursor == SettingsMenuCursor::Borderless,
        );
    }

    fn render_play_scene(&self, frame: &mut FrameBuffer320x200, scene: &DemoPlaybackState) {
        let ship_visual = derive_ship_visual_state(scene, &self.assets.dos_ship_tables);
        let road_coverage = self.build_dos_road_coverage_frame(scene);
        let mut ship_pipeline = build_dos_ship_pipeline(
            scene,
            &self.assets.dos_ship_tables,
            road_coverage.as_ref(),
            &ship_visual,
        );
        frame.clear(RgbColor::new(0, 0, 0));
        let world = self
            .assets
            .worlds
            .get(scene.world_index)
            .or_else(|| self.assets.worlds.first());
        if let Some(world) = world {
            self.draw_archive_frame(frame, world, 0, 1.0, 1.0);
        }
        let drew_dos_road = self.draw_demo_rows_before_ship(frame, scene);
        if !drew_dos_road {
            self.draw_demo_rows_fallback(frame, scene);
        }
        self.draw_ship_shadow(frame, &ship_visual, &ship_pipeline);
        self.draw_ship_sprite(frame, scene.frame_index, &ship_visual, &mut ship_pipeline);
        if drew_dos_road {
            self.draw_demo_rows_after_ship(frame, scene);
        }
        self.draw_archive_frame(frame, &self.assets.dashboard, 0, 1.0, 1.0);
        self.draw_gauge(
            &mut *frame,
            &self.assets.oxygen_gauge,
            scene.snapshot.oxygen_percent,
        );
        self.draw_gauge(
            &mut *frame,
            &self.assets.fuel_gauge,
            scene.snapshot.fuel_percent,
        );
        let speed = scene.snapshot.z_velocity / (0x2AAA as f64 / 0x10000 as f64);
        self.draw_gauge(&mut *frame, &self.assets.speed_gauge, speed);
        if scene.did_win {
            self.draw_archive_frame(frame, &self.assets.go_menu, 1, 1.0, 1.0);
        } else if should_draw_game_over_overlay(scene) {
            self.draw_archive_frame(frame, &self.assets.go_menu, 0, 1.0, 1.0);
        }
    }

    fn render_play_scene_with_debug(
        &self,
        frame: &mut FrameBuffer320x200,
        scene: &DemoPlaybackState,
        debug_view: DebugViewMode,
    ) {
        match debug_view {
            DebugViewMode::Off => self.render_play_scene(frame, scene),
            DebugViewMode::Overlay => {
                self.render_play_scene(frame, scene);
                self.draw_debug_overlay(frame, scene);
            }
            DebugViewMode::Geometry => {
                self.render_play_geometry_debug(frame, scene);
            }
            DebugViewMode::TopDown => {
                self.render_play_topdown_debug(frame, scene);
            }
        }
    }

    fn draw_demo_rows_before_ship(
        &self,
        frame: &mut FrameBuffer320x200,
        scene: &DemoPlaybackState,
    ) -> bool {
        let Some(record) = self.assets.trekdat.records.get(scene.current_row & 7) else {
            return false;
        };
        draw_dos_trekdat_pass(frame, scene, record, DosRoadPhase::BeforeShip)
    }

    fn draw_demo_rows_after_ship(&self, frame: &mut FrameBuffer320x200, scene: &DemoPlaybackState) {
        let Some(record) = self.assets.trekdat.records.get(scene.current_row & 7) else {
            return;
        };
        let _ = draw_dos_trekdat_pass(frame, scene, record, DosRoadPhase::AfterShip);
    }

    fn build_dos_road_coverage_frame(
        &self,
        scene: &DemoPlaybackState,
    ) -> Option<FrameBuffer320x200> {
        let record = self.assets.trekdat.records.get(scene.current_row & 7)?;
        let mut coverage = FrameBuffer320x200::new();
        coverage.clear(RgbColor::new(0, 0, 0));
        let _ = draw_dos_trekdat_pass(&mut coverage, scene, record, DosRoadPhase::BeforeShip);
        let _ = draw_dos_trekdat_pass(&mut coverage, scene, record, DosRoadPhase::AfterShip);
        Some(coverage)
    }

    fn draw_demo_rows_fallback(&self, frame: &mut FrameBuffer320x200, scene: &DemoPlaybackState) {
        for slice in project_road_slices(scene) {
            self.draw_projected_slice(frame, &slice);
        }
    }

    fn draw_ship_sprite(
        &self,
        frame: &mut FrameBuffer320x200,
        frame_index: usize,
        visual: &DerivedShipVisualState,
        pipeline: &mut DosShipPipeline,
    ) {
        let Some(car_atlas) = &self.car_atlas else {
            return;
        };

        if visual.sprite_kind == ShipSpriteKind::Alive {
            if let Some(index) = visual.exact_ship_frame_index {
                if let Some(sprite) = car_atlas.exact_ship_frames_raw.get(index) {
                    self.draw_sprite_with_clip_mask(
                        frame,
                        sprite,
                        pipeline.placement.sprite_left,
                        pipeline.placement.sprite_top,
                        0,
                        0,
                        &mut pipeline.clip_mask,
                    );
                    return;
                }
            }
        }

        let placement = pipeline.placement;
        let sprite = car_atlas.select_sprite(visual, frame_index);
        let draw_width = usize::from(sprite.width) * SHIP_SCALE;
        let draw_height = usize::from(sprite.height) * SHIP_SCALE;
        let x = placement.sprite_center_x - (draw_width as i32 / 2);
        let y = placement.sprite_center_y - (draw_height as i32 / 2);
        self.draw_sprite(frame, sprite, x, y, SHIP_SCALE);
    }

    fn draw_gauge(&self, frame: &mut FrameBuffer320x200, pack: &HudFragmentPack, amount: f64) {
        if pack.fragments.is_empty() {
            return;
        }
        let index = ((amount.clamp(0.0, 1.0) * pack.fragments.len() as f64).round() as isize - 1)
            .clamp(0, pack.fragments.len() as isize - 1) as usize;
        let fragment = &pack.fragments[index];
        for y in 0..usize::from(fragment.height) {
            for x in 0..usize::from(fragment.width) {
                let pixel_index = fragment.pixels[y * usize::from(fragment.width) + x];
                if pixel_index == 0 {
                    continue;
                }
                let color_index = pixel_index.min(2) as usize;
                frame.set_pixel(
                    usize::from(fragment.x) + x,
                    usize::from(fragment.y) + y,
                    DASHBOARD_COLORS[color_index],
                );
            }
        }
    }

    fn draw_archive_frame(
        &self,
        frame: &mut FrameBuffer320x200,
        archive: &ImageArchive,
        frame_index: usize,
        alpha: f32,
        brightness: f32,
    ) {
        if let Some(fragments) = archive.frames.get(frame_index) {
            for fragment in fragments {
                self.draw_fragment(frame, fragment, alpha, brightness, 1.0);
            }
        }
    }

    fn draw_archive_frame_reveal(
        &self,
        frame: &mut FrameBuffer320x200,
        archive: &ImageArchive,
        frame_index: usize,
        progress: f32,
        brightness: f32,
    ) {
        if let Some(fragments) = archive.frames.get(frame_index) {
            for fragment in fragments {
                let reveal_width =
                    (fragment.width as f32 * progress.clamp(0.0, 1.0)).round() as u16;
                self.draw_fragment(
                    frame,
                    fragment,
                    1.0,
                    brightness,
                    reveal_width.max(1) as f32 / fragment.width as f32,
                );
            }
        }
    }

    fn draw_fragment(
        &self,
        frame: &mut FrameBuffer320x200,
        fragment: &ImageFrame,
        alpha: f32,
        brightness: f32,
        horizontal_fraction: f32,
    ) {
        let draw_width =
            (fragment.width as f32 * horizontal_fraction.clamp(0.0, 1.0)).floor() as usize;
        for y in 0..usize::from(fragment.height) {
            for x in 0..draw_width {
                let pixel_index = fragment.pixels[y * usize::from(fragment.width) + x];
                if fragment.transparent_zero && pixel_index == 0 {
                    continue;
                }
                let Some(color) = fragment.palette.colors.get(pixel_index as usize).copied() else {
                    continue;
                };
                let color = scale_brightness(color, brightness);
                frame.blend_pixel(
                    usize::from(fragment.x_offset) + x,
                    usize::from(fragment.y_offset) + y,
                    color,
                    alpha,
                );
            }
        }
    }

    fn draw_sprite(
        &self,
        frame: &mut FrameBuffer320x200,
        sprite: &ImageFrame,
        dest_x: i32,
        dest_y: i32,
        scale: usize,
    ) {
        for y in 0..usize::from(sprite.height) {
            for x in 0..usize::from(sprite.width) {
                let pixel_index = sprite.pixels[y * usize::from(sprite.width) + x];
                if sprite.transparent_zero && pixel_index == 0 {
                    continue;
                }
                let Some(color) = sprite.palette.colors.get(pixel_index as usize).copied() else {
                    continue;
                };
                for sy in 0..scale {
                    for sx in 0..scale {
                        let px = dest_x + (x * scale + sx) as i32;
                        let py = dest_y + (y * scale + sy) as i32;
                        if px < 0 || py < 0 {
                            continue;
                        }
                        frame.set_pixel(px as usize, py as usize, color);
                    }
                }
            }
        }
    }

    fn draw_sprite_with_clip_mask(
        &self,
        frame: &mut FrameBuffer320x200,
        sprite: &ImageFrame,
        dest_x: i32,
        dest_y: i32,
        mask_offset_x: usize,
        mask_offset_y: usize,
        clip_mask: &mut [u8; DOS_SHIP_CLIP_MASK_BYTES],
    ) {
        for y in 0..usize::from(sprite.height) {
            if y + mask_offset_y >= DOS_SHIP_CLIP_MASK_HEIGHT {
                break;
            }
            for x in 0..usize::from(sprite.width) {
                if x + mask_offset_x >= DOS_SHIP_SPRITE_WIDTH {
                    break;
                }
                let pixel_index = sprite.pixels[y * usize::from(sprite.width) + x];
                if sprite.transparent_zero && pixel_index == 0 {
                    continue;
                }
                let mask_index =
                    (y + mask_offset_y) * DOS_SHIP_MASK_ROW_STRIDE + (x + mask_offset_x);
                if clip_mask[mask_index] == 0 {
                    continue;
                }
                let Some(color) = sprite.palette.colors.get(pixel_index as usize).copied() else {
                    continue;
                };
                let px = dest_x + x as i32;
                let py = dest_y + y as i32;
                if px < 0 || py < 0 {
                    continue;
                }
                clip_mask[mask_index] = 2;
                frame.set_pixel(px as usize, py as usize, color);
            }
        }
    }

    fn draw_projected_slice(&self, frame: &mut FrameBuffer320x200, slice: &ProjectedRoadSlice) {
        let height = slice.bottom_y.saturating_sub(slice.top_y).max(1);
        for y in slice.top_y..slice.bottom_y.min(VIEW_BOTTOM_Y) {
            let t = (y - slice.top_y) as f32 / height as f32;
            let center = lerp(slice.center_top, slice.center_bottom, t);
            let road_width = lerp(slice.width_top, slice.width_bottom, t);
            for span in &slice.spans {
                let x0 = project_span_x(center, road_width, span.top_start, span.bottom_start, t);
                let x1 = project_span_x(center, road_width, span.top_end, span.bottom_end, t);
                let width = (x1 - x0).max(1);
                let color = road_color(span.sample_cell);
                frame.fill_rect(x0, y as i32, width, 1, color);
                let edge_color = road_edge_color(span.sample_cell);
                frame.fill_rect(x0, y as i32, 2.min(width), 1, edge_color);
                frame.fill_rect(x0 + width - 1, y as i32, 1, 1, edge_color);
            }

            if let Some((left_fraction, right_fraction)) = slice.tunnel_span {
                let x0 = lerp(
                    center - road_width / 2.0 + road_width * left_fraction,
                    center - road_width / 2.0 + road_width * left_fraction,
                    t,
                )
                .round() as i32;
                let x1 = lerp(
                    center - road_width / 2.0 + road_width * right_fraction,
                    center - road_width / 2.0 + road_width * right_fraction,
                    t,
                )
                .round() as i32;
                let tunnel_height = ((slice.bottom_y - slice.top_y) as f32 * 0.75).round() as i32;
                frame.fill_rect(
                    x0,
                    y as i32 - tunnel_height,
                    (x1 - x0).max(1),
                    1,
                    RgbColor::new(84, 60, 48),
                );
            }
        }

        for obstacle in &slice.obstacles {
            let near_center = slice.center_bottom;
            let near_width = slice.width_bottom;
            let x0 = (near_center - near_width / 2.0 + near_width * obstacle.column_start).round()
                as i32;
            let x1 =
                (near_center - near_width / 2.0 + near_width * obstacle.column_end).round() as i32;
            let width = (x1 - x0).max(2);
            let obstacle_height = (((slice.bottom_y - slice.top_y) as f32)
                * (1.5 + obstacle.height_factor * 1.5))
                .round() as i32;
            let y_top = slice.top_y as i32 - obstacle_height;
            frame.fill_rect(x0, y_top, width, obstacle_height, obstacle.color);
            frame.fill_rect(x0, y_top, width, 2, scale_brightness(obstacle.color, 1.2));
            frame.fill_rect(
                x0 + width - 2,
                y_top,
                2,
                obstacle_height,
                scale_brightness(obstacle.color, 0.7),
            );
        }
    }

    fn draw_ship_shadow(
        &self,
        frame: &mut FrameBuffer320x200,
        visual: &DerivedShipVisualState,
        pipeline: &DosShipPipeline,
    ) {
        if visual.sprite_kind != ShipSpriteKind::Alive {
            return;
        }

        let Some(variant) = pipeline.shadow_variant else {
            return;
        };
        let shadow_mask = &self.assets.dos_ship_tables.shadow_masks[variant];
        for local_y in 0..DOS_SHIP_SHADOW_MASK_HEIGHT {
            for local_x in 0..DOS_SHIP_SHADOW_MASK_WIDTH {
                let shadow_index = local_y * DOS_SHIP_SHADOW_MASK_WIDTH + local_x;
                if shadow_mask[shadow_index] == 0 {
                    continue;
                }
                if pipeline.shadow_surface_mask[shadow_index] == 0 {
                    continue;
                }

                let px = pipeline.placement.shadow_left + local_x as i32;
                let py = pipeline.placement.shadow_top + local_y as i32;
                frame.darken_pixel(px, py, 0.58);
            }
        }
    }

    fn draw_branding(&self, frame: &mut FrameBuffer320x200, y: i32, scale: usize, alpha: f32) {
        let text = "CODEX PORT BY CODEX 5.4 AND AMMAAR";
        let color = scale_brightness(RgbColor::new(245, 214, 109), alpha);
        let shadow = scale_brightness(RgbColor::new(54, 24, 70), alpha);
        self.draw_text_centered(frame, text, y + scale as i32, shadow, scale);
        self.draw_text_centered(frame, text, y, color, scale);
    }

    fn draw_text_centered(
        &self,
        frame: &mut FrameBuffer320x200,
        text: &str,
        y: i32,
        color: RgbColor,
        scale: usize,
    ) {
        let width = text_pixel_width(text, scale);
        let x = (FRAMEBUFFER_WIDTH as i32 - width) / 2;
        self.draw_text(frame, x, y, text, color, scale);
    }

    fn draw_text_with_shadow(
        &self,
        frame: &mut FrameBuffer320x200,
        x: i32,
        y: i32,
        text: &str,
        color: RgbColor,
        scale: usize,
    ) {
        self.draw_text(frame, x + 1, y + 1, text, TEXT_SHADOW, scale);
        self.draw_text(frame, x, y, text, color, scale);
    }

    fn draw_text(
        &self,
        frame: &mut FrameBuffer320x200,
        x: i32,
        y: i32,
        text: &str,
        color: RgbColor,
        scale: usize,
    ) {
        let mut cursor = x;
        for ch in text.chars() {
            if ch == ' ' {
                cursor += (4 * scale) as i32;
                continue;
            }
            let Some(rows) = glyph_rows(ch) else {
                cursor += (4 * scale) as i32;
                continue;
            };
            for (row_index, row_bits) in rows.iter().copied().enumerate() {
                for col_index in 0..3 {
                    if (row_bits >> (2 - col_index)) & 1 == 0 {
                        continue;
                    }
                    frame.fill_rect(
                        cursor + (col_index * scale) as i32,
                        y + (row_index * scale) as i32,
                        scale as i32,
                        scale as i32,
                        color,
                    );
                }
            }
            cursor += (4 * scale) as i32;
        }
    }

    fn draw_settings_display_toggle(
        &self,
        frame: &mut FrameBuffer320x200,
        label: &str,
        center_x: i32,
        enabled: bool,
        selected: bool,
    ) {
        let widget_x = center_x - SETTINGS_WIDGET_WIDTH / 2;
        let widget_y = SETTINGS_WIDGET_TOP;
        let label_color = if enabled {
            SETTINGS_SELECTED_ORANGE
        } else {
            SETTINGS_CURSOR_WHITE
        };
        let status_text = if enabled { "ON" } else { "OFF" };
        let status_color = if enabled {
            SETTINGS_SELECTED_ORANGE
        } else if selected {
            SETTINGS_CURSOR_WHITE
        } else {
            SETTINGS_WIDGET_OUTLINE
        };

        frame.fill_rect(
            widget_x,
            widget_y,
            SETTINGS_WIDGET_WIDTH,
            SETTINGS_WIDGET_HEIGHT,
            SETTINGS_WIDGET_BG,
        );
        frame.stroke_rect(
            widget_x,
            widget_y,
            SETTINGS_WIDGET_WIDTH,
            SETTINGS_WIDGET_HEIGHT,
            SETTINGS_WIDGET_OUTLINE,
        );
        if selected {
            frame.stroke_rect(
                widget_x - 2,
                widget_y - 2,
                SETTINGS_WIDGET_WIDTH + 4,
                SETTINGS_WIDGET_HEIGHT + 4,
                SETTINGS_CURSOR_WHITE,
            );
        }

        let label_width = text_pixel_width(label, 2);
        self.draw_text_with_shadow(
            frame,
            center_x - label_width / 2,
            widget_y + 2,
            label,
            label_color,
            2,
        );

        let status_x = center_x - SETTINGS_WIDGET_STATUS_WIDTH / 2;
        let status_y = widget_y + 11;
        frame.fill_rect(
            status_x,
            status_y,
            SETTINGS_WIDGET_STATUS_WIDTH,
            SETTINGS_WIDGET_STATUS_HEIGHT,
            SETTINGS_WIDGET_BG,
        );
        if enabled {
            frame.fill_rect(
                status_x + 1,
                status_y + 1,
                SETTINGS_WIDGET_STATUS_WIDTH - 2,
                SETTINGS_WIDGET_STATUS_HEIGHT - 2,
                SETTINGS_SELECTED_ORANGE,
            );
        }
        frame.stroke_rect(
            status_x,
            status_y,
            SETTINGS_WIDGET_STATUS_WIDTH,
            SETTINGS_WIDGET_STATUS_HEIGHT,
            status_color,
        );

        let status_text_color = if enabled {
            TEXT_SHADOW
        } else {
            SETTINGS_WIDGET_TEXT_DIM
        };
        let status_text_width = text_pixel_width(status_text, 1);
        self.draw_text_with_shadow(
            frame,
            center_x - status_text_width / 2,
            status_y + 2,
            status_text,
            status_text_color,
            1,
        );
    }

    fn draw_debug_overlay(&self, frame: &mut FrameBuffer320x200, scene: &DemoPlaybackState) {
        let visual = derive_ship_visual_state(scene, &self.assets.dos_ship_tables);
        let slices = project_road_slices(scene);
        let placement = ship_screen_placement_from_slices(
            scene,
            &visual,
            &self.assets.dos_ship_tables,
            &slices,
        );
        self.draw_debug_hud_panel(frame, scene, DebugViewMode::Overlay);
        self.draw_projected_slice_guides(frame, &slices);
        self.draw_ship_debug_guides(frame, scene, &visual, placement);
        self.draw_topdown_inset(frame, scene);
    }

    fn render_play_geometry_debug(
        &self,
        frame: &mut FrameBuffer320x200,
        scene: &DemoPlaybackState,
    ) {
        let visual = derive_ship_visual_state(scene, &self.assets.dos_ship_tables);
        let slices = project_road_slices(scene);
        let road_coverage = self.build_dos_road_coverage_frame(scene);
        let mut pipeline = build_dos_ship_pipeline(
            scene,
            &self.assets.dos_ship_tables,
            road_coverage.as_ref(),
            &visual,
        );
        let placement = pipeline.placement;
        frame.clear(RgbColor::new(8, 8, 14));
        if let Some(world) = self
            .assets
            .worlds
            .get(scene.world_index)
            .or_else(|| self.assets.worlds.first())
        {
            self.draw_archive_frame(frame, world, 0, 0.25, 0.45);
        }
        frame.fill_rect(
            0,
            HORIZON_Y as i32,
            FRAMEBUFFER_WIDTH as i32,
            (VIEW_BOTTOM_Y - HORIZON_Y) as i32,
            RgbColor::new(10, 10, 20),
        );
        for slice in &slices {
            self.draw_projected_slice(frame, slice);
        }
        self.draw_projected_slice_guides(frame, &slices);
        self.draw_ship_shadow(frame, &visual, &pipeline);
        self.draw_ship_sprite(frame, scene.frame_index, &visual, &mut pipeline);
        self.draw_ship_debug_guides(frame, scene, &visual, placement);
        self.draw_topdown_inset(frame, scene);
        self.draw_archive_frame(frame, &self.assets.dashboard, 0, 1.0, 1.0);
        self.draw_debug_hud_panel(frame, scene, DebugViewMode::Geometry);
    }

    fn render_play_topdown_debug(&self, frame: &mut FrameBuffer320x200, scene: &DemoPlaybackState) {
        frame.clear(RgbColor::new(6, 6, 10));
        if let Some(world) = self
            .assets
            .worlds
            .get(scene.world_index)
            .or_else(|| self.assets.worlds.first())
        {
            self.draw_archive_frame(frame, world, 0, 0.20, 0.40);
        }
        frame.fill_rect(
            12,
            18,
            FRAMEBUFFER_WIDTH as i32 - 24,
            (VIEW_BOTTOM_Y - 26) as i32,
            RgbColor::new(16, 18, 26),
        );
        self.draw_topdown_map(frame, scene, 20, 26, 280, 104, true);
        self.draw_archive_frame(frame, &self.assets.dashboard, 0, 1.0, 1.0);
        self.draw_debug_hud_panel(frame, scene, DebugViewMode::TopDown);
    }

    fn draw_debug_hud_panel(
        &self,
        frame: &mut FrameBuffer320x200,
        scene: &DemoPlaybackState,
        mode: DebugViewMode,
    ) {
        frame.fill_rect(
            DEBUG_PANEL_X,
            DEBUG_PANEL_Y,
            DEBUG_PANEL_W,
            DEBUG_PANEL_H,
            RgbColor::new(10, 12, 18),
        );
        stroke_rect(
            frame,
            DEBUG_PANEL_X,
            DEBUG_PANEL_Y,
            DEBUG_PANEL_W,
            DEBUG_PANEL_H,
            RgbColor::new(82, 196, 230),
        );
        let row_state = renderer_row_state(scene.current_row as u16);
        self.draw_text(
            frame,
            DEBUG_PANEL_X + 4,
            DEBUG_PANEL_Y + 4,
            mode.label(),
            RgbColor::new(244, 233, 146),
            1,
        );
        let row_text = format!("ROW {:03}", scene.current_row);
        self.draw_text(
            frame,
            DEBUG_PANEL_X + 4,
            DEBUG_PANEL_Y + 13,
            &row_text,
            RgbColor::new(190, 220, 255),
            1,
        );
        let slot_text = format!(
            "GRP {:02} SLT {}",
            row_state.road_row_group, row_state.trekdat_slot
        );
        self.draw_text(
            frame,
            DEBUG_PANEL_X + 4,
            DEBUG_PANEL_Y + 22,
            &slot_text,
            RgbColor::new(190, 220, 255),
            1,
        );
        let state_text = short_ship_state(scene.ship.state);
        self.draw_text(
            frame,
            DEBUG_PANEL_X + 4,
            DEBUG_PANEL_Y + 31,
            state_text,
            RgbColor::new(247, 160, 160),
            1,
        );
    }

    fn draw_projected_slice_guides(
        &self,
        frame: &mut FrameBuffer320x200,
        slices: &[ProjectedRoadSlice],
    ) {
        for slice in slices {
            let left_top = (slice.center_top - slice.width_top / 2.0).round() as i32;
            let right_top = (slice.center_top + slice.width_top / 2.0).round() as i32;
            let left_bottom = (slice.center_bottom - slice.width_bottom / 2.0).round() as i32;
            let right_bottom = (slice.center_bottom + slice.width_bottom / 2.0).round() as i32;
            frame.fill_rect(
                left_top,
                slice.top_y as i32,
                1,
                1,
                RgbColor::new(110, 255, 170),
            );
            frame.fill_rect(
                right_top,
                slice.top_y as i32,
                1,
                1,
                RgbColor::new(110, 255, 170),
            );
            frame.fill_rect(
                left_bottom,
                slice.bottom_y.saturating_sub(1) as i32,
                1,
                1,
                RgbColor::new(110, 255, 170),
            );
            frame.fill_rect(
                right_bottom,
                slice.bottom_y.saturating_sub(1) as i32,
                1,
                1,
                RgbColor::new(110, 255, 170),
            );
            for obstacle in &slice.obstacles {
                let x0 = (slice.center_bottom - slice.width_bottom / 2.0
                    + slice.width_bottom * obstacle.column_start)
                    .round() as i32;
                let x1 = (slice.center_bottom - slice.width_bottom / 2.0
                    + slice.width_bottom * obstacle.column_end)
                    .round() as i32;
                let height = (((slice.bottom_y - slice.top_y) as f32)
                    * (1.5 + obstacle.height_factor * 1.5))
                    .round() as i32;
                let y0 = slice.top_y as i32 - height;
                stroke_rect(
                    frame,
                    x0,
                    y0,
                    (x1 - x0).max(2),
                    height.max(2),
                    RgbColor::new(255, 122, 122),
                );
            }
            if let Some((left_fraction, right_fraction)) = slice.tunnel_span {
                let x0 = (slice.center_bottom - slice.width_bottom / 2.0
                    + slice.width_bottom * left_fraction)
                    .round() as i32;
                let x1 = (slice.center_bottom - slice.width_bottom / 2.0
                    + slice.width_bottom * right_fraction)
                    .round() as i32;
                let tunnel_height = ((slice.bottom_y - slice.top_y) as f32 * 0.75).round() as i32;
                frame.fill_rect(
                    x0,
                    slice.top_y as i32 - tunnel_height,
                    (x1 - x0).max(1),
                    1,
                    RgbColor::new(255, 179, 87),
                );
            }
        }
    }

    fn draw_ship_debug_guides(
        &self,
        frame: &mut FrameBuffer320x200,
        scene: &DemoPlaybackState,
        visual: &DerivedShipVisualState,
        placement: ShipScreenPlacement,
    ) {
        let Some(car_atlas) = &self.car_atlas else {
            return;
        };
        let (sprite, x, y) = if visual.sprite_kind == ShipSpriteKind::Alive {
            if let Some(index) = visual.exact_ship_frame_index {
                if let Some(sprite) = car_atlas.exact_ship_frames_raw.get(index) {
                    (sprite, placement.sprite_left, placement.sprite_top)
                } else {
                    let sprite = car_atlas.select_sprite(visual, scene.frame_index);
                    let draw_width = usize::from(sprite.width) * SHIP_SCALE;
                    let draw_height = usize::from(sprite.height) * SHIP_SCALE;
                    (
                        sprite,
                        placement.sprite_center_x - (draw_width as i32 / 2),
                        placement.sprite_center_y - (draw_height as i32 / 2),
                    )
                }
            } else {
                let sprite = car_atlas.select_sprite(visual, scene.frame_index);
                let draw_width = usize::from(sprite.width) * SHIP_SCALE;
                let draw_height = usize::from(sprite.height) * SHIP_SCALE;
                (
                    sprite,
                    placement.sprite_center_x - (draw_width as i32 / 2),
                    placement.sprite_center_y - (draw_height as i32 / 2),
                )
            }
        } else {
            let sprite = car_atlas.select_sprite(visual, scene.frame_index);
            let draw_width = usize::from(sprite.width) * SHIP_SCALE;
            let draw_height = usize::from(sprite.height) * SHIP_SCALE;
            (
                sprite,
                placement.sprite_center_x - (draw_width as i32 / 2),
                placement.sprite_center_y - (draw_height as i32 / 2),
            )
        };
        let draw_width = usize::from(sprite.width) * SHIP_SCALE;
        let draw_height = usize::from(sprite.height) * SHIP_SCALE;
        stroke_rect(
            frame,
            x,
            y,
            draw_width as i32,
            draw_height as i32,
            RgbColor::new(100, 220, 255),
        );
        frame.fill_rect(
            placement.sprite_center_x - 8,
            placement.sprite_center_y,
            16,
            1,
            RgbColor::new(255, 230, 120),
        );
        frame.fill_rect(
            placement.sprite_center_x,
            placement.sprite_center_y - 8,
            1,
            16,
            RgbColor::new(255, 230, 120),
        );
    }

    fn draw_topdown_inset(&self, frame: &mut FrameBuffer320x200, scene: &DemoPlaybackState) {
        self.draw_topdown_map(
            frame,
            scene,
            DEBUG_TOPDOWN_INSET_X,
            DEBUG_TOPDOWN_INSET_Y,
            DEBUG_TOPDOWN_INSET_W,
            DEBUG_TOPDOWN_INSET_H,
            false,
        );
    }

    fn draw_topdown_map(
        &self,
        frame: &mut FrameBuffer320x200,
        scene: &DemoPlaybackState,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        large: bool,
    ) {
        frame.fill_rect(x, y, w, h, RgbColor::new(8, 10, 14));
        stroke_rect(frame, x, y, w, h, RgbColor::new(82, 196, 230));
        if scene.rows.is_empty() {
            return;
        }
        let row_h = (h - 8).max(7) / scene.rows.len().max(1) as i32;
        let col_w = (w - 8) / ROAD_COLUMNS as i32;
        let left_edge = LEVEL_CENTER_X - LEVEL_TILE_STRIDE_X * 3.5;
        for (row_idx, row) in scene.rows.iter().enumerate() {
            let cell_y = y + 4 + row_idx as i32 * row_h;
            for (col_idx, cell) in row.cells.iter().enumerate() {
                let cell_x = x + 4 + col_idx as i32 * col_w;
                let color = debug_cell_color(*cell);
                frame.fill_rect(cell_x, cell_y, col_w.max(2) - 1, row_h.max(2) - 1, color);
                if cell.has_tunnel {
                    stroke_rect(
                        frame,
                        cell_x + 1,
                        cell_y + 1,
                        (col_w.max(3) - 3).max(1),
                        (row_h.max(3) - 3).max(1),
                        RgbColor::new(255, 178, 90),
                    );
                }
            }
            if row.row_index == (scene.current_row >> 3) {
                stroke_rect(
                    frame,
                    x + 3,
                    cell_y - 1,
                    w - 6,
                    row_h.max(2) + 1,
                    RgbColor::new(255, 240, 120),
                );
            }
        }
        let row_start = scene.rows.first().map(|row| row.row_index).unwrap_or(0) as f64;
        let row_span = scene.rows.len().max(1) as f64;
        let ship_row = ((scene.ship.z_position - row_start) / row_span).clamp(0.0, 0.999);
        let ship_col = ((scene.ship.x_position - left_edge)
            / (LEVEL_TILE_STRIDE_X * ROAD_COLUMNS as f64))
            .clamp(0.0, 0.999);
        let ship_x = x + 4 + (ship_col * f64::from((w - 8).max(1))) as i32;
        let ship_y = y + 4 + (ship_row * f64::from((h - 8).max(1))) as i32;
        frame.fill_rect(ship_x - 2, ship_y - 2, 5, 5, RgbColor::new(112, 214, 255));
        if large {
            self.draw_text(
                frame,
                x + 4,
                y - 10,
                "TOPDOWN",
                RgbColor::new(244, 233, 146),
                1,
            );
        }
    }
}

fn short_ship_state(state: skyroads_core::ShipState) -> &'static str {
    match state {
        skyroads_core::ShipState::Alive => "ALIVE",
        skyroads_core::ShipState::Exploded => "EXPLODED",
        skyroads_core::ShipState::Fallen => "FALLEN",
        skyroads_core::ShipState::OutOfFuel => "NO FUEL",
        skyroads_core::ShipState::OutOfOxygen => "NO OXY",
    }
}

fn debug_cell_color(cell: LevelCell) -> RgbColor {
    if cell.is_empty() {
        RgbColor::new(20, 22, 28)
    } else if cell.cube_height.is_some() && cell.has_tunnel {
        RgbColor::new(203, 112, 82)
    } else if cell.cube_height.is_some() {
        RgbColor::new(210, 76, 110)
    } else if cell.has_tunnel {
        RgbColor::new(153, 119, 82)
    } else if cell.has_tile {
        road_color(cell)
    } else {
        RgbColor::new(52, 58, 72)
    }
}

fn stroke_rect(frame: &mut FrameBuffer320x200, x: i32, y: i32, w: i32, h: i32, color: RgbColor) {
    if w <= 0 || h <= 0 {
        return;
    }
    frame.fill_rect(x, y, w, 1, color);
    frame.fill_rect(x, y + h - 1, w, 1, color);
    frame.fill_rect(x, y, 1, h, color);
    frame.fill_rect(x + w - 1, y, 1, h, color);
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn scale_brightness(color: RgbColor, brightness: f32) -> RgbColor {
    let brightness = brightness.clamp(0.0, 1.35);
    RgbColor::new(
        (color.r as f32 * brightness).round().clamp(0.0, 255.0) as u8,
        (color.g as f32 * brightness).round().clamp(0.0, 255.0) as u8,
        (color.b as f32 * brightness).round().clamp(0.0, 255.0) as u8,
    )
}

fn road_color(cell: LevelCell) -> RgbColor {
    match cell.tile_effect {
        TouchEffect::Accelerate => RgbColor::new(126, 184, 118),
        TouchEffect::Decelerate => RgbColor::new(183, 146, 106),
        TouchEffect::Kill => RgbColor::new(214, 59, 92),
        TouchEffect::Slide => RgbColor::new(103, 132, 206),
        TouchEffect::RefillOxygen => RgbColor::new(92, 183, 202),
        TouchEffect::None => {
            if cell.cube_height.is_some() {
                RgbColor::new(222, 72, 112)
            } else if cell.has_tunnel {
                RgbColor::new(156, 128, 94)
            } else {
                RgbColor::new(172, 173, 194)
            }
        }
    }
}

fn road_edge_color(cell: LevelCell) -> RgbColor {
    if cell.cube_height.is_some() {
        RgbColor::new(245, 109, 136)
    } else {
        scale_brightness(road_color(cell), 1.2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DosRoadPhase {
    BeforeShip,
    AfterShip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DosCellContext {
    current_cell: LevelCell,
    current: RoadCellBytes,
    inward: RoadCellBytes,
    nearer: RoadCellBytes,
}

impl RoadCellBytes {
    fn from_cell(cell: LevelCell) -> Self {
        Self {
            byte0: cell.raw_descriptor as u8,
            byte1: (cell.raw_descriptor >> 8) as u8,
        }
    }

    fn low_nibble(self) -> u8 {
        self.byte0 & 0x0F
    }

    fn high_nibble(self) -> u8 {
        self.byte0 >> 4
    }

    fn dispatch_kind(self) -> usize {
        usize::from(self.byte1 & 0x0F)
    }
}

impl PrimitiveCursor {
    fn new(start_offset: u16) -> Self {
        Self {
            next_offset: Some(start_offset),
        }
    }

    fn skip(&mut self, record: &TrekdatRecord) -> bool {
        let Some(offset) = self.next_offset else {
            return false;
        };
        self.next_offset = record.next_shape_offset(offset);
        true
    }

    fn emit(
        &mut self,
        frame: &mut FrameBuffer320x200,
        record: &TrekdatRecord,
        cell: LevelCell,
        side: DosRenderSide,
        override_color_code: Option<u8>,
    ) -> bool {
        let Some(offset) = self.next_offset else {
            return false;
        };
        let Some(shape) = record.shape_at_offset(offset) else {
            self.next_offset = None;
            return false;
        };
        self.next_offset = record.next_shape_offset(offset);
        if shape.span_count == 0 {
            return true;
        }
        let color_code = override_color_code.unwrap_or(shape.color);
        let color = dos_shape_color(cell, color_code);
        draw_dos_shape(frame, &shape, color, side);
        true
    }
}

fn draw_dos_trekdat_pass(
    frame: &mut FrameBuffer320x200,
    scene: &DemoPlaybackState,
    record: &TrekdatRecord,
    phase: DosRoadPhase,
) -> bool {
    if scene.rows.is_empty() {
        return false;
    }

    let pointer_rows = record.dos_pointer_layout();
    let current_group = (scene.current_row >> 3) as isize;
    let row_sequence: &[(usize, isize)] = match phase {
        DosRoadPhase::BeforeShip => &[
            (0, 7),
            (1, 6),
            (2, 5),
            (3, 4),
            (4, 3),
            (5, 2),
            (6, 1),
            (11, 0),
        ],
        DosRoadPhase::AfterShip => &[(12, 0), (8, -1), (9, -2), (10, -3)],
    };

    for &(pointer_row_index, row_offset) in row_sequence {
        let row_index = current_group + row_offset;
        let current_row = scene_row(scene, row_index);
        let nearer_row = scene_row(scene, row_index - 1);
        draw_dos_pointer_row(
            frame,
            record,
            &pointer_rows.rows[pointer_row_index],
            current_row,
            nearer_row,
        );
    }

    true
}

fn draw_dos_pointer_row(
    frame: &mut FrameBuffer320x200,
    record: &TrekdatRecord,
    pointer_row: &skyroads_data::TrekdatPointerRow,
    current_row: [LevelCell; ROAD_COLUMNS],
    nearer_row: [LevelCell; ROAD_COLUMNS],
) {
    for (side, columns) in [
        (DosRenderSide::Left, DOS_LEFT_CELL_COLUMNS),
        (DosRenderSide::Right, DOS_RIGHT_CELL_COLUMNS),
    ] {
        for (slot_index, column_index) in columns.iter().copied().enumerate() {
            let inward_index = if side == DosRenderSide::Left {
                (column_index + 1).min(ROAD_COLUMNS - 1)
            } else {
                column_index.saturating_sub(1)
            };
            let context = DosCellContext {
                current_cell: current_row[column_index],
                current: RoadCellBytes::from_cell(current_row[column_index]),
                inward: RoadCellBytes::from_cell(current_row[inward_index]),
                nearer: RoadCellBytes::from_cell(nearer_row[column_index]),
            };
            draw_dos_cell(frame, record, &pointer_row.cells[slot_index], context, side);
        }
    }
}

fn draw_dos_cell(
    frame: &mut FrameBuffer320x200,
    record: &TrekdatRecord,
    cell_pointers: &TrekdatCellPointers,
    context: DosCellContext,
    side: DosRenderSide,
) {
    match context.current.dispatch_kind() {
        0 => draw_dos_type_0(frame, record, cell_pointers, context, side),
        1 => draw_dos_type_1(frame, record, cell_pointers, context, side),
        2 => draw_dos_type_2(frame, record, cell_pointers, context, side),
        3 => draw_dos_type_3(frame, record, cell_pointers, context, side),
        4 => draw_dos_type_4(frame, record, cell_pointers, context, side),
        5 => draw_dos_type_5(frame, record, cell_pointers, context, side),
        _ => {}
    }
}

fn draw_dos_type_0(
    frame: &mut FrameBuffer320x200,
    record: &TrekdatRecord,
    cell_pointers: &TrekdatCellPointers,
    context: DosCellContext,
    side: DosRenderSide,
) {
    let base_color = context.current.low_nibble();
    if base_color == 0 {
        return;
    }

    let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[0]);
    let _ = cursor.emit(frame, record, context.current_cell, side, Some(base_color));
    if context.inward.low_nibble() == 0 {
        let _ = cursor.emit(
            frame,
            record,
            context.current_cell,
            side,
            Some(base_color.saturating_add(0x1E)),
        );
    } else {
        let _ = cursor.skip(record);
    }
    if context.nearer.low_nibble() == 0 {
        let _ = cursor.emit(
            frame,
            record,
            context.current_cell,
            side,
            Some(base_color.saturating_add(0x0F)),
        );
    }
}

fn draw_dos_type_1(
    frame: &mut FrameBuffer320x200,
    record: &TrekdatRecord,
    cell_pointers: &TrekdatCellPointers,
    context: DosCellContext,
    side: DosRenderSide,
) {
    draw_dos_type_0(frame, record, cell_pointers, context, side);

    if context.nearer.byte1 < 1 {
        let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[1]);
        let _ = cursor.emit(frame, record, context.current_cell, side, Some(0x43));
    }

    let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[4]);
    for _ in 0..6 {
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
    }
    if context.nearer.byte1 < 1 {
        for _ in 0..2 {
            let _ = cursor.emit(frame, record, context.current_cell, side, None);
        }
    }
}

fn draw_dos_type_2(
    frame: &mut FrameBuffer320x200,
    record: &TrekdatRecord,
    cell_pointers: &TrekdatCellPointers,
    context: DosCellContext,
    side: DosRenderSide,
) {
    draw_dos_type_0(frame, record, cell_pointers, context, side);

    if context.nearer.byte1 < 2 {
        let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[3]);
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
    }

    let override_color = nonzero_or(context.current.high_nibble(), 0x3D);
    let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[2]);
    let _ = cursor.emit(
        frame,
        record,
        context.current_cell,
        side,
        Some(override_color),
    );
    if context.inward.byte1 < 2 {
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
    }
}

fn draw_dos_type_3(
    frame: &mut FrameBuffer320x200,
    record: &TrekdatRecord,
    cell_pointers: &TrekdatCellPointers,
    context: DosCellContext,
    side: DosRenderSide,
) {
    draw_dos_type_0(frame, record, cell_pointers, context, side);

    if context.nearer.byte1 < 2 {
        let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[1]);
        let _ = cursor.emit(frame, record, context.current_cell, side, Some(0x41));
    }

    let override_color = nonzero_or(context.current.high_nibble(), 0x3D);
    let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[2]);
    let _ = cursor.emit(
        frame,
        record,
        context.current_cell,
        side,
        Some(override_color),
    );
    if context.inward.byte1 < 2 {
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
    }

    if context.nearer.byte1 < 2 {
        let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[3]);
        let _ = cursor.skip(record);
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
    }
}

fn draw_dos_type_4(
    frame: &mut FrameBuffer320x200,
    record: &TrekdatRecord,
    cell_pointers: &TrekdatCellPointers,
    context: DosCellContext,
    side: DosRenderSide,
) {
    draw_dos_type_0(frame, record, cell_pointers, context, side);

    if context.nearer.byte1 < 2 {
        let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[3]);
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
    }

    let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[2]);
    let _ = cursor.skip(record);
    if context.inward.byte1 < 2 {
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
    }

    let override_color = nonzero_or(context.current.high_nibble(), 0x3D);
    let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[5]);
    let _ = cursor.emit(
        frame,
        record,
        context.current_cell,
        side,
        Some(override_color),
    );
    if context.inward.byte1 < 4 {
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
    } else {
        let _ = cursor.skip(record);
    }
    if context.nearer.byte1 < 4 {
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
    }
}

fn draw_dos_type_5(
    frame: &mut FrameBuffer320x200,
    record: &TrekdatRecord,
    cell_pointers: &TrekdatCellPointers,
    context: DosCellContext,
    side: DosRenderSide,
) {
    draw_dos_type_0(frame, record, cell_pointers, context, side);

    if context.nearer.byte1 < 2 {
        let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[1]);
        let _ = cursor.emit(frame, record, context.current_cell, side, Some(0x41));
    }

    let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[2]);
    let _ = cursor.skip(record);
    if context.inward.byte1 < 2 {
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
    }

    if context.nearer.byte1 < 2 {
        let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[3]);
        let _ = cursor.skip(record);
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
    }

    let override_color = nonzero_or(context.current.high_nibble(), 0x3D);
    let mut cursor = PrimitiveCursor::new(cell_pointers.pointers[5]);
    let _ = cursor.emit(
        frame,
        record,
        context.current_cell,
        side,
        Some(override_color),
    );
    if context.inward.byte1 < 4 {
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
    } else {
        let _ = cursor.skip(record);
    }
    if context.nearer.byte1 < 4 {
        let _ = cursor.emit(frame, record, context.current_cell, side, None);
    }
}

fn nonzero_or(value: u8, fallback: u8) -> u8 {
    if value == 0 {
        fallback
    } else {
        value
    }
}

fn scene_row(scene: &DemoPlaybackState, row_index: isize) -> [LevelCell; ROAD_COLUMNS] {
    if row_index < 0 {
        return [LevelCell::EMPTY; ROAD_COLUMNS];
    }
    scene
        .rows
        .iter()
        .find(|row| row.row_index == row_index as usize)
        .map(|row| row.cells)
        .unwrap_or([LevelCell::EMPTY; ROAD_COLUMNS])
}

fn draw_dos_shape(
    frame: &mut FrameBuffer320x200,
    shape: &TrekdatShape,
    color: RgbColor,
    side: DosRenderSide,
) {
    for span in &shape.spans {
        if span.width == 0 {
            continue;
        }
        let x = match side {
            DosRenderSide::Left => span.x as i32,
            DosRenderSide::Right => {
                i32::from(SCREEN_WIDTH) - i32::from(span.x) - i32::from(span.width)
            }
        };
        draw_trekdat_span(frame, x, span.y as i32, span.width as i32, color);
    }
}

fn draw_trekdat_span(frame: &mut FrameBuffer320x200, x: i32, y: i32, width: i32, color: RgbColor) {
    if y < HORIZON_Y as i32 || y >= VIEW_BOTTOM_Y as i32 {
        return;
    }
    frame.fill_rect(x, y, width, 1, color);
}

fn dos_shape_color(cell: LevelCell, color_code: u8) -> RgbColor {
    let base = road_color(cell);
    match color_code {
        0x01..=0x0E => scale_brightness(base, 0.98 + f32::from(color_code & 0x0F) * 0.02),
        0x0F..=0x1D => scale_brightness(base, 1.18),
        0x1E..=0x2D => scale_brightness(base, 0.78),
        0x3D => scale_brightness(base, 0.58),
        0x41 => scale_brightness(base, 0.90),
        0x43 => scale_brightness(base, 0.68),
        62 => scale_brightness(base, 0.72),
        63 => scale_brightness(base, 0.62),
        68 => scale_brightness(base, 1.28),
        _ => scale_brightness(base, 0.92),
    }
}

fn derive_ship_visual_state(
    scene: &DemoPlaybackState,
    ship_tables: &ExeShipRuntimeTables,
) -> DerivedShipVisualState {
    let x_fixed = dos_fixed(scene.ship.x_position);
    let lane_index = dos_ship_lane_index_from_fixed(x_fixed);
    let bank = match lane_index.cmp(&3) {
        std::cmp::Ordering::Less => ShipBank::Left,
        std::cmp::Ordering::Equal => ShipBank::Center,
        std::cmp::Ordering::Greater => ShipBank::Right,
    };
    let sprite_kind = match scene.ship.state {
        skyroads_core::ShipState::Alive => ShipSpriteKind::Alive,
        skyroads_core::ShipState::Exploded => ShipSpriteKind::Exploding,
        skyroads_core::ShipState::Fallen
        | skyroads_core::ShipState::OutOfFuel
        | skyroads_core::ShipState::OutOfOxygen => ShipSpriteKind::Destroyed,
    };
    let jumping = scene.ship.y_position > GROUND_Y + 0.5
        || scene.ship.is_going_up
        || !scene.ship.is_on_ground
        || scene.ship.jump_input;
    let vertical_state = dos_ship_vertical_state(scene);
    let explosion_frame = scene.ship.explosion_timer / 3;
    let thrust_phase = if scene.ship.accel_input > 0 {
        let cycle_index = (scene.frame_index / 2) & 0x03;
        usize::from(ship_tables.thrust_phase_by_cycle[cycle_index])
    } else {
        0
    };

    DerivedShipVisualState {
        sprite_kind,
        bank,
        jumping,
        explosion_frame,
        exact_ship_frame_index: (scene.ship.state == skyroads_core::ShipState::Alive)
            .then_some(((lane_index * 3 + vertical_state) * 3) as usize + thrust_phase),
        on_surface: scene.ship.is_on_ground && scene.ship.state == skyroads_core::ShipState::Alive,
    }
}

fn should_draw_game_over_overlay(scene: &DemoPlaybackState) -> bool {
    let ship_state = scene.snapshot.craft_state;
    if ship_state == skyroads_core::ShipState::Alive {
        return false;
    }

    if scene.ship.explosion_timer != 0
        && scene.ship.explosion_timer <= DOS_EXPLOSION_ANIMATION_TICKS
    {
        return false;
    }

    match ship_state {
        skyroads_core::ShipState::Exploded => true,
        skyroads_core::ShipState::Fallen if scene.ship.explosion_timer != 0 => true,
        skyroads_core::ShipState::Fallen
        | skyroads_core::ShipState::OutOfFuel
        | skyroads_core::ShipState::OutOfOxygen => {
            scene.ship.non_alive_frame_count >= DOS_NON_ALIVE_ANIMATION_TICKS
        }
        skyroads_core::ShipState::Alive => false,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn ship_screen_placement(
    scene: &DemoPlaybackState,
    visual: &DerivedShipVisualState,
    ship_tables: &ExeShipRuntimeTables,
) -> ShipScreenPlacement {
    let slices = project_road_slices(scene);
    ship_screen_placement_from_slices(scene, visual, ship_tables, &slices)
}

fn ship_screen_placement_from_slices(
    scene: &DemoPlaybackState,
    visual: &DerivedShipVisualState,
    ship_tables: &ExeShipRuntimeTables,
    slices: &[ProjectedRoadSlice],
) -> ShipScreenPlacement {
    let _ = slices;
    build_dos_ship_pipeline(scene, ship_tables, None, visual).placement
}

fn settings_menu_selected_control_overlay(control_mode: ControlMode) -> usize {
    match control_mode {
        ControlMode::Keyboard => 6,
        ControlMode::Joystick => 7,
        ControlMode::Mouse => 8,
    }
}

fn dos_fixed(value: f64) -> i32 {
    (value * 128.0).round() as i32
}

fn dos_ship_lane_index_from_fixed(x_fixed: i32) -> i32 {
    ((x_fixed >> 7) - 95)
        .div_euclid(46)
        .clamp(0, DOS_SHIP_LANE_COUNT as i32 - 1)
}

fn dos_ship_raw_screen_x(x_fixed: i32, ship_tables: &ExeShipRuntimeTables) -> i32 {
    let lane_index = dos_ship_lane_index_from_fixed(x_fixed) as usize;
    (x_fixed >> 7) + i32::from(ship_tables.screen_x_bias_by_lane[lane_index])
}

fn dos_ship_raw_screen_y(scene: &DemoPlaybackState, y_fixed: i32) -> i32 {
    let rising = scene.ship.is_going_up || scene.ship.jump_input || scene.ship.y_velocity < 0.0;
    let adjusted_y = if rising { y_fixed - 0x80 } else { y_fixed };
    adjusted_y >> 7
}

fn dos_ship_shadow_height(
    scene: &DemoPlaybackState,
    x_fixed: i32,
    y_fixed: i32,
    ship_tables: &ExeShipRuntimeTables,
) -> i32 {
    if scene.ship.state != skyroads_core::ShipState::Alive {
        return 0;
    }

    let left_surface = dos_surface_height_at_probe(scene, x_fixed - 0x380, ship_tables);
    let right_surface = dos_surface_height_at_probe(scene, x_fixed + 0x380, ship_tables);
    let surface_height = left_surface.min(right_surface);
    ((y_fixed - surface_height).max(0)) >> 7
}

fn dos_surface_height_at_probe(
    scene: &DemoPlaybackState,
    x_fixed: i32,
    ship_tables: &ExeShipRuntimeTables,
) -> i32 {
    if x_fixed < dos_fixed(LEVEL_MIN_X) || x_fixed > dos_fixed(LEVEL_MAX_X) {
        return 0;
    }

    let row_index = scene.ship.z_position.floor().max(0.0) as isize;
    let row = scene_row(scene, row_index);
    let x_position = f64::from(x_fixed) / 128.0;
    let column = ((x_position - 95.0) / LEVEL_TILE_STRIDE_X).floor() as isize;
    if !(0..ROAD_COLUMNS as isize).contains(&column) {
        return 0;
    }

    let cell = row[column as usize];
    let bytes = RoadCellBytes::from_cell(cell);
    if bytes.high_nibble() != 0 {
        let dispatch_kind = bytes.dispatch_kind();
        if dispatch_kind == 1 {
            return 0;
        }
        if let Some(surface_height) = ship_tables
            .surface_height_by_dispatch_kind
            .get(dispatch_kind)
            .copied()
        {
            return i32::from(surface_height);
        }
    }

    if bytes.low_nibble() != 0 {
        0x2800
    } else {
        0
    }
}

fn build_dos_ship_pipeline(
    scene: &DemoPlaybackState,
    ship_tables: &ExeShipRuntimeTables,
    road_coverage: Option<&FrameBuffer320x200>,
    visual: &DerivedShipVisualState,
) -> DosShipPipeline {
    let x_fixed = dos_fixed(scene.ship.x_position);
    let y_fixed = dos_fixed(scene.ship.y_position);
    let raw_x = dos_ship_raw_screen_x(x_fixed, ship_tables);
    let raw_y = dos_ship_raw_screen_y(scene, y_fixed);
    let shadow_height = dos_ship_shadow_height(scene, x_fixed, y_fixed, ship_tables);
    let placement = ShipScreenPlacement {
        sprite_left: raw_x - DOS_SHIP_X_BASE_OFFSET,
        sprite_top: DOS_SHIP_TOP_BASE - raw_y,
        sprite_center_x: raw_x - DOS_SHIP_CENTER_X_OFFSET,
        sprite_center_y: DOS_SHIP_TOP_BASE - raw_y + DOS_SHIP_CENTER_Y_OFFSET,
        shadow_left: raw_x - DOS_SHIP_X_BASE_OFFSET,
        shadow_top: DOS_SHIP_TOP_BASE - raw_y + DOS_SHIP_SHADOW_TOP_OFFSET + shadow_height,
        shadow_center_x: raw_x - DOS_SHIP_CENTER_X_OFFSET,
        shadow_center_y: DOS_SHIP_TOP_BASE - raw_y
            + DOS_SHIP_SHADOW_TOP_OFFSET
            + shadow_height
            + (DOS_SHIP_SHADOW_MASK_HEIGHT as i32 / 2),
    };

    let clip_mask = build_ship_clip_mask_from_road_coverage(
        road_coverage,
        placement.sprite_left,
        placement.sprite_top,
        placement.sprite_center_x,
    );
    let shadow_surface_mask = build_shadow_surface_mask_from_road_coverage(
        road_coverage,
        placement.shadow_left,
        placement.shadow_top,
    );
    let shadow_variant = (visual.sprite_kind == ShipSpriteKind::Alive && shadow_height >= 0)
        .then_some((shadow_height / 5) as usize)
        .filter(|variant| *variant < DOS_SHIP_SHADOW_VARIANTS);

    DosShipPipeline {
        placement,
        clip_mask,
        shadow_surface_mask,
        shadow_variant,
    }
}

fn build_ship_clip_mask_from_road_coverage(
    _road_coverage: Option<&FrameBuffer320x200>,
    _sprite_left: i32,
    _sprite_top: i32,
    _ship_center_x: i32,
) -> [u8; DOS_SHIP_CLIP_MASK_BYTES] {
    // DOS does gate ship/shadow writes through mask buffers, but the ship mask is not derived
    // from the visible road fill. The current road-coverage approximation clips the ship to the
    // road span and produces the sideways "sliced off" artifact visible in render-demo captures.
    // Until the original 0x32A5 row-mask path is ported exactly, keep ship pixels unmasked here
    // and let only the separate shadow surface mask depend on road coverage.
    let mask = [1u8; DOS_SHIP_CLIP_MASK_BYTES];
    mask
}

fn build_shadow_surface_mask_from_road_coverage(
    road_coverage: Option<&FrameBuffer320x200>,
    shadow_left: i32,
    shadow_top: i32,
) -> [u8; DOS_SHIP_SHADOW_MASK_HEIGHT * DOS_SHIP_SHADOW_MASK_WIDTH] {
    let mut mask = [0u8; DOS_SHIP_SHADOW_MASK_HEIGHT * DOS_SHIP_SHADOW_MASK_WIDTH];
    let Some(road_coverage) = road_coverage else {
        mask.fill(1);
        return mask;
    };

    for local_y in 0..DOS_SHIP_SHADOW_MASK_HEIGHT {
        for local_x in 0..DOS_SHIP_SHADOW_MASK_WIDTH {
            let screen_x = shadow_left + local_x as i32;
            let screen_y = shadow_top + local_y as i32;
            if framebuffer_has_color(road_coverage, screen_x, screen_y) {
                mask[local_y * DOS_SHIP_SHADOW_MASK_WIDTH + local_x] = 1;
            }
        }
    }

    mask
}

#[cfg_attr(not(test), allow(dead_code))]
fn clip_window_for_screen_y(
    road_coverage: &FrameBuffer320x200,
    screen_y: i32,
    ship_center_x: i32,
) -> Option<(i32, i32)> {
    if screen_y < 0 || screen_y >= FRAMEBUFFER_HEIGHT as i32 {
        return None;
    }

    let mut best_run = None;
    let mut best_distance = i32::MAX;
    let mut x = 0i32;

    while x < FRAMEBUFFER_WIDTH as i32 {
        while x < FRAMEBUFFER_WIDTH as i32 && !framebuffer_has_color(road_coverage, x, screen_y) {
            x += 1;
        }
        if x >= FRAMEBUFFER_WIDTH as i32 {
            break;
        }

        let run_left = x;
        while x < FRAMEBUFFER_WIDTH as i32 && framebuffer_has_color(road_coverage, x, screen_y) {
            x += 1;
        }
        let run_right = x;
        let contains_ship = ship_center_x >= run_left && ship_center_x < run_right;
        let distance = if contains_ship {
            0
        } else if ship_center_x < run_left {
            run_left - ship_center_x
        } else {
            ship_center_x - (run_right - 1)
        };
        let best_width = best_run.map(|(left, right)| right - left).unwrap_or(0);
        let run_width = run_right - run_left;
        let is_better =
            distance < best_distance || (distance == best_distance && run_width > best_width);
        if is_better {
            best_distance = distance;
            best_run = Some((run_left, run_right));
        }
    }

    best_run
}

fn framebuffer_has_color(frame: &FrameBuffer320x200, x: i32, y: i32) -> bool {
    if x < 0 || y < 0 {
        return false;
    }
    let x = x as usize;
    let y = y as usize;
    if x >= FRAMEBUFFER_WIDTH || y >= FRAMEBUFFER_HEIGHT {
        return false;
    }
    let offset = (y * FRAMEBUFFER_WIDTH + x) * 4;
    frame.pixels_rgba[offset] != 0
        || frame.pixels_rgba[offset + 1] != 0
        || frame.pixels_rgba[offset + 2] != 0
}

fn dos_ship_vertical_state(scene: &DemoPlaybackState) -> i32 {
    const SHIP_RISE_THRESHOLD: f64 = 0x163 as f64 / 0x80 as f64;
    if scene.ship.y_position < GROUND_Y || scene.ship.z_position < 0.0 {
        2
    } else if scene.ship.y_velocity <= -SHIP_RISE_THRESHOLD {
        2
    } else if scene.ship.y_velocity >= SHIP_RISE_THRESHOLD {
        1
    } else {
        0
    }
}

fn project_road_slices(scene: &DemoPlaybackState) -> Vec<ProjectedRoadSlice> {
    if scene.rows.len() < 2 {
        return Vec::new();
    }

    let far_depth = scene
        .rows
        .last()
        .map(|row| road_depth(scene, row.row_index) + 1.0)
        .unwrap_or(20.0)
        .max(12.0);

    let mut slices = Vec::new();
    for (depth_index, window) in scene.rows.windows(2).enumerate() {
        let near_row = &window[0];
        let far_row = &window[1];
        let near_depth = road_depth(scene, near_row.row_index);
        let far_depth_for_row = road_depth(scene, far_row.row_index);
        if far_depth_for_row <= near_depth {
            continue;
        }

        let top_y = projected_y_for_depth(far_depth_for_row, far_depth);
        let bottom_y = projected_y_for_depth(near_depth, far_depth);
        if bottom_y <= top_y || top_y >= VIEW_BOTTOM_Y {
            continue;
        }

        let width_top = projected_width_for_depth(far_depth_for_row, far_depth);
        let width_bottom = projected_width_for_depth(near_depth, far_depth);
        let center_top = projected_center_x(scene, far_depth_for_row, far_depth);
        let center_bottom = projected_center_x(scene, near_depth, far_depth);
        let top_spans = road_surface_spans(&far_row.cells);
        let bottom_spans = road_surface_spans(&near_row.cells);
        let spans = project_surface_spans(&top_spans, &bottom_spans);
        let obstacles = project_obstacles(near_row, near_depth, far_depth);
        let tunnel_span = project_tunnel_span(near_row);
        let row_state = renderer_row_state(near_row.row_index as u16);

        slices.push(ProjectedRoadSlice {
            trekdat_key: TrekdatProjectionKey {
                depth_index,
                road_row_group: row_state.road_row_group,
                trekdat_slot: row_state.trekdat_slot,
            },
            top_y,
            bottom_y: bottom_y.min(VIEW_BOTTOM_Y),
            center_top,
            center_bottom,
            width_top,
            width_bottom,
            spans,
            obstacles,
            tunnel_span,
        });
    }

    slices.reverse();
    slices
}

fn road_depth(scene: &DemoPlaybackState, row_index: usize) -> f32 {
    let current_z_position = scene.current_row as f64 / 8.0 + scene.fractional_z;
    ((row_index as f64 + 1.0) - current_z_position).max(0.0) as f32
}

fn projected_y_for_depth(depth: f32, far_depth: f32) -> usize {
    let view_height = (VIEW_BOTTOM_Y - HORIZON_Y) as f32;
    let near_plane = 0.45f32;
    let inverse = 1.0 / (depth + near_plane);
    let inverse_near = 1.0 / near_plane;
    let inverse_far = 1.0 / (far_depth + near_plane);
    let normalized = ((inverse - inverse_far) / (inverse_near - inverse_far)).clamp(0.0, 1.0);
    (HORIZON_Y as f32 + view_height * normalized).round() as usize
}

fn projected_width_for_depth(depth: f32, far_depth: f32) -> f32 {
    let near_width = 252.0f32;
    let far_width = 34.0f32;
    let near_plane = 0.45f32;
    let inverse = 1.0 / (depth + near_plane);
    let inverse_near = 1.0 / near_plane;
    let inverse_far = 1.0 / (far_depth + near_plane);
    let normalized = ((inverse - inverse_far) / (inverse_near - inverse_far)).clamp(0.0, 1.0);
    lerp(far_width, near_width, normalized.powf(0.75))
}

fn projected_center_x(scene: &DemoPlaybackState, depth: f32, far_depth: f32) -> f32 {
    let _ = (scene, depth, far_depth);
    // Keep the fallback road projection centered until the exact DOS camera path is ported.
    // The gameplay state already carries the ship's world X; applying an extra guessed camera
    // pan here largely cancels the visible left/right movement of the user-controlled ship.
    FRAMEBUFFER_WIDTH as f32 / 2.0
}

fn project_surface_spans(
    top_spans: &[RoadSpan],
    bottom_spans: &[RoadSpan],
) -> Vec<ProjectedRoadSpan> {
    let count = top_spans.len().max(bottom_spans.len());
    let mut spans = Vec::new();
    for index in 0..count {
        let top_span = top_spans.get(index).or_else(|| top_spans.last());
        let bottom_span = bottom_spans.get(index).or_else(|| bottom_spans.last());
        let Some(sample_cell) = bottom_span
            .map(|span| span.sample_cell)
            .or_else(|| top_span.map(|span| span.sample_cell))
        else {
            continue;
        };
        let top_span = top_span.unwrap_or_else(|| bottom_span.unwrap());
        let bottom_span = bottom_span.unwrap_or_else(|| top_span);
        spans.push(ProjectedRoadSpan {
            top_start: top_span.start_column as f32 / ROAD_COLUMNS as f32,
            top_end: top_span.end_column_exclusive as f32 / ROAD_COLUMNS as f32,
            bottom_start: bottom_span.start_column as f32 / ROAD_COLUMNS as f32,
            bottom_end: bottom_span.end_column_exclusive as f32 / ROAD_COLUMNS as f32,
            sample_cell,
        });
    }
    spans
}

fn project_obstacles(
    row: &RoadRenderRow,
    near_depth: f32,
    far_depth: f32,
) -> Vec<ProjectedObstacle> {
    let visibility = (1.0 - (near_depth / far_depth.max(1.0))).clamp(0.0, 1.0);
    row.cells
        .iter()
        .enumerate()
        .filter_map(|(index, cell)| {
            let cube_height = cell.cube_height?;
            Some(ProjectedObstacle {
                column_start: index as f32 / ROAD_COLUMNS as f32,
                column_end: (index + 1) as f32 / ROAD_COLUMNS as f32,
                height_factor: if cube_height >= 120 { 0.8 } else { 0.55 } * visibility.max(0.2),
                color: if cell.has_tunnel {
                    RgbColor::new(198, 74, 112)
                } else {
                    RgbColor::new(230, 64, 94)
                },
            })
        })
        .collect()
}

fn project_tunnel_span(row: &RoadRenderRow) -> Option<(f32, f32)> {
    let mut min_column = ROAD_COLUMNS;
    let mut max_column = 0usize;
    let mut found = false;
    for (index, cell) in row.cells.iter().enumerate() {
        if cell.has_tunnel && cell.has_tile {
            min_column = min_column.min(index);
            max_column = max_column.max(index + 1);
            found = true;
        }
    }
    found.then_some((
        min_column as f32 / ROAD_COLUMNS as f32,
        max_column as f32 / ROAD_COLUMNS as f32,
    ))
}

fn project_span_x(center: f32, road_width: f32, top_value: f32, bottom_value: f32, t: f32) -> i32 {
    let edge_fraction = lerp(top_value, bottom_value, t);
    (center - road_width / 2.0 + road_width * edge_fraction).round() as i32
}

fn road_surface_spans(row: &[LevelCell; ROAD_COLUMNS]) -> Vec<RoadSpan> {
    let mut spans = Vec::new();
    let mut start = None;
    for (index, cell) in row.iter().copied().enumerate() {
        let is_surface = cell.has_tile;
        match (start, is_surface) {
            (None, true) => start = Some((index, cell)),
            (Some((span_start, sample_cell)), false) => {
                spans.push(RoadSpan {
                    start_column: span_start,
                    end_column_exclusive: index,
                    sample_cell,
                });
                start = None;
            }
            _ => {}
        }
    }
    if let Some((span_start, sample_cell)) = start {
        spans.push(RoadSpan {
            start_column: span_start,
            end_column_exclusive: ROAD_COLUMNS,
            sample_cell,
        });
    }
    spans
}

fn split_vertical_sprites(frame: &ImageFrame) -> Vec<ImageFrame> {
    let width = usize::from(frame.width);
    let height = usize::from(frame.height);
    let mut segments = Vec::new();
    let mut row = 0usize;
    while row < height {
        while row < height && is_row_empty(frame, row, width) {
            row += 1;
        }
        if row >= height {
            break;
        }
        let start_row = row;
        while row < height && !is_row_empty(frame, row, width) {
            row += 1;
        }
        let end_row = row;
        let Some((min_x, max_x)) = sprite_x_bounds(frame, start_row, end_row, width) else {
            continue;
        };
        let sprite_width = max_x - min_x + 1;
        let sprite_height = end_row - start_row;
        let mut pixels = Vec::with_capacity(sprite_width * sprite_height);
        for y in start_row..end_row {
            let base = y * width;
            pixels.extend_from_slice(&frame.pixels[base + min_x..base + max_x + 1]);
        }
        segments.push(ImageFrame {
            offset: 0,
            x_offset: 0,
            y_offset: 0,
            width: sprite_width as u16,
            height: sprite_height as u16,
            pixels,
            palette: frame.palette.clone(),
            transparent_zero: frame.transparent_zero,
        });
    }
    segments
}

fn split_vertical_sprites_with_full_width(frame: &ImageFrame) -> Vec<ImageFrame> {
    let width = usize::from(frame.width);
    let height = usize::from(frame.height);
    let mut segments = Vec::new();
    let mut row = 0usize;
    while row < height {
        while row < height && is_row_empty(frame, row, width) {
            row += 1;
        }
        if row >= height {
            break;
        }
        let start_row = row;
        while row < height && !is_row_empty(frame, row, width) {
            row += 1;
        }
        let end_row = row;
        let sprite_height = end_row - start_row;
        let mut pixels = Vec::with_capacity(width * sprite_height);
        for y in start_row..end_row {
            let base = y * width;
            pixels.extend_from_slice(&frame.pixels[base..base + width]);
        }
        segments.push(ImageFrame {
            offset: 0,
            x_offset: 0,
            y_offset: 0,
            width: width as u16,
            height: sprite_height as u16,
            pixels,
            palette: frame.palette.clone(),
            transparent_zero: frame.transparent_zero,
        });
    }
    segments
}

fn is_row_empty(frame: &ImageFrame, row: usize, width: usize) -> bool {
    let base = row * width;
    frame.pixels[base..base + width]
        .iter()
        .all(|value| *value == 0)
}

fn sprite_x_bounds(
    frame: &ImageFrame,
    start_row: usize,
    end_row: usize,
    width: usize,
) -> Option<(usize, usize)> {
    let mut min_x = width;
    let mut max_x = 0usize;
    let mut found = false;
    for row in start_row..end_row {
        let base = row * width;
        for x in 0..width {
            if frame.pixels[base + x] == 0 {
                continue;
            }
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            found = true;
        }
    }
    found.then_some((min_x, max_x))
}

impl CarAtlas {
    fn from_archive(archive: &ImageArchive) -> Option<Self> {
        let frame = archive.frames.first()?.first()?;
        let sprites = split_vertical_sprites(frame);
        let raw_sprites = split_vertical_sprites_with_full_width(frame);
        if sprites.len() < 48 {
            return None;
        }
        if raw_sprites.len() < DOS_EXACT_SHIP_FRAME_START + DOS_EXACT_SHIP_FRAME_COUNT {
            return None;
        }
        let explosion_frames = sprites
            .iter()
            .take(7)
            .cloned()
            .map(trim_sprite)
            .collect::<Vec<_>>();
        // The raw full-width split includes four tiny fragments between the explosion strip and
        // the real 63-frame DOS ship run. Start at the first full ship frame, not at the raw
        // split index that merely follows the explosion frames.
        let exact_ship_frames_raw = raw_sprites
            [DOS_EXACT_SHIP_FRAME_START..DOS_EXACT_SHIP_FRAME_START + DOS_EXACT_SHIP_FRAME_COUNT]
            .iter()
            .cloned()
            .map(rotate_sprite_cw)
            .collect::<Vec<_>>();
        let exact_ship_frames = exact_ship_frames_raw
            .iter()
            .cloned()
            .map(trim_sprite)
            .collect::<Vec<_>>();
        let alive_left = collect_sprite_group(&sprites, 21);
        let alive_center = collect_sprite_group(&sprites, 27);
        let alive_right = collect_sprite_group(&sprites, 30);
        let jump_left = collect_sprite_group(&sprites, 36);
        let jump_center = collect_sprite_group(&sprites, 42);
        let jump_right = collect_sprite_group(&sprites, 45);
        let destroyed = alive_center.first().cloned()?;
        Some(Self {
            explosion_frames,
            exact_ship_frames_raw,
            exact_ship_frames,
            alive_left,
            alive_center,
            alive_right,
            jump_left,
            jump_center,
            jump_right,
            destroyed,
        })
    }

    fn select_sprite<'a>(
        &'a self,
        visual: &DerivedShipVisualState,
        frame_index: usize,
    ) -> &'a ImageFrame {
        match visual.sprite_kind {
            ShipSpriteKind::Exploding => {
                let index = visual
                    .explosion_frame
                    .min(self.explosion_frames.len().saturating_sub(1));
                &self.explosion_frames[index]
            }
            ShipSpriteKind::Destroyed => &self.destroyed,
            ShipSpriteKind::Alive => {
                if let Some(index) = visual.exact_ship_frame_index {
                    if let Some(sprite) = self.exact_ship_frames.get(index) {
                        return sprite;
                    }
                }
                let bank_frames = match (visual.jumping, visual.bank) {
                    (true, ShipBank::Left) => &self.jump_left,
                    (true, ShipBank::Right) => &self.jump_right,
                    (true, ShipBank::Center) => &self.jump_center,
                    (false, ShipBank::Left) => &self.alive_left,
                    (false, ShipBank::Right) => &self.alive_right,
                    (false, ShipBank::Center) => &self.alive_center,
                };
                let phase = (frame_index / 4) % bank_frames.len().max(1);
                &bank_frames[phase.min(bank_frames.len().saturating_sub(1))]
            }
        }
    }
}

fn collect_sprite_group(sprites: &[ImageFrame], start_index: usize) -> Vec<ImageFrame> {
    sprites[start_index..start_index + 3]
        .iter()
        .cloned()
        .map(rotate_sprite_cw)
        .map(trim_sprite)
        .collect()
}

fn rotate_sprite_cw(sprite: ImageFrame) -> ImageFrame {
    let width = usize::from(sprite.width);
    let height = usize::from(sprite.height);
    let new_width = height;
    let new_height = width;
    let mut pixels = vec![0; new_width * new_height];
    for y in 0..height {
        for x in 0..width {
            let pixel = sprite.pixels[y * width + x];
            let dest_x = height - 1 - y;
            let dest_y = x;
            pixels[dest_y * new_width + dest_x] = pixel;
        }
    }
    ImageFrame {
        offset: sprite.offset,
        x_offset: 0,
        y_offset: 0,
        width: new_width as u16,
        height: new_height as u16,
        pixels,
        palette: sprite.palette,
        transparent_zero: sprite.transparent_zero,
    }
}

fn trim_sprite(sprite: ImageFrame) -> ImageFrame {
    let width = usize::from(sprite.width);
    let height = usize::from(sprite.height);
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut found = false;
    for y in 0..height {
        for x in 0..width {
            if sprite.pixels[y * width + x] == 0 {
                continue;
            }
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            found = true;
        }
    }
    if !found {
        return sprite;
    }
    let trimmed_width = max_x - min_x + 1;
    let trimmed_height = max_y - min_y + 1;
    let mut pixels = Vec::with_capacity(trimmed_width * trimmed_height);
    for y in min_y..=max_y {
        let base = y * width;
        pixels.extend_from_slice(&sprite.pixels[base + min_x..base + max_x + 1]);
    }
    ImageFrame {
        offset: sprite.offset,
        x_offset: 0,
        y_offset: 0,
        width: trimmed_width as u16,
        height: trimmed_height as u16,
        pixels,
        palette: sprite.palette,
        transparent_zero: sprite.transparent_zero,
    }
}

#[cfg(test)]
fn sprite_nontransparent_bounds(sprite: &ImageFrame) -> Option<(usize, usize, usize, usize)> {
    let width = usize::from(sprite.width);
    let height = usize::from(sprite.height);
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut found = false;
    for y in 0..height {
        for x in 0..width {
            let pixel = sprite.pixels[y * width + x];
            if sprite.transparent_zero && pixel == 0 {
                continue;
            }
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            found = true;
        }
    }
    found.then_some((min_x, min_y, max_x, max_y))
}

fn text_pixel_width(text: &str, scale: usize) -> i32 {
    let glyph_width = (4 * scale) as i32;
    (text.chars().count() as i32 * glyph_width).saturating_sub(scale as i32)
}

fn glyph_rows(ch: char) -> Option<[u8; 5]> {
    Some(match ch.to_ascii_uppercase() {
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        'C' => [0b011, 0b100, 0b100, 0b100, 0b011],
        'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
        'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
        'G' => [0b011, 0b100, 0b101, 0b101, 0b011],
        'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
        'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
        'J' => [0b001, 0b001, 0b001, 0b101, 0b010],
        'K' => [0b101, 0b101, 0b110, 0b101, 0b101],
        'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'M' => [0b101, 0b111, 0b111, 0b101, 0b101],
        'N' => [0b101, 0b111, 0b111, 0b111, 0b101],
        'O' => [0b010, 0b101, 0b101, 0b101, 0b010],
        'P' => [0b110, 0b101, 0b110, 0b100, 0b100],
        'Q' => [0b010, 0b101, 0b101, 0b011, 0b001],
        'R' => [0b110, 0b101, 0b110, 0b101, 0b101],
        'S' => [0b011, 0b100, 0b010, 0b001, 0b110],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'U' => [0b101, 0b101, 0b101, 0b101, 0b111],
        'V' => [0b101, 0b101, 0b101, 0b101, 0b010],
        'W' => [0b101, 0b101, 0b111, 0b111, 0b101],
        'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
        'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        'Z' => [0b111, 0b001, 0b010, 0b100, 0b111],
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b110, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b110, 0b001, 0b111, 0b001, 0b110],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b110],
        '6' => [0b011, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b110],
        '.' => [0b000, 0b000, 0b000, 0b000, 0b010],
        _ => return None,
    })
}

pub fn frame_hash(frame: &FrameBuffer320x200) -> u64 {
    frame.pixels_rgba.iter().fold(0u64, |acc, value| {
        acc.wrapping_mul(16777619).wrapping_add(u64::from(*value))
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use skyroads_core::{
        AppInput, AttractModeApp, ControlMode, DisplaySettings, RenderScene, SettingsMenuCursor,
        SettingsMenuScene,
    };
    use skyroads_data::{
        level_from_road_entry, load_demo_rec_path, load_roads_lzs_path, load_skyroads_exe_path,
        ExeShipRuntimeTables, RgbColor, GROUND_Y,
    };

    use super::{
        build_dos_ship_pipeline, clip_window_for_screen_y, derive_ship_visual_state,
        dos_ship_vertical_state, frame_hash, ship_screen_placement, should_draw_game_over_overlay,
        sprite_nontransparent_bounds, AttractModeAssets, CarAtlas, DerivedShipVisualState,
        DosShipPipeline, FrameBuffer320x200, ReferenceRenderer, ShipScreenPlacement,
        ShipSpriteKind, DOS_EXPLOSION_ANIMATION_TICKS, DOS_NON_ALIVE_ANIMATION_TICKS,
        DOS_SHIP_CLIP_MASK_BYTES, DOS_SHIP_SHADOW_MASK_HEIGHT, DOS_SHIP_SHADOW_MASK_WIDTH,
    };

    #[derive(Debug, Clone, Copy)]
    struct PlacementProbe {
        frame_index: usize,
        y_position: f64,
        z_position: f64,
        state: skyroads_core::ShipState,
        is_on_ground: bool,
        is_going_up: bool,
        jump_input: bool,
        jumping: bool,
        vertical_state: i32,
        sprite_center_x: i32,
        sprite_center_y: i32,
        shadow_center_x: i32,
        shadow_center_y: i32,
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn dos_ship_tables() -> ExeShipRuntimeTables {
        load_skyroads_exe_path(repo_root().join("SKYROADS.EXE"))
            .unwrap()
            .runtime_tables
            .ship
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

    fn enter_gameplay(app: &mut AttractModeApp) {
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
        assert!(matches!(tick.render_scene, RenderScene::Gameplay(_)));
    }

    fn enter_demo_playback(app: &mut AttractModeApp) {
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
        assert!(matches!(tick.render_scene, RenderScene::DemoPlayback(_)));
    }

    fn gameplay_scene_after_steps(
        input: AppInput,
        steps: usize,
    ) -> skyroads_core::DemoPlaybackState {
        let mut app = make_app();
        enter_gameplay(&mut app);

        let mut scene = None;
        for _ in 0..steps {
            let tick = app.tick(input);
            let RenderScene::Gameplay(current) = tick.render_scene else {
                panic!("expected gameplay render scene");
            };
            scene = Some(current);
        }

        scene.expect("expected at least one gameplay step")
    }

    fn gameplay_scenes_after_steps(
        input: AppInput,
        steps: usize,
    ) -> Vec<skyroads_core::DemoPlaybackState> {
        let mut app = make_app();
        enter_gameplay(&mut app);

        let mut scenes = Vec::new();
        for _ in 0..steps {
            let tick = app.tick(input);
            let RenderScene::Gameplay(current) = tick.render_scene else {
                panic!("expected gameplay render scene");
            };
            scenes.push(current);
        }

        scenes
    }

    fn demo_scene_after_steps(steps: usize) -> skyroads_core::DemoPlaybackState {
        let mut app = make_app();
        enter_demo_playback(&mut app);

        let mut scene = None;
        for _ in 0..steps {
            let tick = app.tick(AppInput::default());
            let RenderScene::DemoPlayback(current) = tick.render_scene else {
                panic!("expected demo playback render scene");
            };
            scene = Some(current);
        }

        scene.expect("expected at least one demo step")
    }

    fn placement_probe(scene: &skyroads_core::DemoPlaybackState) -> PlacementProbe {
        let ship_tables = dos_ship_tables();
        let visual = derive_ship_visual_state(scene, &ship_tables);
        let placement = ship_screen_placement(scene, &visual, &ship_tables);
        PlacementProbe {
            frame_index: scene.frame_index,
            y_position: scene.ship.y_position,
            z_position: scene.ship.z_position,
            state: scene.ship.state,
            is_on_ground: scene.ship.is_on_ground,
            is_going_up: scene.ship.is_going_up,
            jump_input: scene.ship.jump_input,
            jumping: visual.jumping,
            vertical_state: dos_ship_vertical_state(scene),
            sprite_center_x: placement.sprite_center_x,
            sprite_center_y: placement.sprite_center_y,
            shadow_center_x: placement.shadow_center_x,
            shadow_center_y: placement.shadow_center_y,
        }
    }

    fn frame_non_background_bounds(
        frame: &FrameBuffer320x200,
        background: RgbColor,
    ) -> Option<(usize, usize, usize, usize)> {
        let width = usize::from(frame.width);
        let height = usize::from(frame.height);
        let mut min_x = width;
        let mut min_y = height;
        let mut max_x = 0usize;
        let mut max_y = 0usize;
        let mut found = false;

        for y in 0..height {
            for x in 0..width {
                let offset = (y * width + x) * 4;
                let pixel = RgbColor::new(
                    frame.pixels_rgba[offset],
                    frame.pixels_rgba[offset + 1],
                    frame.pixels_rgba[offset + 2],
                );
                if pixel == background {
                    continue;
                }
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                found = true;
            }
        }

        found.then_some((min_x, min_y, max_x, max_y))
    }

    #[test]
    fn attract_assets_load() {
        let assets = AttractModeAssets::load_from_root(repo_root()).unwrap();
        assert_eq!(assets.intro.frame_count(), 10);
        assert_eq!(assets.anim.frame_count(), 100);
        assert_eq!(assets.main_menu.frame_count(), 3);
        assert_eq!(assets.help_menu.frame_count(), 3);
        assert_eq!(assets.worlds.len(), 10);
        assert_eq!(assets.trekdat.record_count(), 8);
    }

    #[test]
    fn car_atlas_extracts_horizontal_ship_frames() {
        let assets = AttractModeAssets::load_from_root(repo_root()).unwrap();
        let atlas = CarAtlas::from_archive(&assets.cars).unwrap();
        assert_eq!(atlas.explosion_frames.len(), 7);
        assert_eq!(atlas.exact_ship_frames_raw.len(), 63);
        assert_eq!(atlas.exact_ship_frames.len(), 63);
        assert_eq!(atlas.alive_center.len(), 3);
        assert!(atlas
            .exact_ship_frames_raw
            .iter()
            .all(|frame| frame.height == 24));
        let first_exact_bounds =
            sprite_nontransparent_bounds(&atlas.exact_ship_frames_raw[0]).unwrap();
        assert!(
            first_exact_bounds.2 - first_exact_bounds.0 >= 20,
            "expected the first exact ship frame to be a full ship sprite, got bounds {first_exact_bounds:?}"
        );
        assert!(atlas.alive_center[0].width > atlas.alive_center[0].height);
    }

    #[test]
    fn exact_ship_frames_keep_dos_anchor_inside_sprite_box() {
        let assets = AttractModeAssets::load_from_root(repo_root()).unwrap();
        let renderer = ReferenceRenderer::new(assets);
        let car_atlas = renderer.car_atlas.as_ref().unwrap();
        let background = RgbColor::new(255, 0, 255);

        for index in 0..car_atlas.exact_ship_frames_raw.len() {
            let visual = DerivedShipVisualState {
                sprite_kind: ShipSpriteKind::Alive,
                bank: super::ShipBank::Center,
                jumping: false,
                explosion_frame: 0,
                exact_ship_frame_index: Some(index),
                on_surface: true,
            };
            let placement = ShipScreenPlacement {
                sprite_left: 80,
                sprite_top: 40,
                sprite_center_x: 0,
                sprite_center_y: 0,
                shadow_left: 0,
                shadow_top: 0,
                shadow_center_x: 0,
                shadow_center_y: 0,
            };
            let mut pipeline = DosShipPipeline {
                placement,
                clip_mask: [1; DOS_SHIP_CLIP_MASK_BYTES],
                shadow_surface_mask: [1; DOS_SHIP_SHADOW_MASK_HEIGHT * DOS_SHIP_SHADOW_MASK_WIDTH],
                shadow_variant: None,
            };
            let mut frame = FrameBuffer320x200::new();
            frame.clear(background);

            renderer.draw_ship_sprite(&mut frame, 0, &visual, &mut pipeline);

            let rendered_bounds =
                frame_non_background_bounds(&frame, background).expect("expected ship pixels");
            let raw_bounds = sprite_nontransparent_bounds(&car_atlas.exact_ship_frames_raw[index])
                .expect("expected raw sprite pixels");

            assert_eq!(
                rendered_bounds.0 as i32 - placement.sprite_left,
                raw_bounds.0 as i32,
                "expected exact frame {index} to keep its raw left anchor"
            );
            assert_eq!(
                rendered_bounds.1 as i32 - placement.sprite_top,
                raw_bounds.1 as i32,
                "expected exact frame {index} to keep its raw top anchor"
            );
            assert_eq!(
                rendered_bounds.2 as i32 - placement.sprite_left,
                raw_bounds.2 as i32,
                "expected exact frame {index} to keep its raw right edge"
            );
            assert_eq!(
                rendered_bounds.3 as i32 - placement.sprite_top,
                raw_bounds.3 as i32,
                "expected exact frame {index} to keep its raw bottom edge"
            );
        }
    }

    #[test]
    fn flat_opening_road_does_not_clip_ship_when_steering() {
        let assets = AttractModeAssets::load_from_root(repo_root()).unwrap();
        let renderer = ReferenceRenderer::new(assets);
        let car_atlas = renderer.car_atlas.as_ref().unwrap();
        let background = RgbColor::new(255, 0, 255);

        for input in [
            AppInput {
                up_held: true,
                left_held: true,
                ..AppInput::default()
            },
            AppInput {
                up_held: true,
                right_held: true,
                ..AppInput::default()
            },
        ] {
            let scene = gameplay_scene_after_steps(input, 8);
            assert!(scene.ship.is_on_ground, "expected early steering scene to stay grounded");
            assert!(
                scene.current_row <= 24,
                "expected early steering scene to stay on the opening flat road, got row {}",
                scene.current_row
            );

            let visual = derive_ship_visual_state(&scene, &renderer.assets.dos_ship_tables);
            let exact_frame_index = visual
                .exact_ship_frame_index
                .expect("expected live steering scene to use an exact ship frame");
            let road_coverage = renderer.build_dos_road_coverage_frame(&scene);
            let mut pipeline = build_dos_ship_pipeline(
                &scene,
                &renderer.assets.dos_ship_tables,
                road_coverage.as_ref(),
                &visual,
            );
            let placement = pipeline.placement;
            let mut frame = FrameBuffer320x200::new();
            frame.clear(background);

            renderer.draw_ship_sprite(&mut frame, scene.frame_index, &visual, &mut pipeline);

            let rendered_bounds =
                frame_non_background_bounds(&frame, background).expect("expected ship pixels");
            let raw_bounds = sprite_nontransparent_bounds(
                &car_atlas.exact_ship_frames_raw[exact_frame_index],
            )
            .expect("expected raw ship pixels");

            assert_eq!(
                rendered_bounds.0 as i32 - placement.sprite_left,
                raw_bounds.0 as i32,
                "expected steering frame {} to keep the raw left edge",
                scene.frame_index
            );
            assert_eq!(
                rendered_bounds.2 as i32 - placement.sprite_left,
                raw_bounds.2 as i32,
                "expected steering frame {} to keep the raw right edge",
                scene.frame_index
            );
        }
    }

    #[test]
    fn airborne_demo_left_edge_keeps_ship_pixels_visible() {
        let assets = AttractModeAssets::load_from_root(repo_root()).unwrap();
        let renderer = ReferenceRenderer::new(assets);
        let scene = demo_scene_after_steps(760);
        let frame = renderer.render_scene(&RenderScene::DemoPlayback(scene.clone()));

        let mut ship_pixels = 0usize;
        for y in 0..72 {
            for x in 0..64 {
                let offset = (y * usize::from(frame.width) + x) * 4;
                let r = frame.pixels_rgba[offset];
                let g = frame.pixels_rgba[offset + 1];
                let b = frame.pixels_rgba[offset + 2];
                if b > 90 && b > r && b > g {
                    ship_pixels += 1;
                }
            }
        }

        assert!(
            ship_pixels > 24,
            "expected airborne left-edge demo frame to keep visible ship pixels, found {ship_pixels}"
        );
    }

    #[test]
    fn road_coverage_clip_window_prefers_span_containing_ship_center() {
        let mut coverage = FrameBuffer320x200::new();
        coverage.clear(RgbColor::new(0, 0, 0));
        coverage.fill_rect(24, 80, 28, 1, RgbColor::new(255, 255, 255));
        coverage.fill_rect(118, 80, 44, 1, RgbColor::new(255, 255, 255));

        let window = clip_window_for_screen_y(&coverage, 80, 136).expect("expected coverage span");

        assert_eq!(window, (118, 162));
    }

    #[test]
    fn airborne_alive_ship_keeps_shadow_variant() {
        let ship_tables = dos_ship_tables();
        let scenes = gameplay_scenes_after_steps(
            AppInput {
                up_held: true,
                space_held: true,
                ..AppInput::default()
            },
            24,
        );
        let airborne_scene = scenes
            .iter()
            .find(|scene| {
                scene.ship.state == skyroads_core::ShipState::Alive
                    && (!scene.ship.is_on_ground || scene.ship.y_position > GROUND_Y)
            })
            .expect("expected a live airborne scene");
        let visual = derive_ship_visual_state(airborne_scene, &ship_tables);
        let pipeline = build_dos_ship_pipeline(airborne_scene, &ship_tables, None, &visual);

        assert!(
            pipeline.shadow_variant.is_some(),
            "expected live airborne ship to keep a DOS shadow variant"
        );
    }

    #[test]
    fn intro_and_menu_render_non_empty_frames() {
        let assets = AttractModeAssets::load_from_root(repo_root()).unwrap();
        let renderer = ReferenceRenderer::new(assets);
        let mut app = make_app();

        let intro = renderer.render_scene(&app.tick(AppInput::default()).render_scene);
        assert_ne!(frame_hash(&intro), 0);

        for _ in 0..35 {
            app.tick(AppInput::default());
        }
        let menu = renderer.render_scene(
            &app.tick(AppInput {
                space: true,
                ..AppInput::default()
            })
            .render_scene,
        );
        assert_ne!(frame_hash(&menu), frame_hash(&intro));
    }

    #[test]
    fn settings_menu_composes_base_and_overlay_fragments() {
        let assets = AttractModeAssets::load_from_root(repo_root()).unwrap();
        let renderer = ReferenceRenderer::new(assets);

        let keyboard = renderer.render_scene(&RenderScene::SettingsMenu(SettingsMenuScene {
            cursor: SettingsMenuCursor::Keyboard,
            control_mode: ControlMode::Keyboard,
            display_settings: DisplaySettings::default(),
            sound_fx_enabled: true,
            music_enabled: true,
        }));
        let mouse = renderer.render_scene(&RenderScene::SettingsMenu(SettingsMenuScene {
            cursor: SettingsMenuCursor::Borderless,
            control_mode: ControlMode::Mouse,
            display_settings: DisplaySettings {
                fullscreen: true,
                borderless: true,
            },
            sound_fx_enabled: false,
            music_enabled: false,
        }));

        assert_ne!(frame_hash(&keyboard), 0);
        assert_ne!(frame_hash(&keyboard), frame_hash(&mouse));
    }

    #[test]
    fn gameplay_renders_visible_playfield_rows() {
        let assets = AttractModeAssets::load_from_root(repo_root()).unwrap();
        let renderer = ReferenceRenderer::new(assets);
        let mut app = make_app();

        for _ in 0..35 {
            app.tick(AppInput::default());
        }
        app.tick(AppInput {
            space: true,
            ..AppInput::default()
        });
        let gameplay = app.tick(AppInput {
            enter: true,
            ..AppInput::default()
        });
        let frame = renderer.render_scene(&gameplay.render_scene);

        let mut non_black_pixels = 0usize;
        for y in 30..138 {
            for x in 0..usize::from(frame.width) {
                let offset = (y * usize::from(frame.width) + x) * 4;
                if frame.pixels_rgba[offset] != 0
                    || frame.pixels_rgba[offset + 1] != 0
                    || frame.pixels_rgba[offset + 2] != 0
                {
                    non_black_pixels += 1;
                }
            }
        }

        assert!(
            non_black_pixels > 5_000,
            "expected visible gameplay pixels in the playfield, found {non_black_pixels}"
        );
    }

    #[test]
    fn gameplay_renders_ship_pixels_in_center_playfield() {
        let assets = AttractModeAssets::load_from_root(repo_root()).unwrap();
        let renderer = ReferenceRenderer::new(assets);
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
        let gameplay = app.tick(AppInput {
            up_held: true,
            ..AppInput::default()
        });
        let frame = renderer.render_scene(&gameplay.render_scene);

        let mut ship_pixels = 0usize;
        for y in 52..104 {
            for x in 120..200 {
                let offset = (y * usize::from(frame.width) + x) * 4;
                let r = frame.pixels_rgba[offset];
                let g = frame.pixels_rgba[offset + 1];
                let b = frame.pixels_rgba[offset + 2];
                if b > 90 && b > r && b > g {
                    ship_pixels += 1;
                }
            }
        }

        assert!(
            ship_pixels > 40,
            "expected visible ship pixels, found {ship_pixels}"
        );
    }

    #[test]
    fn gameplay_ship_placement_moves_visibly_with_steering() {
        let neutral = gameplay_scene_after_steps(
            AppInput {
                up_held: true,
                ..AppInput::default()
            },
            20,
        );
        let left = gameplay_scene_after_steps(
            AppInput {
                up_held: true,
                left_held: true,
                ..AppInput::default()
            },
            20,
        );
        let right = gameplay_scene_after_steps(
            AppInput {
                up_held: true,
                right_held: true,
                ..AppInput::default()
            },
            20,
        );

        let neutral_probe = placement_probe(&neutral);
        let left_probe = placement_probe(&left);
        let right_probe = placement_probe(&right);

        assert!(left.ship.x_position < neutral.ship.x_position);
        assert!(right.ship.x_position > neutral.ship.x_position);
        assert!(
            left_probe.sprite_center_x <= neutral_probe.sprite_center_x - 8,
            "expected left steering to move the ship left on screen: left={} neutral={}",
            left_probe.sprite_center_x,
            neutral_probe.sprite_center_x
        );
        assert!(
            right_probe.sprite_center_x >= neutral_probe.sprite_center_x + 8,
            "expected right steering to move the ship right on screen: right={} neutral={}",
            right_probe.sprite_center_x,
            neutral_probe.sprite_center_x
        );
        assert!(
            left_probe.shadow_center_x <= neutral_probe.shadow_center_x - 8,
            "expected left steering to move the shadow left on screen: left={} neutral={}",
            left_probe.shadow_center_x,
            neutral_probe.shadow_center_x
        );
        assert!(
            right_probe.shadow_center_x >= neutral_probe.shadow_center_x + 8,
            "expected right steering to move the shadow right on screen: right={} neutral={}",
            right_probe.shadow_center_x,
            neutral_probe.shadow_center_x
        );
    }

    #[test]
    fn grounded_throttle_keeps_ship_pose_stable_before_fall() {
        let scenes = gameplay_scenes_after_steps(
            AppInput {
                up_held: true,
                ..AppInput::default()
            },
            220,
        );

        let probes = scenes.iter().map(placement_probe).collect::<Vec<_>>();

        let grounded = probes
            .iter()
            .take_while(|probe| {
                probe.state == skyroads_core::ShipState::Alive && probe.is_on_ground
            })
            .copied()
            .collect::<Vec<_>>();
        let first_sprite_y = grounded
            .first()
            .map(|probe| probe.sprite_center_y)
            .unwrap_or(0);
        let first_shadow_y = grounded
            .first()
            .map(|probe| probe.shadow_center_y)
            .unwrap_or(0);
        let first_shadow_x = grounded
            .first()
            .map(|probe| probe.shadow_center_x)
            .unwrap_or(0);
        let max_sprite_y_delta = grounded
            .iter()
            .map(|probe| (probe.sprite_center_y - first_sprite_y).abs())
            .max()
            .unwrap_or(0);
        let max_shadow_y_delta = grounded
            .iter()
            .map(|probe| (probe.shadow_center_y - first_shadow_y).abs())
            .max()
            .unwrap_or(0);
        let max_shadow_x_delta = grounded
            .iter()
            .map(|probe| (probe.shadow_center_x - first_shadow_x).abs())
            .max()
            .unwrap_or(0);

        for probe in &grounded {
            assert!(
                (probe.y_position - GROUND_Y).abs() < f64::EPSILON,
                "expected throttle frame {} to stay on ground, got y={} z={} state={:?} on_ground={} going_up={} jump_input={} jumping={} vertical_state={}",
                probe.frame_index,
                probe.y_position,
                probe.z_position,
                probe.state,
                probe.is_on_ground,
                probe.is_going_up,
                probe.jump_input,
                probe.jumping,
                probe.vertical_state
            );
            assert!(
                !probe.jumping,
                "expected grounded throttle frame {} to avoid jump pose",
                probe.frame_index
            );
            assert_eq!(
                probe.vertical_state, 0,
                "expected grounded throttle frame {} to use neutral vertical state",
                probe.frame_index
            );
        }

        assert!(
            max_sprite_y_delta <= 2,
            "expected grounded throttle sprite placement to stay stable, got {} grounded frames with max delta {}",
            grounded.len(),
            max_sprite_y_delta
        );
        assert!(
            max_shadow_y_delta <= 2,
            "expected grounded throttle shadow Y to stay stable, got {} grounded frames with max delta {}",
            grounded.len(),
            max_shadow_y_delta
        );
        assert!(
            max_shadow_x_delta <= 2,
            "expected grounded throttle shadow X to stay stable, got {} grounded frames with max delta {}",
            grounded.len(),
            max_shadow_x_delta
        );
    }

    #[test]
    fn explosion_visual_state_follows_dos_explosion_timer() {
        let mut scene = gameplay_scene_after_steps(
            AppInput {
                up_held: true,
                ..AppInput::default()
            },
            4,
        );
        scene.ship.state = skyroads_core::ShipState::Exploded;
        scene.snapshot.craft_state = skyroads_core::ShipState::Exploded;
        scene.ship.explosion_timer = 8;

        let visual = derive_ship_visual_state(&scene, &dos_ship_tables());
        assert_eq!(visual.explosion_frame, 2);
    }

    #[test]
    fn fallen_ship_can_continue_below_the_playfield() {
        let mut scene = gameplay_scene_after_steps(
            AppInput {
                up_held: true,
                ..AppInput::default()
            },
            4,
        );
        scene.ship.state = skyroads_core::ShipState::Fallen;
        scene.snapshot.craft_state = skyroads_core::ShipState::Fallen;
        scene.ship.y_position = GROUND_Y - 140.0;

        let ship_tables = dos_ship_tables();
        let visual = derive_ship_visual_state(&scene, &ship_tables);
        let placement = ship_screen_placement(&scene, &visual, &ship_tables);
        assert!(
            placement.sprite_center_y > i32::from(skyroads_data::SCREEN_HEIGHT),
            "expected fallen ship to move off-screen instead of clamping in view, got y={}",
            placement.sprite_center_y
        );
    }

    #[test]
    fn game_over_overlay_waits_for_dos_explosion_window() {
        let mut scene = gameplay_scene_after_steps(
            AppInput {
                up_held: true,
                ..AppInput::default()
            },
            4,
        );
        scene.snapshot.craft_state = skyroads_core::ShipState::Exploded;
        scene.ship.state = skyroads_core::ShipState::Exploded;
        scene.ship.explosion_timer = DOS_EXPLOSION_ANIMATION_TICKS;
        assert!(
            !should_draw_game_over_overlay(&scene),
            "expected fresh death frame to keep gameplay visible"
        );

        scene.ship.explosion_timer += 1;
        assert!(
            should_draw_game_over_overlay(&scene),
            "expected overlay to appear after the delay elapsed"
        );
    }

    #[test]
    fn game_over_overlay_waits_for_dos_non_alive_window() {
        let mut scene = gameplay_scene_after_steps(
            AppInput {
                up_held: true,
                ..AppInput::default()
            },
            4,
        );
        scene.snapshot.craft_state = skyroads_core::ShipState::Fallen;
        scene.ship.state = skyroads_core::ShipState::Fallen;
        scene.ship.non_alive_frame_count = DOS_NON_ALIVE_ANIMATION_TICKS - 1;
        assert!(
            !should_draw_game_over_overlay(&scene),
            "expected fallen ship to stay visible until the DOS death window ends"
        );

        scene.ship.non_alive_frame_count += 1;
        assert!(
            should_draw_game_over_overlay(&scene),
            "expected fallen ship to hand over to the overlay after the DOS window"
        );
    }

    #[test]
    fn branding_draws_expected_bottom_text_pixels() {
        let assets = AttractModeAssets::load_from_root(repo_root()).unwrap();
        let renderer = ReferenceRenderer::new(assets);
        let mut frame = FrameBuffer320x200::new();
        renderer.draw_branding(&mut frame, 184, 2, 1.0);

        let mut bright_pixels = 0usize;
        for y in 184..196 {
            for x in 40..280 {
                let offset = (y * usize::from(frame.width) + x) * 4;
                if frame.pixels_rgba[offset] > 150 && frame.pixels_rgba[offset + 1] > 120 {
                    bright_pixels += 1;
                }
            }
        }

        assert!(
            bright_pixels > 200,
            "expected visible branding pixels, found {bright_pixels}"
        );
    }
}
