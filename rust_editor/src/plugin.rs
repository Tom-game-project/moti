use anyhow::{Context, Result};
use crossbeam_channel::{Sender};
use std::path::PathBuf;
use wasmtime::*;


#[derive(Debug)]
pub enum PluginEffect {
    Echo(String),
    ApplyTextStyle {
        line: u32,
        start_byte: u32,
        end_byte: u32,
        style_id: u32,
    },
}

pub struct WasmPlugin {
    pub instance: Instance,
    pub store: Store<()>,
    pub highlight_line_func: Option<TypedFunc<(u32, i32, i32), ()>>,
}

pub struct PluginManager {
    engine: Engine,
    pub plugins: Vec<WasmPlugin>,
}

impl PluginManager {
    pub fn new() -> Result<Self> {
        let engine = Engine::new(&Config::new())?;
        Ok(Self {
            engine,
            plugins: Vec::new(),
        })
    }

    pub fn load_plugin(&mut self, path: &PathBuf, effect_sender: Sender<PluginEffect>) -> Result<()> {
        let mut store = Store::new(&self.engine, ());
        let mut linker = Linker::new(&self.engine);

        let sender_clone = effect_sender.clone();
        linker.func_wrap(
            "host",
            "apply_text_style",
            move |line: u32, start_byte: u32, end_byte: u32, style_id: u32| {
                sender_clone
                    .send(PluginEffect::ApplyTextStyle {
                        line,
                        start_byte,
                        end_byte,
                        style_id,
                    })
                    .unwrap();
            },
        )?;

        let module = Module::from_file(&self.engine, path)?;
        let instance = linker.instantiate(&mut store, &module)?;

        let highlight_line_func = instance.get_typed_func::<(u32, i32, i32), ()>(&mut store, "highlight_line").ok();

        self.plugins.push(WasmPlugin {
            instance,
            store,
            highlight_line_func,
        });

        Ok(())
    }

    pub fn trigger_highlight(&mut self, line_idx: usize, line_content: &str) -> Result<()> {
        for plugin in self.plugins.iter_mut() {
            if let Some(func) = &plugin.highlight_line_func {
                let memory = plugin.instance.get_memory(&mut plugin.store, "memory").context("memory export not found")?;
                let ptr = Self::write_string_to_wasm(&mut plugin.store, &plugin.instance, &memory, line_content)?;
                func.call(&mut plugin.store, (line_idx as u32, ptr, line_content.len() as i32))?;
            }
        }
        Ok(())
    }

    fn write_string_to_wasm(store: &mut Store<()>, instance: &Instance, memory: &Memory, s: &str) -> Result<i32> {
        let bytes = s.as_bytes();
        let alloc_func = instance
            .get_typed_func::<i32, i32>(&mut *store, "alloc")
            .context("`alloc` function not found in Wasm module")?;
        
        let ptr = alloc_func.call(&mut *store, bytes.len() as i32)?;
        memory.write(&mut *store, ptr as usize, bytes)?;
        Ok(ptr)
    }
}
