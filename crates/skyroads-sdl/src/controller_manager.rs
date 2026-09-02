use crate::gamepad::GamepadSnapshot;
use crate::sdl::{
    DeviceIndex, GameController, GameControllerState, InputDeviceEvent, InputDeviceInfo, Joystick,
    JoystickState, Sdl,
};

type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerKind {
    MappedGameController,
    RawJoystickFallback,
}

impl ControllerKind {
    fn diagnostic_label(self) -> &'static str {
        match self {
            Self::MappedGameController => "mapped",
            Self::RawJoystickFallback => "raw fallback",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerMetadata {
    pub kind: ControllerKind,
    pub name: String,
    pub instance_id: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerEventOutcome {
    Ignored,
    SelectionRescanned,
    MetadataRefreshed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControllerSample {
    pub snapshot: GamepadSnapshot,
    pub input_discontinuity: bool,
    /// True only when the snapshot came from a successful state query on an
    /// active device. Synthetic neutral and "no device" cannot prove release.
    pub device_state_observed: bool,
}

pub struct ControllerManager<'sdl> {
    sdl: &'sdl Sdl,
    active: ActiveController<'sdl>,
    neutral_sample_pending: bool,
    live_sample_rebase_pending: bool,
}

enum ActiveController<'sdl> {
    NoController,
    MappedGameController(GameController<'sdl>),
    RawJoystickFallback(Joystick<'sdl>),
}

impl<'sdl> ControllerManager<'sdl> {
    pub fn new(sdl: &'sdl Sdl) -> Result<Self> {
        let mut manager = Self::without_active_controller(sdl);
        manager
            .rescan()
            .map_err(|error| format!("could not discover an SDL controller: {error}"))?;
        Ok(manager)
    }

    pub fn without_active_controller(sdl: &'sdl Sdl) -> Self {
        Self {
            sdl,
            active: ActiveController::NoController,
            neutral_sample_pending: false,
            live_sample_rebase_pending: false,
        }
    }

    pub fn metadata(&self) -> Option<ControllerMetadata> {
        match &self.active {
            ActiveController::NoController => None,
            ActiveController::MappedGameController(controller) => Some(ControllerMetadata {
                kind: ControllerKind::MappedGameController,
                name: controller.name().to_string(),
                instance_id: controller.instance_id().value(),
            }),
            ActiveController::RawJoystickFallback(joystick) => Some(ControllerMetadata {
                kind: ControllerKind::RawJoystickFallback,
                name: joystick.name().to_string(),
                instance_id: joystick.instance_id().value(),
            }),
        }
    }

    pub fn is_connected(&self) -> bool {
        !matches!(self.active, ActiveController::NoController)
    }

    pub fn status_line(&self) -> String {
        format_status_line(self.metadata().as_ref())
    }

    pub fn handle_event(&mut self, event: InputDeviceEvent) -> Result<ControllerEventOutcome> {
        let active = active_identity(&self.active);
        let action = lifecycle_action(active, LifecycleEvent::from(event));

        match action {
            LifecycleAction::Ignore => Ok(ControllerEventOutcome::Ignored),
            LifecycleAction::Rescan => {
                self.rescan().map_err(|error| {
                    format!("could not rescan SDL controllers after a device was added: {error}")
                })?;
                Ok(ControllerEventOutcome::SelectionRescanned)
            }
            LifecycleAction::DisconnectAndRescan => {
                self.disconnect_active();
                self.rescan().map_err(|error| {
                    format!("could not rescan SDL controllers after a disconnect: {error}")
                })?;
                Ok(ControllerEventOutcome::SelectionRescanned)
            }
            LifecycleAction::RefreshMapped => self.refresh_mapped_controller(),
        }
    }

    pub fn sample(&mut self) -> Result<ControllerSample> {
        if let Some(snapshot) = neutral_snapshot_if_pending(self.neutral_sample_pending) {
            return Ok(ControllerSample {
                snapshot,
                input_discontinuity: false,
                device_state_observed: false,
            });
        }

        let device_state_observed = !matches!(&self.active, ActiveController::NoController);
        let snapshot = match &self.active {
            ActiveController::NoController => Ok(GamepadSnapshot::default()),
            ActiveController::MappedGameController(controller) => {
                controller.state().map(mapped_snapshot)
            }
            ActiveController::RawJoystickFallback(joystick) => joystick.state().map(raw_snapshot),
        }?;
        let input_discontinuity = take_live_sample_rebase(&mut self.live_sample_rebase_pending);

        Ok(ControllerSample {
            snapshot,
            input_discontinuity,
            device_state_observed,
        })
    }

    pub fn acknowledge_neutral_sample(&mut self) {
        acknowledge_pending_neutral(
            &mut self.neutral_sample_pending,
            &mut self.live_sample_rebase_pending,
        );
    }

    fn rescan(&mut self) -> Result<()> {
        debug_assert!(matches!(self.active, ActiveController::NoController));

        let devices = self.sdl.input_devices()?;
        let Some(selection) = select_device(&devices) else {
            return Ok(());
        };

        self.active = match selection {
            DeviceSelection::Mapped(device_index) => {
                let controller = GameController::open(self.sdl, device_index).map_err(|error| {
                    format!(
                        "failed to open mapped device index {}: {error}",
                        device_index.value()
                    )
                })?;
                ActiveController::MappedGameController(controller)
            }
            DeviceSelection::RawIndexZero(device_index) => {
                let joystick = Joystick::open(self.sdl, device_index).map_err(|error| {
                    format!(
                        "failed to open raw fallback at device index {}: {error}",
                        device_index.value()
                    )
                })?;
                ActiveController::RawJoystickFallback(joystick)
            }
        };
        Ok(())
    }

    fn disconnect_active(&mut self) {
        self.active = ActiveController::NoController;
        self.neutral_sample_pending = true;
    }

    fn refresh_mapped_controller(&mut self) -> Result<ControllerEventOutcome> {
        let is_attached = match &self.active {
            ActiveController::MappedGameController(controller) => controller.is_attached(),
            _ => return Ok(ControllerEventOutcome::Ignored),
        };

        if !is_attached {
            self.disconnect_active();
            self.rescan().map_err(|error| {
                format!("could not rescan SDL controllers after a detached remap: {error}")
            })?;
            return Ok(ControllerEventOutcome::SelectionRescanned);
        }

        if let ActiveController::MappedGameController(controller) = &mut self.active {
            controller.refresh_name();
        }
        Ok(ControllerEventOutcome::MetadataRefreshed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceSelection {
    Mapped(DeviceIndex),
    RawIndexZero(DeviceIndex),
}

fn select_device(devices: &[InputDeviceInfo]) -> Option<DeviceSelection> {
    let mapped = devices.iter().find(|device| device.mapped);
    if let Some(device) = mapped {
        return Some(DeviceSelection::Mapped(device.device_index));
    }

    devices
        .iter()
        .find(|device| device.device_index.value() == 0)
        .map(|device| DeviceSelection::RawIndexZero(device.device_index))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveIdentity {
    kind: ControllerKind,
    instance_id: i32,
}

fn active_identity(active: &ActiveController<'_>) -> Option<ActiveIdentity> {
    match active {
        ActiveController::NoController => None,
        ActiveController::MappedGameController(controller) => Some(ActiveIdentity {
            kind: ControllerKind::MappedGameController,
            instance_id: controller.instance_id().value(),
        }),
        ActiveController::RawJoystickFallback(joystick) => Some(ActiveIdentity {
            kind: ControllerKind::RawJoystickFallback,
            instance_id: joystick.instance_id().value(),
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LifecycleEvent {
    Added,
    MappedRemoved(i32),
    MappedRemapped(i32),
    RawRemoved(i32),
}

impl From<InputDeviceEvent> for LifecycleEvent {
    fn from(event: InputDeviceEvent) -> Self {
        match event {
            InputDeviceEvent::MappedAdded(_) | InputDeviceEvent::RawAdded(_) => Self::Added,
            InputDeviceEvent::MappedRemoved(instance_id) => {
                Self::MappedRemoved(instance_id.value())
            }
            InputDeviceEvent::MappedRemapped(instance_id) => {
                Self::MappedRemapped(instance_id.value())
            }
            InputDeviceEvent::RawRemoved(instance_id) => Self::RawRemoved(instance_id.value()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LifecycleAction {
    Ignore,
    Rescan,
    DisconnectAndRescan,
    RefreshMapped,
}

fn lifecycle_action(active: Option<ActiveIdentity>, event: LifecycleEvent) -> LifecycleAction {
    let Some(active) = active else {
        return match event {
            LifecycleEvent::Added => LifecycleAction::Rescan,
            _ => LifecycleAction::Ignore,
        };
    };

    match (active.kind, event) {
        (_, LifecycleEvent::Added) => LifecycleAction::Ignore,
        (ControllerKind::MappedGameController, LifecycleEvent::MappedRemoved(instance_id))
            if active.instance_id == instance_id =>
        {
            LifecycleAction::DisconnectAndRescan
        }
        (ControllerKind::RawJoystickFallback, LifecycleEvent::RawRemoved(instance_id))
            if active.instance_id == instance_id =>
        {
            LifecycleAction::DisconnectAndRescan
        }
        (ControllerKind::MappedGameController, LifecycleEvent::MappedRemapped(instance_id))
            if active.instance_id == instance_id =>
        {
            LifecycleAction::RefreshMapped
        }
        _ => LifecycleAction::Ignore,
    }
}

fn mapped_snapshot(state: GameControllerState) -> GamepadSnapshot {
    GamepadSnapshot {
        left_stick_x: state.left_x,
        left_stick_y: state.left_y,
        left_trigger: state.left_trigger,
        right_trigger: state.right_trigger,
        dpad_up: state.dpad_up,
        dpad_down: state.dpad_down,
        dpad_left: state.dpad_left,
        dpad_right: state.dpad_right,
        south_pressed: state.south_pressed,
        east_pressed: state.east_pressed,
        start_pressed: state.start_pressed,
        back_pressed: state.back_pressed,
        left_shoulder_pressed: state.left_shoulder_pressed,
        right_shoulder_pressed: state.right_shoulder_pressed,
    }
}

fn raw_snapshot(state: JoystickState) -> GamepadSnapshot {
    GamepadSnapshot {
        left_stick_x: state.x_axis,
        left_stick_y: state.y_axis,
        south_pressed: state.jump_pressed,
        ..GamepadSnapshot::default()
    }
}

fn neutral_snapshot_if_pending(neutral_sample_pending: bool) -> Option<GamepadSnapshot> {
    neutral_sample_pending.then(GamepadSnapshot::default)
}

fn acknowledge_pending_neutral(
    neutral_sample_pending: &mut bool,
    live_sample_rebase_pending: &mut bool,
) {
    if *neutral_sample_pending {
        *neutral_sample_pending = false;
        *live_sample_rebase_pending = true;
    }
}

fn take_live_sample_rebase(live_sample_rebase_pending: &mut bool) -> bool {
    std::mem::take(live_sample_rebase_pending)
}

fn format_status_line(metadata: Option<&ControllerMetadata>) -> String {
    match metadata {
        Some(metadata) => format!(
            "controller: selected {} name={:?} instance_id={}",
            metadata.kind.diagnostic_label(),
            metadata.name,
            metadata.instance_id
        ),
        None => "controller: selected none".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        acknowledge_pending_neutral, format_status_line, lifecycle_action, mapped_snapshot,
        neutral_snapshot_if_pending, raw_snapshot, select_device, take_live_sample_rebase,
        ActiveIdentity, ControllerKind, ControllerMetadata, DeviceSelection, LifecycleAction,
        LifecycleEvent,
    };
    use crate::gamepad::GamepadSnapshot;
    use crate::sdl::{DeviceIndex, GameControllerState, InputDeviceInfo, JoystickState};

    fn device(index: i32, mapped: bool) -> InputDeviceInfo {
        InputDeviceInfo {
            device_index: DeviceIndex::new(index).unwrap(),
            name: format!("device {index}"),
            mapped,
        }
    }

    fn active(kind: ControllerKind, instance_id: i32) -> Option<ActiveIdentity> {
        Some(ActiveIdentity { kind, instance_id })
    }

    #[test]
    fn selection_prefers_the_first_mapped_device() {
        let devices = [device(0, false), device(1, true), device(2, true)];

        assert_eq!(
            select_device(&devices),
            Some(DeviceSelection::Mapped(DeviceIndex::new(1).unwrap()))
        );
    }

    #[test]
    fn selection_uses_only_raw_index_zero_when_no_mapping_exists() {
        let devices = [device(4, false), device(0, false), device(2, false)];
        assert_eq!(
            select_device(&devices),
            Some(DeviceSelection::RawIndexZero(DeviceIndex::new(0).unwrap()))
        );

        let devices_without_zero = [device(1, false), device(2, false)];
        assert_eq!(select_device(&devices_without_zero), None);
    }

    #[test]
    fn add_events_rescan_only_while_no_controller_is_active() {
        assert_eq!(
            lifecycle_action(None, LifecycleEvent::Added),
            LifecycleAction::Rescan
        );
        assert_eq!(
            lifecycle_action(
                active(ControllerKind::MappedGameController, 17),
                LifecycleEvent::Added,
            ),
            LifecycleAction::Ignore
        );
        assert_eq!(
            lifecycle_action(
                active(ControllerKind::RawJoystickFallback, 17),
                LifecycleEvent::Added,
            ),
            LifecycleAction::Ignore
        );
    }

    #[test]
    fn mapped_removal_requires_the_active_stable_instance_id() {
        let mapped = active(ControllerKind::MappedGameController, 17);

        assert_eq!(
            lifecycle_action(mapped, LifecycleEvent::MappedRemoved(17)),
            LifecycleAction::DisconnectAndRescan
        );
        assert_eq!(
            lifecycle_action(mapped, LifecycleEvent::MappedRemoved(18)),
            LifecycleAction::Ignore
        );
        assert_eq!(
            lifecycle_action(mapped, LifecycleEvent::RawRemoved(17)),
            LifecycleAction::Ignore
        );
    }

    #[test]
    fn raw_removal_requires_the_active_stable_instance_id() {
        let raw = active(ControllerKind::RawJoystickFallback, 23);

        assert_eq!(
            lifecycle_action(raw, LifecycleEvent::RawRemoved(23)),
            LifecycleAction::DisconnectAndRescan
        );
        assert_eq!(
            lifecycle_action(raw, LifecycleEvent::RawRemoved(24)),
            LifecycleAction::Ignore
        );
        assert_eq!(
            lifecycle_action(raw, LifecycleEvent::MappedRemoved(23)),
            LifecycleAction::Ignore
        );
    }

    #[test]
    fn remap_refreshes_only_the_matching_mapped_controller() {
        assert_eq!(
            lifecycle_action(
                active(ControllerKind::MappedGameController, 31),
                LifecycleEvent::MappedRemapped(31),
            ),
            LifecycleAction::RefreshMapped
        );
        assert_eq!(
            lifecycle_action(
                active(ControllerKind::MappedGameController, 31),
                LifecycleEvent::MappedRemapped(32),
            ),
            LifecycleAction::Ignore
        );
        assert_eq!(
            lifecycle_action(
                active(ControllerKind::RawJoystickFallback, 31),
                LifecycleEvent::MappedRemapped(31),
            ),
            LifecycleAction::Ignore
        );
    }

    #[test]
    fn removal_and_remap_events_do_nothing_without_an_active_controller() {
        for event in [
            LifecycleEvent::MappedRemoved(1),
            LifecycleEvent::MappedRemapped(1),
            LifecycleEvent::RawRemoved(1),
        ] {
            assert_eq!(lifecycle_action(None, event), LifecycleAction::Ignore);
        }
    }

    #[test]
    fn disconnect_neutral_remains_pending_across_frames_until_acknowledged() {
        let mut pending = true;
        let mut live_sample_rebase_pending = false;

        assert_eq!(
            neutral_snapshot_if_pending(pending),
            Some(GamepadSnapshot::default())
        );
        assert_eq!(
            neutral_snapshot_if_pending(pending),
            Some(GamepadSnapshot::default()),
            "a presentation frame without a simulation tick must not consume neutral input"
        );

        acknowledge_pending_neutral(&mut pending, &mut live_sample_rebase_pending);
        assert_eq!(neutral_snapshot_if_pending(pending), None);
        assert!(take_live_sample_rebase(&mut live_sample_rebase_pending));
        assert!(
            !take_live_sample_rebase(&mut live_sample_rebase_pending),
            "only the first live replacement sample should request a rebase"
        );
    }

    #[test]
    fn mapped_samples_preserve_every_logical_control() {
        let snapshot = mapped_snapshot(GameControllerState {
            left_x: -12_345,
            left_y: 23_456,
            left_trigger: 12_000,
            right_trigger: 30_000,
            dpad_up: true,
            dpad_down: false,
            dpad_left: true,
            dpad_right: false,
            south_pressed: true,
            east_pressed: false,
            start_pressed: true,
            back_pressed: false,
            left_shoulder_pressed: true,
            right_shoulder_pressed: false,
        });

        assert_eq!(
            snapshot,
            GamepadSnapshot {
                left_stick_x: -12_345,
                left_stick_y: 23_456,
                left_trigger: 12_000,
                right_trigger: 30_000,
                dpad_up: true,
                dpad_down: false,
                dpad_left: true,
                dpad_right: false,
                south_pressed: true,
                east_pressed: false,
                start_pressed: true,
                back_pressed: false,
                left_shoulder_pressed: true,
                right_shoulder_pressed: false,
            }
        );
    }

    #[test]
    fn raw_samples_fill_only_the_common_axes_and_south_button() {
        let snapshot = raw_snapshot(JoystickState {
            x_axis: -20_000,
            y_axis: 10_000,
            jump_pressed: true,
        });

        assert_eq!(
            snapshot,
            GamepadSnapshot {
                left_stick_x: -20_000,
                left_stick_y: 10_000,
                south_pressed: true,
                ..GamepadSnapshot::default()
            }
        );
    }

    #[test]
    fn diagnostics_state_the_selection_kind_name_and_instance_id() {
        let metadata = ControllerMetadata {
            kind: ControllerKind::MappedGameController,
            name: "Example Pad".to_string(),
            instance_id: 44,
        };

        assert_eq!(
            format_status_line(Some(&metadata)),
            "controller: selected mapped name=\"Example Pad\" instance_id=44"
        );
        assert_eq!(format_status_line(None), "controller: selected none");
    }
}
