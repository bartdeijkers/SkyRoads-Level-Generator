mod app;
mod gameplay;

use skyroads_data::{
    analyze_road_descriptor, DemoInput, DemoRecording, ExeDispatchEntry, RoadDescriptor, RoadEntry,
    SkyroadsExe, DEMO_TILE_POSITION_STEP_FP16, ROAD_COLUMNS,
};

pub use app::{
    AppInput, AppMode, AppTickResult, AttractModeApp, AudioCommand, ControlMode,
    DemoPlaybackState, HelpMenuScene, IntroSequenceState, MainMenuScene, MenuCursor,
    RenderScene, RoadRenderRow, SettingsMenuCursor, SettingsMenuScene, ShipRenderState,
};
pub use gameplay::{
    controller_state_from_demo_input, controller_state_from_dos_joystick,
    controller_state_from_dos_mouse,
    sample_demo_input_for_ship, ControllerState, GameSnapshot, GameplayEvent,
    GameplayFrameResult, GameplaySession, Ship, ShipState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemoCursor {
    pub z_position_fp16: u32,
    pub index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RendererRowState {
    pub current_row: u16,
    pub road_row_group: usize,
    pub trekdat_slot: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RendererCellPlan {
    pub current_row: u16,
    pub road_row_group: usize,
    pub trekdat_slot: usize,
    pub descriptor: RoadDescriptor,
    pub tile_class: u8,
    pub dispatch: ExeDispatchEntry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RendererRowPlan {
    pub current_row: u16,
    pub road_row_group: usize,
    pub trekdat_slot: usize,
    pub cells: [RendererCellPlan; ROAD_COLUMNS],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameplayFramePlan<'a> {
    pub z_position_fp16: u32,
    pub demo_cursor: DemoCursor,
    pub demo_input: Option<&'a DemoInput>,
    pub road_index: usize,
    pub road_row_index: usize,
    pub renderer_row: RendererRowPlan,
}

pub fn demo_index_for_z_position(z_position_fp16: u32) -> usize {
    (z_position_fp16 / DEMO_TILE_POSITION_STEP_FP16) as usize
}

pub fn demo_cursor(z_position_fp16: u32) -> DemoCursor {
    DemoCursor {
        z_position_fp16,
        index: demo_index_for_z_position(z_position_fp16),
    }
}

pub fn sample_demo_input<'a>(
    demo: &'a DemoRecording,
    z_position_fp16: u32,
) -> Option<&'a DemoInput> {
    demo.entries.get(demo_index_for_z_position(z_position_fp16))
}

pub fn renderer_row_state(current_row: u16) -> RendererRowState {
    RendererRowState {
        current_row,
        road_row_group: usize::from(current_row >> 3),
        trekdat_slot: usize::from(current_row & 0x0007),
    }
}

pub fn plan_renderer_cell(
    exe: &SkyroadsExe,
    current_row: u16,
    descriptor_raw: u16,
) -> RendererCellPlan {
    let row_state = renderer_row_state(current_row);
    let descriptor = analyze_road_descriptor(descriptor_raw);
    let tile_class =
        exe.runtime_tables.tile_class_by_low3.values[usize::from(descriptor.dispatch_variant_low3)];
    let dispatch =
        exe.runtime_tables.draw_dispatch_by_type.entries[usize::from(descriptor.dispatch_kind)];

    RendererCellPlan {
        current_row,
        road_row_group: row_state.road_row_group,
        trekdat_slot: row_state.trekdat_slot,
        descriptor,
        tile_class,
        dispatch,
    }
}

pub fn plan_renderer_row(
    exe: &SkyroadsExe,
    current_row: u16,
    road_row: &[u16; ROAD_COLUMNS],
) -> RendererRowPlan {
    let row_state = renderer_row_state(current_row);
    let cells = std::array::from_fn(|column_index| {
        plan_renderer_cell(exe, current_row, road_row[column_index])
    });
    RendererRowPlan {
        current_row,
        road_row_group: row_state.road_row_group,
        trekdat_slot: row_state.trekdat_slot,
        cells,
    }
}

pub fn plan_gameplay_frame<'a>(
    exe: &SkyroadsExe,
    demo: &'a DemoRecording,
    road: &'a RoadEntry,
    road_row_index: usize,
    current_row: u16,
    z_position_fp16: u32,
) -> Option<GameplayFramePlan<'a>> {
    let road_row = road.rows.get(road_row_index)?;
    let demo_cursor = demo_cursor(z_position_fp16);
    Some(GameplayFramePlan {
        z_position_fp16,
        demo_cursor,
        demo_input: demo.entries.get(demo_cursor.index),
        road_index: road.index,
        road_row_index,
        renderer_row: plan_renderer_row(exe, current_row, road_row),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use skyroads_data::{load_demo_rec_path, load_roads_lzs_path, load_skyroads_exe_path};

    use super::{
        demo_cursor, demo_index_for_z_position, plan_gameplay_frame, plan_renderer_cell,
        plan_renderer_row, renderer_row_state, sample_demo_input,
    };

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn demo_indexing_matches_fixed_point_step_boundaries() {
        assert_eq!(demo_index_for_z_position(0), 0);
        assert_eq!(demo_index_for_z_position(0x0665), 0);
        assert_eq!(demo_index_for_z_position(0x0666), 1);
        assert_eq!(demo_index_for_z_position(0x0CCC), 2);

        let cursor = demo_cursor(0x1332);
        assert_eq!(cursor.index, 3);
    }

    #[test]
    fn shipped_demo_sampling_matches_known_bytes() {
        let demo = load_demo_rec_path(repo_root().join("DEMO.REC")).unwrap();

        let entry0 = sample_demo_input(&demo, 0).unwrap();
        assert_eq!(entry0.index, 0);
        assert_eq!(entry0.byte, 0);
        assert_eq!(entry0.accelerate_decelerate, -1);
        assert_eq!(entry0.left_right, -1);
        assert!(!entry0.jump);

        let entry1 = sample_demo_input(&demo, 0x0666).unwrap();
        assert_eq!(entry1.index, 1);
        assert_eq!(entry1.byte, 5);
        assert_eq!(entry1.accelerate_decelerate, 0);
        assert_eq!(entry1.left_right, 0);
        assert!(!entry1.jump);

        assert!(sample_demo_input(&demo, demo.approx_tile_length_fp16() + 0x0666).is_none());
    }

    #[test]
    fn renderer_row_state_matches_verified_bit_ops() {
        let state = renderer_row_state(0x0015);
        assert_eq!(state.road_row_group, 2);
        assert_eq!(state.trekdat_slot, 5);
    }

    #[test]
    fn renderer_plan_uses_exe_runtime_tables() {
        let exe = load_skyroads_exe_path(repo_root().join("SKYROADS.EXE")).unwrap();

        let plan_type_2 = plan_renderer_cell(&exe, 0x0015, 0x0200);
        assert_eq!(plan_type_2.road_row_group, 2);
        assert_eq!(plan_type_2.trekdat_slot, 5);
        assert_eq!(plan_type_2.tile_class, 3);
        assert_eq!(plan_type_2.dispatch.target, 0x2E9F);
        assert_eq!(plan_type_2.dispatch.target_label, Some("draw_type_2"));

        let plan_type_5 = plan_renderer_cell(&exe, 0x0007, 0x0507);
        assert_eq!(plan_type_5.road_row_group, 0);
        assert_eq!(plan_type_5.trekdat_slot, 7);
        assert_eq!(plan_type_5.tile_class, 4);
        assert_eq!(plan_type_5.dispatch.target, 0x2FB0);
        assert_eq!(plan_type_5.dispatch.target_label, Some("draw_type_5"));
    }

    #[test]
    fn renderer_row_plan_matches_shipped_mixed_dispatch_row() {
        let exe = load_skyroads_exe_path(repo_root().join("SKYROADS.EXE")).unwrap();
        let roads = load_roads_lzs_path(repo_root().join("ROADS.LZS")).unwrap();

        let road_row = &roads.roads[0].rows[83];
        let plan = plan_renderer_row(&exe, 0x0015, road_row);
        assert_eq!(plan.road_row_group, 2);
        assert_eq!(plan.trekdat_slot, 5);
        assert_eq!(plan.cells[0].descriptor.raw, 0x0400);
        assert_eq!(plan.cells[0].dispatch.target, 0x2F3C);
        assert_eq!(plan.cells[1].descriptor.raw, 0x0260);
        assert_eq!(plan.cells[1].dispatch.target, 0x2E9F);
        assert_eq!(plan.cells[2].descriptor.raw, 0x0000);
        assert_eq!(plan.cells[2].dispatch.target, 0x2E50);
        assert_eq!(plan.cells[6].descriptor.raw, 0x0400);
        assert_eq!(plan.cells[6].dispatch.target, 0x2F3C);
    }

    #[test]
    fn gameplay_frame_plan_combines_demo_and_row_dispatch() {
        let exe = load_skyroads_exe_path(repo_root().join("SKYROADS.EXE")).unwrap();
        let roads = load_roads_lzs_path(repo_root().join("ROADS.LZS")).unwrap();
        let demo = load_demo_rec_path(repo_root().join("DEMO.REC")).unwrap();

        let frame = plan_gameplay_frame(&exe, &demo, &roads.roads[2], 80, 0x0015, 0x0666).unwrap();
        assert_eq!(frame.demo_cursor.index, 1);
        assert_eq!(frame.demo_input.unwrap().byte, 5);
        assert_eq!(frame.road_index, 2);
        assert_eq!(frame.road_row_index, 80);
        assert_eq!(frame.renderer_row.cells[0].descriptor.raw, 0x0507);
        assert_eq!(frame.renderer_row.cells[0].tile_class, 4);
        assert_eq!(frame.renderer_row.cells[0].dispatch.target, 0x2FB0);
        assert_eq!(frame.renderer_row.cells[3].descriptor.raw, 0x000D);
        assert_eq!(frame.renderer_row.cells[3].dispatch.target, 0x2E50);
    }
}
