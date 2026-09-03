use std::fmt;
use std::str::FromStr;

use skyroads_data::{Level, LevelCell, LevelKind, LevelTheme, ROAD_COLUMNS};

const GENERATOR_VERSION: u8 = 4;
const FIRST_GENERATOR_VERSION: u8 = 1;
const SEED_MASK: u64 = (1_u64 << 40) - 1;
const BASE32: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const START_ROWS: usize = 12;
const FINISH_ROWS: usize = 16;
const ROADLESS_APPROACH_ROWS: usize = 10;

const ACCELERATE: u16 = 0x000A;
const SLIDE: u16 = 0x0008;
const REFILL: u16 = 0x0009;
const HAZARD: u16 = 0x000C;
const TUNNEL: u16 = 0x0103;

const ROAD_COLORS: [u8; 9] = [3, 4, 5, 6, 7, 11, 13, 14, 15];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProceduralDifficulty {
    Easy,
    Classic,
    Hard,
}

impl ProceduralDifficulty {
    pub const ALL: [Self; 3] = [Self::Easy, Self::Classic, Self::Hard];

    pub fn code(self) -> char {
        match self {
            Self::Easy => 'E',
            Self::Classic => 'C',
            Self::Hard => 'H',
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Easy => "EASY",
            Self::Classic => "CLASSIC",
            Self::Hard => "HARD",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Easy => Self::Classic,
            Self::Classic => Self::Hard,
            Self::Hard => Self::Easy,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Easy => Self::Hard,
            Self::Classic => Self::Easy,
            Self::Hard => Self::Classic,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            b'E' => Some(Self::Easy),
            b'C' => Some(Self::Classic),
            b'H' => Some(Self::Hard),
            _ => None,
        }
    }

    fn profile(self) -> DifficultyProfile {
        match self {
            Self::Easy => DifficultyProfile {
                length: 96,
                gravity: 7,
                fuel: 225,
                oxygen: 180,
                safe_half_width: 1,
                jump_gap_rows: 1,
                intensity_bias: -1,
            },
            Self::Classic => DifficultyProfile {
                length: 144,
                gravity: 8,
                fuel: 180,
                oxygen: 90,
                safe_half_width: 1,
                jump_gap_rows: 2,
                intensity_bias: 0,
            },
            Self::Hard => DifficultyProfile {
                length: 192,
                gravity: 10,
                fuel: 150,
                oxygen: 60,
                safe_half_width: 0,
                jump_gap_rows: 2,
                intensity_bias: 1,
            },
        }
    }
}

impl fmt::Display for ProceduralDifficulty {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationId {
    version: u8,
    difficulty: ProceduralDifficulty,
    seed: u64,
}

impl GenerationId {
    pub fn new(seed: u64, difficulty: ProceduralDifficulty) -> Self {
        Self {
            version: GENERATOR_VERSION,
            difficulty,
            seed: seed & SEED_MASK,
        }
    }

    pub fn version(self) -> u8 {
        self.version
    }

    pub fn difficulty(self) -> ProceduralDifficulty {
        self.difficulty
    }

    pub fn seed(self) -> u64 {
        self.seed
    }

    pub fn with_difficulty(self, difficulty: ProceduralDifficulty) -> Self {
        Self { difficulty, ..self }
    }

    pub fn world_index(self) -> usize {
        (self.seed % 10) as usize
    }

    pub fn palette_index(self) -> usize {
        1 + self.world_index() * 3 + ((self.seed >> 8) % 3) as usize
    }

    fn profile(self) -> DifficultyProfile {
        let mut profile = self.difficulty.profile();
        if self.version >= 2 {
            profile.length += 32;
        }
        profile
    }

    fn compact(self) -> String {
        let mut compact = format!("SR{}{}", self.version, self.difficulty.code());
        compact.push_str(&encode_base32(self.seed, 8));
        let checksum = crc10_atm(compact.as_bytes());
        compact.push_str(&encode_base32(u64::from(checksum), 2));
        compact
    }
}

impl Default for GenerationId {
    fn default() -> Self {
        Self::new(0x534B_5952, ProceduralDifficulty::Classic)
    }
}

impl fmt::Display for GenerationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let compact = self.compact();
        write!(
            formatter,
            "{}-{}-{}-{}-{}",
            &compact[0..3],
            &compact[3..4],
            &compact[4..8],
            &compact[8..12],
            &compact[12..14]
        )
    }
}

impl FromStr for GenerationId {
    type Err = GenerationIdError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let compact = normalize_generation_id(input)?;
        if compact.len() != 14 {
            return Err(GenerationIdError::InvalidLength);
        }
        if &compact[0..2] != b"SR" {
            return Err(GenerationIdError::InvalidPrefix);
        }

        let version = compact[2]
            .checked_sub(b'0')
            .filter(|version| (FIRST_GENERATOR_VERSION..=GENERATOR_VERSION).contains(version))
            .ok_or(GenerationIdError::UnsupportedVersion)?;
        let difficulty = ProceduralDifficulty::from_code(compact[3])
            .ok_or(GenerationIdError::InvalidDifficulty)?;
        let seed = decode_base32(&compact[4..12])?;
        let supplied_checksum = decode_base32(&compact[12..14])? as u16;
        let expected_checksum = crc10_atm(&compact[..12]);
        if supplied_checksum != expected_checksum {
            return Err(GenerationIdError::InvalidChecksum);
        }

