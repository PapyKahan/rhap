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
}

/// Resolve a PipeWire object.path like "alsa:acp:DAC:1:playback" to an ALSA hw: path.
/// Uses /proc/asound/<card_id> symlink to map the card ID to a card number.
fn resolve_alsa_path_from_object_path(object_path: &str) -> Option<String> {
    // Format: "alsa:acp:<card_id>:<profile_device>:playback"
    let parts: Vec<&str> = object_path.split(':').collect();
    if parts.len() < 4 || parts[0] != "alsa" {
        return None;
    }
    let card_id = parts[2];

    // Resolve card ID to card number via /proc/asound/<card_id> → cardN symlink
    let proc_path = format!("/proc/asound/{}", card_id);
    let link_target = std::fs::read_link(&proc_path).ok()?;
    let target_str = link_target.to_str()?;
    let card_num: u32 = target_str.strip_prefix("card")?.parse().ok()?;

    // USB audio devices typically have a single PCM device 0.
    // For multi-device cards, device 0 is the primary playback device.
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

            debug!(
                "Found PipeWire sink: id={}, name={}, desc={}, alsa={:?}",
                global.id, node_name, description, alsa_path
            );

            sinks_for_global.borrow_mut().push(SinkInfo {
                id: global.id,
                node_name,
                description,
                alsa_path,
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
        let devices: Vec<crate::audio::Device> = sinks
            .into_iter()
            .enumerate()
            .map(|(i, sink)| {
                crate::audio::Device::PipeWire(Device::new(
                    sink.id,
                    sink.node_name,
                    sink.description,
                    i == 0,
                    sink.alsa_path,
                ))
            })
            .collect();
        Ok(devices)
    }

    fn create_device(&self, id: Option<u32>) -> Result<crate::audio::Device> {
        let sinks = enumerate_sinks()?;
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
                    false,
                    sink.alsa_path,
                )))
            }
            None => {
                let sink = sinks
                    .into_iter()
                    .next()
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
