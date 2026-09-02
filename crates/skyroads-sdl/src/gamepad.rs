use skyroads_core::{
    AppInput, ControllerState, DirectionalActivation, InputActivationPreview, InputTuning,
    SensitivityPercent, SteeringActivation, ThrottleActivation, TriggerActivation,
};

const DEFAULT_STICK_ENGAGE: u32 = 0x4000;
const DEFAULT_TRIGGER_ENGAGE: u32 = i16::MAX as u32 / 2;
const MAX_STICK_MAGNITUDE: u32 = i16::MAX as u32;
const MAX_TRIGGER_VALUE: u32 = i16::MAX as u32;
const MENU_RELEASE_NUMERATOR: u32 = 3;
const MENU_RELEASE_DENOMINATOR: u32 = 4;

const MOUSE_WIDTH: u16 = 320;
const MOUSE_HEIGHT: u16 = 200;
const MOUSE_CENTER_X: u16 = MOUSE_WIDTH / 2;
const MOUSE_CENTER_Y: u16 = MOUSE_HEIGHT / 2;
const DEFAULT_MOUSE_HORIZONTAL_DISTANCE: u32 = 10;
const DEFAULT_MOUSE_VERTICAL_DISTANCE: u32 = 85;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GamepadSnapshot {
    pub left_stick_x: i16,
    pub left_stick_y: i16,
    pub left_trigger: u16,
    pub right_trigger: u16,
    pub dpad_up: bool,
    pub dpad_down: bool,
    pub dpad_left: bool,
    pub dpad_right: bool,
    pub south_pressed: bool,
    pub east_pressed: bool,
    pub start_pressed: bool,
    pub back_pressed: bool,
    pub left_shoulder_pressed: bool,
    pub right_shoulder_pressed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControllerThresholds {
    /// A stick direction is active only when its magnitude is greater than this value.
    pub stick_engage: i32,
    /// An engaged menu direction is held while its magnitude is greater than this value.
    pub stick_release: i32,
    /// A trigger is active only when its value is greater than this value.
    pub trigger_engage: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseThresholds {
    pub steer_left_below: u16,
    pub steer_right_above: u16,
    pub accelerate_below: u16,
    pub brake_above: u16,
}

pub fn controller_thresholds(sensitivity: SensitivityPercent) -> ControllerThresholds {
    let stick_engage =
        inverse_scaled_threshold(DEFAULT_STICK_ENGAGE, sensitivity, MAX_STICK_MAGNITUDE);
    let stick_release = stick_engage
        .checked_mul(MENU_RELEASE_NUMERATOR)
        .and_then(|value| value.checked_div(MENU_RELEASE_DENOMINATOR))
        .expect("fixed controller hysteresis arithmetic fits in u32");
    let trigger_engage =
        inverse_scaled_threshold(DEFAULT_TRIGGER_ENGAGE, sensitivity, MAX_TRIGGER_VALUE);

    ControllerThresholds {
        stick_engage: stick_engage as i32,
        stick_release: stick_release as i32,
        trigger_engage: trigger_engage as u16,
    }
}

pub fn mouse_thresholds(sensitivity: SensitivityPercent) -> MouseThresholds {
    let maximum_horizontal_distance = u32::from(MOUSE_WIDTH - 1 - MOUSE_CENTER_X);
    let maximum_vertical_distance = u32::from(MOUSE_HEIGHT - 1 - MOUSE_CENTER_Y);
    let horizontal_distance = inverse_scaled_threshold(
        DEFAULT_MOUSE_HORIZONTAL_DISTANCE,
        sensitivity,
        maximum_horizontal_distance,
    ) as u16;
    let vertical_distance = inverse_scaled_threshold(
        DEFAULT_MOUSE_VERTICAL_DISTANCE,
        sensitivity,
        maximum_vertical_distance,
    ) as u16;

    MouseThresholds {
        steer_left_below: MOUSE_CENTER_X
            .checked_sub(horizontal_distance)
            .expect("derived horizontal mouse distance stays within the framebuffer"),
        steer_right_above: MOUSE_CENTER_X
            .checked_add(horizontal_distance)
            .expect("derived horizontal mouse distance stays within the framebuffer"),
        accelerate_below: MOUSE_CENTER_Y
            .checked_sub(vertical_distance)
            .expect("derived vertical mouse distance stays within the framebuffer"),
        brake_above: MOUSE_CENTER_Y
            .checked_add(vertical_distance)
            .expect("derived vertical mouse distance stays within the framebuffer"),
    }
}

pub fn controller_state(
    snapshot: GamepadSnapshot,
    sensitivity: SensitivityPercent,
) -> ControllerState {
    let thresholds = controller_thresholds(sensitivity);
    let turn_input = horizontal_gameplay_input(snapshot, thresholds);
    let accel_input = vertical_gameplay_input(snapshot, thresholds);

    ControllerState::new(turn_input, accel_input, snapshot.south_pressed)
}

pub fn mouse_controller_state(
    mouse_x: u16,
    mouse_y: u16,
    jump_pressed: bool,
    sensitivity: SensitivityPercent,
) -> ControllerState {
    let thresholds = mouse_thresholds(sensitivity);
    let steer_left = mouse_x < thresholds.steer_left_below;
    let steer_right = mouse_x > thresholds.steer_right_above;
    let accelerate = mouse_y < thresholds.accelerate_below;
    let brake = mouse_y > thresholds.brake_above;
    let turn_input = digital_axis(steer_left, steer_right).value();
    let accel_input = digital_axis(brake, accelerate).value();

    ControllerState::new(turn_input, accel_input, jump_pressed)
}

pub fn input_activation_preview(
    mouse_x: u16,
    mouse_y: u16,
    snapshot: GamepadSnapshot,
    tuning: InputTuning,
) -> InputActivationPreview {
    let mouse_thresholds = mouse_thresholds(tuning.mouse_sensitivity());
    let controller_thresholds = controller_thresholds(tuning.controller_sensitivity());

    InputActivationPreview {
        mouse: DirectionalActivation {
            steering: steering_activation(
                mouse_x < mouse_thresholds.steer_left_below,
                mouse_x > mouse_thresholds.steer_right_above,
            ),
            throttle: throttle_activation(
                mouse_y > mouse_thresholds.brake_above,
                mouse_y < mouse_thresholds.accelerate_below,
            ),
        },
        controller_stick: DirectionalActivation {
            steering: steering_activation(
                i32::from(snapshot.left_stick_x) < -controller_thresholds.stick_engage,
                i32::from(snapshot.left_stick_x) > controller_thresholds.stick_engage,
            ),
            throttle: throttle_activation(
                i32::from(snapshot.left_stick_y) > controller_thresholds.stick_engage,
                i32::from(snapshot.left_stick_y) < -controller_thresholds.stick_engage,
            ),
        },
        controller_triggers: TriggerActivation {
            brake: snapshot.left_trigger > controller_thresholds.trigger_engage,
            accelerate: snapshot.right_trigger > controller_thresholds.trigger_engage,
        },
    }
}

fn steering_activation(left: bool, right: bool) -> SteeringActivation {
    match digital_axis(left, right) {
        DigitalAxis::Negative => SteeringActivation::Left,
        DigitalAxis::Positive => SteeringActivation::Right,
        DigitalAxis::Inactive | DigitalAxis::Contradictory => SteeringActivation::Neutral,
    }
}

fn throttle_activation(brake: bool, accelerate: bool) -> ThrottleActivation {
    match digital_axis(brake, accelerate) {
        DigitalAxis::Negative => ThrottleActivation::Brake,
        DigitalAxis::Positive => ThrottleActivation::Accelerate,
        DigitalAxis::Inactive | DigitalAxis::Contradictory => ThrottleActivation::Neutral,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GamepadLatch {
    stick_horizontal: MenuDirection,
    stick_vertical: MenuDirection,
    previous: MenuHeld,
    previous_left_shoulder: bool,
    previous_right_shoulder: bool,
    pending_app_input: AppInput,
}

impl GamepadLatch {
    pub fn sample(
        &mut self,
        snapshot: GamepadSnapshot,
        sensitivity: SensitivityPercent,
    ) -> AppInput {
        let thresholds = controller_thresholds(sensitivity);
        self.stick_horizontal =
            latched_stick_direction(self.stick_horizontal, snapshot.left_stick_x, thresholds);
        self.stick_vertical =
            latched_stick_direction(self.stick_vertical, snapshot.left_stick_y, thresholds);

        let horizontal = menu_direction_with_dpad_priority(
            snapshot.dpad_left,
            snapshot.dpad_right,
            self.stick_horizontal,
        );
        let vertical = menu_direction_with_dpad_priority(
            snapshot.dpad_up,
            snapshot.dpad_down,
            self.stick_vertical,
        );
        let held = MenuHeld {
            up: vertical == MenuDirection::Negative,
            down: vertical == MenuDirection::Positive,
            left: horizontal == MenuDirection::Negative,
            right: horizontal == MenuDirection::Positive,
            confirm: snapshot.south_pressed || snapshot.start_pressed,
            back: snapshot.east_pressed || snapshot.back_pressed,
        };
        let current = app_input_from_menu_state(self.previous, held);
        self.previous = held;
        self.pending_app_input.up |= current.up;
        self.pending_app_input.down |= current.down;
        self.pending_app_input.left |= current.left;
        self.pending_app_input.right |= current.right;
        self.pending_app_input.enter |= current.enter;
        self.pending_app_input.escape |= current.escape;
        self.pending_app_input.up_held = current.up_held;
        self.pending_app_input.down_held = current.down_held;
        self.pending_app_input.left_held = current.left_held;
        self.pending_app_input.right_held = current.right_held;
        self.pending_app_input.enter_held = current.enter_held;
        self.pending_app_input.previous_setting |=
            snapshot.left_shoulder_pressed && !self.previous_left_shoulder;
        self.pending_app_input.next_setting |=
            snapshot.right_shoulder_pressed && !self.previous_right_shoulder;
        self.previous_left_shoulder = snapshot.left_shoulder_pressed;
        self.previous_right_shoulder = snapshot.right_shoulder_pressed;

        self.pending_app_input
    }

    pub fn consume_app_edges(&mut self) {
        self.pending_app_input = AppInput {
            up_held: self.pending_app_input.up_held,
            down_held: self.pending_app_input.down_held,
            left_held: self.pending_app_input.left_held,
            right_held: self.pending_app_input.right_held,
            enter_held: self.pending_app_input.enter_held,
            ..AppInput::default()
        };
    }

    pub fn menu_held(&self) -> MenuHeld {
        self.previous
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

fn inverse_scaled_threshold(
    default_threshold: u32,
    sensitivity: SensitivityPercent,
    maximum_value: u32,
) -> u32 {
    let numerator = default_threshold
        .checked_mul(u32::from(SensitivityPercent::DEFAULT.get()))
        .expect("fixed sensitivity threshold arithmetic fits in u32");
    let scaled = numerator
        .checked_div(u32::from(sensitivity.get()))
        .expect("validated sensitivity is non-zero");
    let largest_reachable_threshold = maximum_value
        .checked_sub(1)
        .expect("input ranges contain more than one value");

    scaled.clamp(1, largest_reachable_threshold)
}

fn horizontal_gameplay_input(snapshot: GamepadSnapshot, thresholds: ControllerThresholds) -> i8 {
    let dpad_is_active = snapshot.dpad_left || snapshot.dpad_right;
    if dpad_is_active {
        return digital_axis(snapshot.dpad_left, snapshot.dpad_right).value();
    }

    let stick_left = i32::from(snapshot.left_stick_x) < -thresholds.stick_engage;
    let stick_right = i32::from(snapshot.left_stick_x) > thresholds.stick_engage;
    digital_axis(stick_left, stick_right).value()
}

fn vertical_gameplay_input(snapshot: GamepadSnapshot, thresholds: ControllerThresholds) -> i8 {
    let dpad = digital_axis(snapshot.dpad_down, snapshot.dpad_up);
    let left_trigger_active = snapshot.left_trigger > thresholds.trigger_engage;
    let right_trigger_active = snapshot.right_trigger > thresholds.trigger_engage;
    let triggers = digital_axis(left_trigger_active, right_trigger_active);

    if dpad.is_contradictory() || triggers.is_contradictory() {
        return 0;
    }

    let dpad_input = dpad.value();
    let trigger_input = triggers.value();
    let digital_inputs_conflict =
        dpad_input != 0 && trigger_input != 0 && dpad_input != trigger_input;
    if digital_inputs_conflict {
        return 0;
    }
    if dpad_input != 0 {
        return dpad_input;
    }
    if trigger_input != 0 {
        return trigger_input;
    }

    let stick_brake = i32::from(snapshot.left_stick_y) > thresholds.stick_engage;
    let stick_accelerate = i32::from(snapshot.left_stick_y) < -thresholds.stick_engage;
    digital_axis(stick_brake, stick_accelerate).value()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DigitalAxis {
    Inactive,
    Negative,
    Positive,
    Contradictory,
}

impl DigitalAxis {
    fn value(self) -> i8 {
        match self {
            Self::Negative => -1,
            Self::Positive => 1,
            Self::Inactive | Self::Contradictory => 0,
        }
    }

    fn is_contradictory(self) -> bool {
        self == Self::Contradictory
    }
}

fn digital_axis(negative: bool, positive: bool) -> DigitalAxis {
    match (negative, positive) {
        (false, false) => DigitalAxis::Inactive,
        (true, false) => DigitalAxis::Negative,
        (false, true) => DigitalAxis::Positive,
        (true, true) => DigitalAxis::Contradictory,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum MenuDirection {
    Negative,
    #[default]
    Neutral,
    Positive,
}

fn latched_stick_direction(
    previous: MenuDirection,
    raw_value: i16,
    thresholds: ControllerThresholds,
) -> MenuDirection {
    let raw_value = i32::from(raw_value);
    let engages_negative = raw_value < -thresholds.stick_engage;
    let engages_positive = raw_value > thresholds.stick_engage;

    if engages_negative {
        return MenuDirection::Negative;
    }
    if engages_positive {
        return MenuDirection::Positive;
    }

    let remains_negative =
        previous == MenuDirection::Negative && raw_value < -thresholds.stick_release;
    let remains_positive =
        previous == MenuDirection::Positive && raw_value > thresholds.stick_release;

    if remains_negative {
        MenuDirection::Negative
    } else if remains_positive {
        MenuDirection::Positive
    } else {
        MenuDirection::Neutral
    }
}

fn menu_direction_with_dpad_priority(
    dpad_negative: bool,
    dpad_positive: bool,
    stick: MenuDirection,
) -> MenuDirection {
    if !dpad_negative && !dpad_positive {
        return stick;
    }

    match digital_axis(dpad_negative, dpad_positive) {
        DigitalAxis::Negative => MenuDirection::Negative,
        DigitalAxis::Positive => MenuDirection::Positive,
        DigitalAxis::Inactive | DigitalAxis::Contradictory => MenuDirection::Neutral,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuHeld {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub confirm: bool,
    pub back: bool,
}

impl MenuHeld {
    pub fn combined_with(self, other: Self) -> Self {
        Self {
            up: self.up || other.up,
            down: self.down || other.down,
            left: self.left || other.left,
            right: self.right || other.right,
            confirm: self.confirm || other.confirm,
            back: self.back || other.back,
        }
    }
}

fn app_input_from_menu_state(previous: MenuHeld, current: MenuHeld) -> AppInput {
    AppInput {
        up: current.up && !previous.up,
        down: current.down && !previous.down,
        left: current.left && !previous.left,
        right: current.right && !previous.right,
        enter: current.confirm && !previous.confirm,
        escape: current.back && !previous.back,
        up_held: current.up,
        down_held: current.down,
        left_held: current.left,
        right_held: current.right,
        enter_held: current.confirm,
        ..AppInput::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skyroads_core::{controller_state_from_dos_joystick, controller_state_from_dos_mouse};

    #[test]
    fn sensitivity_validation_accepts_exactly_the_documented_values() {
        for value in 0..=u16::MAX {
            let expected_valid = (50..=200).contains(&value) && value % 5 == 0;
            assert_eq!(
                SensitivityPercent::new(value).is_ok(),
                expected_valid,
                "unexpected validation result for {value}%"
            );
        }
    }

    #[test]
    fn sensitivity_error_explains_range_step_and_value() {
        assert_eq!(
            SensitivityPercent::new(52).unwrap_err().to_string(),
            "sensitivity must be 50% through 200% in 5% steps, got 52%"
        );
    }

    #[test]
    fn input_tuning_defaults_both_devices_to_one_hundred_percent() {
        assert_eq!(
            InputTuning::default(),
            InputTuning::new(SensitivityPercent::DEFAULT, SensitivityPercent::DEFAULT)
        );
    }

    #[test]
    fn controller_thresholds_cover_minimum_default_and_maximum_sensitivity() {
        let cases = [
            (
                SensitivityPercent::MIN,
                ControllerThresholds {
                    stick_engage: 32_766,
                    stick_release: 24_574,
                    trigger_engage: 32_766,
                },
            ),
            (
                SensitivityPercent::DEFAULT,
                ControllerThresholds {
                    stick_engage: 0x4000,
                    stick_release: 0x3000,
                    trigger_engage: 16_383,
                },
            ),
            (
                SensitivityPercent::MAX,
                ControllerThresholds {
                    stick_engage: 0x2000,
                    stick_release: 0x1800,
                    trigger_engage: 8_191,
                },
            ),
        ];

        for (sensitivity, expected) in cases {
            assert_eq!(controller_thresholds(sensitivity), expected);
        }
    }

    #[test]
    fn controller_thresholds_are_monotonic_and_reachable_at_every_valid_step() {
        let mut previous = controller_thresholds(SensitivityPercent::MIN);

        for value in (50..=200).step_by(5) {
            let sensitivity = SensitivityPercent::new(value).unwrap();
            let thresholds = controller_thresholds(sensitivity);
            assert!(thresholds.stick_engage <= previous.stick_engage);
            assert!(thresholds.stick_release <= previous.stick_release);
            assert!(thresholds.trigger_engage <= previous.trigger_engage);
            assert!(thresholds.stick_engage < i32::from(i16::MAX));
            assert!(thresholds.trigger_engage < i16::MAX as u16);

            let positive_stick = controller_state(
                GamepadSnapshot {
                    left_stick_x: i16::MAX,
                    ..GamepadSnapshot::default()
                },
                sensitivity,
            );
            let negative_stick = controller_state(
                GamepadSnapshot {
                    left_stick_x: i16::MIN,
                    ..GamepadSnapshot::default()
                },
                sensitivity,
            );
            let trigger = controller_state(
                GamepadSnapshot {
                    right_trigger: i16::MAX as u16,
                    ..GamepadSnapshot::default()
                },
                sensitivity,
            );
            assert_eq!(positive_stick.turn_input, 1);
            assert_eq!(negative_stick.turn_input, -1);
            assert_eq!(trigger.accel_input, 1);

            previous = thresholds;
        }
    }

    #[test]
    fn stick_and_trigger_boundaries_use_strict_activation() {
        for sensitivity in [
            SensitivityPercent::MIN,
            SensitivityPercent::DEFAULT,
            SensitivityPercent::MAX,
        ] {
            let thresholds = controller_thresholds(sensitivity);
            let stick_threshold = thresholds.stick_engage as i16;
            let trigger_threshold = thresholds.trigger_engage;
            let cases = [
                (
                    GamepadSnapshot {
                        left_stick_x: -stick_threshold,
                        ..GamepadSnapshot::default()
                    },
                    ControllerState::NEUTRAL,
                ),
                (
                    GamepadSnapshot {
                        left_stick_x: -stick_threshold - 1,
                        ..GamepadSnapshot::default()
                    },
                    ControllerState::new(-1, 0, false),
                ),
                (
                    GamepadSnapshot {
                        left_stick_x: stick_threshold,
                        ..GamepadSnapshot::default()
                    },
                    ControllerState::NEUTRAL,
                ),
                (
                    GamepadSnapshot {
                        left_stick_x: stick_threshold + 1,
                        ..GamepadSnapshot::default()
                    },
                    ControllerState::new(1, 0, false),
                ),
                (
                    GamepadSnapshot {
                        right_trigger: trigger_threshold,
                        ..GamepadSnapshot::default()
                    },
                    ControllerState::NEUTRAL,
                ),
                (
                    GamepadSnapshot {
                        right_trigger: trigger_threshold + 1,
                        ..GamepadSnapshot::default()
                    },
                    ControllerState::new(0, 1, false),
                ),
            ];

            for (snapshot, expected) in cases {
                assert_eq!(controller_state(snapshot, sensitivity), expected);
            }
        }
    }

    #[test]
    fn default_controller_thresholds_match_the_existing_dos_joystick_decoder() {
        let axis_values = [
            i16::MIN,
            -16_385,
            -16_384,
            -16_383,
            -1,
            0,
            1,
            16_383,
            16_384,
            16_385,
            i16::MAX,
        ];

        for x in axis_values {
            for y in axis_values {
                for jump_pressed in [false, true] {
                    let snapshot = GamepadSnapshot {
                        left_stick_x: x,
                        left_stick_y: y,
                        south_pressed: jump_pressed,
                        ..GamepadSnapshot::default()
                    };
                    let raw_x = (i32::from(x) + 32_768) as u16;
                    let raw_y = (i32::from(y) + 32_768) as u16;
                    let expected = controller_state_from_dos_joystick(raw_x, raw_y, jump_pressed);

                    assert_eq!(
                        controller_state(snapshot, SensitivityPercent::DEFAULT),
                        expected,
                        "default controller mismatch for x={x}, y={y}, jump={jump_pressed}"
                    );
                }
            }
        }
    }

    #[test]
    fn gameplay_dpad_has_priority_and_contradictions_are_neutral() {
        let cases = [
            (
                GamepadSnapshot {
                    left_stick_x: i16::MAX,
                    dpad_left: true,
                    ..GamepadSnapshot::default()
                },
                -1,
            ),
            (
                GamepadSnapshot {
                    left_stick_x: i16::MIN,
                    dpad_right: true,
                    ..GamepadSnapshot::default()
                },
                1,
            ),
            (
                GamepadSnapshot {
                    left_stick_x: i16::MAX,
                    dpad_left: true,
                    dpad_right: true,
                    ..GamepadSnapshot::default()
                },
                0,
            ),
            (
                GamepadSnapshot {
                    left_stick_y: i16::MAX,
                    dpad_up: true,
                    ..GamepadSnapshot::default()
                },
                1,
            ),
            (
                GamepadSnapshot {
                    left_stick_y: i16::MIN,
                    dpad_down: true,
                    ..GamepadSnapshot::default()
                },
                -1,
            ),
            (
                GamepadSnapshot {
                    left_stick_y: i16::MIN,
                    dpad_up: true,
                    dpad_down: true,
                    ..GamepadSnapshot::default()
                },
                0,
            ),
        ];

        for (snapshot, expected_axis) in cases {
            let state = controller_state(snapshot, SensitivityPercent::DEFAULT);
            let actual_axis = if snapshot.dpad_left || snapshot.dpad_right {
                state.turn_input
            } else {
                state.accel_input
            };
            assert_eq!(actual_axis, expected_axis, "snapshot: {snapshot:?}");
        }
    }

    #[test]
    fn gameplay_triggers_have_priority_and_digital_conflicts_are_neutral() {
        let pressed = controller_thresholds(SensitivityPercent::DEFAULT).trigger_engage + 1;
        let cases = [
            (
                GamepadSnapshot {
                    left_stick_y: i16::MAX,
                    right_trigger: pressed,
                    ..GamepadSnapshot::default()
                },
                1,
            ),
            (
                GamepadSnapshot {
                    left_stick_y: i16::MIN,
                    left_trigger: pressed,
                    ..GamepadSnapshot::default()
                },
                -1,
            ),
            (
                GamepadSnapshot {
                    left_stick_y: i16::MIN,
                    left_trigger: pressed,
                    right_trigger: pressed,
                    ..GamepadSnapshot::default()
                },
                0,
            ),
            (
                GamepadSnapshot {
                    dpad_up: true,
                    left_trigger: pressed,
                    ..GamepadSnapshot::default()
                },
                0,
            ),
            (
                GamepadSnapshot {
                    dpad_down: true,
                    right_trigger: pressed,
                    ..GamepadSnapshot::default()
                },
                0,
            ),
            (
                GamepadSnapshot {
                    dpad_up: true,
                    right_trigger: pressed,
                    ..GamepadSnapshot::default()
                },
                1,
            ),
            (
                GamepadSnapshot {
                    dpad_down: true,
                    left_trigger: pressed,
                    ..GamepadSnapshot::default()
                },
                -1,
            ),
        ];

        for (snapshot, expected) in cases {
            assert_eq!(
                controller_state(snapshot, SensitivityPercent::DEFAULT).accel_input,
                expected,
                "snapshot: {snapshot:?}"
            );
        }
    }

    #[test]
    fn only_the_south_button_is_gameplay_jump() {
        let button_cases = [
            (GamepadSnapshot::default(), false),
            (
                GamepadSnapshot {
                    south_pressed: true,
                    ..GamepadSnapshot::default()
                },
                true,
            ),
            (
                GamepadSnapshot {
                    east_pressed: true,
                    start_pressed: true,
                    back_pressed: true,
                    ..GamepadSnapshot::default()
                },
                false,
            ),
        ];

        for (snapshot, expected) in button_cases {
            assert_eq!(
                controller_state(snapshot, SensitivityPercent::DEFAULT).jump_input,
                expected
            );
        }
    }

    #[test]
    fn mouse_thresholds_cover_minimum_default_and_maximum_sensitivity() {
        let cases = [
            (
                SensitivityPercent::MIN,
                MouseThresholds {
                    steer_left_below: 140,
                    steer_right_above: 180,
                    accelerate_below: 2,
                    brake_above: 198,
                },
            ),
            (
                SensitivityPercent::DEFAULT,
                MouseThresholds {
                    steer_left_below: 0x0096,
                    steer_right_above: 0x00AA,
                    accelerate_below: 0x000F,
                    brake_above: 0x00B9,
                },
            ),
            (
                SensitivityPercent::MAX,
                MouseThresholds {
                    steer_left_below: 155,
                    steer_right_above: 165,
                    accelerate_below: 58,
                    brake_above: 142,
                },
            ),
        ];

        for (sensitivity, expected) in cases {
            assert_eq!(mouse_thresholds(sensitivity), expected);
        }
    }

    #[test]
    fn mouse_thresholds_are_monotonic_inside_the_framebuffer_and_reachable() {
        let mut previous_horizontal_distance = u16::MAX;
        let mut previous_vertical_distance = u16::MAX;

        for value in (50..=200).step_by(5) {
            let sensitivity = SensitivityPercent::new(value).unwrap();
            let thresholds = mouse_thresholds(sensitivity);
            let horizontal_distance = thresholds.steer_right_above - MOUSE_CENTER_X;
            let vertical_distance = thresholds.brake_above - MOUSE_CENTER_Y;

            assert!(horizontal_distance <= previous_horizontal_distance);
            assert!(vertical_distance <= previous_vertical_distance);
            assert!(thresholds.steer_left_below > 0);
            assert!(thresholds.steer_right_above < MOUSE_WIDTH - 1);
            assert!(thresholds.accelerate_below > 0);
            assert!(thresholds.brake_above < MOUSE_HEIGHT - 1);

            let full_left = mouse_controller_state(0, MOUSE_CENTER_Y, false, sensitivity);
            let full_right =
                mouse_controller_state(MOUSE_WIDTH - 1, MOUSE_CENTER_Y, false, sensitivity);
            let full_accelerate = mouse_controller_state(MOUSE_CENTER_X, 0, false, sensitivity);
            let full_brake =
                mouse_controller_state(MOUSE_CENTER_X, MOUSE_HEIGHT - 1, false, sensitivity);
            assert_eq!(full_left.turn_input, -1);
            assert_eq!(full_right.turn_input, 1);
            assert_eq!(full_accelerate.accel_input, 1);
            assert_eq!(full_brake.accel_input, -1);

            previous_horizontal_distance = horizontal_distance;
            previous_vertical_distance = vertical_distance;
        }
    }

    #[test]
    fn default_mouse_thresholds_match_the_existing_decoder_at_every_position() {
        for mouse_x in 0..MOUSE_WIDTH {
            for mouse_y in 0..MOUSE_HEIGHT {
                for jump_pressed in [false, true] {
                    let actual = mouse_controller_state(
                        mouse_x,
                        mouse_y,
                        jump_pressed,
                        SensitivityPercent::DEFAULT,
                    );
                    let expected =
                        controller_state_from_dos_mouse(mouse_x, mouse_y, u16::from(jump_pressed));
                    assert_eq!(actual, expected, "mouse position: ({mouse_x}, {mouse_y})");
                }
            }
        }
    }

    #[test]
    fn mouse_boundaries_are_neutral_and_activate_one_step_beyond() {
        for sensitivity in [
            SensitivityPercent::MIN,
            SensitivityPercent::DEFAULT,
            SensitivityPercent::MAX,
        ] {
            let thresholds = mouse_thresholds(sensitivity);
            let cases = [
                (
                    thresholds.steer_left_below,
                    MOUSE_CENTER_Y,
                    ControllerState::NEUTRAL,
                ),
                (
                    thresholds.steer_left_below - 1,
                    MOUSE_CENTER_Y,
                    ControllerState::new(-1, 0, false),
                ),
                (
                    thresholds.steer_right_above,
                    MOUSE_CENTER_Y,
                    ControllerState::NEUTRAL,
                ),
                (
                    thresholds.steer_right_above + 1,
                    MOUSE_CENTER_Y,
                    ControllerState::new(1, 0, false),
                ),
                (
                    MOUSE_CENTER_X,
                    thresholds.accelerate_below,
                    ControllerState::NEUTRAL,
                ),
                (
                    MOUSE_CENTER_X,
                    thresholds.accelerate_below - 1,
                    ControllerState::new(0, 1, false),
                ),
                (
                    MOUSE_CENTER_X,
                    thresholds.brake_above,
                    ControllerState::NEUTRAL,
                ),
                (
                    MOUSE_CENTER_X,
                    thresholds.brake_above + 1,
                    ControllerState::new(0, -1, false),
                ),
            ];

            for (x, y, expected) in cases {
                assert_eq!(mouse_controller_state(x, y, false, sensitivity), expected);
            }
        }
    }

    #[test]
    fn menu_stick_uses_engage_and_release_hysteresis_without_repeating_edges() {
        let mut latch = GamepadLatch::default();
        let cases = [
            (0, false, false),
            (16_384, false, false),
            (16_385, true, true),
            (15_000, false, true),
            (12_289, false, true),
            (12_288, false, false),
            (16_385, true, true),
        ];

        for (raw_x, expected_edge, expected_held) in cases {
            let input = latch.sample(
                GamepadSnapshot {
                    left_stick_x: raw_x,
                    ..GamepadSnapshot::default()
                },
                SensitivityPercent::DEFAULT,
            );
            assert_eq!(input.right, expected_edge, "raw x: {raw_x}");
            assert_eq!(input.right_held, expected_held, "raw x: {raw_x}");
            latch.consume_app_edges();
        }
    }

    #[test]
    fn menu_stick_can_switch_direction_without_visiting_neutral() {
        let mut latch = GamepadLatch::default();
        let right = latch.sample(
            GamepadSnapshot {
                left_stick_x: 16_385,
                ..GamepadSnapshot::default()
            },
            SensitivityPercent::DEFAULT,
        );
        latch.consume_app_edges();
        let left = latch.sample(
            GamepadSnapshot {
                left_stick_x: -16_385,
                ..GamepadSnapshot::default()
            },
            SensitivityPercent::DEFAULT,
        );

        assert!(right.right && right.right_held);
        assert!(left.left && left.left_held);
        assert!(!left.right && !left.right_held);
    }

    #[test]
    fn menu_dpad_has_priority_and_contradictory_directions_are_neutral() {
        let cases = [
            (
                GamepadSnapshot {
                    left_stick_x: i16::MAX,
                    dpad_left: true,
                    ..GamepadSnapshot::default()
                },
                (true, false),
            ),
            (
                GamepadSnapshot {
                    left_stick_x: i16::MIN,
                    dpad_right: true,
                    ..GamepadSnapshot::default()
                },
                (false, true),
            ),
            (
                GamepadSnapshot {
                    left_stick_x: i16::MAX,
                    dpad_left: true,
                    dpad_right: true,
                    ..GamepadSnapshot::default()
                },
                (false, false),
            ),
            (
                GamepadSnapshot {
                    left_stick_y: i16::MAX,
                    dpad_up: true,
                    ..GamepadSnapshot::default()
                },
                (true, false),
            ),
            (
                GamepadSnapshot {
                    left_stick_y: i16::MIN,
                    dpad_down: true,
                    ..GamepadSnapshot::default()
                },
                (false, true),
            ),
            (
                GamepadSnapshot {
                    left_stick_y: i16::MAX,
                    dpad_up: true,
                    dpad_down: true,
                    ..GamepadSnapshot::default()
                },
                (false, false),
            ),
        ];

        for (snapshot, (expected_negative, expected_positive)) in cases {
            let mut latch = GamepadLatch::default();
            let input = latch.sample(snapshot, SensitivityPercent::DEFAULT);
            let (actual_negative, actual_positive) = if snapshot.dpad_left || snapshot.dpad_right {
                (input.left_held, input.right_held)
            } else {
                (input.up_held, input.down_held)
            };
            assert_eq!(
                (actual_negative, actual_positive),
                (expected_negative, expected_positive),
                "snapshot: {snapshot:?}"
            );
        }
    }

    #[test]
    fn menu_confirm_combines_south_and_start_into_one_held_action() {
        let mut latch = GamepadLatch::default();
        let cases = [
            (
                GamepadSnapshot {
                    south_pressed: true,
                    ..GamepadSnapshot::default()
                },
                (true, true),
            ),
            (
                GamepadSnapshot {
                    south_pressed: true,
                    start_pressed: true,
                    ..GamepadSnapshot::default()
                },
                (false, true),
            ),
            (
                GamepadSnapshot {
                    start_pressed: true,
                    ..GamepadSnapshot::default()
                },
                (false, true),
            ),
            (GamepadSnapshot::default(), (false, false)),
            (
                GamepadSnapshot {
                    start_pressed: true,
                    ..GamepadSnapshot::default()
                },
                (true, true),
            ),
        ];

        for (snapshot, (expected_edge, expected_held)) in cases {
            let input = latch.sample(snapshot, SensitivityPercent::DEFAULT);
            assert_eq!(input.enter, expected_edge);
            assert_eq!(input.enter_held, expected_held);
            latch.consume_app_edges();
        }
    }

    #[test]
    fn menu_back_combines_east_and_back_into_one_edge() {
        let mut latch = GamepadLatch::default();
        let cases = [
            (
                GamepadSnapshot {
                    east_pressed: true,
                    ..GamepadSnapshot::default()
                },
                true,
            ),
            (
                GamepadSnapshot {
                    east_pressed: true,
                    back_pressed: true,
                    ..GamepadSnapshot::default()
                },
                false,
            ),
            (
                GamepadSnapshot {
                    back_pressed: true,
                    ..GamepadSnapshot::default()
                },
                false,
            ),
            (GamepadSnapshot::default(), false),
            (
                GamepadSnapshot {
                    back_pressed: true,
                    ..GamepadSnapshot::default()
                },
                true,
            ),
        ];

        for (snapshot, expected_edge) in cases {
            let input = latch.sample(snapshot, SensitivityPercent::DEFAULT);
            assert_eq!(input.escape, expected_edge);
            latch.consume_app_edges();
        }
    }

    #[test]
    fn shoulder_buttons_emit_one_setting_edge_per_press() {
        let mut latch = GamepadLatch::default();
        let both_held = GamepadSnapshot {
            left_shoulder_pressed: true,
            right_shoulder_pressed: true,
            ..GamepadSnapshot::default()
        };

        let pressed = latch.sample(both_held, SensitivityPercent::DEFAULT);
        latch.consume_app_edges();
        let still_held = latch.sample(both_held, SensitivityPercent::DEFAULT);
        latch.consume_app_edges();
        latch.sample(GamepadSnapshot::default(), SensitivityPercent::DEFAULT);
        latch.consume_app_edges();
        let pressed_again = latch.sample(both_held, SensitivityPercent::DEFAULT);

        assert!(pressed.previous_setting && pressed.next_setting);
        assert!(!still_held.previous_setting && !still_held.next_setting);
        assert!(pressed_again.previous_setting && pressed_again.next_setting);
    }

    #[test]
    fn latch_retains_edges_until_consumed_and_reset_clears_all_history() {
        let mut latch = GamepadLatch::default();
        let held = GamepadSnapshot {
            dpad_right: true,
            south_pressed: true,
            ..GamepadSnapshot::default()
        };

        let first = latch.sample(held, SensitivityPercent::DEFAULT);
        let before_consume = latch.sample(held, SensitivityPercent::DEFAULT);
        latch.consume_app_edges();
        let after_consume = latch.sample(held, SensitivityPercent::DEFAULT);
        latch.reset();
        let after_reset = latch.sample(held, SensitivityPercent::DEFAULT);

        assert!(first.right && first.enter);
        assert!(before_consume.right && before_consume.enter);
        assert!(before_consume.right_held && before_consume.enter_held);
        assert!(!after_consume.right && !after_consume.enter);
        assert!(after_consume.right_held && after_consume.enter_held);
        assert!(after_reset.right && after_reset.enter);
    }

    #[test]
    fn neutral_snapshot_produces_neutral_gameplay_and_menu_input() {
        let snapshot = GamepadSnapshot::default();
        let mut latch = GamepadLatch::default();

        assert_eq!(
            controller_state(snapshot, SensitivityPercent::DEFAULT),
            ControllerState::NEUTRAL
        );
        assert_eq!(
            latch.sample(snapshot, SensitivityPercent::DEFAULT),
            AppInput::default()
        );
    }

    #[test]
    fn live_preview_reports_mouse_stick_and_triggers_independently() {
        let preview = input_activation_preview(
            149,
            186,
            GamepadSnapshot {
                left_stick_x: 16_385,
                left_stick_y: -16_385,
                left_trigger: 16_384,
                right_trigger: 16_383,
                ..GamepadSnapshot::default()
            },
            InputTuning::default(),
        );

        assert_eq!(preview.mouse.steering, SteeringActivation::Left);
        assert_eq!(preview.mouse.throttle, ThrottleActivation::Brake);
        assert_eq!(preview.controller_stick.steering, SteeringActivation::Right);
        assert_eq!(
            preview.controller_stick.throttle,
            ThrottleActivation::Accelerate
        );
        assert!(preview.controller_triggers.brake);
        assert!(!preview.controller_triggers.accelerate);
    }
}
