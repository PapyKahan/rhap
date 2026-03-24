use anyhow::Result;
use log::debug;
use pipewire as pw;
use pw::registry::GlobalObject;
use pw::spa::utils::dict::DictRef;
use std::cell::RefCell;
use std::rc::Rc;

use super::api::init_pipewire;
use super::device::Device;
use crate::audio::HostTrait;

#[derive(Clone, Copy)]
pub struct Host;

#[derive(Debug, Clone)]
struct SinkInfo {
    id: u32,
    node_name: String,
    description: String,
    alsa_path: Option<String>,
    priority: u32,
}

/// Resolve a PipeWire object.path like "alsa:acp:DAC:1:playback" to an ALSA card number.
/// Uses /proc/asound/<card_id> symlink to map the card ID to a card number.
/// Returns the card number only — the ALSA probe function will use it to check
/// USB descriptors (/proc/asound/cardN/stream0) or probe hw:N,0 via hwparams.
/// Note: the profile_device index in object.path does NOT map to the ALSA PCM
/// device number, so we always probe device 0 (correct for USB DACs; for multi-device
/// HDA cards, capabilities are typically uniform across PCM devices).
fn resolve_alsa_path_from_object_path(object_path: &str) -> Option<String> {
    let parts: Vec<&str> = object_path.split(':').collect();
    if parts.len() < 4 || parts[0] != "alsa" {
        return None;
    }
    let card_id = parts[2];

    let proc_path = format!("/proc/asound/{}", card_id);
    let link_target = std::fs::read_link(&proc_path).ok()?;
    let target_str = link_target.to_str()?;
    let card_num: u32 = target_str.strip_prefix("card")?.parse().ok()?;

    Some(format!("hw:{},0", card_num))
}

fn enumerate_sinks() -> Result<Vec<SinkInfo>> {
    init_pipewire();

    let main_loop = pw::main_loop::MainLoopBox::new(None)?;
    let context = pw::context::ContextBox::new(main_loop.loop_(), None)?;
    let core = context.connect(None)?;
    let registry = core.get_registry()?;

    let sinks: Rc<RefCell<Vec<SinkInfo>>> = Rc::new(RefCell::new(Vec::new()));
    let done = Rc::new(RefCell::new(false));

    let sinks_for_global = Rc::clone(&sinks);

    let _listener = registry
        .add_listener_local()
        .global(move |global: &GlobalObject<&DictRef>| {
            if global.type_ != pw::types::ObjectType::Node {
                return;
            }
            let props = match global.props {
                Some(p) => p,
                None => return,
            };
            let media_class = match props.get("media.class") {
                Some(c) => c,
                None => return,
            };
            if media_class != "Audio/Sink" {
                return;
            }
            let node_name = props.get("node.name").unwrap_or("unknown").to_string();
            let description = props
                .get("node.description")
                .or_else(|| props.get("node.nick"))
                .unwrap_or(&node_name)
                .to_string();

            // Derive ALSA hw: path from object.path (e.g. "alsa:acp:DAC:1:playback").
            // The card ID (e.g. "DAC") resolves to a card number via /proc/asound/<id> symlink.
            let alsa_path = props.get("object.path")
                .and_then(|op| resolve_alsa_path_from_object_path(op));

            let priority = props.get("priority.session")
                .and_then(|p| p.parse::<u32>().ok())
                .unwrap_or(0);

            debug!(
                "Found PipeWire sink: id={}, name={}, desc={}, alsa={:?}, priority={}",
                global.id, node_name, description, alsa_path, priority
            );

            sinks_for_global.borrow_mut().push(SinkInfo {
                id: global.id,
                node_name,
                description,
                alsa_path,
                priority,
            });
        })
        .register();

    // Perform a roundtrip to ensure all globals are received.
    let pending = core.sync(0)?;
    let done_clone = Rc::clone(&done);
    let main_loop_ptr: *const pw::main_loop::MainLoop = &*main_loop;

    let _core_listener = core
        .add_listener_local()
        .done(move |_id, seq| {
            if seq == pending {
                *done_clone.borrow_mut() = true;
                // SAFETY: main_loop_ptr is valid during main_loop.run()
                unsafe { (*main_loop_ptr).quit() };
            }
        })
        .register();

    main_loop.run();

    // Drop listeners to release their Rc clones of `sinks` before unwrapping.
    drop(_core_listener);
    drop(_listener);

    Ok(Rc::try_unwrap(sinks)
        .map_err(|_| anyhow::anyhow!("Sink Rc still held"))?
        .into_inner())
}

impl HostTrait for Host {
    fn get_devices(&self) -> Result<Vec<crate::audio::Device>> {
        let sinks = enumerate_sinks()?;
        // The sink with the highest priority.session is the default
        let max_priority = sinks.iter().map(|s| s.priority).max().unwrap_or(0);
        let devices: Vec<crate::audio::Device> = sinks
            .into_iter()
            .map(|sink| {
                let is_default = sink.priority == max_priority;
                crate::audio::Device::PipeWire(Device::new(
                    sink.id,
                    sink.node_name,
                    sink.description,
                    is_default,
                    sink.alsa_path,
                ))
            })
            .collect();
        Ok(devices)
    }

    fn create_device(&self, id: Option<u32>) -> Result<crate::audio::Device> {
        let sinks = enumerate_sinks()?;
        let max_priority = sinks.iter().map(|s| s.priority).max().unwrap_or(0);
        match id {
            Some(i) => {
                let sink = sinks
                    .into_iter()
                    .nth(i as usize)
                    .ok_or_else(|| anyhow::anyhow!("PipeWire device index {} out of range", i))?;
                Ok(crate::audio::Device::PipeWire(Device::new(
                    sink.id,
                    sink.node_name,
                    sink.description,
                    sink.priority == max_priority,
                    sink.alsa_path,
                )))
            }
            None => {
                // Pick the highest-priority sink (PipeWire's default)
                let sink = sinks
                    .into_iter()
                    .max_by_key(|s| s.priority)
                    .ok_or_else(|| anyhow::anyhow!("No PipeWire sinks found"))?;
                Ok(crate::audio::Device::PipeWire(Device::new(
                    sink.id,
                    sink.node_name,
                    sink.description,
                    true,
                    sink.alsa_path,
                )))
            }
        }
    }

    fn get_default_device(&self) -> Result<crate::audio::Device> {
        self.create_device(None)
    }
}
