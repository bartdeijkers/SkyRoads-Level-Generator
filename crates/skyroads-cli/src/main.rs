use std::env;
use std::path::{Path, PathBuf};
use std::process;

use skyroads_core::{GameplaySession, ShipState};
use skyroads_data::{
    level_from_road_entry, load_demo_rec_path, load_muzax_lzs_path, load_roads_lzs_path,
    load_skyroads_exe_path, load_trekdat_lzs_path, Result,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = env::args();
    let program = args.next().unwrap_or_else(|| "skyroads-cli".to_string());
    let command = args.next();
    let source_root = args.next();
    let extra = args.collect::<Vec<_>>();

    match (command.as_deref(), source_root) {
        (Some("summary"), Some(source_root)) => summary(Path::new(&source_root)),
        (Some("demo-sim"), Some(source_root)) => demo_sim(Path::new(&source_root), &extra),
        _ => {
            eprintln!("usage: {program} <summary|demo-sim> <source_root> [args]");
            process::exit(2);
        }
    }
}

fn summary(source_root: &Path) -> Result<()> {
    let roads = load_roads_lzs_path(source_root.join("ROADS.LZS"))?;
    let demo = load_demo_rec_path(source_root.join("DEMO.REC"))?;
    let trekdat = load_trekdat_lzs_path(source_root.join("TREKDAT.LZS"))?;
    let muzax = load_muzax_lzs_path(source_root.join("MUZAX.LZS"))?;
    let exe = load_skyroads_exe_path(source_root.join("SKYROADS.EXE"))?;

    println!("SkyRoads Native Baseline");
    println!("source_root: {}", normalize_path(source_root));
    println!();

    println!("roads:");
    println!("  road_count: {}", roads.road_count());
    println!(
        "  used_dispatch_kinds: {}",
        join_u8s(roads.used_dispatch_kinds())
    );
    println!(
        "  distinct_descriptor_count: {}",
        roads.distinct_descriptor_count()
    );
    for entry in &roads.descriptor_catalog.dispatch_kinds {
        println!(
            "  dispatch_kind_{}: count={} descriptors={}",
            entry.dispatch_kind, entry.count, entry.descriptor_count
        );
    }
    println!();

    println!("demo:");
    println!("  byte_count: {}", demo.byte_count());
    println!(
        "  approx_tile_length_fp16: 0x{:08X}",
        demo.approx_tile_length_fp16()
    );
    println!("  approx_tile_length: {:.9}", demo.approx_tile_length());
    println!(
        "  accelerate_decelerate_counts: {}",
        join_i8_counts(&demo.accelerate_decelerate_counts)
    );
    println!(
        "  left_right_counts: {}",
        join_i8_counts(&demo.left_right_counts)
    );
    println!(
        "  jump_counts: false={} true={}",
        demo.jump_counts.false_count, demo.jump_counts.true_count
    );
    println!();

    println!("trekdat:");
    println!("  record_count: {}", trekdat.record_count());
    println!("  pointer_grid: 13x24");
    let expanded_sizes = trekdat
        .records
        .iter()
        .map(|record| record.load_buff_end.to_string())
        .collect::<Vec<_>>()
        .join(",");
    println!("  expanded_sizes: {}", expanded_sizes);
    let span_counts = trekdat
        .records
        .iter()
        .map(|record| record.total_span_count().to_string())
        .collect::<Vec<_>>()
        .join(",");
    println!("  total_span_counts: {}", span_counts);
    let pointer_maxes = trekdat
        .records
        .iter()
        .map(|record| record.pointer_max().to_string())
        .collect::<Vec<_>>()
        .join(",");
    println!("  pointer_maxes: {}", pointer_maxes);
    println!();

    println!("muzax:");
    println!("  song_table_size: {}", muzax.song_table_size);
    println!("  song_count: {}", muzax.song_count());
    println!("  populated_song_count: {}", muzax.populated_song_count());
    if let Some(song0) = muzax.songs.first() {
        if let Some(widths) = song0.widths {
            println!("  song_0_widths: {},{},{}", widths[0], widths[1], widths[2]);
        }
        println!("  song_0_instrument_bytes: {}", song0.instrument_bytes);
        println!("  song_0_command_bytes: {}", song0.command_bytes);
        if let Some(summary) = &song0.command_summary {
            println!(
                "  song_0_function_counts: {}",
                summary
                    .function_counts
                    .iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
    }
    println!();

    println!("exe:");
    println!("  header_bytes: {}", exe.header_bytes);
    println!("  image_size: {}", exe.image_size);
    println!("  entry_file_offset: {}", exe.entry_file_offset);
    println!(
        "  exe_reader_base_file_offset: {}",
        exe.exe_reader_base_file_offset
    );
    println!(
        "  tile_class_by_low3: {}",
        exe.runtime_tables
            .tile_class_by_low3
            .values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    println!(
        "  draw_dispatch_by_type: {}",
        exe.runtime_tables
            .draw_dispatch_by_type
            .entries
            .iter()
            .map(|entry| format!("{:04X}", entry.target))
            .collect::<Vec<_>>()
            .join(",")
    );

    Ok(())
}

fn demo_sim(source_root: &Path, extra: &[String]) -> Result<()> {
    let frame_count = extra
        .first()
        .map(|value| {
            value.parse::<usize>().unwrap_or_else(|_| {
                eprintln!("invalid frame count: {value}");
                process::exit(2);
            })
        })
        .unwrap_or(60);

    let roads = load_roads_lzs_path(source_root.join("ROADS.LZS"))?;
    let demo = load_demo_rec_path(source_root.join("DEMO.REC"))?;
    let level = level_from_road_entry(&roads.roads[0]);
    let mut session = GameplaySession::new(level.clone());

    println!("SkyRoads Demo Simulation");
    println!("source_root: {}", normalize_path(source_root));
    println!("level: {} (index {})", level.name, level.road_index);
    println!(
        "gravity: {} fuel: {} oxygen: {}",
        level.gravity, level.fuel, level.oxygen
    );
    println!("frames: {}", frame_count);
    println!();

    for _ in 0..frame_count {
        let frame = session.run_demo_frame(&demo);
        println!(
            "frame={:04} turn={:+} accel={:+} jump={} row={} pos=({:.6},{:.6},{:.6}) zvel={:.6} oxygen={:.6} fuel={:.6} state={} events={}",
            frame.frame_index,
            frame.controls.turn_input,
            frame.controls.accel_input,
            if frame.controls.jump_input { 1 } else { 0 },
            frame.road_row_index,
            frame.snapshot.x_position,
            frame.snapshot.y_position,
            frame.snapshot.z_position,
            frame.snapshot.z_velocity,
            frame.snapshot.oxygen_percent,
            frame.snapshot.fuel_percent,
            ship_state_name(frame.snapshot.craft_state),
            join_events(&frame.events),
        );
    }

    Ok(())
}

fn normalize_path(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(path)
            }
        })
        .display()
        .to_string()
}

fn join_u8s(values: &[u8]) -> String {
    values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn join_i8_counts(counts: &std::collections::BTreeMap<i8, usize>) -> String {
    counts
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn ship_state_name(state: ShipState) -> &'static str {
    match state {
        ShipState::Alive => "Alive",
        ShipState::Exploded => "Exploded",
        ShipState::Fallen => "Fallen",
        ShipState::OutOfFuel => "OutOfFuel",
        ShipState::OutOfOxygen => "OutOfOxygen",
    }
}

fn join_events(events: &[skyroads_core::GameplayEvent]) -> String {
    if events.is_empty() {
        return "-".to_string();
    }

    events
        .iter()
        .map(|event| match event {
            skyroads_core::GameplayEvent::ShipBumpedWall => "ShipBumpedWall",
            skyroads_core::GameplayEvent::ShipExploded => "ShipExploded",
            skyroads_core::GameplayEvent::ShipBounced => "ShipBounced",
            skyroads_core::GameplayEvent::ShipRefilled => "ShipRefilled",
        })
        .collect::<Vec<_>>()
        .join(",")
}
