mod cfg;
mod compression;
mod dashboard;
mod demo;
mod error;
mod exe;
mod image;
mod level;
mod muzax;
mod roads;
mod sound;
mod trekdat;

pub use cfg::{load_cfg_bytes, load_cfg_path, SkyroadsCfg};
pub use compression::decompress_stream;
pub use dashboard::{
    load_dashboard_dat_bytes, load_dashboard_dat_path, HudFragment, HudFragmentPack,
    DASHBOARD_COLORS,
};
pub use demo::{
    load_demo_rec_bytes, load_demo_rec_path, DemoInput, DemoRecording, JumpCounts,
    DEMO_TILE_POSITION_STEP_FP16,
};
pub use error::{Error, Result};
pub use exe::{
    load_skyroads_exe_bytes, load_skyroads_exe_path, ExeDispatchEntry, ExeRelocation,
    ExeRuntimeDispatchTable, ExeRuntimeTables, ExeRuntimeU8Table, ExeShipRuntimeTables,
    SkyroadsExe, DOS_SHIP_LANE_COUNT, DOS_SHIP_SHADOW_MASK_BYTES, DOS_SHIP_SHADOW_MASK_COUNT,
    DOS_SHIP_SHADOW_MASK_HEIGHT, DOS_SHIP_SHADOW_MASK_WIDTH, DOS_SHIP_SURFACE_HEIGHT_COUNT,
    DOS_SHIP_THRUST_PHASE_COUNT, EXE_READER_SEGMENT_BASE,
};
pub use image::{
    load_image_archive_bytes, load_image_archive_path, ImageArchive, ImageArchiveKind, ImageFrame,
    ImagePalette, RgbColor, SCREEN_HEIGHT, SCREEN_WIDTH,
};
pub use level::{
    level_from_road_entry, levels_from_roads_archive, Level, LevelCell, TouchEffect, GROUND_Y,
    LEVEL_CENTER_X, LEVEL_MAX_X, LEVEL_MIN_X, LEVEL_TILE_STRIDE_X,
};
pub use muzax::{
    load_muzax_lzs_bytes, load_muzax_lzs_path, MuzaxArchive, MuzaxCommandHead, MuzaxCommandSummary,
    MuzaxInstrument, MuzaxOscillator, MuzaxSong, MuzaxSongHeader,
};
pub use roads::{
    analyze_road_descriptor, load_roads_lzs_bytes, load_roads_lzs_path, DispatchKindEntry,
    DispatchSample, RoadDescriptor, RoadDescriptorCatalog, RoadDescriptorEntry, RoadEntry,
    RoadSample, RoadsArchive, ROAD_COLUMNS,
};
pub use sound::{
    load_intro_snd_bytes, load_intro_snd_path, load_sfx_snd_bytes, load_sfx_snd_path, Pcm8Sample,
    SfxBank, SfxEntry, SAMPLE_RATE_PCM_8K,
};
pub use trekdat::{
    load_trekdat_lzs_bytes, load_trekdat_lzs_path, TrekdatArchive, TrekdatBbox,
    TrekdatCellPointers, TrekdatDosPointerLayout, TrekdatPointerRow, TrekdatRecord, TrekdatShape,
    TrekdatSpan, TREKDAT_DOS_DRAW_COLUMNS, TREKDAT_DOS_DRAW_ROWS, TREKDAT_DOS_POINTERS_PER_CELL,
    TREKDAT_POINTER_COLUMNS, TREKDAT_POINTER_COUNT, TREKDAT_POINTER_ROWS,
    TREKDAT_POINTER_TABLE_BYTES, TREKDAT_SHAPE_BASE, TREKDAT_SHAPE_ROWS, TREKDAT_VIEWPORT_HEIGHT,
    TREKDAT_VIEWPORT_WIDTH,
};
