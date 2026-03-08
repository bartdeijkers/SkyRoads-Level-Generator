use skyroads_data::{
    DemoInput, DemoRecording, Level, TouchEffect, GROUND_Y, LEVEL_CENTER_X, LEVEL_MAX_X,
    LEVEL_MIN_X,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControllerState {
    pub turn_input: i8,
    pub accel_input: i8,
    pub jump_input: bool,
}

impl ControllerState {
    pub const NEUTRAL: Self = Self {
        turn_input: 0,
        accel_input: 0,
        jump_input: false,
    };

    pub fn new(turn_input: i8, accel_input: i8, jump_input: bool) -> Self {
        assert!((-1..=1).contains(&turn_input));
        assert!((-1..=1).contains(&accel_input));
        Self {
            turn_input,
            accel_input,
            jump_input,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipState {
    Alive,
    Exploded,
    Fallen,
    OutOfFuel,
    OutOfOxygen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameplayEvent {
    ShipBumpedWall,
    ShipExploded,
    ShipBounced,
    ShipRefilled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ship {
    pub x_position: f64,
    pub y_position: f64,
    pub z_position: f64,
    pub slide_amount: f64,
    pub sliding_accel: i16,
    pub x_movement_base: f64,
    pub y_velocity: f64,
    pub z_velocity: f64,
    pub fuel_remaining: f64,
    pub oxygen_remaining: f64,
    pub offset_at_which_not_inside_tile: i16,
    pub is_on_ground: bool,
    pub is_going_up: bool,
    pub has_run_jump_o_master: bool,
    pub jump_o_master_velocity_delta: f64,
    pub jump_o_master_in_use: bool,
    pub jumped_from_y_position: f64,
    pub state: ShipState,
}

impl Ship {
    pub fn new() -> Self {
        Self {
            x_position: LEVEL_CENTER_X,
            y_position: GROUND_Y,
            z_position: 3.0,
            slide_amount: 0.0,
            sliding_accel: 0,
            x_movement_base: 0.0,
            y_velocity: 0.0,
            z_velocity: 0.0,
            fuel_remaining: 0x7530 as f64,
            oxygen_remaining: 0x7530 as f64,
            offset_at_which_not_inside_tile: 0,
            is_on_ground: true,
            is_going_up: false,
            has_run_jump_o_master: false,
            jump_o_master_velocity_delta: 0.0,
            jump_o_master_in_use: false,
            jumped_from_y_position: 0.0,
            state: ShipState::Alive,
        }
    }

    fn sanitize_parameters(&mut self) {
        self.x_position = round16_nearest(self.x_position);
        self.y_position = round16_nearest(self.y_position);
        self.z_position = round32_nearest(self.z_position);
    }

    fn is_different_height(&self, other: &Self) -> bool {
        (other.y_position - self.y_position).abs() > 0.01
    }

    fn update(
        &mut self,
        level: &Level,
        expected: &mut Self,
        controls: ControllerState,
    ) -> Vec<GameplayEvent> {
        let mut events = Vec::new();
        self.sanitize_parameters();
        let can_control = self.state == ShipState::Alive;

        let cell = level.get_cell(self.x_position, self.y_position, self.z_position);
        let is_above_nothing = cell.is_empty();
        let touch_effect = self.get_touch_effect(cell);
        let is_on_sliding_tile = touch_effect == TouchEffect::Slide;
        let is_on_decel_pad = touch_effect == TouchEffect::Decelerate;

        self.apply_touch_effect(touch_effect, &mut events);
        self.update_y_velocity(expected, level, &mut events);
        self.update_z_velocity(can_control, controls.accel_input);
        self.update_x_velocity(
            can_control,
            controls.turn_input,
            is_on_sliding_tile,
            is_above_nothing,
        );
        self.update_jump(can_control, is_above_nothing, controls.jump_input, level);
        self.update_jump_o_master(controls, level);
        self.update_gravity(level.gravity_acceleration());

        *expected = *self;
        expected.attempt_motion(is_on_decel_pad);
        expected.sanitize_parameters();
        self.move_to(expected, level);
        self.sanitize_parameters();
        expected.sanitize_parameters();
        self.handle_bumps(expected, level, &mut events);
        self.handle_collision(expected, &mut events);
        self.handle_slide_collision(expected);
        self.handle_bounce(expected, level);
        self.handle_oxygen_and_fuel(level);
        self.handle_fall_below_ground();

        events
    }

    fn get_touch_effect(&self, cell: skyroads_data::LevelCell) -> TouchEffect {
        if !self.is_on_ground {
            return TouchEffect::None;
        }
        if self.y_position.floor() == GROUND_Y && cell.has_tile {
            cell.tile_effect
        } else if self.y_position.floor() > GROUND_Y
            && cell.cube_height == Some(self.y_position.floor() as u16)
        {
            cell.cube_effect
        } else {
            TouchEffect::None
        }
    }

    fn apply_touch_effect(&mut self, effect: TouchEffect, events: &mut Vec<GameplayEvent>) {
        match effect {
            TouchEffect::Accelerate => self.z_velocity += 0x12F as f64 / 0x10000 as f64,
            TouchEffect::Decelerate => self.z_velocity -= 0x12F as f64 / 0x10000 as f64,
            TouchEffect::Kill => {
                if self.state != ShipState::Exploded {
                    events.push(GameplayEvent::ShipExploded);
                }
                self.state = ShipState::Exploded;
            }
            TouchEffect::RefillOxygen => {
                if self.state == ShipState::Alive {
                    if self.fuel_remaining < 0x6978 as f64 || self.oxygen_remaining < 0x6978 as f64
                    {
                        events.push(GameplayEvent::ShipRefilled);
                    }
                    self.fuel_remaining = 0x7530 as f64;
                    self.oxygen_remaining = 0x7530 as f64;
                }
            }
            TouchEffect::Slide | TouchEffect::None => {}
        }
        self.clamp_global_z_velocity();
    }

    fn update_y_velocity(
        &mut self,
        expected: &Self,
        level: &Level,
        events: &mut Vec<GameplayEvent>,
    ) {
        if self.is_different_height(expected) {
            if self.slide_amount == 0.0 || self.offset_at_which_not_inside_tile >= 2 {
                let yvel = self.y_velocity.abs();
                if yvel > (level.gravity as f64 * 0x104 as f64 / 8.0 / 0x80 as f64) {
                    if self.y_velocity < 0.0 {
                        events.push(GameplayEvent::ShipBounced);
                    }
                    self.y_velocity = -0.5 * self.y_velocity;
                } else {
                    self.y_velocity = 0.0;
                }
            } else {
                self.y_velocity = 0.0;
            }
        }
    }

    fn update_z_velocity(&mut self, can_control: bool, accel_input: i8) {
        self.z_velocity +=
            if can_control { accel_input as f64 } else { 0.0 } * 0x4B as f64 / 0x10000 as f64;
        self.clamp_global_z_velocity();
    }

    fn update_x_velocity(
        &mut self,
        can_control: bool,
        turn_input: i8,
        is_on_sliding_tile: bool,
        is_above_nothing: bool,
    ) {
        if !is_on_sliding_tile {
            let can_control_1 = (self.is_going_up || is_above_nothing)
                && self.x_movement_base == 0.0
                && self.y_velocity > 0.0
                && (self.y_position - self.jumped_from_y_position) < 30.0;
            let can_control_2 = !self.is_going_up && !is_above_nothing;
            if can_control_1 || can_control_2 {
                self.x_movement_base = if can_control {
                    turn_input as f64 * 0x1D as f64 / 0x80 as f64
                } else {
                    0.0
                };
            }
        }
    }

    fn update_jump(
        &mut self,
        can_control: bool,
        is_above_nothing: bool,
        jump_input: bool,
        level: &Level,
    ) {
        if !self.is_going_up
            && !is_above_nothing
            && jump_input
            && level.gravity < 0x14
            && can_control
        {
            self.y_velocity = 0x480 as f64 / 0x80 as f64;
            self.is_going_up = true;
            self.jumped_from_y_position = self.y_position;
        }
    }

    fn update_jump_o_master(&mut self, controls: ControllerState, level: &Level) {
        if self.is_going_up && !self.has_run_jump_o_master && self.y_position >= 110.0 {
            self.run_jump_o_master(controls, level);
            self.has_run_jump_o_master = true;
        }
    }

    fn update_gravity(&mut self, gravity_acceleration: f64) {
        if self.y_position >= 0x28 as f64 {
            self.y_velocity += gravity_acceleration;
            self.y_velocity = s_floor(self.y_velocity * 0x80 as f64) / 0x80 as f64;
        } else if self.y_velocity > -(105.0 / 0x80 as f64) {
            self.y_velocity = -(105.0 / 0x80 as f64);
        }
    }

    fn attempt_motion(&mut self, on_decel_pad: bool) {
        let is_dead = self.state != ShipState::Alive;
        let mut motion_vel = self.z_velocity;
        if !on_decel_pad {
            motion_vel += 0x618 as f64 / 0x10000 as f64;
        }
        let x_motion = s_floor(self.x_movement_base * 0x80 as f64)
            * s_floor(motion_vel * 0x10000 as f64)
            / 0x10000 as f64
            + self.slide_amount;
        if !is_dead {
            self.x_position += x_motion;
            self.y_position += self.y_velocity;
            self.z_position += self.z_velocity;
        }
    }

    fn move_to(&mut self, dest: &Self, level: &Level) {
        if self.x_position == dest.x_position
            && self.y_position == dest.y_position
            && self.z_position == dest.z_position
        {
            return;
        }

        let mut fake: Self;
        let mut iter = 1usize;
        for step in 1..=5 {
            fake = *self;
            fake.interp(dest, step as f64 / 5.0);
            if level.is_inside_tile(fake.x_position, fake.y_position, fake.z_position) {
                iter = step;
                break;
            }
            iter = step + 1;
        }

        let percent = (iter.saturating_sub(1)) as f64 / 5.0;
        self.interp(dest, percent);

        let mut z_gran = 0x1000 as f64 / 0x10000 as f64;
        while z_gran != 0.0 {
            fake = *self;
            fake.z_position += z_gran;
            if dest.z_position - self.z_position >= z_gran
                && !level.is_inside_tile(fake.x_position, fake.y_position, fake.z_position)
            {
                self.z_position = fake.z_position;
            } else {
                z_gran /= 16.0;
                z_gran = floor32(z_gran);
            }
        }
        self.z_position = floor32(self.z_position);

        let mut x_gran = if dest.x_position > self.x_position {
            0x7D as f64 / 0x80 as f64
        } else {
            -(0x7D as f64 / 0x80 as f64)
        };
        while x_gran.abs() > 0.0 {
            fake = *self;
            fake.x_position += x_gran;
            if (dest.x_position - self.x_position).abs() >= x_gran.abs()
                && !level.is_inside_tile(fake.x_position, fake.y_position, fake.z_position)
            {
                self.x_position = fake.x_position;
            } else {
                x_gran = s_floor(x_gran / 5.0 * 0x80 as f64) / 0x80 as f64;
            }
        }
        self.x_position = floor16(self.x_position);

        let mut y_gran = if dest.y_position > self.y_position {
            0x7D as f64 / 0x80 as f64
        } else {
            -(0x7D as f64 / 0x80 as f64)
        };
        while y_gran.abs() > 0.0 {
            fake = *self;
            fake.y_position += y_gran;
            if (dest.y_position - self.y_position).abs() >= y_gran.abs()
                && !level.is_inside_tile(fake.x_position, fake.y_position, fake.z_position)
            {
                self.y_position = fake.y_position;
            } else {
                y_gran = s_floor(y_gran / 5.0 * 0x80 as f64) / 0x80 as f64;
            }
        }
        self.y_position = floor16(self.y_position);
    }

    fn handle_bumps(
        &mut self,
        expected: &mut Self,
        level: &Level,
        events: &mut Vec<GameplayEvent>,
    ) {
        let mut moved_ship = *self;
        moved_ship.z_position = expected.z_position;
        if self.z_position != expected.z_position
            && level.is_inside_tile(
                moved_ship.x_position,
                moved_ship.y_position,
                moved_ship.z_position,
            )
        {
            let bump_off = 0x3A0 as f64 / 0x80 as f64;
            moved_ship = *self;
            moved_ship.x_position = self.x_position - bump_off;
            moved_ship.z_position = expected.z_position;
            if !level.is_inside_tile(
                moved_ship.x_position,
                moved_ship.y_position,
                moved_ship.z_position,
            ) {
                self.x_position = moved_ship.x_position;
                expected.z_position = self.z_position;
                events.push(GameplayEvent::ShipBumpedWall);
            } else {
                moved_ship.x_position = self.x_position + bump_off;
                if !level.is_inside_tile(
                    moved_ship.x_position,
                    moved_ship.y_position,
                    moved_ship.z_position,
                ) {
                    self.x_position = moved_ship.x_position;
                    expected.z_position = self.z_position;
                    events.push(GameplayEvent::ShipBumpedWall);
                }
            }
        }
    }

    fn handle_collision(&mut self, expected: &Self, events: &mut Vec<GameplayEvent>) {
        if (self.z_position - expected.z_position).abs() > 0.01 {
            if self.z_velocity < (1.0 / 3.0) * (0x2AAA as f64 / 0x10000 as f64) {
                self.z_velocity = 0.0;
                events.push(GameplayEvent::ShipBumpedWall);
            } else if self.state != ShipState::Exploded {
                self.state = ShipState::Exploded;
                events.push(GameplayEvent::ShipExploded);
            }
        }
    }

    fn handle_slide_collision(&mut self, expected: &mut Self) {
        if (self.x_position - expected.x_position).abs() > 0.01 {
            self.x_movement_base = 0.0;
            if self.slide_amount != 0.0 {
                expected.x_position = self.x_position;
                self.slide_amount = 0.0;
            }
            self.z_velocity -= 0x97 as f64 / 0x10000 as f64;
            self.clamp_global_z_velocity();
        }
    }

    fn handle_bounce(&mut self, expected: &Self, level: &Level) {
        self.is_on_ground = false;
        if self.y_velocity < 0.0 && expected.y_position != self.y_position {
            self.z_velocity += self.jump_o_master_velocity_delta;
            self.jump_o_master_velocity_delta = 0.0;
            self.has_run_jump_o_master = false;
            self.jump_o_master_in_use = false;
            self.is_going_up = false;
            self.is_on_ground = true;
            self.sliding_accel = 0;

            let mut moved_ship: Self;
            for i in 1..=0xE {
                moved_ship = *self;
                moved_ship.x_position += i as f64;
                moved_ship.y_position -= 1.0 / 0x80 as f64;
                if !level.is_inside_tile(
                    moved_ship.x_position,
                    moved_ship.y_position,
                    moved_ship.z_position,
                ) {
                    self.sliding_accel += 1;
                    self.offset_at_which_not_inside_tile = i;
                    break;
                }
            }

            for i in 1..=0xE {
                moved_ship = *self;
                moved_ship.x_position -= i as f64;
                moved_ship.y_position -= 1.0 / 0x80 as f64;
                if !level.is_inside_tile(
                    moved_ship.x_position,
                    moved_ship.y_position,
                    moved_ship.z_position,
                ) {
                    self.sliding_accel -= 1;
                    self.offset_at_which_not_inside_tile = i;
                    break;
                }
            }

            if self.sliding_accel != 0 {
                self.slide_amount += 0x11 as f64 * self.sliding_accel as f64 / 0x80 as f64;
            } else {
                self.slide_amount = 0.0;
            }
        }
    }

    fn handle_oxygen_and_fuel(&mut self, level: &Level) {
        self.oxygen_remaining -= 0x7530 as f64 / (0x24 as f64 * level.oxygen as f64);
        if self.oxygen_remaining <= 0.0 {
            self.oxygen_remaining = 0.0;
            self.state = ShipState::OutOfOxygen;
        }

        self.fuel_remaining -= self.z_velocity * 0x7530 as f64 / level.fuel as f64;
        if self.fuel_remaining <= 0.0 {
            self.fuel_remaining = 0.0;
            self.state = ShipState::OutOfFuel;
        }
    }

    fn handle_fall_below_ground(&mut self) {
        if self.state == ShipState::Alive && self.y_position < GROUND_Y {
            self.state = ShipState::Fallen;
            self.y_position = self.y_position.min(GROUND_Y);
            self.y_velocity = 0.0;
            self.z_velocity = 0.0;
            self.x_movement_base = 0.0;
            self.slide_amount = 0.0;
            self.jump_o_master_velocity_delta = 0.0;
            self.jump_o_master_in_use = false;
            self.has_run_jump_o_master = false;
            self.is_on_ground = false;
            self.is_going_up = false;
        }
    }

    fn interp(&mut self, dest: &Self, percent: f64) {
        self.x_position = floor16((dest.x_position - self.x_position) * percent + self.x_position);
        self.y_position = floor16((dest.y_position - self.y_position) * percent + self.y_position);
        self.z_position = floor32((dest.z_position - self.z_position) * percent + self.z_position);
    }

    fn run_jump_o_master(&mut self, controls: ControllerState, level: &Level) {
        if self.will_land_on_tile(controls, *self, level) {
            return;
        }

        let z_velocity = self.z_velocity;
        let x_movement_base = self.x_movement_base;
        let mut success_index = None;
        for i in 1..=6 {
            self.x_movement_base = floor16(x_movement_base + x_movement_base * i as f64 / 10.0);
            if self.will_land_on_tile(controls, *self, level) {
                success_index = Some(i);
                break;
            }

            self.x_movement_base = floor16(x_movement_base - x_movement_base * i as f64 / 10.0);
            if self.will_land_on_tile(controls, *self, level) {
                success_index = Some(i);
                break;
            }

            self.x_movement_base = x_movement_base;

            let zv2 = floor32(z_velocity + z_velocity * i as f64 / 10.0);
            self.z_velocity = self.clamp_z_velocity(zv2);
            if self.z_velocity == zv2 && self.will_land_on_tile(controls, *self, level) {
                success_index = Some(i);
                break;
            }

            let zv2 = floor32(z_velocity - z_velocity * i as f64 / 10.0);
            self.z_velocity = self.clamp_z_velocity(zv2);
            if self.z_velocity == zv2 && self.will_land_on_tile(controls, *self, level) {
                success_index = Some(i);
                break;
            }

            self.z_velocity = z_velocity;
        }

        self.jump_o_master_velocity_delta = z_velocity - self.z_velocity;
        if success_index.is_some() {
            self.jump_o_master_in_use = true;
        }
    }

    fn is_on_nothing(level: &Level, x_position: f64, z_position: f64) -> bool {
        let cell = level.get_cell(x_position, 0.0, z_position);
        cell.is_empty() || (cell.has_tile && cell.tile_effect == TouchEffect::Kill)
    }

    fn will_land_on_tile(&self, controls: ControllerState, ship: Self, level: &Level) -> bool {
        let mut x_pos = ship.x_position;
        let mut y_pos = ship.y_position;
        let mut z_pos = ship.z_position;
        let x_velocity = ship.x_movement_base;
        let mut y_velocity = ship.y_velocity;
        let mut z_velocity = ship.z_velocity;

        loop {
            let current_x = x_pos;
            let current_slide_amount = self.slide_amount;
            let current_z = z_pos;

            y_velocity += level.gravity_acceleration();
            z_pos += z_velocity;

            let x_rate = z_velocity + 0x618 as f64 / 0x10000 as f64;
            let x_mov = x_velocity * x_rate * 128.0 + current_slide_amount;
            x_pos += x_mov;
            if !(LEVEL_MIN_X..=LEVEL_MAX_X).contains(&x_pos) {
                return false;
            }

            y_pos += y_velocity;
            z_velocity = self.clamp_z_velocity(
                z_velocity + controls.accel_input as f64 * 0x4B as f64 / 0x10000 as f64,
            );

            if y_pos <= GROUND_Y {
                return !Self::is_on_nothing(level, current_x, current_z)
                    && !Self::is_on_nothing(level, x_pos, z_pos);
            }
        }
    }

    fn clamp_global_z_velocity(&mut self) {
        self.z_velocity = self.clamp_z_velocity(self.z_velocity);
    }

    fn clamp_z_velocity(&self, z_velocity: f64) -> f64 {
        z_velocity.clamp(0.0, 0x2AAA as f64 / 0x10000 as f64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameSnapshot {
    pub x_position: f64,
    pub y_position: f64,
    pub z_position: f64,
    pub z_velocity: f64,
    pub craft_state: ShipState,
    pub oxygen_percent: f64,
    pub fuel_percent: f64,
    pub jump_o_master_in_use: bool,
    pub jump_o_master_velocity_delta: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameplayFrameResult {
    pub frame_index: usize,
    pub controls: ControllerState,
    pub snapshot: GameSnapshot,
    pub events: Vec<GameplayEvent>,
    pub did_win: bool,
    pub road_row_index: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameplaySession {
    pub level: Level,
    pub ship: Ship,
    pub expected_ship: Ship,
    pub did_win: bool,
    pub(crate) last_controls: ControllerState,
    pub(crate) death_frame_index: Option<usize>,
    pub(crate) frame_index: usize,
}

impl GameplaySession {
    pub fn new(level: Level) -> Self {
        let ship = Ship::new();
        Self {
            level,
            ship,
            expected_ship: ship,
            did_win: false,
            last_controls: ControllerState::NEUTRAL,
            death_frame_index: None,
            frame_index: 0,
        }
    }

    pub fn frame_index(&self) -> usize {
        self.frame_index
    }

    pub fn run_frame(&mut self, controls: ControllerState) -> GameplayFrameResult {
        self.last_controls = controls;
        let previous_state = self.ship.state;
        let events = self
            .ship
            .update(&self.level, &mut self.expected_ship, controls);
        if previous_state == ShipState::Alive
            && self.ship.state != ShipState::Alive
            && self.death_frame_index.is_none()
        {
            self.death_frame_index = Some(self.frame_index);
        }
        if self.ship.z_position >= self.level.length() as f64 - 0.5
            && self.level.is_inside_tunnel(
                self.ship.x_position,
                self.ship.y_position,
                self.ship.z_position,
            )
        {
            self.did_win = true;
        }

        let result = GameplayFrameResult {
            frame_index: self.frame_index,
            controls,
            snapshot: GameSnapshot {
                x_position: self.ship.x_position,
                y_position: self.ship.y_position,
                z_position: self.ship.z_position,
                z_velocity: self.ship.z_velocity + self.ship.jump_o_master_velocity_delta,
                craft_state: self.ship.state,
                oxygen_percent: self.ship.oxygen_remaining / 0x7530 as f64,
                fuel_percent: self.ship.fuel_remaining / 0x7530 as f64,
                jump_o_master_in_use: self.ship.jump_o_master_in_use,
                jump_o_master_velocity_delta: self.ship.jump_o_master_velocity_delta,
            },
            events,
            did_win: self.did_win,
            road_row_index: self.ship.z_position.floor().max(0.0) as usize,
        };
        self.frame_index += 1;
        result
    }

    pub fn run_demo_frame(&mut self, demo: &DemoRecording) -> GameplayFrameResult {
        self.run_frame(controller_state_from_demo_input(
            sample_demo_input_for_ship(demo, self.ship),
        ))
    }
}

pub fn sample_demo_input_for_ship<'a>(
    demo: &'a DemoRecording,
    ship: Ship,
) -> Option<&'a DemoInput> {
    demo.entries
        .get((ship.z_position * (0x10000 as f64 / 0x0666 as f64)).floor() as usize)
}

pub fn controller_state_from_demo_input(input: Option<&DemoInput>) -> ControllerState {
    match input {
        Some(input) => {
            ControllerState::new(input.left_right, input.accelerate_decelerate, input.jump)
        }
        None => ControllerState::NEUTRAL,
    }
}

fn floor16(value: f64) -> f64 {
    (value * 0x80 as f64).floor() / 0x80 as f64
}

fn floor32(value: f64) -> f64 {
    (value * 0x10000 as f64).floor() / 0x10000 as f64
}

fn round16_nearest(value: f64) -> f64 {
    (value * 0x80 as f64).round() / 0x80 as f64
}

fn round32_nearest(value: f64) -> f64 {
    (value * 0x10000 as f64).round() / 0x10000 as f64
}

fn s_floor(value: f64) -> f64 {
    if value >= 0.0 {
        value.floor()
    } else {
        -(-value).floor()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use skyroads_data::{level_from_road_entry, load_demo_rec_path, load_roads_lzs_path, GROUND_Y};

    use super::{
        controller_state_from_demo_input, sample_demo_input_for_ship, ControllerState,
        GameplaySession, ShipState,
    };

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn demo_input_maps_to_controller_state() {
        let demo = load_demo_rec_path(repo_root().join("DEMO.REC")).unwrap();
        let ship = super::Ship::new();
        let input = sample_demo_input_for_ship(&demo, ship).unwrap();
        let controls = controller_state_from_demo_input(Some(input));
        assert_eq!(input.index, 120);
        assert_eq!(controls, ControllerState::new(0, 1, false));
    }

    #[test]
    fn first_demo_frame_matches_expected_fall_state() {
        let roads = load_roads_lzs_path(repo_root().join("ROADS.LZS")).unwrap();
        let level = level_from_road_entry(&roads.roads[0]);
        let demo = load_demo_rec_path(repo_root().join("DEMO.REC")).unwrap();
        let mut session = GameplaySession::new(level);

        let frame = session.run_demo_frame(&demo);
        assert_eq!(frame.frame_index, 0);
        assert_eq!(frame.controls, ControllerState::new(0, 1, false));
        assert_eq!(frame.snapshot.craft_state, ShipState::Alive);
        assert_eq!(frame.snapshot.x_position, 256.0);
        assert_eq!(frame.snapshot.y_position, 80.0);
        assert_eq!(frame.snapshot.z_position, 3.0011444091796875);
        assert_eq!(frame.snapshot.z_velocity, 0.0011444091796875);
        assert!(frame.events.is_empty());
        assert!(!frame.did_win);
        assert_eq!(frame.road_row_index, 3);
    }

    #[test]
    fn later_demo_frames_continue_consuming_resources() {
        let roads = load_roads_lzs_path(repo_root().join("ROADS.LZS")).unwrap();
        let level = level_from_road_entry(&roads.roads[0]);
        let demo = load_demo_rec_path(repo_root().join("DEMO.REC")).unwrap();
        let mut session = GameplaySession::new(level);

        let frame0 = session.run_demo_frame(&demo);
        let frame1 = session.run_demo_frame(&demo);
        let frame2 = session.run_demo_frame(&demo);

        assert!(frame1.snapshot.oxygen_percent < frame0.snapshot.oxygen_percent);
        assert!(frame2.snapshot.oxygen_percent < frame1.snapshot.oxygen_percent);
        assert!(frame2.snapshot.z_position > frame1.snapshot.z_position);
        assert!(frame2.snapshot.fuel_percent < frame1.snapshot.fuel_percent);
        assert_eq!(frame2.snapshot.craft_state, ShipState::Alive);
    }

    #[test]
    fn falling_below_ground_latches_non_alive_state() {
        let roads = load_roads_lzs_path(repo_root().join("ROADS.LZS")).unwrap();
        let level = level_from_road_entry(&roads.roads[0]);
        let mut session = GameplaySession::new(level);
        session.ship.x_position = 0.0;
        session.ship.y_position = GROUND_Y - 1.0;
        session.ship.y_velocity = -2.0;
        session.ship.z_velocity = 0.1;
        session.ship.x_movement_base = 1.0;

        let frame = session.run_frame(ControllerState::NEUTRAL);
        assert_eq!(frame.snapshot.craft_state, ShipState::Fallen);
        assert!(session.death_frame_index.is_some());
        assert_eq!(session.ship.z_velocity, 0.0);
        assert_eq!(session.ship.y_velocity, 0.0);
        assert_eq!(session.ship.x_movement_base, 0.0);
        assert!(frame.events.is_empty());
    }

    #[test]
    fn fallen_ship_no_longer_advances_through_level() {
        let roads = load_roads_lzs_path(repo_root().join("ROADS.LZS")).unwrap();
        let level = level_from_road_entry(&roads.roads[0]);
        let mut session = GameplaySession::new(level);
        session.ship.state = ShipState::Fallen;
        session.ship.x_position = 220.0;
        session.ship.y_position = 79.5;
        session.ship.z_position = 64.25;
        session.ship.z_velocity = 0.12;
        session.ship.y_velocity = -1.0;
        session.ship.x_movement_base = 0.5;

        let before = session.ship;
        let frame = session.run_frame(ControllerState::new(1, 1, true));
        assert_eq!(frame.snapshot.craft_state, ShipState::Fallen);
        assert_eq!(session.ship.x_position, before.x_position);
        assert_eq!(session.ship.y_position, before.y_position);
        assert_eq!(session.ship.z_position, before.z_position);
        assert!(frame.events.is_empty());
    }
}
