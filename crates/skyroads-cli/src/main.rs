use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;

use skyroads_core::{AppInput, AttractModeApp, GameplaySession, RenderScene, ShipState};
use skyroads_data::{
    level_from_road_entry, load_demo_rec_path, load_muzax_lzs_path, load_roads_lzs_path,
    load_skyroads_exe_path, load_trekdat_lzs_path, DemoRecording, Error, Level, Result,
};
use skyroads_renderer_ref::{frame_hash, AttractModeAssets, FrameBuffer320x200, ReferenceRenderer};

const CAPTURE_MANIFEST_FILE_NAME: &str = "manifest.tsv";
const INTRO_SKIP_TICKS: usize = 35;
const MENU_IDLE_DEMO_TICKS: usize = 70 * 5;

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
    let path_arg = args.next();
    let extra = args.collect::<Vec<_>>();

    match (command.as_deref(), path_arg) {
        (Some("summary"), Some(source_root)) => summary(Path::new(&source_root)),
        (Some("demo-sim"), Some(source_root)) => demo_sim(Path::new(&source_root), &extra),
        (Some("render-capture"), Some(source_root)) => {
            let options = parse_render_capture_options(&extra).unwrap_or_else(|message| {
                eprintln!("{message}");
                print_usage(&program);
                process::exit(2);
            });
            render_capture(Path::new(&source_root), &options)
        }
        (Some("render-demo"), Some(source_root)) => {
            let options = parse_render_demo_options(&extra).unwrap_or_else(|message| {
                eprintln!("{message}");
                print_usage(&program);
                process::exit(2);
            });
            render_demo(Path::new(&source_root), &options)
        }
        (Some("render-compare"), Some(left_capture_root)) => {
            let Some(right_capture_root) = extra.first() else {
                print_usage(&program);
                process::exit(2);
            };
            render_compare(Path::new(&left_capture_root), Path::new(right_capture_root))
        }
        _ => {
            print_usage(&program);
            process::exit(2);
        }
    }
}

