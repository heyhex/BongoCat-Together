//! One independent cat runtime, with no configuration/audio/OS input service.
use bongocat_model::{ModelId, ModelPackageLimits, PresetModelCatalog};
use bongocat_render::RenderConsumer;
use bongocat_runtime::{RuntimeCommand, RuntimeOwner};
use std::{path::Path, sync::Arc, time::Duration};

pub struct TogetherCat {
    pub runtime: RuntimeOwner,
    pub frames: RenderConsumer,
}

impl TogetherCat {
    /// Called on a background loader. The native GPU commit remains on the
    /// overlay owner thread and uses the original prepare/commit protocol.
    pub fn prepare(
        preset_root: &Path,
        display: &bongocat_ui_protocol::TogetherLayoutSnapshot,
    ) -> Result<Self, String> {
        let model_key = &display.model;
        let catalog = PresetModelCatalog::open(preset_root, ModelPackageLimits::default())
            .map_err(|_| "friend model catalog unavailable")?;
        let id = ModelId::parse(&model_key.id)
            .or_else(|error| {
                if display.follow_peer_model {
                    ModelId::parse("standard")
                } else {
                    Err(error)
                }
            })
            .map_err(|_| "invalid friend model")?;
        let model = match model_key.origin {
            bongocat_ui_protocol::SettingsModelOrigin::BuiltIn => catalog
                .load(&id)
                .or_else(|error| {
                    if display.follow_peer_model {
                        catalog.load(&ModelId::parse("standard")?)
                    } else {
                        Err(error)
                    }
                })
                .map_err(|_| "friend model unavailable")?,
            bongocat_ui_protocol::SettingsModelOrigin::Imported => {
                let layout = bongocat_config::platform_layout(crate::BUILD_ENVIRONMENT)
                    .map_err(|_| "friend model store unavailable")?;
                let store = bongocat_model_store::ModelStore::new(
                    &layout.models,
                    layout.locks.join("models.writer.lock"),
                    ModelPackageLimits::default(),
                )
                .map_err(|_| "friend model store unavailable")?;
                store
                    .load(&id)
                    .map(bongocat_model::CommittedModel::from)
                    .map_err(|_| "friend model unavailable")?
            }
        };
        let bindings = crate::model_input::input_bindings_for_committed_model(&model);
        let (runtime, frames) = RuntimeOwner::start_with_rendering(true, 128);
        let client = runtime.client();
        client
            .send(RuntimeCommand::SetModelSettings(
                bongocat_runtime::ModelSettings {
                    show_all_pressed_keys: true,
                    ..Default::default()
                },
            ))
            .map_err(|_| "friend key display settings rejected")?;
        client
            .send(RuntimeCommand::SetOverlaySettings(
                bongocat_runtime::OverlaySettings {
                    click_through: false,
                    scale_percent: display.scale_percent,
                    ..Default::default()
                },
            ))
            .map_err(|_| "friend display settings rejected")?;
        client
            .send(RuntimeCommand::SetOverlayVisible(display.visible))
            .map_err(|_| "friend visibility rejected")?;
        let sequence = client
            .send(RuntimeCommand::ActivateModelWithBindings {
                model: Arc::new(model),
                input_bindings: Arc::new(bindings),
            })
            .map_err(|_| "friend model command failed")?;
        let snapshot = client
            .wait_for_model_preparation(sequence, Duration::from_secs(3))
            .ok_or("friend model preparation timed out")?;
        if snapshot.pending_model.is_none() || snapshot.last_command_failure.is_some() {
            return Err("friend model preparation failed".into());
        }
        Ok(Self { runtime, frames })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bongocat_input::{
        InputControl, InputEdge, InputEvent, InputSource, MonotonicMillis, PhysicalKey,
    };

    #[test]
    fn independent_cat_runtimes_do_not_share_pressed_keys() {
        let a = RuntimeOwner::start(true, 64);
        let b = RuntimeOwner::start(true, 64);
        let sequence = a
            .input_producer()
            .publish(InputEvent::Edge {
                control: InputControl::Key(PhysicalKey::KEY_A),
                edge: InputEdge::Down,
                source: InputSource::Capture,
                at: MonotonicMillis::new(1),
            })
            .unwrap();
        let a_state = a
            .client()
            .wait_for_input_sequence(sequence, Duration::from_secs(2))
            .unwrap();
        assert_eq!(a_state.input.pressed_key_count, 1);
        assert_eq!(b.client().snapshot().input.pressed_key_count, 0);
        a.shutdown(Duration::from_secs(2)).unwrap();
        b.shutdown(Duration::from_secs(2)).unwrap();
    }
}
