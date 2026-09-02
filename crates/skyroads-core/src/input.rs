use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SensitivityPercent(u16);

impl SensitivityPercent {
    pub const MIN: Self = Self(50);
    pub const DEFAULT: Self = Self(100);
    pub const MAX: Self = Self(200);
    pub const STEP: u16 = 5;

    pub const fn new(percent: u16) -> Result<Self, InvalidSensitivityPercent> {
        let is_in_range = percent >= Self::MIN.0 && percent <= Self::MAX.0;
        let is_whole_step = percent.is_multiple_of(Self::STEP);

        if is_in_range && is_whole_step {
            Ok(Self(percent))
        } else {
            Err(InvalidSensitivityPercent { percent })
        }
    }

    pub const fn percent(self) -> u16 {
        self.0
    }

    pub const fn get(self) -> u16 {
        self.percent()
    }

    pub const fn increase(self) -> Self {
        if self.0 >= Self::MAX.0 {
            Self::MAX
        } else {
            Self(self.0 + Self::STEP)
        }
    }

    pub const fn decrease(self) -> Self {
        if self.0 <= Self::MIN.0 {
            Self::MIN
        } else {
            Self(self.0 - Self::STEP)
        }
    }
}

impl Default for SensitivityPercent {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl TryFrom<u16> for SensitivityPercent {
    type Error = InvalidSensitivityPercent;

    fn try_from(percent: u16) -> Result<Self, Self::Error> {
        Self::new(percent)
    }
}

impl From<SensitivityPercent> for u16 {
    fn from(sensitivity: SensitivityPercent) -> Self {
        sensitivity.percent()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidSensitivityPercent {
    percent: u16,
}

impl fmt::Display for InvalidSensitivityPercent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "sensitivity must be 50% through 200% in 5% steps, got {}%",
            self.percent
        )
    }
}

impl std::error::Error for InvalidSensitivityPercent {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputTuning {
    mouse_sensitivity: SensitivityPercent,
    controller_sensitivity: SensitivityPercent,
}

impl InputTuning {
    pub const DEFAULT: Self = Self {
        mouse_sensitivity: SensitivityPercent::DEFAULT,
        controller_sensitivity: SensitivityPercent::DEFAULT,
    };

    pub const fn new(
        mouse_sensitivity: SensitivityPercent,
        controller_sensitivity: SensitivityPercent,
    ) -> Self {
        Self {
            mouse_sensitivity,
            controller_sensitivity,
        }
    }

    pub const fn mouse_sensitivity(self) -> SensitivityPercent {
        self.mouse_sensitivity
    }

    pub const fn controller_sensitivity(self) -> SensitivityPercent {
        self.controller_sensitivity
    }

    pub const fn with_mouse_sensitivity(self, mouse_sensitivity: SensitivityPercent) -> Self {
        Self {
            mouse_sensitivity,
            ..self
        }
    }

    pub const fn with_controller_sensitivity(
        self,
        controller_sensitivity: SensitivityPercent,
    ) -> Self {
        Self {
            controller_sensitivity,
            ..self
        }
    }
}

impl Default for InputTuning {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SteeringActivation {
    Left,
    #[default]
    Neutral,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThrottleActivation {
    Brake,
    #[default]
    Neutral,
    Accelerate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DirectionalActivation {
    pub steering: SteeringActivation,
    pub throttle: ThrottleActivation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TriggerActivation {
    pub brake: bool,
    pub accelerate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputActivationPreview {
    pub mouse: DirectionalActivation,
    pub controller_stick: DirectionalActivation,
    pub controller_triggers: TriggerActivation,
}

#[cfg(test)]
mod tests {
    use super::{
        InputActivationPreview, InputTuning, SensitivityPercent, SteeringActivation,
        ThrottleActivation,
    };

    #[test]
    fn sensitivity_accepts_only_the_supported_five_percent_steps() {
        assert_eq!(SensitivityPercent::new(50), Ok(SensitivityPercent::MIN));
        assert_eq!(
            SensitivityPercent::new(100),
            Ok(SensitivityPercent::DEFAULT)
        );
        assert_eq!(SensitivityPercent::new(200), Ok(SensitivityPercent::MAX));

        for invalid in [0, 49, 51, 99, 101, 199, 201, u16::MAX] {
            let error = SensitivityPercent::new(invalid).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("sensitivity must be 50% through 200% in 5% steps, got {invalid}%")
            );
        }
    }

    #[test]
    fn sensitivity_adjustments_saturate_at_the_supported_bounds() {
        assert_eq!(SensitivityPercent::MIN.decrease(), SensitivityPercent::MIN);
        assert_eq!(SensitivityPercent::MIN.increase().percent(), 55);
        assert_eq!(SensitivityPercent::MAX.increase(), SensitivityPercent::MAX);
        assert_eq!(SensitivityPercent::MAX.decrease().percent(), 195);
    }

    #[test]
    fn input_defaults_are_neutral_and_dos_faithful() {
        let tuning = InputTuning::default();

        assert_eq!(tuning.mouse_sensitivity(), SensitivityPercent::DEFAULT);
        assert_eq!(tuning.controller_sensitivity(), SensitivityPercent::DEFAULT);
        let preview = InputActivationPreview::default();
        assert_eq!(preview.mouse.steering, SteeringActivation::Neutral);
        assert_eq!(preview.mouse.throttle, ThrottleActivation::Neutral);
        assert_eq!(
            preview.controller_stick.steering,
            SteeringActivation::Neutral
        );
        assert_eq!(
            preview.controller_stick.throttle,
            ThrottleActivation::Neutral
        );
        assert!(!preview.controller_triggers.brake);
        assert!(!preview.controller_triggers.accelerate);
    }

    #[test]
    fn tuning_updates_one_device_family_without_changing_the_other() {
        let tuning = InputTuning::default()
            .with_mouse_sensitivity(SensitivityPercent::MIN)
            .with_controller_sensitivity(SensitivityPercent::MAX);

        assert_eq!(tuning.mouse_sensitivity(), SensitivityPercent::MIN);
        assert_eq!(tuning.controller_sensitivity(), SensitivityPercent::MAX);
    }
}