fn print_usage(program: &str) {
    eprintln!(
        "usage: {program} <summary|demo-sim|render-capture|render-demo|render-compare> <path> [args]"
    );
    eprintln!("  {program} summary <source_root>");
    eprintln!("  {program} demo-sim <source_root> [frame_count]");
    eprintln!(
        "  {program} render-capture <source_root> <output_root> [--x265-video <video_path>] [--video-fps <fps>]"
    );
    eprintln!(
        "  {program} render-demo <source_root> <output_root> [--x265-video <video_path>] [--video-fps <fps>]"
    );
    eprintln!("  {program} render-compare <left_capture_root> <right_capture_root>");
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderOutputOptions {
    output_root: PathBuf,
    x265_video: Option<X265VideoOptions>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct X265VideoOptions {
    output_path: PathBuf,
    fps: u32,
}

fn parse_render_output_options(
    command_name: &str,
    extra: &[String],
) -> std::result::Result<RenderOutputOptions, String> {
    let Some(output_root) = extra.first() else {
        return Err(format!("{command_name} requires <output_root>"));
    };

    let mut x265_video_path = None;
    let mut video_fps = None;
    let mut index = 1usize;

    while index < extra.len() {
        let flag = &extra[index];
        match flag.as_str() {
            "--x265-video" => {
                let Some(value) = extra.get(index + 1) else {
                    return Err("--x265-video requires <video_path>".to_string());
                };
                if x265_video_path.is_some() {
                    return Err("--x265-video can only be provided once".to_string());
                }
                x265_video_path = Some(PathBuf::from(value));
                index += 2;
            }
            "--video-fps" => {
                let Some(value) = extra.get(index + 1) else {
                    return Err("--video-fps requires <fps>".to_string());
                };
                if video_fps.is_some() {
                    return Err("--video-fps can only be provided once".to_string());
                }

                let fps = value
                    .parse::<u32>()
                    .map_err(|_| format!("--video-fps must be a positive integer, got {value}"))?;
                if fps == 0 {
                    return Err("--video-fps must be greater than 0".to_string());
                }

                video_fps = Some(fps);
                index += 2;
            }
            _ => {
                return Err(format!("unknown {command_name} argument: {flag}"));
            }
        }
    }

    let x265_video = match x265_video_path {
        Some(output_path) => Some(X265VideoOptions {
            output_path,
            fps: video_fps.unwrap_or(30),
        }),
        None => {
            if video_fps.is_some() {
                return Err("--video-fps requires --x265-video".to_string());
            }
            None
        }
    };

    Ok(RenderOutputOptions {
        output_root: PathBuf::from(output_root),
        x265_video,
    })
}

fn parse_render_capture_options(
    extra: &[String],
) -> std::result::Result<RenderOutputOptions, String> {
    parse_render_output_options("render-capture", extra)
}

fn parse_render_demo_options(extra: &[String]) -> std::result::Result<RenderOutputOptions, String> {
    parse_render_output_options("render-demo", extra)
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

fn render_capture(source_root: &Path, options: &RenderOutputOptions) -> Result<()> {
    fs::create_dir_all(&options.output_root)?;

    let roads = load_roads_lzs_path(source_root.join("ROADS.LZS"))?;
    let demo = load_demo_rec_path(source_root.join("DEMO.REC"))?;
    let levels = roads
        .roads
        .iter()
        .map(level_from_road_entry)
        .collect::<Vec<_>>();
    let renderer = ReferenceRenderer::new(AttractModeAssets::load_from_root(source_root)?);

    let mut entries = Vec::new();
    for scenario in CaptureScenario::ALL {
        capture_scenario(
            &options.output_root,
            scenario,
            &levels,
            &demo,
            &renderer,
            &mut entries,
        )?;
    }

    write_capture_manifest(&options.output_root, &entries)?;

    if let Some(video_options) = &options.x265_video {
        encode_x265_video(&options.output_root, &entries, video_options)?;
    }

    println!("SkyRoads Render Capture");
    println!("source_root: {}", normalize_path(source_root));
    println!("output_root: {}", normalize_path(&options.output_root));
    println!("frames: {}", entries.len());
    println!(
        "manifest: {}",
        options
            .output_root
            .join(CAPTURE_MANIFEST_FILE_NAME)
            .display()
    );
    if let Some(video_options) = &options.x265_video {
        println!("x265_video: {}", normalize_path(&video_options.output_path));
        println!("x265_video_fps: {}", video_options.fps);
    }
    for scenario in CaptureScenario::ALL {
        let count = entries
            .iter()
            .filter(|entry| entry.scenario == scenario.name())
            .count();
        println!("  {}: {}", scenario.name(), count);
    }

    Ok(())
}

fn render_demo(source_root: &Path, options: &RenderOutputOptions) -> Result<()> {
    fs::create_dir_all(&options.output_root)?;

    let roads = load_roads_lzs_path(source_root.join("ROADS.LZS"))?;
    let demo = load_demo_rec_path(source_root.join("DEMO.REC"))?;
    let levels = roads
        .roads
        .iter()
        .map(level_from_road_entry)
        .collect::<Vec<_>>();
    let renderer = ReferenceRenderer::new(AttractModeAssets::load_from_root(source_root)?);

    let mut app = make_app(&levels, &demo);
    let initial_scene = enter_demo_playback(&mut app)?;

    let mut entries = Vec::new();
    capture_play_scene(
        &options.output_root,
        &renderer,
        "demo/frame_000000".to_string(),
        "demo",
        0,
        RenderScene::DemoPlayback(initial_scene.clone()),
        &initial_scene,
        &mut entries,
    )?;

    while let RenderScene::DemoPlayback(scene) = app.tick(AppInput::default()).render_scene {
        let capture_index = entries.len();
        let label = format!("demo/frame_{capture_index:06}");
        capture_play_scene(
            &options.output_root,
            &renderer,
            label,
            "demo",
            capture_index,
            RenderScene::DemoPlayback(scene.clone()),
            &scene,
            &mut entries,
        )?;
    }

    write_capture_manifest(&options.output_root, &entries)?;

    if let Some(video_options) = &options.x265_video {
        encode_x265_video(&options.output_root, &entries, video_options)?;
    }

    println!("SkyRoads Demo Render");
    println!("source_root: {}", normalize_path(source_root));
    println!("output_root: {}", normalize_path(&options.output_root));
    println!("frames: {}", entries.len());
    println!(
        "manifest: {}",
        options
            .output_root
            .join(CAPTURE_MANIFEST_FILE_NAME)
            .display()
    );
    if let Some(video_options) = &options.x265_video {
        println!("x265_video: {}", normalize_path(&video_options.output_path));
        println!("x265_video_fps: {}", video_options.fps);
    }

    Ok(())
}

fn render_compare(left_capture_root: &Path, right_capture_root: &Path) -> Result<()> {
    let left_entries = load_capture_manifest(left_capture_root)?;
    let right_entries = load_capture_manifest(right_capture_root)?;
    let comparison = compare_capture_entries(&left_entries, &right_entries);

    println!("SkyRoads Render Compare");
    println!("left: {}", normalize_path(left_capture_root));
    println!("right: {}", normalize_path(right_capture_root));
    println!("unchanged: {}", comparison.unchanged);
    println!("changed: {}", comparison.changed.len());
    println!("added: {}", comparison.added.len());
    println!("removed: {}", comparison.removed.len());

    if !comparison.changed.is_empty() {
        println!();
        println!("changed_frames:");
        for diff in &comparison.changed {
            println!(
                "  {} {} -> {}",
                diff.label,
                format_hash(diff.left_hash.unwrap_or(0)),
                format_hash(diff.right_hash.unwrap_or(0)),
            );
        }
    }

    if !comparison.added.is_empty() {
        println!();
        println!("added_frames:");
        for diff in &comparison.added {
            println!("  {}", diff.label);
        }
    }

    if !comparison.removed.is_empty() {
        println!();
        println!("removed_frames:");
        for diff in &comparison.removed {
            println!("  {}", diff.label);
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureScenario {
    Start,
    Neutral,
    Throttle,
    Left,
    Right,
    Jump,
}

impl CaptureScenario {
    const ALL: [Self; 6] = [
        Self::Start,
        Self::Neutral,
        Self::Throttle,
        Self::Left,
        Self::Right,
        Self::Jump,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Neutral => "neutral",
            Self::Throttle => "throttle",
            Self::Left => "left",
            Self::Right => "right",
            Self::Jump => "jump",
        }
    }

    fn frame_count(self) -> usize {
        match self {
            Self::Start => 1,
            Self::Neutral => 8,
            Self::Throttle => 24,
            Self::Left | Self::Right => 24,
            Self::Jump => 24,
        }
    }

    fn gameplay_input(self, capture_index: usize) -> AppInput {
        match self {
            Self::Start | Self::Neutral => AppInput::default(),
            Self::Throttle => AppInput {
                up_held: true,
                ..AppInput::default()
            },
            Self::Left => AppInput {
                up_held: true,
                left_held: true,
                ..AppInput::default()
            },
            Self::Right => AppInput {
                up_held: true,
                right_held: true,
                ..AppInput::default()
            },
            Self::Jump => AppInput {
                up_held: true,
                space_held: (8..=9).contains(&capture_index),
                ..AppInput::default()
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CaptureEntry {
    label: String,
    scenario: String,
    capture_index: usize,
    gameplay_frame_index: usize,
    road_row: usize,
    ship_x: f64,
    ship_y: f64,
    ship_z: f64,
    ship_state: ShipState,
    frame_hash: u64,
    ppm_path: String,
}

impl CaptureEntry {
    fn manifest_header() -> &'static str {
        "label\tscenario\tcapture_index\tgameplay_frame_index\troad_row\tship_x\tship_y\tship_z\tship_state\tframe_hash\tppm_path"
    }

    fn to_manifest_row(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{:.6}\t{:.6}\t{:.6}\t{}\t{}\t{}",
            self.label,
            self.scenario,
            self.capture_index,
            self.gameplay_frame_index,
            self.road_row,
            self.ship_x,
            self.ship_y,
            self.ship_z,
            ship_state_name(self.ship_state),
            format_hash(self.frame_hash),
            self.ppm_path,
        )
    }

    fn from_manifest_row(line: &str) -> Result<Self> {
        let columns = line.split('\t').collect::<Vec<_>>();
        if columns.len() != 11 {
            return Err(Error::invalid_format(format!(
                "capture manifest row should have 11 columns, got {}",
                columns.len()
            )));
        }

        Ok(Self {
            label: columns[0].to_string(),
            scenario: columns[1].to_string(),
            capture_index: parse_usize(columns[2], "capture_index")?,
            gameplay_frame_index: parse_usize(columns[3], "gameplay_frame_index")?,
            road_row: parse_usize(columns[4], "road_row")?,
            ship_x: parse_f64(columns[5], "ship_x")?,
            ship_y: parse_f64(columns[6], "ship_y")?,
            ship_z: parse_f64(columns[7], "ship_z")?,
            ship_state: parse_ship_state(columns[8])?,
            frame_hash: parse_hash(columns[9])?,
            ppm_path: columns[10].to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaptureDiff {
    label: String,
    left_hash: Option<u64>,
    right_hash: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaptureComparison {
    unchanged: usize,
    changed: Vec<CaptureDiff>,
    added: Vec<CaptureDiff>,
    removed: Vec<CaptureDiff>,
}

fn capture_scenario(
    output_root: &Path,
    scenario: CaptureScenario,
    levels: &[Level],
    demo: &DemoRecording,
    renderer: &ReferenceRenderer,
    entries: &mut Vec<CaptureEntry>,
) -> Result<()> {
    let scenario_root = output_root.join(scenario.name());
    fs::create_dir_all(&scenario_root)?;

    let mut app = make_app(levels, demo);
    let initial_scene = enter_gameplay(&mut app)?;

    if scenario == CaptureScenario::Start {
        let label = format!("{}/frame_{:03}", scenario.name(), 0);
        capture_play_scene(
            output_root,
            renderer,
            label,
            scenario.name(),
            0,
            RenderScene::Gameplay(initial_scene.clone()),
            &initial_scene,
            entries,
        )?;
        return Ok(());
    }

    for capture_index in 0..scenario.frame_count() {
        let tick = app.tick(scenario.gameplay_input(capture_index));
        let RenderScene::Gameplay(scene) = tick.render_scene else {
            return Err(Error::invalid_format(
                "capture scenario left gameplay unexpectedly",
            ));
        };
        let label = format!("{}/frame_{capture_index:03}", scenario.name());
        capture_play_scene(
            output_root,
            renderer,
            label,
            scenario.name(),
            capture_index,
            RenderScene::Gameplay(scene.clone()),
            &scene,
            entries,
        )?;
    }

    Ok(())
}

fn capture_play_scene(
    output_root: &Path,
    renderer: &ReferenceRenderer,
    label: String,
    scenario: &str,
    capture_index: usize,
    render_scene: RenderScene,
    scene: &skyroads_core::DemoPlaybackState,
    entries: &mut Vec<CaptureEntry>,
) -> Result<()> {
    let ppm_path = format!("{label}.ppm");
    let frame = renderer.render_scene(&render_scene);
    write_frame_ppm(&output_root.join(&ppm_path), &frame)?;

    entries.push(CaptureEntry {
        label,
        scenario: scenario.to_string(),
        capture_index,
        gameplay_frame_index: scene.frame_index,
        road_row: scene.current_row,
        ship_x: scene.ship.x_position,
        ship_y: scene.ship.y_position,
        ship_z: scene.ship.z_position,
        ship_state: scene.ship.state,
        frame_hash: frame_hash(&frame),
        ppm_path,
    });

    Ok(())
}

fn write_capture_manifest(output_root: &Path, entries: &[CaptureEntry]) -> Result<()> {
    let manifest = std::iter::once(CaptureEntry::manifest_header().to_string())
        .chain(entries.iter().map(CaptureEntry::to_manifest_row))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        output_root.join(CAPTURE_MANIFEST_FILE_NAME),
        manifest + "\n",
    )?;
    Ok(())
}

fn encode_x265_video(
    output_root: &Path,
    entries: &[CaptureEntry],
    video_options: &X265VideoOptions,
) -> Result<()> {
    if entries.is_empty() {
        return Err(Error::invalid_format(
            "cannot encode x265 video from an empty render capture",
        ));
    }

    if let Some(parent) = video_options.output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let fps = video_options.fps.to_string();
    let mut ffmpeg = process::Command::new("ffmpeg");
    ffmpeg
        .arg("-y")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-f")
        .arg("image2pipe")
        .arg("-vcodec")
        .arg("ppm")
        .arg("-framerate")
        .arg(&fps)
        .arg("-i")
        .arg("-")
        .arg("-an")
        .arg("-c:v")
        .arg("libx265")
        .arg("-preset")
        .arg("medium")
        .arg("-crf")
        .arg("18")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg(&video_options.output_path)
        .stdin(process::Stdio::piped())
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::piped());

    let mut child = ffmpeg
        .spawn()
        .map_err(|error| Error::invalid_format(format!("failed to start ffmpeg: {error}")))?;

    let Some(mut stdin) = child.stdin.take() else {
        return Err(Error::invalid_format(
            "failed to open ffmpeg stdin for x265 video encoding",
        ));
    };

    for entry in entries {
        let frame_path = output_root.join(&entry.ppm_path);
        let frame = fs::read(&frame_path)?;
        stdin.write_all(&frame).map_err(|error| {
            Error::invalid_format(format!(
                "failed to stream frame {} into ffmpeg: {error}",
                frame_path.display()
            ))
        })?;
    }
    drop(stdin);

    let output = child.wait_with_output().map_err(|error| {
        Error::invalid_format(format!(
            "failed while waiting for ffmpeg to finish: {error}"
        ))
    })?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let status_message = match output.status.code() {
        Some(code) => format!("ffmpeg exited with status code {code}"),
        None => "ffmpeg exited without a status code".to_string(),
    };
    let error_message = if stderr.is_empty() {
        status_message
    } else {
        format!("{status_message}: {stderr}")
    };

    Err(Error::invalid_format(format!(
        "failed to encode x265 video: {error_message}"
    )))
}

fn load_capture_manifest(root: &Path) -> Result<Vec<CaptureEntry>> {
    let manifest_path = root.join(CAPTURE_MANIFEST_FILE_NAME);
    let manifest = fs::read_to_string(&manifest_path)?;
    let mut entries = Vec::new();

    for (line_index, line) in manifest.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if line_index == 0 {
            if line != CaptureEntry::manifest_header() {
                return Err(Error::invalid_format(format!(
                    "unexpected capture manifest header in {}",
                    manifest_path.display()
                )));
            }
            continue;
        }
        entries.push(CaptureEntry::from_manifest_row(line)?);
    }

    Ok(entries)
}

fn compare_capture_entries(left: &[CaptureEntry], right: &[CaptureEntry]) -> CaptureComparison {
    let left_by_label = left
        .iter()
        .map(|entry| (entry.label.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let right_by_label = right
        .iter()
        .map(|entry| (entry.label.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let labels = left_by_label
        .keys()
        .chain(right_by_label.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut unchanged = 0usize;
    let mut changed = Vec::new();
    let mut added = Vec::new();
    let mut removed = Vec::new();

    for label in labels {
        let left_entry = left_by_label.get(&label).copied();
        let right_entry = right_by_label.get(&label).copied();

        match (left_entry, right_entry) {
            (Some(left_entry), Some(right_entry))
                if left_entry.frame_hash == right_entry.frame_hash =>
            {
                unchanged += 1;
            }
            (Some(left_entry), Some(right_entry)) => changed.push(CaptureDiff {
                label,
                left_hash: Some(left_entry.frame_hash),
                right_hash: Some(right_entry.frame_hash),
            }),
            (None, Some(right_entry)) => added.push(CaptureDiff {
                label,
                left_hash: None,
                right_hash: Some(right_entry.frame_hash),
            }),
            (Some(left_entry), None) => removed.push(CaptureDiff {
                label,
                left_hash: Some(left_entry.frame_hash),
                right_hash: None,
            }),
            (None, None) => {}
        }
    }

    CaptureComparison {
        unchanged,
        changed,
        added,
        removed,
    }
}

fn make_app(levels: &[Level], demo: &DemoRecording) -> AttractModeApp {
    AttractModeApp::new(levels.to_vec(), demo.clone())
}

fn skip_intro_to_menu(app: &mut AttractModeApp) {
    for _ in 0..INTRO_SKIP_TICKS {
        app.tick(AppInput::default());
    }
    app.tick(AppInput {
        space: true,
        ..AppInput::default()
    });
}

fn enter_demo_playback(app: &mut AttractModeApp) -> Result<skyroads_core::DemoPlaybackState> {
    skip_intro_to_menu(app);
    for _ in 0..MENU_IDLE_DEMO_TICKS {
        app.tick(AppInput::default());
    }

    let tick = app.tick(AppInput::default());
    let RenderScene::DemoPlayback(scene) = tick.render_scene else {
        return Err(Error::invalid_format(
            "expected demo playback render scene after idling in the main menu",
        ));
    };
    Ok(scene)
}

fn enter_gameplay(app: &mut AttractModeApp) -> Result<skyroads_core::DemoPlaybackState> {
    skip_intro_to_menu(app);
    let tick = app.tick(AppInput {
        enter: true,
        ..AppInput::default()
    });
    let RenderScene::Gameplay(scene) = tick.render_scene else {
        return Err(Error::invalid_format(
            "expected gameplay render scene after entering gameplay",
        ));
    };
    Ok(scene)
}

fn write_frame_ppm(path: &Path, frame: &FrameBuffer320x200) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut output =
        Vec::with_capacity(32 + usize::from(frame.width) * usize::from(frame.height) * 3);
    output.extend_from_slice(format!("P6\n{} {}\n255\n", frame.width, frame.height).as_bytes());
    for pixel in frame.pixels_rgba.chunks_exact(4) {
        output.extend_from_slice(&pixel[..3]);
    }
    fs::write(path, output)?;
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

fn join_i8_counts(counts: &BTreeMap<i8, usize>) -> String {
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

fn parse_ship_state(value: &str) -> Result<ShipState> {
    match value {
        "Alive" => Ok(ShipState::Alive),
        "Exploded" => Ok(ShipState::Exploded),
        "Fallen" => Ok(ShipState::Fallen),
        "OutOfFuel" => Ok(ShipState::OutOfFuel),
        "OutOfOxygen" => Ok(ShipState::OutOfOxygen),
        _ => Err(Error::invalid_format(format!(
            "unknown ship state in capture manifest: {value}"
        ))),
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

fn format_hash(value: u64) -> String {
    format!("0x{value:016X}")
}

fn parse_usize(value: &str, field_name: &str) -> Result<usize> {
    value.parse::<usize>().map_err(|_| {
        Error::invalid_format(format!(
            "capture manifest field {field_name} is not a valid usize: {value}"
        ))
    })
}

fn parse_f64(value: &str, field_name: &str) -> Result<f64> {
    value.parse::<f64>().map_err(|_| {
        Error::invalid_format(format!(
            "capture manifest field {field_name} is not a valid f64: {value}"
        ))
    })
}

fn parse_hash(value: &str) -> Result<u64> {
    let Some(value) = value.strip_prefix("0x") else {
        return Err(Error::invalid_format(format!(
            "capture manifest hash must start with 0x: {value}"
        )));
    };
    u64::from_str_radix(value, 16).map_err(|_| {
        Error::invalid_format(format!(
            "capture manifest hash is not valid hexadecimal: 0x{value}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        compare_capture_entries, format_hash, parse_render_capture_options,
        parse_render_demo_options, CaptureEntry, CaptureScenario, RenderOutputOptions, ShipState,
        X265VideoOptions,
    };
    use std::path::PathBuf;

    fn sample_entry(label: &str, hash: u64) -> CaptureEntry {
        CaptureEntry {
            label: label.to_string(),
            scenario: CaptureScenario::Throttle.name().to_string(),
            capture_index: 3,
            gameplay_frame_index: 12,
            road_row: 27,
            ship_x: 95.0,
            ship_y: 80.0,
            ship_z: 4.25,
            ship_state: ShipState::Alive,
            frame_hash: hash,
            ppm_path: format!("{label}.ppm"),
        }
    }

    #[test]
    fn capture_manifest_row_round_trips() {
        let entry = sample_entry("throttle/frame_003", 0x1234ABCD);
        let parsed = CaptureEntry::from_manifest_row(&entry.to_manifest_row()).unwrap();
        assert_eq!(parsed, entry);
    }

    #[test]
    fn compare_capture_entries_reports_hash_and_membership_diffs() {
        let left = vec![
            sample_entry("start/frame_000", 0x01),
            sample_entry("throttle/frame_003", 0x02),
        ];
        let right = vec![
            sample_entry("start/frame_000", 0x01),
            sample_entry("throttle/frame_003", 0x09),
            sample_entry("jump/frame_004", 0x0A),
        ];

        let comparison = compare_capture_entries(&left, &right);

        assert_eq!(comparison.unchanged, 1);
        assert_eq!(comparison.changed.len(), 1);
        assert_eq!(comparison.changed[0].label, "throttle/frame_003");
        assert_eq!(comparison.changed[0].left_hash, Some(0x02));
        assert_eq!(comparison.changed[0].right_hash, Some(0x09));
        assert_eq!(comparison.added.len(), 1);
        assert_eq!(comparison.added[0].label, "jump/frame_004");
        assert_eq!(comparison.removed.len(), 0);
        assert_eq!(format_hash(0xABCD), "0x000000000000ABCD");
    }

    #[test]
    fn render_capture_options_parse_x265_video_and_fps() {
        let args = vec![
            "/tmp/capture".to_string(),
            "--x265-video".to_string(),
            "/tmp/capture.mp4".to_string(),
            "--video-fps".to_string(),
            "60".to_string(),
        ];

        let parsed = parse_render_capture_options(&args).unwrap();

        assert_eq!(
            parsed,
            RenderOutputOptions {
                output_root: PathBuf::from("/tmp/capture"),
                x265_video: Some(X265VideoOptions {
                    output_path: PathBuf::from("/tmp/capture.mp4"),
                    fps: 60,
                }),
            }
        );
    }

    #[test]
    fn render_capture_options_reject_video_fps_without_video_output() {
        let args = vec![
            "/tmp/capture".to_string(),
            "--video-fps".to_string(),
            "60".to_string(),
        ];

        let error = parse_render_capture_options(&args).unwrap_err();

        assert_eq!(error, "--video-fps requires --x265-video");
    }

    #[test]
    fn render_demo_options_parse_x265_video_and_fps() {
        let args = vec![
            "/tmp/demo".to_string(),
            "--x265-video".to_string(),
            "/tmp/demo.mp4".to_string(),
            "--video-fps".to_string(),
            "24".to_string(),
        ];

        let parsed = parse_render_demo_options(&args).unwrap();

        assert_eq!(
            parsed,
            RenderOutputOptions {
                output_root: PathBuf::from("/tmp/demo"),
                x265_video: Some(X265VideoOptions {
                    output_path: PathBuf::from("/tmp/demo.mp4"),
                    fps: 24,
                }),
            }
        );
    }
}
