use super::inventory::ItemId;

pub const BREAK_DURATION: f32 = 0.26;
pub const PLACE_DURATION: f32 = 0.18;
pub const SWITCH_DURATION: f32 = 0.20;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HandAction {
    Idle,
    Break {
        elapsed: f32,
    },
    Place {
        elapsed: f32,
    },
    Switch {
        elapsed: f32,
        from: ItemId,
        to: ItemId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionRequest {
    Place,
    Break,
    Switch { from: ItemId, to: ItemId },
}

impl HandAction {
    pub fn from_request(request: Option<ActionRequest>) -> Self {
        match request {
            Some(ActionRequest::Place) => Self::Place { elapsed: 0.0 },
            Some(ActionRequest::Break) => Self::Break { elapsed: 0.0 },
            Some(ActionRequest::Switch { from, to }) if from != to => Self::Switch {
                elapsed: 0.0,
                from,
                to,
            },
            _ => Self::Idle,
        }
    }

    pub fn choose(place: bool, break_action: bool, switch: Option<(ItemId, ItemId)>) -> Self {
        if place {
            Self::from_request(Some(ActionRequest::Place))
        } else if break_action {
            Self::from_request(Some(ActionRequest::Break))
        } else {
            Self::from_request(switch.map(|(from, to)| ActionRequest::Switch { from, to }))
        }
    }

    pub fn duration(self) -> f32 {
        match self {
            Self::Idle => 0.0,
            Self::Break { .. } => BREAK_DURATION,
            Self::Place { .. } => PLACE_DURATION,
            Self::Switch { .. } => SWITCH_DURATION,
        }
    }

    pub fn elapsed(self) -> f32 {
        match self {
            Self::Idle => 0.0,
            Self::Break { elapsed } | Self::Place { elapsed } | Self::Switch { elapsed, .. } => {
                elapsed
            }
        }
    }

    fn with_elapsed(self, elapsed: f32) -> Self {
        match self {
            Self::Idle => Self::Idle,
            Self::Break { .. } => Self::Break { elapsed },
            Self::Place { .. } => Self::Place { elapsed },
            Self::Switch { from, to, .. } => Self::Switch { elapsed, from, to },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandTransform {
    pub translation: [f32; 3],
    pub rotation_degrees: [f32; 3],
}

impl HandTransform {
    pub const BASE: Self = Self {
        translation: [0.58, -0.52, -0.88],
        rotation_degrees: [-22.0, -28.0, -8.0],
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewModelState {
    pub action: HandAction,
    /// Monotonic interaction token shared with the audio/event layer.
    pub interaction_id: u64,
    pub interaction_time: f32,
    pub walk_phase: f32,
    pub walk_bob: f32,
    pub held_item: ItemId,
}

impl ViewModelState {
    pub fn new(held_item: ItemId) -> Self {
        Self {
            action: HandAction::Idle,
            interaction_id: 0,
            interaction_time: 0.0,
            walk_phase: 0.0,
            walk_bob: 0.0,
            held_item,
        }
    }

    pub fn start_action(&mut self, request: ActionRequest) {
        self.start_action_with_id(request, self.interaction_id.wrapping_add(1).max(1));
    }

    pub fn start_action_with_id(&mut self, request: ActionRequest, interaction_id: u64) {
        self.interaction_id = interaction_id.max(1);
        self.action = HandAction::from_request(Some(request));
    }

    pub fn advance(
        &mut self,
        delta_seconds: f32,
        horizontal_distance: f32,
        walking: bool,
        sprinting: bool,
        paused: bool,
    ) {
        if paused {
            return;
        }
        let delta = delta_seconds.max(0.0);
        self.interaction_time += delta;
        if walking {
            let sprint = if sprinting { 1.35 } else { 1.0 };
            self.walk_phase += horizontal_distance.max(0.0) * std::f32::consts::PI / 0.90 * sprint;
            self.walk_bob = 1.0;
        } else {
            self.walk_bob *= (-std::f32::consts::LN_2 * delta / 0.15).exp();
        }
        let next_elapsed = self.action.elapsed() + delta;
        self.action = self
            .action
            .with_elapsed(next_elapsed.min(self.action.duration()));
        if self.action.duration() > 0.0 && next_elapsed >= self.action.duration() {
            if let HandAction::Switch { to, .. } = self.action {
                self.held_item = to;
            }
            self.action = HandAction::Idle;
        }
    }

    pub fn displayed_item(&self) -> ItemId {
        match self.action {
            HandAction::Switch { elapsed, from, .. } if elapsed < SWITCH_DURATION * 0.5 => from,
            HandAction::Switch { to, .. } => to,
            _ => self.held_item,
        }
    }

    pub fn transform(&self) -> HandTransform {
        let mut result = HandTransform::BASE;
        let t = self.interaction_time;
        result.translation[0] += (t * 0.8).sin() * 0.008;
        result.translation[1] += (t * 1.6).sin() * 0.006;
        result.rotation_degrees[2] += (t * 0.8).sin() * 0.8;

        result.translation[0] += self.walk_phase.sin() * 0.025 * self.walk_bob;
        result.translation[1] += self.walk_phase.cos().abs() * 0.018 * self.walk_bob;
        result.rotation_degrees[2] += self.walk_phase.sin() * 2.5 * self.walk_bob;

        match self.action {
            HandAction::Break { elapsed } => {
                let s = (std::f32::consts::PI * (elapsed / BREAK_DURATION).clamp(0.0, 1.0)).sin();
                result.translation[0] -= 0.10 * s;
                result.translation[1] -= 0.05 * s;
                result.translation[2] += 0.06 * s;
                result.rotation_degrees[0] -= 72.0 * s;
                result.rotation_degrees[1] += 18.0 * s;
                result.rotation_degrees[2] += 24.0 * s;
            }
            HandAction::Place { elapsed } => {
                let s = (std::f32::consts::PI * (elapsed / PLACE_DURATION).clamp(0.0, 1.0)).sin();
                result.translation[1] -= 0.03 * s;
                result.translation[2] -= 0.16 * s;
                result.rotation_degrees[0] += 18.0 * s;
                result.rotation_degrees[2] -= 8.0 * s;
            }
            HandAction::Switch { elapsed, .. } => {
                let p = (elapsed / SWITCH_DURATION).clamp(0.0, 1.0);
                let y_offset = -0.48 * (1.0 - (2.0 * p - 1.0).powi(2));
                result.translation[1] += y_offset;
            }
            HandAction::Idle => {}
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewmodel_action_priority_is_stable() {
        assert!(matches!(
            HandAction::choose(true, true, Some((1, 2))),
            HandAction::Place { .. }
        ));
        assert!(matches!(
            HandAction::choose(false, true, Some((1, 2))),
            HandAction::Break { .. }
        ));
        assert!(matches!(
            HandAction::choose(false, false, Some((1, 2))),
            HandAction::Switch { .. }
        ));
        assert!(matches!(
            HandAction::choose(false, false, None),
            HandAction::Idle
        ));
    }

    #[test]
    fn viewmodel_break_duration_is_point_two_six() {
        assert_eq!(HandAction::Break { elapsed: 0.0 }.duration(), 0.26);
    }

    #[test]
    fn viewmodel_place_duration_is_point_one_eight() {
        assert_eq!(HandAction::Place { elapsed: 0.0 }.duration(), 0.18);
    }

    #[test]
    fn viewmodel_pauses_with_inventory() {
        let mut state = ViewModelState::new(1);
        state.start_action(ActionRequest::Break);
        state.advance(0.10, 0.25, true, false, false);
        let before = state;
        let before_bits = before.transform();
        state.advance(1.0, 2.0, true, true, true);
        assert_eq!(state, before);
        assert_eq!(state.transform(), before_bits);
    }

    #[test]
    fn interaction_id_is_carried_by_hand_action() {
        let mut state = ViewModelState::new(1);
        state.start_action_with_id(ActionRequest::Place, 42);
        assert_eq!(state.interaction_id, 42);
        state.start_action_with_id(ActionRequest::Break, 43);
        assert_eq!(state.interaction_id, 43);
    }

    #[test]
    fn place_uses_contract_item_base_and_z_rotation() {
        let mut state = ViewModelState::new(1);
        state.start_action(ActionRequest::Place);
        let transform = state.transform();
        assert!((transform.translation[0] - HandTransform::BASE.translation[0]).abs() < 1.0e-6);
        assert!((transform.translation[1] - HandTransform::BASE.translation[1]).abs() < 1.0e-6);
        assert!((transform.translation[2] - HandTransform::BASE.translation[2]).abs() < 1.0e-6);
        state.advance(PLACE_DURATION * 0.5, 0.0, false, false, false);
        let mid = state.transform();
        let mut idle = ViewModelState::new(1);
        idle.advance(PLACE_DURATION * 0.5, 0.0, false, false, false);
        let idle_rotation = idle.transform().rotation_degrees[2];
        assert!((mid.rotation_degrees[2] - idle_rotation + 8.0).abs() < 1.0e-5);
    }
}
