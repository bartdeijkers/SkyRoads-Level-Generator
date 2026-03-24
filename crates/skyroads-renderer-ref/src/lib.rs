use std::path::Path;

use skyroads_core::{
    renderer_row_state, ControlMode, DemoPlaybackState, HelpMenuScene, IntroSequenceState,
    MainMenuScene, RenderScene, RoadRenderRow, SettingsMenuScene,
};
use skyroads_data::{
    load_dashboard_dat_path, load_image_archive_path, load_trekdat_lzs_path, HudFragmentPack,
    ImageArchive, ImageFrame, LevelCell, Result, RgbColor, TouchEffect, TrekdatArchive,
    TrekdatCellPointers, TrekdatRecord, TrekdatShape, DASHBOARD_COLORS, GROUND_Y,
    LEVEL_CENTER_X, LEVEL_TILE_STRIDE_X, ROAD_COLUMNS, SCREEN_HEIGHT, SCREEN_WIDTH,
};

const FRAMEBUFFER_WIDTH: usize = SCREEN_WIDTH as usize;
const FRAMEBUFFER_HEIGHT: usize = SCREEN_HEIGHT as usize;
const DASHBOARD_TOP: usize = 138;
const HORIZON_Y: usize = 24;
const VIEW_BOTTOM_Y: usize = DASHBOARD_TOP;
const SHIP_SCALE: usize = 1;
const SHIP_SCREEN_X: i32 = 160;
const SHIP_SCREEN_Y: i32 = 84;
const GAME_OVER_OVERLAY_DELAY_FRAMES: usize = 24;
const DEBUG_PANEL_X: i32 = 8;
const DEBUG_PANEL_Y: i32 = 8;
const DEBUG_PANEL_W: i32 = 124;
const DEBUG_PANEL_H: i32 = 42;
const DEBUG_TOPDOWN_INSET_X: i32 = 206;
const DEBUG_TOPDOWN_INSET_Y: i32 = 28;
const DEBUG_TOPDOWN_INSET_W: i32 = 104;
const DEBUG_TOPDOWN_INSET_H: i32 = 84;

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
    thrust_on: bool,
    jumping: bool,
    explosion_frame: usize,
    exact_ship_frame_index: Option<usize>,
    ship_screen_bias_x: i32,
    vertical_offset_y: i32,
    on_surface: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShipScreenPlacement {
    sprite_center_x: i32,
    sprite_center_y: i32,
    shadow_center_x: i32,
    shadow_center_y: i32,
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
        self.draw_archive_frame(
            frame,
            &self.assets.settings_menu,
            scene.cursor.overlay_frame_index(),
            1.0,
            1.0,
        );
    }

    fn render_play_scene(&self, frame: &mut FrameBuffer320x200, scene: &DemoPlaybackState) {
        let ship_visual = derive_ship_visual_state(scene);
        let ship_placement = ship_screen_placement(scene, &ship_visual);
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
        self.draw_ship_shadow(frame, &ship_visual, ship_placement);
        self.draw_ship_sprite(frame, scene.frame_index, &ship_visual, ship_placement);
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
        placement: ShipScreenPlacement,
    ) {
        let Some(car_atlas) = &self.car_atlas else {
            return;
        };
        let sprite = car_atlas.select_sprite(visual, frame_index);

        let draw_width = usize::from(sprite.width) * SHIP_SCALE;
        let draw_height = usize::from(sprite.height) * SHIP_SCALE;
        let x = placement.sprite_center_x - (draw_width as i32 / 2);
        let explode_offset = if visual.sprite_kind == ShipSpriteKind::Exploding {
            -2
        } else {
            0
        };
        let y = placement.sprite_center_y + explode_offset - (draw_height as i32 / 2);
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
        placement: ShipScreenPlacement,
    ) {
        if !visual.on_surface || visual.sprite_kind != ShipSpriteKind::Alive {
            return;
        }
        let shadow_center_x = placement.shadow_center_x;
        let shadow_center_y = placement.shadow_center_y;
        let hover = (-visual.vertical_offset_y).max(0);
        let radius_x = (9 - hover / 6).clamp(4, 9);
        let radius_y = (4 - hover / 10).clamp(2, 4);
        for dy in -radius_y..=radius_y {
            for dx in -radius_x..=radius_x {
                let ellipse = (dx * dx * 100) / (radius_x * radius_x)
                    + (dy * dy * 100) / (radius_y * radius_y);
                if ellipse > 100 {
                    continue;
                }
                let px = shadow_center_x + dx;
                let py = shadow_center_y + dy;
                if px < 0 || py < HORIZON_Y as i32 || py >= VIEW_BOTTOM_Y as i32 {
                    continue;
                }
                frame.blend_pixel(px as usize, py as usize, RgbColor::new(0, 0, 0), 0.18);
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

    fn draw_debug_overlay(&self, frame: &mut FrameBuffer320x200, scene: &DemoPlaybackState) {
        let visual = derive_ship_visual_state(scene);
        let slices = project_road_slices(scene);
        let placement = ship_screen_placement_from_slices(scene, &visual, &slices);
        self.draw_debug_hud_panel(frame, scene, DebugViewMode::Overlay);
        self.draw_projected_slice_guides(frame, &slices);
        self.draw_ship_debug_guides(frame, scene, &visual, placement);
        self.draw_topdown_inset(frame, scene);
    }

    fn render_play_geometry_debug(&self, frame: &mut FrameBuffer320x200, scene: &DemoPlaybackState) {
        let visual = derive_ship_visual_state(scene);
        let slices = project_road_slices(scene);
        let placement = ship_screen_placement_from_slices(scene, &visual, &slices);
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
        self.draw_ship_shadow(frame, &visual, placement);
        self.draw_ship_sprite(frame, scene.frame_index, &visual, placement);
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
        let slot_text = format!("GRP {:02} SLT {}", row_state.road_row_group, row_state.trekdat_slot);
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
            frame.fill_rect(left_top, slice.top_y as i32, 1, 1, RgbColor::new(110, 255, 170));
            frame.fill_rect(right_top, slice.top_y as i32, 1, 1, RgbColor::new(110, 255, 170));
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
        let sprite = car_atlas.select_sprite(visual, scene.frame_index);
        let draw_width = usize::from(sprite.width) * SHIP_SCALE;
        let draw_height = usize::from(sprite.height) * SHIP_SCALE;
        let x = placement.sprite_center_x - (draw_width as i32 / 2);
        let y = placement.sprite_center_y - (draw_height as i32 / 2);
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
        let ship_col = ((scene.ship.x_position - left_edge) / (LEVEL_TILE_STRIDE_X * ROAD_COLUMNS as f64))
            .clamp(0.0, 0.999);
        let ship_x = x + 4 + (ship_col * f64::from((w - 8).max(1))) as i32;
        let ship_y = y + 4 + (ship_row * f64::from((h - 8).max(1))) as i32;
        frame.fill_rect(ship_x - 2, ship_y - 2, 5, 5, RgbColor::new(112, 214, 255));
        if large {
            self.draw_text(frame, x + 4, y - 10, "TOPDOWN", RgbColor::new(244, 233, 146), 1);
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

fn derive_ship_visual_state(scene: &DemoPlaybackState) -> DerivedShipVisualState {
    let lane_index = dos_ship_lane_index(scene.ship.x_position);
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
        | skyroads_core::ShipState::OutOfOxygen => {
            ShipSpriteKind::Destroyed
        }
    };
    let jumping = scene.ship.y_position > GROUND_Y + 0.5
        || scene.ship.is_going_up
        || !scene.ship.is_on_ground
        || scene.ship.jump_input;
    let vertical_state = dos_ship_vertical_state(scene);
    let lane_bias = (scene.ship.x_position - LEVEL_CENTER_X) / LEVEL_TILE_STRIDE_X;
    let ship_lane_bias = (lane_bias * 30.0).round() as i32;
    let ship_screen_bias_x = ship_lane_bias.clamp(-96, 96);
    let height_delta = scene.ship.y_position - GROUND_Y;
    let vertical_offset_y = (-height_delta * 0.55).round().clamp(-26.0, 18.0) as i32;
    let explosion_frame = scene
        .ship
        .death_frame_index
        .map(|death_frame| scene.frame_index.saturating_sub(death_frame) / 3)
        .unwrap_or(0);

    DerivedShipVisualState {
        sprite_kind,
        bank,
        thrust_on: scene.ship.accel_input > 0 && scene.ship.state == skyroads_core::ShipState::Alive,
        jumping,
        explosion_frame,
        exact_ship_frame_index: (scene.ship.state == skyroads_core::ShipState::Alive)
            .then_some(((lane_index * 3 + vertical_state) * 3) as usize),
        ship_screen_bias_x,
        vertical_offset_y,
        on_surface: scene.ship.is_on_ground && scene.ship.state == skyroads_core::ShipState::Alive,
    }
}

fn should_draw_game_over_overlay(scene: &DemoPlaybackState) -> bool {
    if scene.snapshot.craft_state == skyroads_core::ShipState::Alive {
        return false;
    }
    let Some(death_frame_index) = scene.ship.death_frame_index else {
        return true;
    };
    scene.frame_index.saturating_sub(death_frame_index) >= GAME_OVER_OVERLAY_DELAY_FRAMES
}

fn ship_screen_placement(
    scene: &DemoPlaybackState,
    visual: &DerivedShipVisualState,
) -> ShipScreenPlacement {
    let slices = project_road_slices(scene);
    ship_screen_placement_from_slices(scene, visual, &slices)
}

fn ship_screen_placement_from_slices(
    scene: &DemoPlaybackState,
    visual: &DerivedShipVisualState,
    slices: &[ProjectedRoadSlice],
) -> ShipScreenPlacement {
    let _ = (scene, slices);
    let fallback_center_x = SHIP_SCREEN_X + visual.ship_screen_bias_x;
    let fallback_center_y = SHIP_SCREEN_Y + visual.vertical_offset_y;
    let fallback_shadow_x = fallback_center_x;
    let fallback_shadow_y = SHIP_SCREEN_Y + 18;

    // Keep the shadow in the same stable screen-space frame as the ship until the exact
    // DOS shadow/contact path is ported. The old slice-based shadow anchor drifted against
    // the stabilized ship sprite and produced visibly unrealistic motion.
    ShipScreenPlacement {
        sprite_center_x: fallback_center_x,
        sprite_center_y: fallback_center_y,
        shadow_center_x: fallback_shadow_x,
        shadow_center_y: fallback_shadow_y,
    }
}

fn settings_menu_selected_control_overlay(control_mode: ControlMode) -> usize {
    match control_mode {
        ControlMode::Keyboard => 6,
        ControlMode::Joystick => 7,
        ControlMode::Mouse => 8,
    }
}

fn dos_ship_lane_index(x_position: f64) -> i32 {
    let coarse_x = x_position.floor() as i32;
    ((coarse_x - 95) / 46).clamp(0, 6)
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
        if sprites.len() < 48 {
            return None;
        }
        let explosion_frames = sprites
            .iter()
            .take(7)
            .cloned()
            .map(trim_sprite)
            .collect::<Vec<_>>();
        let exact_ship_frames = sprites[14..77]
            .iter()
            .cloned()
            .map(rotate_sprite_cw)
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
                let phase = if visual.thrust_on {
                    (frame_index / 4) % bank_frames.len().max(1)
                } else {
                    0
                };
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

    use skyroads_core::{AppInput, AttractModeApp, ControlMode, RenderScene, SettingsMenuCursor, SettingsMenuScene};
    use skyroads_data::{level_from_road_entry, load_demo_rec_path, load_roads_lzs_path, GROUND_Y};

    use super::{
        derive_ship_visual_state, dos_ship_vertical_state, frame_hash, ship_screen_placement,
        should_draw_game_over_overlay, AttractModeAssets, CarAtlas, FrameBuffer320x200,
        ReferenceRenderer, GAME_OVER_OVERLAY_DELAY_FRAMES, SHIP_SCREEN_X, SHIP_SCREEN_Y,
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
        ship_screen_bias_x: i32,
        vertical_offset_y: i32,
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

    fn gameplay_scene_after_steps(input: AppInput, steps: usize) -> skyroads_core::DemoPlaybackState {
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

    fn placement_probe(scene: &skyroads_core::DemoPlaybackState) -> PlacementProbe {
        let visual = derive_ship_visual_state(scene);
        let placement = ship_screen_placement(scene, &visual);
        PlacementProbe {
            frame_index: scene.frame_index,
            y_position: scene.ship.y_position,
            z_position: scene.ship.z_position,
            state: scene.ship.state,
            is_on_ground: scene.ship.is_on_ground,
            is_going_up: scene.ship.is_going_up,
            jump_input: scene.ship.jump_input,
            ship_screen_bias_x: visual.ship_screen_bias_x,
            vertical_offset_y: visual.vertical_offset_y,
            jumping: visual.jumping,
            vertical_state: dos_ship_vertical_state(scene),
            sprite_center_x: placement.sprite_center_x,
            sprite_center_y: placement.sprite_center_y,
            shadow_center_x: placement.shadow_center_x,
            shadow_center_y: placement.shadow_center_y,
        }
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
        assert_eq!(atlas.alive_center.len(), 3);
        assert!(atlas.alive_center[0].width > atlas.alive_center[0].height);
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
            sound_fx_enabled: true,
            music_enabled: true,
        }));
        let mouse = renderer.render_scene(&RenderScene::SettingsMenu(SettingsMenuScene {
            cursor: SettingsMenuCursor::Music,
            control_mode: ControlMode::Mouse,
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
    fn grounded_throttle_keeps_ship_pose_stable_and_airborne_ship_uses_fallback_anchor() {
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
            .take_while(|probe| probe.state == skyroads_core::ShipState::Alive && probe.is_on_ground)
            .copied()
            .collect::<Vec<_>>();
        let first_sprite_y = grounded.first().map(|probe| probe.sprite_center_y).unwrap_or(0);
        let first_shadow_y = grounded.first().map(|probe| probe.shadow_center_y).unwrap_or(0);
        let first_shadow_x = grounded.first().map(|probe| probe.shadow_center_x).unwrap_or(0);
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

        let airborne = probes
            .iter()
            .find(|probe| probe.state != skyroads_core::ShipState::Alive || !probe.is_on_ground)
            .copied()
            .expect("expected sustained throttle to reach an airborne or fallen frame");
        let expected_fallback_x = SHIP_SCREEN_X + airborne.ship_screen_bias_x;
        let expected_sprite_y = SHIP_SCREEN_Y + airborne.vertical_offset_y;
        let expected_shadow_y = SHIP_SCREEN_Y + 18;
        assert_eq!(
            airborne.sprite_center_x, expected_fallback_x,
            "expected airborne ship sprite to use fallback X anchor at frame {}, got x={} expected={} world_y={} z={} state={:?}",
            airborne.frame_index,
            airborne.sprite_center_x,
            expected_fallback_x,
            airborne.y_position,
            airborne.z_position,
            airborne.state
        );
        assert_eq!(
            airborne.shadow_center_x, expected_fallback_x,
            "expected airborne shadow to use fallback X anchor at frame {}, got x={} expected={} world_y={} z={} state={:?}",
            airborne.frame_index,
            airborne.shadow_center_x,
            expected_fallback_x,
            airborne.y_position,
            airborne.z_position,
            airborne.state
        );
        assert_eq!(
            airborne.sprite_center_y, expected_sprite_y,
            "expected airborne ship sprite to use fallback Y anchor at frame {}, got y={} expected={} world_y={} z={} state={:?}",
            airborne.frame_index,
            airborne.sprite_center_y,
            expected_sprite_y,
            airborne.y_position,
            airborne.z_position,
            airborne.state
        );
        assert_eq!(
            airborne.shadow_center_y, expected_shadow_y,
            "expected airborne shadow to use fallback Y anchor at frame {}, got y={} expected={} world_y={} z={} state={:?}",
            airborne.frame_index,
            airborne.shadow_center_y,
            expected_shadow_y,
            airborne.y_position,
            airborne.z_position,
            airborne.state
        );
    }

    #[test]
    fn game_over_overlay_waits_before_covering_gameplay() {
        let mut scene = gameplay_scene_after_steps(
            AppInput {
                up_held: true,
                ..AppInput::default()
            },
            4,
        );
        scene.snapshot.craft_state = skyroads_core::ShipState::Exploded;
        scene.ship.state = skyroads_core::ShipState::Exploded;
        scene.ship.death_frame_index = Some(scene.frame_index);
        assert!(
            !should_draw_game_over_overlay(&scene),
            "expected fresh death frame to keep gameplay visible"
        );

        scene.frame_index += GAME_OVER_OVERLAY_DELAY_FRAMES - 1;
        assert!(
            !should_draw_game_over_overlay(&scene),
            "expected overlay to stay hidden until the full delay elapsed"
        );

        scene.frame_index += 1;
        assert!(
            should_draw_game_over_overlay(&scene),
            "expected overlay to appear after the delay elapsed"
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