        Ok(Self {
            version,
            difficulty,
            seed,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationIdError {
    InvalidCharacter,
    InvalidLength,
    InvalidPrefix,
    UnsupportedVersion,
    InvalidDifficulty,
    InvalidChecksum,
}

impl fmt::Display for GenerationIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidCharacter => "ID contains an unsupported character",
            Self::InvalidLength => "ID must contain 14 letters and digits",
            Self::InvalidPrefix => "ID must start with SR",
            Self::UnsupportedVersion => "this generator version is not supported",
            Self::InvalidDifficulty => "ID has an invalid difficulty",
            Self::InvalidChecksum => "ID checksum does not match",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for GenerationIdError {}

#[derive(Debug, Clone, Copy)]
struct DifficultyProfile {
    length: usize,
    gravity: u16,
    fuel: u16,
    oxygen: u16,
    safe_half_width: usize,
    jump_gap_rows: usize,
    intensity_bias: i8,
}

#[derive(Debug, Clone, Copy)]
enum VisualFamily {
    PrismRibbon,
    Checkerboard,
    Colonnade,
    Constellation,
    Chevrons,
    Helix,
}

impl VisualFamily {
    fn from_index(index: u64) -> Self {
        match index % 6 {
            0 => Self::PrismRibbon,
            1 => Self::Checkerboard,
            2 => Self::Colonnade,
            3 => Self::Constellation,
            4 => Self::Chevrons,
            _ => Self::Helix,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct StyleScript {
    primary: VisualFamily,
    secondary: VisualFamily,
    dominant_color: u8,
    support_color: u8,
    accent_color: u8,
}

impl StyleScript {
    fn generate(random: &mut SplitMix64) -> Self {
        let primary_index = random.range(6);
        let secondary_offset = 1 + random.range(5);
        let color_offset = random.range(ROAD_COLORS.len() as u64) as usize;

        Self {
            primary: VisualFamily::from_index(primary_index),
            secondary: VisualFamily::from_index(primary_index + secondary_offset),
            dominant_color: themed_color(color_offset, 0),
            support_color: themed_color(color_offset, 2 + random.range(2) as usize),
            accent_color: themed_color(color_offset, 5 + random.range(3) as usize),
        }
    }

    fn family_for_motif(self, motif_index: usize) -> VisualFamily {
        // A, A', B, A gives the road a recognizable identity and periodic contrast.
        if motif_index % 4 == 2 {
            self.secondary
        } else {
            self.primary
        }
    }

    fn color_for_phase(self, phase: usize) -> u8 {
        match phase % 6 {
            0 | 1 | 5 => self.dominant_color,
            2 | 4 => self.support_color,
            _ => self.accent_color,
        }
    }

    fn with_theme(mut self, theme: RoadTheme) -> Self {
        let colors = match theme {
            RoadTheme::NeonCity => [13, 14, 15],
            RoadTheme::CrystalCavern => [5, 6, 11],
            RoadTheme::AlienRuins => [3, 7, 13],
            RoadTheme::OrbitalVoid => [4, 11, 15],
            RoadTheme::SolarHighway => [6, 14, 13],
            RoadTheme::HazardFoundry => [7, 4, 14],
        };
        self.dominant_color = colors[0];
        self.support_color = colors[1];
        self.accent_color = colors[2];
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MotifKind {
    Cruise,
    Fan,
    LaneShift,
    Slalom,
    SplitMerge,
    JumpGap,
    NeonGate,
    SpeedFork,
    HazardChicane,
    Recovery,
    PrismCrown,
    TunnelRun,
    Skyway,
    AuroraColonnade,
    HelixBeacons,
    ConstellationField,
    PrismFanBoulevard,
    RuinedAqueduct,
    CheckerboardCauseway,
    CrownParade,
    CometTail,
    TripleIslandHop,
    SwitchbackArchipelago,
    HighLowFork,
    DescendingTerraces,
    PadLogicFork,
    OxygenOasisDetour,
    BrakeOrFly,
    CubeHurdleRhythm,
    TunnelLaneWeave,
    EclipseBreakout,
    SuspendedSerpent,
    TwinSkybridges,
    PyramidSummit,
    MeteorShower,
    PortalRoulette,
    SpiralTemple,
    CollapsingSkybridge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetPieceFocus {
    Decoration,
    Gameplay,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoadTheme {
    NeonCity,
    CrystalCavern,
    AlienRuins,
    OrbitalVoid,
    SolarHighway,
    HazardFoundry,
}

impl RoadTheme {
    fn from_index(index: u64) -> Self {
        match index % 6 {
            0 => Self::NeonCity,
            1 => Self::CrystalCavern,
            2 => Self::AlienRuins,
            3 => Self::OrbitalVoid,
            4 => Self::SolarHighway,
            _ => Self::HazardFoundry,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MotifDefinition {
    kind: MotifKind,
    min_rows: usize,
    max_rows: usize,
    intensity: u8,
    weight: [u16; 3],
    needs_recovery: bool,
}

const MOTIFS: [MotifDefinition; 13] = [
    motif(MotifKind::Cruise, 6, 10, 0, [8, 5, 3], false),
    motif(MotifKind::Fan, 7, 13, 1, [7, 6, 4], false),
    motif(MotifKind::LaneShift, 6, 9, 1, [8, 7, 5], false),
    motif(MotifKind::Slalom, 16, 20, 2, [3, 7, 8], false),
    motif(MotifKind::SplitMerge, 9, 13, 2, [4, 6, 7], false),
    motif(MotifKind::JumpGap, 12, 14, 2, [3, 6, 7], true),
    motif(MotifKind::NeonGate, 6, 9, 2, [4, 6, 6], false),
    motif(MotifKind::SpeedFork, 8, 11, 2, [4, 7, 8], false),
    motif(MotifKind::HazardChicane, 8, 12, 3, [0, 4, 8], true),
    motif(MotifKind::Recovery, 6, 9, 0, [6, 5, 4], false),
    motif(MotifKind::PrismCrown, 10, 13, 4, [0, 2, 6], true),
    // SR2 schedules these landmarks explicitly. Zero random weight keeps SR1
    // selection and its stable IDs byte-for-byte compatible.
    motif(MotifKind::TunnelRun, 8, 11, 2, [0, 0, 0], false),
    motif(MotifKind::Skyway, 20, 24, 3, [0, 0, 0], true),
];

const SET_PIECES: [MotifDefinition; 26] = [
    set_piece(MotifKind::Skyway, 20, 24, 3, [5, 7, 8], true),
    set_piece(MotifKind::AuroraColonnade, 8, 13, 1, [9, 7, 5], false),
    set_piece(MotifKind::HelixBeacons, 8, 13, 1, [8, 7, 6], false),
    set_piece(MotifKind::ConstellationField, 8, 13, 1, [8, 7, 6], false),
    set_piece(MotifKind::PrismFanBoulevard, 9, 14, 1, [9, 7, 5], false),
    set_piece(MotifKind::RuinedAqueduct, 9, 14, 1, [7, 7, 6], false),
    set_piece(MotifKind::CheckerboardCauseway, 8, 13, 1, [9, 7, 5], false),
    set_piece(MotifKind::CrownParade, 8, 13, 1, [7, 7, 6], false),
    set_piece(MotifKind::CometTail, 8, 13, 1, [8, 7, 6], false),
    set_piece(MotifKind::TripleIslandHop, 14, 19, 3, [5, 7, 8], true),
    set_piece(
        MotifKind::SwitchbackArchipelago,
        12,
        18,
        3,
        [6, 8, 9],
        false,
    ),
    set_piece(MotifKind::HighLowFork, 10, 16, 2, [8, 8, 7], false),
    set_piece(MotifKind::DescendingTerraces, 12, 18, 3, [6, 8, 9], false),
    set_piece(MotifKind::PadLogicFork, 9, 14, 2, [8, 8, 7], false),
    set_piece(MotifKind::OxygenOasisDetour, 10, 15, 2, [9, 7, 5], false),
    set_piece(MotifKind::BrakeOrFly, 11, 16, 3, [6, 8, 9], false),
    set_piece(MotifKind::CubeHurdleRhythm, 10, 16, 3, [6, 8, 9], false),
    set_piece(MotifKind::TunnelLaneWeave, 11, 17, 3, [6, 8, 9], false),
    set_piece(MotifKind::EclipseBreakout, 14, 20, 4, [4, 7, 9], true),
    set_piece(MotifKind::SuspendedSerpent, 13, 19, 3, [5, 8, 9], false),
    set_piece(MotifKind::TwinSkybridges, 13, 19, 3, [5, 8, 9], false),
    set_piece(MotifKind::PyramidSummit, 14, 20, 4, [4, 7, 9], false),
    set_piece(MotifKind::MeteorShower, 11, 17, 3, [6, 8, 9], false),
    set_piece(MotifKind::PortalRoulette, 11, 17, 3, [6, 8, 9], false),
    set_piece(MotifKind::SpiralTemple, 13, 19, 4, [4, 7, 9], false),
    set_piece(MotifKind::CollapsingSkybridge, 14, 20, 4, [4, 7, 9], true),
];

const fn motif(
    kind: MotifKind,
    min_rows: usize,
    max_rows: usize,
    intensity: u8,
    weight: [u16; 3],
    needs_recovery: bool,
) -> MotifDefinition {
    MotifDefinition {
        kind,
        min_rows,
        max_rows,
        intensity,
        weight,
        needs_recovery,
    }
}

const fn set_piece(
    kind: MotifKind,
    min_rows: usize,
    max_rows: usize,
    intensity: u8,
    weight: [u16; 3],
    needs_recovery: bool,
) -> MotifDefinition {
    motif(kind, min_rows, max_rows, intensity, weight, needs_recovery)
}

#[derive(Debug, Clone, Copy)]
struct MotifPlan {
    definition: MotifDefinition,
    rows: usize,
    entry_lane: usize,
    exit_lane: usize,
    mirrored: bool,
    is_climax: bool,
    generator_version: u8,
}

pub fn generate_procedural_level(id: GenerationId) -> Level {
    let profile = id.profile();
    let mut visual_random = SplitMix64::for_stream(id.seed, 0x5649_5355_414C);
    let mut reward_random = SplitMix64::for_stream(id.seed, 0x5245_5741_5244);
    let mut style = StyleScript::generate(&mut visual_random);
    if id.version >= 3 {
        style = style.with_theme(surprise_theme(id.seed));
    }
    let motif_plans = plan_road(id, profile);
    let mut rows = Vec::with_capacity(profile.length);

    for row_index in 0..START_ROWS {
        let color = style.color_for_phase(row_index / 3);
        let mut row = [LevelCell::EMPTY; ROAD_COLUMNS];
        paint_safe_route(&mut row, 3, 3, 1, color);
        if row_index % 3 == 0 {
            row[0] = descriptor(cube_descriptor_raw(0, style.support_color));
            row[6] = descriptor(cube_descriptor_raw(0, style.support_color));
        }
        if row_index == START_ROWS - 3 {
            row[3] = descriptor(ACCELERATE);
        }
        rows.push(row);
    }

    for (motif_index, plan) in motif_plans.iter().copied().enumerate() {
        append_motif(
            &mut rows,
            plan,
            motif_index,
            profile,
            style,
            &mut reward_random,
        );
    }

    let finish_lane = motif_plans.last().map(|plan| plan.exit_lane).unwrap_or(3);
    append_finish(&mut rows, finish_lane, style, profile.length);

    Level {
        // The legacy diagnostic value remains a valid shipped palette index.
        // Progression uses LevelKind and never interprets this as campaign data.
        road_index: id.palette_index(),
        kind: LevelKind::Procedural,
        theme: LevelTheme {
            world_index: id.world_index(),
            palette_index: id.palette_index(),
        },
        name: format!("Procedural {id}"),
        gravity: profile.gravity,
        fuel: profile.fuel,
        oxygen: profile.oxygen,
        cells: rows,
    }
}

fn surprise_theme(seed: u64) -> RoadTheme {
    let mut random = SplitMix64::for_stream(seed, 0x5355_5250_5249_5345);
    RoadTheme::from_index(random.range(6))
}

fn plan_road(id: GenerationId, profile: DifficultyProfile) -> Vec<MotifPlan> {
    if id.version >= 3 {
        return plan_surprise_road(id, profile);
    }

    plan_legacy_road(id, profile)
}

fn plan_legacy_road(id: GenerationId, profile: DifficultyProfile) -> Vec<MotifPlan> {
    let mut pacing_random = SplitMix64::for_stream(id.seed, 0x5041_4349_4E47);
    let mut layout_random = SplitMix64::for_stream(id.seed, 0x4C41_594F_5554);
    let body_length = profile.length - START_ROWS - FINISH_ROWS;
    let mut plans = Vec::new();
    let mut used_rows = 0usize;
    let mut safe_lane = 3usize;
    let mut previous_kind = None;
    let mut recovery_required = false;
    let mut jump_gap_used = false;
    let mut tunnel_used = false;
    let mut skyway_used = false;
    let mut climax_used = false;
    while used_rows < body_length {
        let remaining = body_length - used_rows;
        assert!(remaining >= 6, "motif planner left an unusable short tail");
        let progress_percent = used_rows * 100 / body_length.max(1);
        let scheduled_climax =
            progress_percent >= 68 && jump_gap_used && !climax_used && !recovery_required;
        let selection = MotifSelection {
            difficulty: id.difficulty,
            body_progress: used_rows,
            body_length,
            remaining,
            motif_index: plans.len(),
            previous_kind,
            recovery_required,
            jump_gap_used,
            tunnel_used,
            skyway_used,
            climax_due: scheduled_climax,
            generator_version: id.version,
        };
        let mut starts_climax = scheduled_climax;
        let mut selected_motif = choose_motif(selection, &mut pacing_random);
        let landmarks = LandmarkProgress {
            jump_gap_used,
            tunnel_used,
            skyway_used,
            climax_used,
            generator_version: id.version,
        };
        let mut future_rows =
            required_future_rows(id.difficulty, selected_motif, landmarks, starts_climax);

        if selected_motif.min_rows + future_rows > remaining {
            selected_motif = if recovery_required {
                definition(MotifKind::Recovery)
            } else if !jump_gap_used {
                definition(MotifKind::JumpGap)
            } else if !climax_used {
                starts_climax = true;
                climax_definition(id.difficulty, previous_kind)
            } else {
                connector_definition(previous_kind, remaining)
            };
            future_rows =
                required_future_rows(id.difficulty, selected_motif, landmarks, starts_climax);
        }

        let row_budget = remaining - future_rows;
        assert!(
            selected_motif.min_rows <= row_budget,
            "required procedural beats exceed the remaining row budget for {id}: \
             selected={selected_motif:?} remaining={remaining} future={future_rows}"
        );
        let must_partition_tail = climax_used;
        if must_partition_tail && !motif_can_fit(selected_motif, row_budget) {
            selected_motif = connector_definition(previous_kind, row_budget);
        }

        let plan = plan_motif(
            MotifPlanRequest {
                definition: selected_motif,
                remaining: row_budget,
                entry_lane: safe_lane,
                is_climax: starts_climax,
                must_partition_tail,
                generator_version: id.version,
            },
            &mut pacing_random,
            &mut layout_random,
        );
        used_rows += plan.rows;
        safe_lane = plan.exit_lane;
        previous_kind = Some(plan.definition.kind);
        recovery_required = plan.definition.needs_recovery;
        jump_gap_used |= plan.definition.kind == MotifKind::JumpGap;
        tunnel_used |= plan.definition.kind == MotifKind::TunnelRun;
        skyway_used |= plan.definition.kind == MotifKind::Skyway;
        climax_used |= starts_climax;
        plans.push(plan);
    }
    plans
}

fn plan_surprise_road(id: GenerationId, profile: DifficultyProfile) -> Vec<MotifPlan> {
    let mut selection_random = SplitMix64::for_stream(id.seed, 0x5355_5250_5249_5345);
    let mut layout_random = SplitMix64::for_stream(id.seed, 0x5352_334C_4159_4F55);
    let theme = RoadTheme::from_index(selection_random.range(6));
    let set_piece_count = match id.difficulty {
        ProceduralDifficulty::Easy => 5,
        ProceduralDifficulty::Classic => 6 + selection_random.range(2) as usize,
        ProceduralDifficulty::Hard => 7 + selection_random.range(2) as usize,
    };
    let selected = choose_set_pieces(
        id.difficulty,
        theme,
        set_piece_count,
        profile.length - START_ROWS - FINISH_ROWS - (set_piece_count + 1) * 6,
        id.version,
        &mut selection_random,
    );

    let mut definitions = Vec::with_capacity(set_piece_count * 2 + 1);
    for (index, set_piece) in selected.into_iter().enumerate() {
        definitions.push(choose_connector(index, &mut selection_random));
        definitions.push(set_piece);
    }
    definitions.push(choose_connector(set_piece_count, &mut selection_random));

    let body_length = profile.length - START_ROWS - FINISH_ROWS;
    let minimum_rows: usize = definitions
        .iter()
        .map(|definition| definition.min_rows)
        .sum();
    assert!(
        minimum_rows <= body_length,
        "SR3 set pieces exceed the available road length"
    );

    let mut row_counts: Vec<usize> = definitions
        .iter()
        .map(|definition| definition.min_rows)
        .collect();
    let mut rows_left = body_length - minimum_rows;
    while rows_left > 0 {
        let expandable: Vec<usize> = definitions
            .iter()
            .zip(&row_counts)
            .enumerate()
            .filter_map(|(index, (definition, rows))| {
                (*rows < definition.max_rows).then_some(index)
            })
            .collect();
        assert!(!expandable.is_empty(), "SR3 road plan lacks row capacity");
        let chosen = expandable[layout_random.range(expandable.len() as u64) as usize];
        row_counts[chosen] += 1;
        rows_left -= 1;
    }

    let mut safe_lane = 3;
    definitions
        .into_iter()
        .zip(row_counts)
        .map(|(definition, rows)| {
            let mirrored = layout_random.range(2) == 1;
            let exit_lane = choose_exit_lane(safe_lane, definition.kind, mirrored);
            let plan = MotifPlan {
                definition,
                rows,
                entry_lane: safe_lane,
                exit_lane,
                mirrored,
                is_climax: false,
                generator_version: id.version,
            };
            safe_lane = exit_lane;
            plan
        })
        .collect()
}

fn choose_set_pieces(
    difficulty: ProceduralDifficulty,
    theme: RoadTheme,
    count: usize,
    row_budget: usize,
    generator_version: u8,
    random: &mut SplitMix64,
) -> Vec<MotifDefinition> {
    let mut selected = Vec::with_capacity(count);
    let mut decoration_streak = 0;
    let mut gameplay_count = 0;

    while selected.len() < count {
        let slots_left = count - selected.len();
        let rows_used: usize = selected
            .iter()
            .map(|piece: &MotifDefinition| piece.min_rows)
            .sum();
        let gameplay_needed = 2_usize.saturating_sub(gameplay_count);
        let must_affect_gameplay = slots_left <= gameplay_needed;
        let easy_long_piece_used = difficulty == ProceduralDifficulty::Easy
            && selected
                .iter()
                .any(|piece: &MotifDefinition| piece.min_rows >= 18);
        let mut candidates = Vec::new();
        let mut total_weight = 0_u64;

        for candidate in SET_PIECES {
            let candidate = current_set_piece_definition(candidate, generator_version);
            let focus = set_piece_focus(candidate.kind);
            let already_selected = selected
                .iter()
                .any(|selected: &MotifDefinition| selected.kind == candidate.kind);
            let too_many_decorations = focus == SetPieceFocus::Decoration && decoration_streak >= 2;
            let does_not_meet_quota = must_affect_gameplay && focus == SetPieceFocus::Decoration;
            let repeats_long_easy_piece = easy_long_piece_used && candidate.min_rows >= 18;
            let minimum_future_rows = (slots_left - 1) * 9;
            let fits = rows_used + candidate.min_rows + minimum_future_rows <= row_budget;
            if already_selected
                || too_many_decorations
                || does_not_meet_quota
                || repeats_long_easy_piece
                || !fits
            {
                continue;
            }

            let theme_multiplier = if set_piece_theme(candidate.kind) == theme {
                3
            } else {
                1
            };
            let weight =
                u64::from(candidate.weight[difficulty_index(difficulty)]) * theme_multiplier;
            total_weight += weight;
            candidates.push((candidate, total_weight));
        }

        let roll = random.range(total_weight);
        let chosen = candidates
            .into_iter()
            .find(|(_, cumulative_weight)| roll < *cumulative_weight)
            .map(|(candidate, _)| candidate)
            .expect("SR3 surprise bag always has a candidate");
        let focus = set_piece_focus(chosen.kind);
        if focus == SetPieceFocus::Decoration {
            decoration_streak += 1;
        } else {
            decoration_streak = 0;
            gameplay_count += 1;
        }
        selected.push(chosen);
    }

    selected
}

fn current_set_piece_definition(
    mut definition: MotifDefinition,
    generator_version: u8,
) -> MotifDefinition {
    if generator_version < 4 {
        return definition;
    }

    match definition.kind {
        MotifKind::CheckerboardCauseway => {
            definition.min_rows = 18;
            definition.max_rows = 24;
        }
        MotifKind::DescendingTerraces | MotifKind::SuspendedSerpent | MotifKind::TwinSkybridges => {
            definition.min_rows = 18;
            definition.max_rows = 24;
        }
        _ => {}
    }
    definition
}

fn choose_connector(index: usize, random: &mut SplitMix64) -> MotifDefinition {
    const CONNECTORS: [MotifKind; 3] =
        [MotifKind::Cruise, MotifKind::LaneShift, MotifKind::Recovery];
    let offset = random.range(CONNECTORS.len() as u64) as usize;
    let mut connector = definition(CONNECTORS[(index + offset) % CONNECTORS.len()]);
    connector.max_rows = 24;
    connector
}

fn set_piece_focus(kind: MotifKind) -> SetPieceFocus {
    match kind {
        MotifKind::AuroraColonnade
        | MotifKind::HelixBeacons
        | MotifKind::ConstellationField
        | MotifKind::PrismFanBoulevard
        | MotifKind::RuinedAqueduct
        | MotifKind::CrownParade
        | MotifKind::CometTail => SetPieceFocus::Decoration,
        MotifKind::TripleIslandHop
        | MotifKind::SwitchbackArchipelago
        | MotifKind::HighLowFork
        | MotifKind::DescendingTerraces
        | MotifKind::CheckerboardCauseway
        | MotifKind::PadLogicFork
        | MotifKind::OxygenOasisDetour
        | MotifKind::BrakeOrFly
        | MotifKind::CubeHurdleRhythm
        | MotifKind::TunnelLaneWeave => SetPieceFocus::Gameplay,
        MotifKind::Skyway
        | MotifKind::EclipseBreakout
        | MotifKind::SuspendedSerpent
        | MotifKind::TwinSkybridges
        | MotifKind::PyramidSummit
        | MotifKind::MeteorShower
        | MotifKind::PortalRoulette
        | MotifKind::SpiralTemple
        | MotifKind::CollapsingSkybridge => SetPieceFocus::Both,
        _ => panic!("connector is not an SR3 set piece: {kind:?}"),
    }
}

#[cfg(test)]
fn is_set_piece(kind: MotifKind) -> bool {
    SET_PIECES.iter().any(|definition| definition.kind == kind)
}

fn set_piece_theme(kind: MotifKind) -> RoadTheme {
    match kind {
        MotifKind::AuroraColonnade
        | MotifKind::PrismFanBoulevard
        | MotifKind::CheckerboardCauseway
        | MotifKind::PadLogicFork
        | MotifKind::PortalRoulette => RoadTheme::NeonCity,
        MotifKind::HelixBeacons
        | MotifKind::CrownParade
        | MotifKind::DescendingTerraces
        | MotifKind::PyramidSummit => RoadTheme::CrystalCavern,
        MotifKind::RuinedAqueduct
        | MotifKind::OxygenOasisDetour
        | MotifKind::SpiralTemple
        | MotifKind::SwitchbackArchipelago => RoadTheme::AlienRuins,
        MotifKind::ConstellationField
        | MotifKind::CometTail
        | MotifKind::HighLowFork
        | MotifKind::SuspendedSerpent
        | MotifKind::TwinSkybridges => RoadTheme::OrbitalVoid,
        MotifKind::Skyway
        | MotifKind::TripleIslandHop
        | MotifKind::BrakeOrFly
        | MotifKind::EclipseBreakout => RoadTheme::SolarHighway,
        MotifKind::CubeHurdleRhythm
        | MotifKind::TunnelLaneWeave
        | MotifKind::MeteorShower
        | MotifKind::CollapsingSkybridge => RoadTheme::HazardFoundry,
        _ => panic!("connector is not an SR3 set piece: {kind:?}"),
    }
}

fn motif_color(style: StyleScript, motif_index: usize, local_row: usize, lane: usize) -> u8 {
    let family = style.family_for_motif(motif_index);
    let phase = match family {
        VisualFamily::PrismRibbon => motif_index + local_row + lane,
        VisualFamily::Checkerboard => motif_index + (local_row + lane) % 2 * 2,
        VisualFamily::Colonnade => motif_index + local_row / 3,
        VisualFamily::Constellation => motif_index + (local_row * 2 + lane * 3) % 5,
        VisualFamily::Chevrons => motif_index + local_row / 2 + lane.abs_diff(3),
        VisualFamily::Helix => motif_index + local_row + lane.abs_diff(3) * 2,
    };
    style.color_for_phase(phase)
}

#[derive(Debug, Clone, Copy)]
struct MotifSelection {
    difficulty: ProceduralDifficulty,
    body_progress: usize,
    body_length: usize,
    remaining: usize,
    motif_index: usize,
    previous_kind: Option<MotifKind>,
    recovery_required: bool,
    jump_gap_used: bool,
    tunnel_used: bool,
    skyway_used: bool,
    climax_due: bool,
    generator_version: u8,
}

fn choose_motif(selection: MotifSelection, random: &mut SplitMix64) -> MotifDefinition {
    if selection.climax_due {
        return climax_definition(selection.difficulty, selection.previous_kind);
    }
    if selection.remaining < 6 {
        return if selection.previous_kind == Some(MotifKind::Cruise) {
            definition(MotifKind::Recovery)
        } else {
            definition(MotifKind::Cruise)
        };
    }
    if selection.motif_index == 0 {
        return definition(MotifKind::Cruise);
    }
    if selection.motif_index == 1 {
        return definition(MotifKind::LaneShift);
    }
    if selection.recovery_required {
        return definition(MotifKind::Recovery);
    }

    let progress_percent = selection.body_progress * 100 / selection.body_length.max(1);
    if selection.generator_version >= 2 && progress_percent >= 18 && !selection.tunnel_used {
        return definition(MotifKind::TunnelRun);
    }
    if progress_percent >= 28 && !selection.jump_gap_used {
        return definition(MotifKind::JumpGap);
    }
    if selection.generator_version >= 2 && progress_percent >= 48 && !selection.skyway_used {
        return definition(MotifKind::Skyway);
    }
    let target_intensity = target_intensity(progress_percent, selection.difficulty);
    let difficulty_index = difficulty_index(selection.difficulty);
    let mut candidates = Vec::new();
    let mut total_weight = 0_u64;
    for candidate in MOTIFS {
        let base_weight = candidate.weight[difficulty_index];
        let fits = motif_can_fit(candidate, selection.remaining);
        let differs_from_previous = Some(candidate.kind) != selection.previous_kind;
        if base_weight == 0 || !fits || !differs_from_previous {
            continue;
        }

        let intensity_difference = candidate.intensity.abs_diff(target_intensity);
        let intensity_fit = u64::from(5_u8.saturating_sub(intensity_difference).max(1));
        let weight = u64::from(base_weight) * intensity_fit;
        total_weight += weight;
        candidates.push((candidate, total_weight));
    }

    if candidates.is_empty() {
        return definition(MotifKind::Cruise);
    }

    let roll = random.range(total_weight);
    candidates
        .into_iter()
        .find(|(_, cumulative_weight)| roll < *cumulative_weight)
        .map(|(candidate, _)| candidate)
        .unwrap_or_else(|| definition(MotifKind::Cruise))
}

fn climax_definition(
    difficulty: ProceduralDifficulty,
    previous_kind: Option<MotifKind>,
) -> MotifDefinition {
    let kind = match difficulty {
        ProceduralDifficulty::Easy if previous_kind == Some(MotifKind::SpeedFork) => {
            MotifKind::NeonGate
        }
        ProceduralDifficulty::Easy => MotifKind::SpeedFork,
        ProceduralDifficulty::Classic | ProceduralDifficulty::Hard => MotifKind::PrismCrown,
    };
    definition(kind)
}

#[derive(Debug, Clone, Copy)]
struct LandmarkProgress {
    jump_gap_used: bool,
    tunnel_used: bool,
    skyway_used: bool,
    climax_used: bool,
    generator_version: u8,
}

fn required_future_rows(
    difficulty: ProceduralDifficulty,
    selected_motif: MotifDefinition,
    landmarks: LandmarkProgress,
    starts_climax: bool,
) -> usize {
    let recovery_rows = definition(MotifKind::Recovery).min_rows;
    let selected_needs_recovery = usize::from(selected_motif.needs_recovery) * recovery_rows;
    let jump_rows = if landmarks.jump_gap_used || selected_motif.kind == MotifKind::JumpGap {
        0
    } else {
        definition(MotifKind::JumpGap).min_rows + recovery_rows
    };
    let tunnel_rows = if landmarks.generator_version == 1
        || landmarks.tunnel_used
        || selected_motif.kind == MotifKind::TunnelRun
    {
        0
    } else {
        definition(MotifKind::TunnelRun).min_rows
    };
    let skyway_rows = if landmarks.generator_version == 1
        || landmarks.skyway_used
        || selected_motif.kind == MotifKind::Skyway
    {
        0
    } else {
        let skyway = definition(MotifKind::Skyway);
        skyway.min_rows + usize::from(skyway.needs_recovery) * recovery_rows
    };
    let climax_rows = if starts_climax {
        6
    } else if landmarks.climax_used {
        0
    } else {
        let climax = match difficulty {
            ProceduralDifficulty::Easy => definition(MotifKind::SpeedFork),
            ProceduralDifficulty::Classic | ProceduralDifficulty::Hard => {
                definition(MotifKind::PrismCrown)
            }
        };
        let climax_recovery = usize::from(climax.needs_recovery) * recovery_rows;
        climax.min_rows + climax_recovery + 6
    };

    selected_needs_recovery + jump_rows + tunnel_rows + skyway_rows + climax_rows
}

fn target_intensity(progress_percent: usize, difficulty: ProceduralDifficulty) -> u8 {
    let base = match progress_percent {
        0..=14 => 0_i8,
        15..=34 => 1,
        35..=54 => 2,
        55..=64 => 1,
        65..=84 => 3,
        _ => 1,
    };
    (base + difficulty.profile().intensity_bias).clamp(0, 4) as u8
}

fn difficulty_index(difficulty: ProceduralDifficulty) -> usize {
    match difficulty {
        ProceduralDifficulty::Easy => 0,
        ProceduralDifficulty::Classic => 1,
        ProceduralDifficulty::Hard => 2,
    }
}

fn definition(kind: MotifKind) -> MotifDefinition {
    MOTIFS
        .into_iter()
        .find(|candidate| candidate.kind == kind)
        .expect("every motif kind has a definition")
}

fn motif_can_fit(motif: MotifDefinition, remaining: usize) -> bool {
    let can_consume_remainder = (motif.min_rows..=motif.max_rows).contains(&remaining);
    let can_leave_a_full_motif = remaining >= motif.min_rows + 6;
    can_consume_remainder || can_leave_a_full_motif
}

fn connector_definition(previous_kind: Option<MotifKind>, remaining: usize) -> MotifDefinition {
    [
        MotifKind::Cruise,
        MotifKind::Recovery,
        MotifKind::SpeedFork,
        MotifKind::Fan,
        MotifKind::LaneShift,
        MotifKind::NeonGate,
    ]
    .into_iter()
    .map(definition)
    .find(|candidate| Some(candidate.kind) != previous_kind && motif_can_fit(*candidate, remaining))
    .unwrap_or_else(|| {
        panic!(
            "safe connector library does not cover remaining={remaining}, previous={previous_kind:?}"
        )
    })
}

#[derive(Debug, Clone, Copy)]
struct MotifPlanRequest {
    definition: MotifDefinition,
    remaining: usize,
    entry_lane: usize,
    is_climax: bool,
    must_partition_tail: bool,
    generator_version: u8,
}

fn plan_motif(
    request: MotifPlanRequest,
    pacing_random: &mut SplitMix64,
    layout_random: &mut SplitMix64,
) -> MotifPlan {
    let MotifPlanRequest {
        definition,
        remaining,
        entry_lane,
        is_climax,
        must_partition_tail,
        generator_version,
    } = request;
    let available_rows = definition.max_rows.min(remaining);
    let minimum_rows = definition.min_rows;
    let row_variation = available_rows - minimum_rows;
    let mut rows = minimum_rows + pacing_random.range(row_variation as u64 + 1) as usize;
    let leftover_rows = remaining - rows;
    if must_partition_tail && (1..6).contains(&leftover_rows) {
        let can_consume_leftover = rows + leftover_rows <= definition.max_rows;
        if can_consume_leftover {
            rows += leftover_rows;
        } else {
            rows -= 6 - leftover_rows;
        }
    }
    let mirrored = layout_random.range(2) == 1;
    let exit_lane = choose_exit_lane(entry_lane, definition.kind, mirrored);

    MotifPlan {
        definition,
        rows,
        entry_lane,
        exit_lane,
        mirrored,
        is_climax,
        generator_version,
    }
}

fn choose_exit_lane(entry_lane: usize, kind: MotifKind, mirrored: bool) -> usize {
    let changes_lane = matches!(
        kind,
        MotifKind::LaneShift
            | MotifKind::Slalom
            | MotifKind::HazardChicane
            | MotifKind::PrismCrown
            | MotifKind::SwitchbackArchipelago
            | MotifKind::SuspendedSerpent
            | MotifKind::SpiralTemple
    );
    if !changes_lane {
        return entry_lane;
    }

    let maximum_step = 1;
    let direction = if mirrored { -1_isize } else { 1 };
    let candidate = entry_lane as isize + direction * maximum_step;
    if (1..=5).contains(&candidate) {
        candidate as usize
    } else {
        (entry_lane as isize - direction * maximum_step).clamp(1, 5) as usize
    }
}

fn append_motif(
    rows: &mut Vec<[LevelCell; ROAD_COLUMNS]>,
    plan: MotifPlan,
    motif_index: usize,
    profile: DifficultyProfile,
    style: StyleScript,
    reward_random: &mut SplitMix64,
) {
    for local_row in 0..plan.rows {
        let mut row = [LevelCell::EMPTY; ROAD_COLUMNS];
        let current_lane = route_lane(plan, local_row);
        let previous_lane = previous_route_lane(plan, local_row, current_lane);
        let route_color = if plan.is_climax && local_row % 2 == 0 {
            style.accent_color
        } else {
            motif_color(style, motif_index, local_row, current_lane)
        };
        paint_safe_route(
            &mut row,
            previous_lane,
            current_lane,
            profile.safe_half_width,
            route_color,
        );
        apply_motif_gameplay(
            &mut row,
            plan,
            RouteStep {
                row_index: local_row,
                previous_lane,
                current_lane,
            },
            profile,
            style,
            reward_random,
        );
        rows.push(row);
    }
}

#[derive(Debug, Clone, Copy)]
struct RouteStep {
    row_index: usize,
    previous_lane: usize,
    current_lane: usize,
}

fn previous_route_lane(plan: MotifPlan, local_row: usize, current_lane: usize) -> usize {
    if local_row == 0 {
        return plan.entry_lane;
    }

    if plan.generator_version >= 4 && is_roadless_set_piece(plan.definition.kind) {
        return route_lane(plan, local_row - 1);
    }

    if plan.definition.kind == MotifKind::Slalom && local_row >= 3 {
        let lane_three_rows_ago = route_lane(plan, local_row - 3);
        if lane_three_rows_ago != current_lane {
            return lane_three_rows_ago;
        }
    }

    let sustains_lane_change = matches!(
        plan.definition.kind,
        MotifKind::LaneShift
            | MotifKind::HazardChicane
            | MotifKind::PrismCrown
            | MotifKind::SwitchbackArchipelago
            | MotifKind::SuspendedSerpent
            | MotifKind::SpiralTemple
    );
    let transition_started = current_lane == plan.exit_lane && plan.entry_lane != plan.exit_lane;
    let transition_still_visible = local_row < plan.rows / 2 + 3;
    if sustains_lane_change && transition_started && transition_still_visible {
        return plan.entry_lane;
    }

    route_lane(plan, local_row - 1)
}

fn route_lane(plan: MotifPlan, local_row: usize) -> usize {
    if plan.generator_version >= 4 && is_roadless_set_piece(plan.definition.kind) {
        return roadless_route_lane(plan, local_row);
    }

    match plan.definition.kind {
        MotifKind::Slalom => {
            if local_row + 8 >= plan.rows {
                return if local_row + 4 >= plan.rows {
                    plan.exit_lane
                } else {
                    plan.entry_lane
                };
            }
            let direction = if plan.mirrored { -1_isize } else { 1 };
            match (local_row / 5) % 4 {
                1 => (plan.entry_lane as isize + direction).clamp(1, 5) as usize,
                3 => (plan.entry_lane as isize - direction).clamp(1, 5) as usize,
                _ => plan.entry_lane,
            }
        }
        MotifKind::LaneShift
        | MotifKind::HazardChicane
        | MotifKind::PrismCrown
        | MotifKind::SwitchbackArchipelago
        | MotifKind::SuspendedSerpent
        | MotifKind::SpiralTemple => {
            if local_row * 2 < plan.rows {
                plan.entry_lane
            } else {
                plan.exit_lane
            }
        }
        _ => plan.entry_lane,
    }
}

fn is_roadless_set_piece(kind: MotifKind) -> bool {
    matches!(
        kind,
        MotifKind::CheckerboardCauseway
            | MotifKind::DescendingTerraces
            | MotifKind::SuspendedSerpent
            | MotifKind::TwinSkybridges
    )
}

fn roadless_route_lane(plan: MotifPlan, local_row: usize) -> usize {
    if local_row + 2 >= plan.rows {
        return plan.exit_lane;
    }

    let direction = match plan.entry_lane.cmp(&3) {
        std::cmp::Ordering::Less => -1_isize,
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Equal if plan.mirrored => -1,
        std::cmp::Ordering::Equal => 1,
    };
    let offset = match plan.definition.kind {
        MotifKind::CheckerboardCauseway
        | MotifKind::SuspendedSerpent
        | MotifKind::TwinSkybridges
        | MotifKind::DescendingTerraces => {
            if local_row < 3 || local_row + 3 >= plan.rows {
                0
            } else if local_row < 6 || local_row + 6 >= plan.rows {
                1
            } else {
                2
            }
        }
        _ => unreachable!("only roadless set pieces use a roadless route"),
    };

    (plan.entry_lane as isize + direction * offset).clamp(1, ROAD_COLUMNS as isize - 2) as usize
}

fn apply_motif_gameplay(
    row: &mut [LevelCell; ROAD_COLUMNS],
    plan: MotifPlan,
    step: RouteStep,
    profile: DifficultyProfile,
    style: StyleScript,
    reward_random: &mut SplitMix64,
) {
    let RouteStep {
        row_index: local_row,
        previous_lane,
        current_lane,
    } = step;
    let middle = plan.rows / 2;
    match plan.definition.kind {
        MotifKind::Cruise | MotifKind::LaneShift => {
            let is_sr3_connector = plan.definition.max_rows == 24;
            if is_sr3_connector && local_row == middle {
                row[current_lane] = descriptor(REFILL);
            }
        }
        MotifKind::Fan => {
            let distance_from_middle = local_row.abs_diff(middle);
            let fan_width = 3_usize.saturating_sub(distance_from_middle);
            let left = current_lane.saturating_sub(fan_width);
            let right = (current_lane + fan_width).min(ROAD_COLUMNS - 1);
            for cell in &mut row[left..=right] {
                *cell = descriptor(u16::from(style.color_for_phase(local_row)));
            }
        }
        MotifKind::Slalom if local_row % 3 == 1 => {
            let outside_lane = if current_lane <= 3 { 5 } else { 1 };
            row[outside_lane] = descriptor(SLIDE);
        }
        MotifKind::SplitMerge if local_row > 2 && local_row + 3 < plan.rows => {
            let left_branch = current_lane.saturating_sub(1);
            let right_branch = (current_lane + 1).min(ROAD_COLUMNS - 1);
            row[current_lane] = LevelCell::EMPTY;
            row[left_branch] = descriptor(u16::from(style.support_color));
            row[right_branch] = descriptor(u16::from(style.accent_color));
            if local_row == middle {
                let reward_lane = if plan.mirrored {
                    left_branch
                } else {
                    right_branch
                };
                row[reward_lane] = descriptor(ACCELERATE);
            }
        }
        MotifKind::SplitMerge if local_row == 1 || local_row + 2 == plan.rows => {
            let left_bridge = current_lane.saturating_sub(1);
            let right_bridge = (current_lane + 1).min(ROAD_COLUMNS - 1);
            row[left_bridge] = descriptor(u16::from(style.support_color));
            row[right_bridge] = descriptor(u16::from(style.accent_color));
        }
        MotifKind::SplitMerge if local_row == 2 || local_row + 3 == plan.rows => {
            let left_bridge = current_lane.saturating_sub(1);
            let right_bridge = (current_lane + 1).min(ROAD_COLUMNS - 1);
            row[left_bridge] = descriptor(u16::from(style.support_color));
            row[right_bridge] = descriptor(u16::from(style.accent_color));
        }
        MotifKind::JumpGap => {
            let gap_start = 6;
            let gap_end = gap_start + profile.jump_gap_rows;
            if local_row == gap_start - 1 {
                row[current_lane] = descriptor(ACCELERATE);
            } else if (gap_start..gap_end).contains(&local_row) {
                *row = [LevelCell::EMPTY; ROAD_COLUMNS];
            }
        }
        MotifKind::NeonGate if local_row == middle => {
            let left = current_lane.saturating_sub(profile.safe_half_width + 1);
            let right = (current_lane + profile.safe_half_width + 1).min(ROAD_COLUMNS - 1);
            row[left] = descriptor(cube_descriptor_raw(0, style.accent_color));
            row[right] = descriptor(cube_descriptor_raw(0, style.accent_color));
        }
        MotifKind::SpeedFork if (middle.saturating_sub(1)..=middle + 1).contains(&local_row) => {
            let risky_lane = if plan.mirrored {
                current_lane.saturating_sub(1)
            } else {
                (current_lane + 1).min(ROAD_COLUMNS - 1)
            };
            row[risky_lane] = descriptor(ACCELERATE);
            if local_row == middle && reward_random.range(3) == 0 {
                row[current_lane] = descriptor(REFILL);
            }
        }
        MotifKind::HazardChicane if local_row >= middle + 3 => {
            row[plan.entry_lane] = descriptor(HAZARD);
            row[current_lane] = descriptor(u16::from(style.dominant_color));
        }
        MotifKind::Recovery if local_row == middle => {
            row[current_lane] = descriptor(REFILL);
        }
        MotifKind::Recovery if plan.definition.max_rows == 24 && local_row == middle + 1 => {
            row[current_lane] = descriptor(ACCELERATE);
        }
        MotifKind::PrismCrown if local_row >= 2 && local_row + 2 < plan.rows => {
            let distance = 1 + local_row % 2;
            let left = current_lane.saturating_sub(distance);
            let right = (current_lane + distance).min(ROAD_COLUMNS - 1);
            if !is_protected_lane(left, previous_lane, current_lane, profile.safe_half_width) {
                row[left] = descriptor(HAZARD);
            }
            if !is_protected_lane(right, previous_lane, current_lane, profile.safe_half_width) {
                row[right] = descriptor(HAZARD);
            }
            row[current_lane] = descriptor(u16::from(style.accent_color));
        }
        MotifKind::TunnelRun if local_row >= 1 && local_row + 1 < plan.rows => {
            add_tunnel_to_route(row, previous_lane, current_lane, profile.safe_half_width);

            let portal_row = local_row == 1 || local_row + 2 == plan.rows;
            if portal_row {
                let left = current_lane.saturating_sub(profile.safe_half_width + 1);
                let right = (current_lane + profile.safe_half_width + 1).min(ROAD_COLUMNS - 1);
                row[left] = descriptor(cube_descriptor_raw(0, style.support_color));
                row[right] = descriptor(cube_descriptor_raw(0, style.support_color));
            }
        }
        MotifKind::Skyway => {
            apply_skyway(row, plan, local_row, current_lane, profile, style);
        }
        MotifKind::AuroraColonnade
        | MotifKind::HelixBeacons
        | MotifKind::ConstellationField
        | MotifKind::PrismFanBoulevard
        | MotifKind::RuinedAqueduct
        | MotifKind::CheckerboardCauseway
        | MotifKind::CrownParade
        | MotifKind::CometTail
        | MotifKind::TripleIslandHop
        | MotifKind::SwitchbackArchipelago
        | MotifKind::HighLowFork
        | MotifKind::DescendingTerraces
        | MotifKind::PadLogicFork
        | MotifKind::OxygenOasisDetour
        | MotifKind::BrakeOrFly
        | MotifKind::CubeHurdleRhythm
        | MotifKind::TunnelLaneWeave
        | MotifKind::EclipseBreakout
        | MotifKind::SuspendedSerpent
        | MotifKind::TwinSkybridges
        | MotifKind::PyramidSummit
        | MotifKind::MeteorShower
        | MotifKind::PortalRoulette
        | MotifKind::SpiralTemple
        | MotifKind::CollapsingSkybridge => {
            apply_sr3_set_piece(row, plan, step, profile, style, reward_random)
        }
        _ => {}
    }
}

fn apply_sr3_set_piece(
    row: &mut [LevelCell; ROAD_COLUMNS],
    plan: MotifPlan,
    step: RouteStep,
    profile: DifficultyProfile,
    style: StyleScript,
    reward_random: &mut SplitMix64,
) {
    if plan.generator_version >= 4 && is_roadless_set_piece(plan.definition.kind) {
        apply_roadless_set_piece(row, plan, step, profile, style);
        return;
    }

    let local_row = step.row_index;
    let lane = step.current_lane;
    let middle = plan.rows / 2;
    let left_edge = 0;
    let right_edge = ROAD_COLUMNS - 1;
    let near_side = if plan.mirrored { left_edge } else { right_edge };
    let far_side = if plan.mirrored { right_edge } else { left_edge };

    match plan.definition.kind {
        MotifKind::AuroraColonnade => {
            if local_row.is_multiple_of(2) {
                row[left_edge] = descriptor(cube_descriptor_raw(0, style.support_color));
                row[right_edge] = descriptor(cube_descriptor_raw(0, style.accent_color));
            }
        }
        MotifKind::HelixBeacons => {
            let beacon_lane = if (local_row / 2).is_multiple_of(2) {
                near_side
            } else {
                far_side
            };
            let height = if local_row % 4 < 2 { 100 } else { 120 };
            row[beacon_lane] = descriptor(platform_descriptor_raw(height, style.accent_color));
        }
        MotifKind::ConstellationField => {
            let star_lane = (local_row * 3 + usize::from(plan.mirrored)) % ROAD_COLUMNS;
            if !is_protected_lane(star_lane, step.previous_lane, lane, profile.safe_half_width) {
                row[star_lane] = descriptor(cube_descriptor_raw(0, style.accent_color));
            }
        }
        MotifKind::PrismFanBoulevard => {
            let fan_width = 1 + local_row.min(plan.rows - 1 - local_row).min(3);
            let left = lane.saturating_sub(fan_width);
            let right = (lane + fan_width).min(right_edge);
            for (fan_lane, cell) in row.iter_mut().enumerate().take(right + 1).skip(left) {
                *cell = descriptor(u16::from(if fan_lane.abs_diff(lane).is_multiple_of(2) {
                    style.dominant_color
                } else {
                    style.support_color
                }));
            }
        }
        MotifKind::RuinedAqueduct => {
            let fragment_lane = if local_row % 4 < 2 {
                near_side
            } else {
                far_side
            };
            row[fragment_lane] = if local_row.is_multiple_of(3) {
                descriptor(TUNNEL)
            } else {
                descriptor(cube_descriptor_raw(0, style.support_color))
            };
        }
        MotifKind::CheckerboardCauseway => {
            for (road_lane, cell) in row.iter_mut().enumerate().take(6).skip(1) {
                let color = if (road_lane + local_row).is_multiple_of(2) {
                    style.dominant_color
                } else {
                    style.support_color
                };
                *cell = descriptor(u16::from(color));
            }
        }
        MotifKind::CrownParade => {
            if local_row % 3 != 1 {
                return;
            }
            for crown_lane in [lane.saturating_sub(2), (lane + 2).min(right_edge)] {
                if !is_protected_lane(
                    crown_lane,
                    step.previous_lane,
                    lane,
                    profile.safe_half_width,
                ) {
                    let height = if local_row.is_multiple_of(2) {
                        120
                    } else {
                        100
                    };
                    row[crown_lane] =
                        descriptor(platform_descriptor_raw(height, style.accent_color));
                }
            }
        }
        MotifKind::CometTail => {
            let sweep = local_row * (ROAD_COLUMNS - 1) / plan.rows.max(1);
            let comet_lane = if plan.mirrored {
                right_edge - sweep
            } else {
                sweep
            };
            if !is_protected_lane(
                comet_lane,
                step.previous_lane,
                lane,
                profile.safe_half_width,
            ) {
                row[comet_lane] = descriptor(cube_descriptor_raw(0, style.accent_color));
            }
        }
        MotifKind::TripleIslandHop => {
            let first_gap = 5;
            let second_gap = 10;
            apply_repeated_jump(row, local_row, lane, profile, [first_gap, second_gap]);
        }
        MotifKind::SwitchbackArchipelago => {
            if local_row % 4 == 2 {
                row[far_side] = descriptor(HAZARD);
            }
        }
        MotifKind::HighLowFork => {
            let deck_lane = side_lane(lane, plan.mirrored, 2);
            if (2..plan.rows - 2).contains(&local_row) {
                row[deck_lane] = descriptor(platform_descriptor_raw(100, style.accent_color));
            }
            if local_row == 2 {
                row[deck_lane] = descriptor(platform_descriptor_raw(100, 10));
            }
        }
        MotifKind::DescendingTerraces => {
            let terrace_lane = side_lane(lane, plan.mirrored, 2);
            let height = if local_row < plan.rows / 3 { 120 } else { 100 };
            if (2..plan.rows - 2).contains(&local_row) {
                row[terrace_lane] =
                    descriptor(platform_descriptor_raw(height, style.support_color));
            }
        }
        MotifKind::PadLogicFork => {
            if (2..plan.rows - 2).contains(&local_row) {
                let boost_lane = side_lane(lane, plan.mirrored, 1);
                let slide_lane = side_lane(lane, !plan.mirrored, 1);
                row[boost_lane] = descriptor(ACCELERATE);
                row[slide_lane] = descriptor(SLIDE);
            }
        }
        MotifKind::OxygenOasisDetour => {
            let oasis_lane = side_lane(lane, plan.mirrored, 2);
            if (2..plan.rows - 2).contains(&local_row) {
                row[oasis_lane] = descriptor(u16::from(style.support_color));
            }
            if local_row == middle {
                row[oasis_lane] = descriptor(REFILL);
            }
        }
        MotifKind::BrakeOrFly => {
            let flying_lane = side_lane(lane, plan.mirrored, 1);
            let braking_lane = side_lane(lane, !plan.mirrored, 1);
            if local_row == 3 {
                row[flying_lane] = descriptor(ACCELERATE);
                row[braking_lane] = descriptor(SLIDE);
            }
            if (6..6 + profile.jump_gap_rows).contains(&local_row) {
                row[flying_lane] = LevelCell::EMPTY;
            }
        }
        MotifKind::CubeHurdleRhythm => {
            if local_row % 3 == 1 {
                let hurdle_lane = side_lane(lane, plan.mirrored, 1 + local_row % 2);
                if !is_protected_lane(
                    hurdle_lane,
                    step.previous_lane,
                    lane,
                    profile.safe_half_width,
                ) {
                    row[hurdle_lane] = descriptor(cube_descriptor_raw(0, style.accent_color));
                }
            }
        }
        MotifKind::TunnelLaneWeave => {
            if (1..plan.rows - 1).contains(&local_row) {
                add_tunnel_to_route(row, step.previous_lane, lane, profile.safe_half_width);
                let blocker_lane = side_lane(lane, local_row.is_multiple_of(2), 2);
                row[blocker_lane] = descriptor(cube_descriptor_raw(0, style.support_color));
            }
        }
        MotifKind::EclipseBreakout => {
            let gap_start = middle + 1;
            if local_row < middle {
                add_tunnel_to_route(row, step.previous_lane, lane, profile.safe_half_width);
            } else if local_row == middle {
                row[lane] = descriptor(ACCELERATE);
            } else if (gap_start..gap_start + profile.jump_gap_rows).contains(&local_row) {
                *row = [LevelCell::EMPTY; ROAD_COLUMNS];
            }
        }
        MotifKind::SuspendedSerpent => {
            let deck_lane = side_lane(lane, (local_row / 3).is_multiple_of(2), 2);
            if (2..plan.rows - 2).contains(&local_row) {
                row[deck_lane] = descriptor(platform_descriptor_raw(100, style.accent_color));
            }
        }
        MotifKind::TwinSkybridges => {
            if (2..plan.rows - 2).contains(&local_row) {
                for bridge_lane in [lane.saturating_sub(2), (lane + 2).min(right_edge)] {
                    row[bridge_lane] =
                        descriptor(platform_descriptor_raw(100, style.support_color));
                }
                if local_row.is_multiple_of(4) {
                    row[if plan.mirrored {
                        lane.saturating_sub(2)
                    } else {
                        (lane + 2).min(right_edge)
                    }] = LevelCell::EMPTY;
                }
            }
        }
        MotifKind::PyramidSummit => {
            let summit_lane = side_lane(lane, plan.mirrored, 2);
            let height = if local_row.abs_diff(middle) <= 2 {
                120
            } else {
                100
            };
            if (2..plan.rows - 2).contains(&local_row) {
                row[summit_lane] = descriptor(platform_descriptor_raw(height, style.accent_color));
            }
        }
        MotifKind::MeteorShower => {
            if local_row.is_multiple_of(2) {
                let meteor_lane = (local_row * 2 + usize::from(plan.mirrored)) % ROAD_COLUMNS;
                if !is_protected_lane(
                    meteor_lane,
                    step.previous_lane,
                    lane,
                    profile.safe_half_width,
                ) {
                    row[meteor_lane] = if local_row.is_multiple_of(4) {
                        descriptor(HAZARD)
                    } else {
                        descriptor(cube_descriptor_raw(0, style.accent_color))
                    };
                }
            }
        }
        MotifKind::PortalRoulette => {
            if (1..plan.rows - 1).contains(&local_row) {
                add_tunnel_to_route(row, step.previous_lane, lane, profile.safe_half_width);
                let option_lane = side_lane(lane, plan.mirrored, 1);
                row[option_lane] = descriptor(if local_row < middle {
                    ACCELERATE
                } else {
                    SLIDE
                });
                if local_row == middle && reward_random.range(2) == 0 {
                    row[lane] = descriptor(REFILL | 0x0100);
                }
            }
        }
        MotifKind::SpiralTemple => {
            let altar_lane = if plan.mirrored { 5 } else { 1 };
            let altar_crosses_route = is_protected_lane(
                altar_lane,
                step.previous_lane,
                lane,
                profile.safe_half_width,
            );
            if local_row.is_multiple_of(3) && !altar_crosses_route {
                let height = if local_row.abs_diff(middle) <= 2 {
                    120
                } else {
                    100
                };
                row[altar_lane] = descriptor(platform_descriptor_raw(height, style.accent_color));
            }
        }
        MotifKind::CollapsingSkybridge => {
            let bridge_lane = side_lane(lane, plan.mirrored, 2);
            if (2..plan.rows - 2).contains(&local_row) && !local_row.is_multiple_of(4) {
                row[bridge_lane] = descriptor(platform_descriptor_raw(100, style.support_color));
            }
            if local_row == 2 {
                row[bridge_lane] = descriptor(platform_descriptor_raw(100, 10));
            }
        }
        _ => unreachable!("only SR3 set pieces reach this painter"),
    }
}

fn apply_roadless_set_piece(
    row: &mut [LevelCell; ROAD_COLUMNS],
    plan: MotifPlan,
    step: RouteStep,
    profile: DifficultyProfile,
    style: StyleScript,
) {
    *row = [LevelCell::EMPTY; ROAD_COLUMNS];
    let local_row = step.row_index;
    let route_height = roadless_route_height(plan, local_row);
    let route_color = if local_row.is_multiple_of(2) {
        style.support_color
    } else {
        style.accent_color
    };

    let needs_flanking_transition = matches!(
        plan.definition.kind,
        MotifKind::CheckerboardCauseway
            | MotifKind::DescendingTerraces
            | MotifKind::SuspendedSerpent
            | MotifKind::TwinSkybridges
    );
    let transition_anchor = if needs_flanking_transition && local_row < ROADLESS_APPROACH_ROWS {
        plan.entry_lane
    } else if needs_flanking_transition && local_row + 6 >= plan.rows {
        plan.exit_lane
    } else if matches!(
        plan.definition.kind,
        MotifKind::SuspendedSerpent | MotifKind::TwinSkybridges
    ) {
        side_lane(step.current_lane, step.current_lane > 3, 1)
    } else {
        step.previous_lane
    };
    paint_roadless_step(
        row,
        transition_anchor,
        step.current_lane,
        route_height,
        route_color,
    );
    if needs_flanking_transition && local_row < ROADLESS_APPROACH_ROWS {
        let branch_lane = roadless_route_lane(plan, 6);
        paint_roadless_step(row, plan.entry_lane, branch_lane, 80, route_color);
    }
    if needs_flanking_transition && local_row + 6 >= plan.rows {
        let left = step.current_lane.saturating_sub(1);
        let right = (step.current_lane + 1).min(ROAD_COLUMNS - 1);
        paint_roadless_step(row, left, right, route_height, route_color);
    }
    let has_bridge_gap = matches!(
        plan.definition.kind,
        MotifKind::SuspendedSerpent | MotifKind::TwinSkybridges
    );
    let landing_start = plan.rows - 6 + profile.jump_gap_rows;
    if has_bridge_gap && local_row >= landing_start {
        let left = plan.entry_lane.saturating_sub(2);
        let right = (plan.entry_lane + 2).min(ROAD_COLUMNS - 1);
        paint_roadless_step(row, left, right, 80, route_color);
    }

    match plan.definition.kind {
        MotifKind::CheckerboardCauseway => {
            for (lane, cell) in row.iter_mut().enumerate() {
                let belongs_to_checkerboard = (lane + local_row / 2).is_multiple_of(2);
                let would_restore_center_road = lane == 3 && cell.is_empty();
                if !belongs_to_checkerboard || !cell.is_empty() || would_restore_center_road {
                    continue;
                }
                let height = match (lane + local_row / 3) % 3 {
                    0 => 80,
                    1 => 100,
                    _ => 120,
                };
                *cell = roadless_surface(height, style.color_for_phase(lane + local_row));
            }
        }
        MotifKind::DescendingTerraces => {
            let flank = side_lane(step.current_lane, step.current_lane > 3, 1);
            if row[flank].is_empty() {
                row[flank] = roadless_surface(route_height, style.dominant_color);
            }
        }
        MotifKind::SuspendedSerpent => {
            let beacon_lane = if roadless_route_lane(plan, 6) < 3 {
                ROAD_COLUMNS - 1
            } else {
                0
            };
            if row[beacon_lane].is_empty() && local_row.is_multiple_of(3) {
                row[beacon_lane] = roadless_surface(120, style.accent_color);
            }
        }
        MotifKind::TwinSkybridges => {
            let opposite_lane = if roadless_route_lane(plan, 6) < 3 {
                ROAD_COLUMNS - 1
            } else {
                0
            };
            if local_row >= 6 && local_row + 3 < plan.rows && !local_row.is_multiple_of(4) {
                row[opposite_lane] = roadless_surface(120, style.dominant_color);
            }
        }
        _ => unreachable!("only roadless set pieces reach this painter"),
    }
}

fn roadless_route_height(plan: MotifPlan, local_row: usize) -> u16 {
    let approach_rows = ROADLESS_APPROACH_ROWS;
    if local_row < approach_rows || local_row + 3 >= plan.rows {
        return 80;
    }

    match plan.definition.kind {
        MotifKind::CheckerboardCauseway | MotifKind::DescendingTerraces => {
            if local_row < approach_rows + 2 {
                100
            } else {
                80
            }
        }
        MotifKind::SuspendedSerpent | MotifKind::TwinSkybridges => 80,
        _ => unreachable!("only roadless set pieces have a roadless height"),
    }
}

fn paint_roadless_step(
    row: &mut [LevelCell; ROAD_COLUMNS],
    previous_lane: usize,
    current_lane: usize,
    height: u16,
    color: u8,
) {
    let first_lane = previous_lane.min(current_lane);
    let last_lane = previous_lane.max(current_lane);
    for cell in &mut row[first_lane..=last_lane] {
        *cell = roadless_surface(height, color);
    }
}

fn roadless_surface(height: u16, color: u8) -> LevelCell {
    if height == 80 {
        descriptor(u16::from(color))
    } else {
        descriptor(platform_descriptor_raw(height, color))
    }
}

fn side_lane(center: usize, use_left: bool, distance: usize) -> usize {
    if use_left {
        center.saturating_sub(distance)
    } else {
        (center + distance).min(ROAD_COLUMNS - 1)
    }
}

fn apply_repeated_jump(
    row: &mut [LevelCell; ROAD_COLUMNS],
    local_row: usize,
    lane: usize,
    profile: DifficultyProfile,
    gap_starts: [usize; 2],
) {
    for gap_start in gap_starts {
        if local_row + 1 == gap_start {
            row[lane] = descriptor(ACCELERATE);
        }
        if (gap_start..gap_start + profile.jump_gap_rows).contains(&local_row) {
            *row = [LevelCell::EMPTY; ROAD_COLUMNS];
        }
    }
}

fn add_tunnel_to_route(
    row: &mut [LevelCell; ROAD_COLUMNS],
    previous_lane: usize,
    current_lane: usize,
    half_width: usize,
) {
    let route_left = current_lane.saturating_sub(half_width);
    let route_right = (current_lane + half_width).min(ROAD_COLUMNS - 1);
    let transition_left = previous_lane.min(current_lane).min(route_left);
    let transition_right = previous_lane.max(current_lane).max(route_right);

    for cell in &mut row[transition_left..=transition_right] {
        if cell.has_tile {
            *cell = descriptor(cell.raw_descriptor | 0x0100);
        }
    }
}

fn apply_skyway(
    row: &mut [LevelCell; ROAD_COLUMNS],
    plan: MotifPlan,
    local_row: usize,
    current_lane: usize,
    profile: DifficultyProfile,
    style: StyleScript,
) {
    const ASCENT_START: usize = 5;
    const UPPER_DECK_START: usize = 10;
    const LANDING_ISLAND_ROWS: usize = 2;
    const EXIT_ROWS: usize = 1;

    if local_row == ASCENT_START - 1 {
        row[current_lane] = descriptor(ACCELERATE);
        return;
    }

    let gap_rows = profile.jump_gap_rows;
    let island_start = plan.rows - EXIT_ROWS - LANDING_ISLAND_ROWS;
    let gap_start = island_start - gap_rows;
    if !(ASCENT_START..island_start + LANDING_ISLAND_ROWS).contains(&local_row) {
        return;
    }

    if (gap_start..island_start).contains(&local_row) {
        *row = [LevelCell::EMPTY; ROAD_COLUMNS];
        return;
    }

    let is_landing_island = local_row >= island_start;
    let deck_height = if local_row >= UPPER_DECK_START && !is_landing_island {
        120
    } else {
        100
    };
    let deck_color = if deck_height == 120 {
        style.accent_color
    } else {
        style.support_color
    };
    let left = current_lane.saturating_sub(profile.safe_half_width);
    let right = (current_lane + profile.safe_half_width).min(ROAD_COLUMNS - 1);
    for cell in &mut row[left..=right] {
        *cell = descriptor(platform_descriptor_raw(deck_height, deck_color));
    }

    let needs_second_jump = local_row == UPPER_DECK_START - 1;
    if needs_second_jump {
        row[current_lane] = descriptor(platform_descriptor_raw(100, 10));
    }
}

fn is_protected_lane(
    lane: usize,
    previous_lane: usize,
    lane_now: usize,
    half_width: usize,
) -> bool {
    let route_left = lane_now.saturating_sub(half_width);
    let route_right = (lane_now + half_width).min(ROAD_COLUMNS - 1);
    let transition_left = previous_lane.min(lane_now);
    let transition_right = previous_lane.max(lane_now);

    (route_left..=route_right).contains(&lane)
        || (transition_left..=transition_right).contains(&lane)
}

fn paint_safe_route(
    row: &mut [LevelCell; ROAD_COLUMNS],
    previous_lane: usize,
    lane: usize,
    half_width: usize,
    color: u8,
) {
    let left = lane.saturating_sub(half_width);
    let right = (lane + half_width).min(ROAD_COLUMNS - 1);
    for cell in &mut row[left..=right] {
        *cell = descriptor(u16::from(color));
    }

    let transition_left = previous_lane.min(lane);
    let transition_right = previous_lane.max(lane);
    for cell in &mut row[transition_left..=transition_right] {
        *cell = descriptor(u16::from(color));
    }
}

fn append_finish(
    rows: &mut Vec<[LevelCell; ROAD_COLUMNS]>,
    start_lane: usize,
    style: StyleScript,
    total_length: usize,
) {
    let approach_rows = FINISH_ROWS - 6;
    for index in 0..approach_rows {
        let lane = if index < approach_rows / 2 {
            start_lane
        } else {
            3
        };
        let mut row = [LevelCell::EMPTY; ROAD_COLUMNS];
        paint_safe_route(
            &mut row,
            start_lane,
            lane,
            1,
            style.color_for_phase(total_length / 4 + index),
        );
        rows.push(row);
    }

    for index in 0..6 {
        let mut row = [LevelCell::EMPTY; ROAD_COLUMNS];
        for cell in &mut row[2..=4] {
            *cell = descriptor(TUNNEL);
        }
        if index % 2 == 0 {
            row[1] = descriptor(cube_descriptor_raw(0, 13));
            row[5] = descriptor(cube_descriptor_raw(0, 13));
        }
        rows.push(row);
    }
}

fn themed_color(base: usize, phase: usize) -> u8 {
    ROAD_COLORS[(base + phase) % ROAD_COLORS.len()]
}

fn cube_descriptor_raw(base_color: u8, cube_color: u8) -> u16 {
    0x0200 | (u16::from(cube_color) << 4) | u16::from(base_color)
}

fn platform_descriptor_raw(height: u16, top_color: u8) -> u16 {
    let height_flag = match height {
        100 => 0x0200,
        120 => 0x0400,
        _ => panic!("unsupported procedural platform height {height}"),
    };
    height_flag | (u16::from(top_color) << 4)
}

fn descriptor(raw: u16) -> LevelCell {
    LevelCell::from_raw_descriptor(raw)
}

fn normalize_generation_id(input: &str) -> Result<Vec<u8>, GenerationIdError> {
    let mut normalized = Vec::with_capacity(14);
    for character in input.chars() {
        if character == '-' || character.is_ascii_whitespace() {
            continue;
        }
        if !character.is_ascii_alphanumeric() {
            return Err(GenerationIdError::InvalidCharacter);
        }
        let character = match character.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        normalized.push(character as u8);
    }
    Ok(normalized)
}

fn encode_base32(mut value: u64, digits: usize) -> String {
    let mut encoded = vec![b'0'; digits];
    for character in encoded.iter_mut().rev() {
        *character = BASE32[(value & 31) as usize];
        value >>= 5;
    }
    String::from_utf8(encoded).expect("Crockford alphabet is ASCII")
}

fn decode_base32(value: &[u8]) -> Result<u64, GenerationIdError> {
    let mut decoded = 0_u64;
    for character in value {
        let Some(index) = BASE32.iter().position(|candidate| candidate == character) else {
            return Err(GenerationIdError::InvalidCharacter);
        };
        decoded = (decoded << 5) | index as u64;
    }
    Ok(decoded)
}

fn crc10_atm(bytes: &[u8]) -> u16 {
    const WIDTH_MASK: u16 = 0x03FF;
    const POLYNOMIAL: u16 = 0x0233;
    let mut crc = 0_u16;
    for byte in bytes {
        crc ^= u16::from(*byte) << 2;
        for _ in 0..8 {
            let top_bit_set = crc & 0x0200 != 0;
            crc = (crc << 1) & WIDTH_MASK;
            if top_bit_set {
                crc ^= POLYNOMIAL;
            }
        }
    }
    crc
}

#[derive(Debug, Clone, Copy)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn for_stream(seed: u64, stream_tag: u64) -> Self {
        let mut mixer = Self::new(seed ^ stream_tag);
        Self::new(mixer.next())
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    fn range(&mut self, upper_bound: u64) -> u64 {
        debug_assert!(upper_bound > 0);
        self.next() % upper_bound
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use skyroads_data::{LevelKind, TouchEffect, ROAD_COLUMNS};

    use crate::{ControllerState, GameplaySession, ShipState};

    use super::{
        difficulty_index, generate_procedural_level, plan_road, GenerationId, MotifKind,
        ProceduralDifficulty, SetPieceFocus, FINISH_ROWS, MOTIFS, SET_PIECES, START_ROWS,
    };

    #[test]
    fn generation_id_round_trips_and_normalizes_human_input() {
        let id = GenerationId::new(0x007A_4B19_D2EF, ProceduralDifficulty::Classic);
        let canonical = id.to_string();

        assert_eq!(GenerationId::from_str(&canonical).unwrap(), id);
        assert_eq!(
            GenerationId::from_str(&canonical.to_ascii_lowercase()).unwrap(),
            id
        );
        assert_eq!(
            GenerationId::from_str(&canonical.replace('-', " ")).unwrap(),
            id
        );

        let legacy = GenerationId::from_str("SR1-C-28T5-CY4T-94").unwrap();
        assert_eq!(
            legacy.with_difficulty(ProceduralDifficulty::Easy).version(),
            1
        );
    }

    #[test]
    fn generation_id_rejects_typing_errors() {
        let id = GenerationId::new(42, ProceduralDifficulty::Easy).to_string();
        let mut mistyped = id.into_bytes();
        let seed_character = mistyped
            .iter()
            .position(|character| *character == b'0')
            .unwrap();
        mistyped[seed_character] = b'2';

        assert!(GenerationId::from_str(std::str::from_utf8(&mistyped).unwrap()).is_err());
    }

    #[test]
    fn same_id_always_generates_the_same_level() {
        let id = GenerationId::new(0x0012_3456_789A, ProceduralDifficulty::Classic);
        assert_eq!(generate_procedural_level(id), generate_procedural_level(id));
    }

    #[test]
    fn sr1_golden_levels_are_stable() {
        for (literal_id, expected_hash) in [
            ("SR1-E-0000-0000-2V", 8_237_451_473_836_760_186_u64),
            ("SR1-C-28T5-CY4T-94", 14_718_310_549_744_874_615_u64),
            ("SR1-H-ZZZZ-ZZZZ-V3", 4_742_048_487_628_060_296_u64),
        ] {
            let id = GenerationId::from_str(literal_id).unwrap();
            let level = generate_procedural_level(id);
            let actual_hash = level_hash(&level);
            assert_eq!(actual_hash, expected_hash, "ID {literal_id}");
        }
    }

    #[test]
    fn sr2_golden_levels_are_stable() {
        for (literal_id, expected_hash) in [
            ("SR2-E-0000-0000-AN", 6_930_780_338_863_169_677_u64),
            ("SR2-C-F95H-KMQF-X9", 8_832_970_714_021_891_862_u64),
            ("SR2-H-ZZZZ-ZZZZ-KD", 6_552_174_076_606_989_051_u64),
        ] {
            let id = GenerationId::from_str(literal_id).unwrap();
            let actual_hash = level_hash(&generate_procedural_level(id));
            assert_eq!(actual_hash, expected_hash, "ID {literal_id}");
        }
    }

    #[test]
    fn sr3_golden_levels_are_stable() {
        for (literal_id, expected_hash) in [
            ("SR3-E-0000-0000-XY", 7_921_431_999_870_134_454_u64),
            ("SR3-C-F95H-KMQF-A2", 3_637_871_947_162_107_020_u64),
            ("SR3-H-ZZZZ-ZZZZ-46", 1_482_197_957_753_541_120_u64),
        ] {
            let id = GenerationId::from_str(literal_id).unwrap();
            let actual_hash = level_hash(&generate_procedural_level(id));
            assert_eq!(actual_hash, expected_hash, "ID {literal_id}");
        }
    }

    #[test]
    fn profiles_have_the_promised_length_resources_and_theme() {
        for (difficulty, length, gravity, fuel, oxygen) in [
            (ProceduralDifficulty::Easy, 128, 7, 225, 180),
            (ProceduralDifficulty::Classic, 176, 8, 180, 90),
            (ProceduralDifficulty::Hard, 224, 10, 150, 60),
        ] {
            let id = GenerationId::new(0x00AB_CDEF_0123, difficulty);
            let level = generate_procedural_level(id);
            assert_eq!(level.kind, LevelKind::Procedural);
            assert_eq!(level.length(), length);
            assert_eq!(level.gravity, gravity);
            assert_eq!(level.fuel, fuel);
            assert_eq!(level.oxygen, oxygen);
            assert_eq!(level.theme.world_index, id.world_index());
            assert_eq!(level.theme.palette_index, id.palette_index());
        }
    }

    #[test]
    fn generated_roads_are_colorful_structured_and_finish_in_a_tunnel() {
        for difficulty in ProceduralDifficulty::ALL {
            for seed in 0..10_000 {
                let id = GenerationId::new(seed, difficulty);
                let level = generate_procedural_level(id);
                let distinct_descriptors = level
                    .cells
                    .iter()
                    .flatten()
                    .map(|cell| cell.raw_descriptor)
                    .collect::<std::collections::BTreeSet<_>>();
                let normal_colors = level
                    .cells
                    .iter()
                    .flatten()
                    .filter(|cell| {
                        cell.has_tile && !cell.has_tunnel && cell.tile_effect == TouchEffect::None
                    })
                    .map(|cell| cell.color_index_low)
                    .collect::<std::collections::BTreeSet<_>>();
                let landmark_count = level
                    .cells
                    .iter()
                    .flatten()
                    .filter(|cell| {
                        cell.cube_height.is_some() || cell.tile_effect != TouchEffect::None
                    })
                    .count();
                let row_patterns = level
                    .cells
                    .iter()
                    .map(|row| row.map(|cell| cell.raw_descriptor))
                    .collect::<std::collections::BTreeSet<_>>();
                let longest_gap = longest_empty_row_run(&level);

                assert!(
                    distinct_descriptors.len() >= 8,
                    "{difficulty:?} seed {seed} lacked color variety"
                );
                assert!(
                    normal_colors.len() >= 3,
                    "{difficulty:?} seed {seed} lacked a color story"
                );
                assert!(
                    landmark_count >= 12,
                    "{difficulty:?} seed {seed} lacked non-empty landmarks"
                );
                assert!(
                    row_patterns.len() >= 12,
                    "{difficulty:?} seed {seed} lacked patterned structure"
                );
                assert!(longest_gap <= difficulty.profile().jump_gap_rows);
                assert!(level.cells.last().unwrap()[2..=4]
                    .iter()
                    .all(|cell| cell.has_tunnel));
                assert!(level.cells.iter().all(|row| row.len() == ROAD_COLUMNS));
                assert!(
                    has_abstract_safe_route(&level),
                    "{difficulty:?} seed {seed} lacked an abstract safe route: {:?}",
                    plan_road(id, id.profile())
                );
            }
        }
    }

    fn has_midroad_tunnel(level: &skyroads_data::Level) -> bool {
        level.cells[..level.cells.len() - FINISH_ROWS]
            .windows(6)
            .any(|rows| {
                rows.iter()
                    .all(|row| row.iter().any(|cell| cell.has_tunnel && cell.has_tile))
            })
    }

    fn has_staircase_jump_to_island(level: &skyroads_data::Level) -> bool {
        let row_height = |row: &[skyroads_data::LevelCell; ROAD_COLUMNS]| {
            row.iter().filter_map(|cell| cell.cube_height).max()
        };
        let mut phase = 0;
        for row in &level.cells[..level.cells.len() - FINISH_ROWS] {
            let height = row_height(row);
            let is_gap = row.iter().all(skyroads_data::LevelCell::is_empty);
            phase = match (phase, height, is_gap) {
                (0, Some(100), _) => 1,
                (1, Some(120), _) => 2,
                (2, _, true) => 3,
                (3, Some(100), _) => return true,
                (1, Some(100), _) | (2, Some(120), _) | (3, _, true) => phase,
                _ => 0,
            };
        }
        false
    }

    #[test]
    fn current_tunnels_and_staircase_jumps_remain_surprises() {
        let levels: Vec<_> = (0..512)
            .map(|seed| {
                generate_procedural_level(GenerationId::new(seed, ProceduralDifficulty::Classic))
            })
            .collect();

        assert!(levels.iter().any(has_midroad_tunnel));
        assert!(levels.iter().any(|level| !has_midroad_tunnel(level)));
        assert!(levels.iter().any(has_staircase_jump_to_island));
        assert!(levels
            .iter()
            .any(|level| !has_staircase_jump_to_island(level)));
    }

    #[test]
    fn current_roads_include_sparse_side_routes_and_vertical_checkerboards() {
        let mut roadless_kinds = std::collections::HashSet::new();

        for difficulty in ProceduralDifficulty::ALL {
            for seed in 0..512 {
                let id = GenerationId::new(seed, difficulty);
                let level = generate_procedural_level(id);
                let mut row_offset = START_ROWS;

                for plan in plan_road(id, id.profile()) {
                    let motif_rows = &level.cells[row_offset..row_offset + plan.rows];
                    row_offset += plan.rows;
                    if !super::is_roadless_set_piece(plan.definition.kind) {
                        continue;
                    }

                    roadless_kinds.insert(plan.definition.kind);
                    let has_empty_center = motif_rows.iter().any(|row| row[3].is_empty());
                    let has_elevated_block = motif_rows
                        .iter()
                        .flatten()
                        .any(|cell| cell.cube_height.is_some());
                    let has_sparse_row = motif_rows
                        .iter()
                        .any(|row| row.iter().filter(|cell| !cell.is_empty()).count() <= 4);

                    assert!(has_empty_center, "{:?}", plan.definition.kind);
                    assert!(has_elevated_block, "{:?}", plan.definition.kind);
                    assert!(has_sparse_row, "{:?}", plan.definition.kind);

                    if plan.definition.kind == MotifKind::CheckerboardCauseway {
                        let heights: std::collections::HashSet<_> = motif_rows
                            .iter()
                            .flatten()
                            .filter_map(|cell| {
                                if cell.has_tile && cell.cube_height.is_none() {
                                    Some(80)
                                } else {
                                    cell.cube_height
                                }
                            })
                            .collect();
                        assert_eq!(heights, std::collections::HashSet::from([80, 100, 120]));
                    }
                }
            }
        }

        assert_eq!(roadless_kinds.len(), 4);
    }

    fn longest_empty_row_run(level: &skyroads_data::Level) -> usize {
        let mut longest = 0;
        let mut current = 0;
        for row in &level.cells {
            if row.iter().all(skyroads_data::LevelCell::is_empty) {
                current += 1;
                longest = longest.max(current);
            } else {
                current = 0;
            }
        }
        longest
    }

    #[test]
    fn sampled_roads_are_finishable_with_real_ship_physics() {
        let boundary_ids = [
            GenerationId::from_str("SR1-E-0000-0000-2V").unwrap(),
            GenerationId::from_str("SR1-C-28T5-CY4T-94").unwrap(),
            GenerationId::from_str("SR1-H-ZZZZ-ZZZZ-V3").unwrap(),
        ];
        let stratified_ids = ProceduralDifficulty::ALL
            .into_iter()
            .flat_map(|difficulty| {
                (0..32).map(move |index| {
                    let seed = (index * 0x1F12_3BB5 + difficulty_index(difficulty)) as u64;
                    GenerationId::new(seed, difficulty)
                })
            });

        for id in boundary_ids.into_iter().chain(stratified_ids) {
            let level = generate_procedural_level(id);
            let reached_finish = autopilot_reaches_finish(level, id);
            assert!(
                reached_finish,
                "real ship physics could not finish {id}: {:?}",
                plan_road(id, id.profile())
            );
        }
    }

    #[test]
    fn motif_plans_cover_the_library_and_honor_join_and_pacing_contracts() {
        let mut kinds = std::collections::HashSet::new();
        let mut saw_mirrored = false;
        let mut saw_unmirrored = false;

        for difficulty in ProceduralDifficulty::ALL {
            for seed in 0..128 {
                let id = GenerationId {
                    version: 2,
                    difficulty,
                    seed,
                };
                let profile = id.profile();
                let plans = plan_road(id, profile);

                assert_eq!(
                    plans.iter().map(|plan| plan.rows).sum::<usize>(),
                    profile.length - START_ROWS - FINISH_ROWS
                );
                assert_eq!(
                    plans.iter().filter(|plan| plan.is_climax).count(),
                    1,
                    "{difficulty:?} seed {seed}: {plans:?}"
                );
                assert!(plans.iter().all(|plan| {
                    plan.rows >= plan.definition.min_rows && plan.rows <= plan.definition.max_rows
                }));
                assert!(plans.iter().filter(|plan| plan.is_climax).all(|plan| {
                    match difficulty {
                        ProceduralDifficulty::Easy => matches!(
                            plan.definition.kind,
                            MotifKind::SpeedFork | MotifKind::NeonGate
                        ),
                        ProceduralDifficulty::Classic | ProceduralDifficulty::Hard => {
                            plan.definition.kind == MotifKind::PrismCrown
                        }
                    }
                }));
                assert!(plans
                    .iter()
                    .any(|plan| plan.definition.kind == MotifKind::JumpGap));
                assert!(plans
                    .windows(2)
                    .all(|pair| pair[0].definition.kind != pair[1].definition.kind));
                assert!(plans
                    .windows(2)
                    .all(|pair| pair[0].exit_lane == pair[1].entry_lane));

                for plan in plans {
                    kinds.insert(plan.definition.kind);
                    saw_mirrored |= plan.mirrored;
                    saw_unmirrored |= !plan.mirrored;
                }
            }
        }

        assert_eq!(kinds.len(), MOTIFS.len());
        assert!(saw_mirrored && saw_unmirrored);
    }

    #[test]
    fn current_surprise_bag_honors_variety_and_pacing_contracts() {
        let mut kinds = std::collections::HashSet::new();
        let mut roads_without_tunnel = 0;
        let mut roads_without_skyway = 0;

        for difficulty in ProceduralDifficulty::ALL {
            let mut difficulty_kinds = std::collections::HashSet::new();
            for seed in 0..512 {
                let id = GenerationId::new(seed, difficulty);
                let profile = id.profile();
                let plans = plan_road(id, profile);
                let set_pieces: Vec<_> = plans
                    .iter()
                    .filter(|plan| super::is_set_piece(plan.definition.kind))
                    .collect();
                let expected_range = match difficulty {
                    ProceduralDifficulty::Easy => 5..=5,
                    ProceduralDifficulty::Classic => 6..=7,
                    ProceduralDifficulty::Hard => 7..=8,
                };

                assert!(expected_range.contains(&set_pieces.len()));
                assert_eq!(
                    plans.iter().map(|plan| plan.rows).sum::<usize>(),
                    profile.length - START_ROWS - FINISH_ROWS
                );
                assert!(plans.windows(2).all(|pair| {
                    pair[0].exit_lane == pair[1].entry_lane
                        && super::is_set_piece(pair[0].definition.kind)
                            != super::is_set_piece(pair[1].definition.kind)
                }));

                let unique: std::collections::HashSet<_> =
                    set_pieces.iter().map(|plan| plan.definition.kind).collect();
                assert_eq!(unique.len(), set_pieces.len());
                let gameplay_count = set_pieces
                    .iter()
                    .filter(|plan| {
                        super::set_piece_focus(plan.definition.kind) != SetPieceFocus::Decoration
                    })
                    .count();
                assert!(gameplay_count >= 2);
                assert!(set_pieces.windows(3).all(|window| {
                    window.iter().any(|plan| {
                        super::set_piece_focus(plan.definition.kind) != SetPieceFocus::Decoration
                    })
                }));

                roads_without_tunnel += usize::from(!plans.iter().any(|plan| {
                    matches!(
                        plan.definition.kind,
                        MotifKind::TunnelLaneWeave | MotifKind::EclipseBreakout
                    )
                }));
                roads_without_skyway += usize::from(
                    !plans
                        .iter()
                        .any(|plan| plan.definition.kind == MotifKind::Skyway),
                );
                difficulty_kinds.extend(unique.iter().copied());
                kinds.extend(unique);
            }
            assert_eq!(difficulty_kinds.len(), SET_PIECES.len());
        }

        assert_eq!(kinds.len(), SET_PIECES.len());
        assert!(roads_without_tunnel > 0);
        assert!(roads_without_skyway > 0);
    }

    fn autopilot_reaches_finish(level: skyroads_data::Level, id: GenerationId) -> bool {
        let roadless_route_lanes = planned_roadless_route_lanes(id);
        let mut session = GameplaySession::new(level);
        for _ in 0..20_000 {
            if session.did_win {
                return true;
            }
            if session.ship.state != ShipState::Alive {
                eprintln!(
                    "autopilot failed: state={:?} row={} x={} y={} zvel={}",
                    session.ship.state,
                    session.ship.z_position,
                    session.ship.x_position,
                    session.ship.y_position,
                    session.ship.z_velocity
                );
                let failed_row = session.ship.z_position.floor() as usize;
                let final_debug_row = (failed_row + 3).min(session.level.length() - 1);
                for row_index in failed_row.saturating_sub(15)..=final_debug_row {
                    eprintln!(
                        "{row_index}: {:?}",
                        session.level.cells[row_index]
                            .iter()
                            .map(|cell| cell.raw_descriptor)
                            .collect::<Vec<_>>()
                    );
                }
                return false;
            }

            let row_index = session.ship.z_position.floor().max(0.0) as usize;
            let current_lane = lane_for_x_position(session.ship.x_position);
            let roadless_target = roadless_route_lanes.get(row_index + 2).copied().flatten();
            let target_lane = roadless_target
                .unwrap_or_else(|| next_safe_lane(&session.level, row_index, current_lane));
            let target_x = skyroads_data::LEVEL_CENTER_X
                + (target_lane as f64 - 3.0) * skyroads_data::LEVEL_TILE_STRIDE_X;
            let turn_input = if session.ship.x_position < target_x - 3.0 {
                1
            } else if session.ship.x_position > target_x + 3.0 {
                -1
            } else {
                0
            };
            let platform_lookahead = if roadless_target.is_some() { 1.5 } else { 0.9 };
            let gap_lookahead = if roadless_target.is_some() { 1.5 } else { 0.65 };
            let jump_input = jump_is_close(
                &session,
                row_index,
                target_lane,
                platform_lookahead,
                gap_lookahead,
            );
            session.run_frame(ControllerState::new(turn_input, 1, jump_input));
        }
        false
    }

    fn planned_roadless_route_lanes(id: GenerationId) -> Vec<Option<usize>> {
        let profile = id.profile();
        let plans = plan_road(id, profile);
        let mut lanes = vec![None; START_ROWS];
        for plan in plans.iter().copied() {
            if super::is_roadless_set_piece(plan.definition.kind) && id.version >= 4 {
                lanes.extend(
                    (0..plan.rows).map(|local_row| Some(super::route_lane(plan, local_row))),
                );
            } else {
                lanes.extend(std::iter::repeat_n(None, plan.rows));
            }
        }
        lanes.extend(std::iter::repeat_n(None, FINISH_ROWS));
        lanes
    }

    fn lane_for_x_position(x_position: f64) -> usize {
        let lane_offset = ((x_position - skyroads_data::LEVEL_CENTER_X)
            / skyroads_data::LEVEL_TILE_STRIDE_X)
            .round() as isize;
        (3 + lane_offset).clamp(0, ROAD_COLUMNS as isize - 1) as usize
    }

    fn next_safe_lane(
        level: &skyroads_data::Level,
        row_index: usize,
        current_lane: usize,
    ) -> usize {
        let is_safe = |cell: &skyroads_data::LevelCell| {
            let route_effect = matches!(
                cell.tile_effect,
                TouchEffect::None | TouchEffect::Accelerate | TouchEffect::RefillOxygen
            );
            cell.has_tile && route_effect && cell.cube_height.is_none()
        };

        for lookahead in 1..=4 {
            let Some(row) = level.row(row_index + lookahead) else {
                break;
            };
            if row.iter().all(skyroads_data::LevelCell::is_empty) || is_safe(&row[current_lane]) {
                continue;
            }

            return (0..ROAD_COLUMNS)
                .filter(|lane| {
                    let target_is_safe_now = level
                        .row(row_index)
                        .is_some_and(|current_row| is_safe(&current_row[*lane]));
                    target_is_safe_now && is_safe(&row[*lane])
                })
                .min_by_key(|lane| lane.abs_diff(current_lane))
                .unwrap_or(current_lane);
        }
        current_lane
    }

    fn jump_is_close(
        session: &GameplaySession,
        row_index: usize,
        target_lane: usize,
        platform_lookahead: f64,
        gap_lookahead: f64,
    ) -> bool {
        for lookahead in 1..=3 {
            let Some(row) = session.level.row(row_index + lookahead) else {
                return false;
            };
            let distance = (row_index + lookahead) as f64 - session.ship.z_position;
            let is_gap = row.iter().all(skyroads_data::LevelCell::is_empty);
            if is_gap && distance <= gap_lookahead {
                return true;
            }

            let higher_platform = row[target_lane]
                .cube_height
                .is_some_and(|height| f64::from(height) > session.ship.y_position + 0.5);
            if higher_platform && distance <= platform_lookahead {
                return true;
            }
        }
        false
    }

    fn has_abstract_safe_route(level: &skyroads_data::Level) -> bool {
        const HEIGHTS: [u16; 3] = [80, 100, 120];
        let safe_surface_height = |cell: &skyroads_data::LevelCell| {
            if let Some(height) = cell.cube_height {
                (cell.cube_effect != TouchEffect::Kill).then_some(height)
            } else {
                (cell.has_tile && cell.tile_effect != TouchEffect::Kill).then_some(80)
            }
        };
        const MAX_GAP_ROWS: usize = 2;
        let mut reachable = [[[false; MAX_GAP_ROWS + 1]; 3]; ROAD_COLUMNS];
        reachable[3][0][0] = true;

        for row in &level.cells[3..] {
            let row_is_jump_gap = row.iter().all(skyroads_data::LevelCell::is_empty);
            let mut next = [[[false; MAX_GAP_ROWS + 1]; 3]; ROAD_COLUMNS];
            for lane in 0..ROAD_COLUMNS {
                let first_previous = lane.saturating_sub(1);
                let last_previous = (lane + 1).min(ROAD_COLUMNS - 1);
                for previous in &reachable[first_previous..=last_previous] {
                    for (height_index, gaps_at_height) in previous.iter().enumerate() {
                        for (gap_rows, was_reachable) in gaps_at_height.iter().enumerate() {
                            if !was_reachable {
                                continue;
                            }
                            if let Some(surface_height) = safe_surface_height(&row[lane]) {
                                let surface_index = HEIGHTS
                                    .iter()
                                    .position(|height| *height == surface_height)
                                    .expect("procedural routes use known platform heights");
                                let can_reach_height = surface_index <= height_index + 1;
                                if can_reach_height {
                                    next[lane][surface_index][0] = true;
                                }
                            } else if row_is_jump_gap && gap_rows < MAX_GAP_ROWS {
                                next[lane][height_index][gap_rows + 1] = true;
                            }
                        }
                    }
                }
            }
            reachable = next;
            if !reachable
                .iter()
                .flatten()
                .flatten()
                .any(|reachable| *reachable)
            {
                return false;
            }
        }

        reachable[2..=4]
            .iter()
            .any(|lane| lane.iter().flatten().any(|reachable| *reachable))
    }

    fn level_hash(level: &skyroads_data::Level) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut add_bytes = |bytes: &[u8]| {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
            }
        };
        add_bytes(&(level.cells.len() as u64).to_le_bytes());
        add_bytes(&[match level.kind {
            LevelKind::Demo => 0,
            LevelKind::Campaign => 1,
            LevelKind::Procedural => 2,
        }]);
        add_bytes(level.name.as_bytes());
        add_bytes(&level.gravity.to_le_bytes());
        add_bytes(&level.fuel.to_le_bytes());
        add_bytes(&level.oxygen.to_le_bytes());
        add_bytes(&(level.theme.world_index as u64).to_le_bytes());
        add_bytes(&(level.theme.palette_index as u64).to_le_bytes());
        for cell in level.cells.iter().flatten() {
            add_bytes(&cell.raw_descriptor.to_le_bytes());
        }
        hash
    }
}
