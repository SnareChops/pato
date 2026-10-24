//! User-saved model positions ("presets"), stored per model so a preset made
//! for one model never lands badly on another.

use serde::{Deserialize, Serialize};

use crate::model::Position;
use crate::pato::plugin::storage;

pub const DEFAULT_DURATION: f64 = 0.5;
/// `MoveModelRequest.timeInSeconds` accepts 0-2.
pub const MAX_DURATION: f64 = 2.0;
pub const MAX_NAME_LEN: usize = 40;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub id: u32,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub size: f64,
    /// Seconds the move takes.
    pub duration: f64,
}

impl Preset {
    /// Whether the model is currently at this preset, within the slack the
    /// polled position and an in-flight move leave.
    pub fn matches(&self, p: &Position) -> bool {
        (self.x - p.x).abs() < 0.02
            && (self.y - p.y).abs() < 0.02
            && (self.rotation - p.rotation).abs() < 1.0
            && (self.size - p.size).abs() < 1.0
    }
}

pub fn clamp_duration(seconds: f64) -> f64 {
    if seconds.is_nan() {
        return DEFAULT_DURATION;
    }
    seconds.clamp(0.0, MAX_DURATION)
}

fn storage_key(model_id: &str) -> String {
    format!("vts_positions_{model_id}")
}

fn parse(json: &str) -> Vec<Preset> {
    serde_json::from_str(json).unwrap_or_default()
}

fn next_id(presets: &[Preset]) -> u32 {
    presets.iter().map(|p| p.id).max().map_or(1, |m| m + 1)
}

/// The presets of the currently loaded model (`model_id` empty: none).
#[derive(Default)]
pub struct Positions {
    model_id: String,
    pub presets: Vec<Preset>,
}

impl Positions {
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn load(model_id: &str) -> Self {
        let presets = if model_id.is_empty() {
            vec![]
        } else {
            storage::get(&storage_key(model_id)).map(|j| parse(&j)).unwrap_or_default()
        };
        Positions { model_id: model_id.to_string(), presets }
    }

    pub fn get(&self, id: u32) -> Option<&Preset> {
        self.presets.iter().find(|p| p.id == id)
    }

    pub fn add(&mut self, name: String, at: &Position, duration: f64) {
        let id = next_id(&self.presets);
        self.presets.push(Preset {
            id,
            name,
            x: at.x,
            y: at.y,
            rotation: at.rotation,
            size: at.size,
            duration,
        });
        self.persist();
    }

    pub fn edit(&mut self, id: u32, name: String, duration: f64) {
        if let Some(p) = self.presets.iter_mut().find(|p| p.id == id) {
            p.name = name;
            p.duration = duration;
            self.persist();
        }
    }

    pub fn remove(&mut self, id: u32) {
        self.presets.retain(|p| p.id != id);
        self.persist();
    }

    fn persist(&self) {
        if self.model_id.is_empty() {
            return;
        }
        if let Ok(json) = serde_json::to_string(&self.presets) {
            storage::set(&storage_key(&self.model_id), &json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(id: u32) -> Preset {
        Preset { id, name: "n".into(), x: 0.5, y: -0.5, rotation: 0.0, size: -20.0, duration: 0.5 }
    }

    #[test]
    fn matches_within_tolerance_only() {
        let p = preset(1);
        let at = |x, size| Position { x, y: -0.5, rotation: 0.0, size };
        assert!(p.matches(&at(0.51, -20.4)));
        assert!(!p.matches(&at(0.6, -20.0)));
        assert!(!p.matches(&at(0.5, -25.0)));
    }

    #[test]
    fn ids_never_reuse_a_removed_one_below_the_max() {
        assert_eq!(next_id(&[]), 1);
        assert_eq!(next_id(&[preset(1), preset(4)]), 5);
    }

    #[test]
    fn duration_is_clamped_to_what_vts_accepts() {
        assert_eq!(clamp_duration(-1.0), 0.0);
        assert_eq!(clamp_duration(9.0), MAX_DURATION);
        assert_eq!(clamp_duration(f64::NAN), DEFAULT_DURATION);
        assert_eq!(clamp_duration(0.75), 0.75);
    }

    #[test]
    fn corrupt_storage_yields_no_presets() {
        assert!(parse("not json").is_empty());
        let json = serde_json::to_string(&[preset(2)]).unwrap();
        assert_eq!(parse(&json), vec![preset(2)]);
    }
}
