use omnibus_std::{Api, debug_print, Extern, Querier, Storage};
use rhai::Engine;

pub struct OmnibusEngine  {
    rhai_engine: Engine,
}

impl OmnibusEngine{
    pub fn new() -> Self {
        let mut engine = Self {
            rhai_engine: Engine::new(),
        };

        engine.init();
        engine
    }

    pub fn init(&mut self) {
        self.register_handlers();
        self.register_functions();
    }

    pub fn register_handlers(&mut self) {
        // TODO: Clean up.
        self.engine.on_print(|text| {
            println!("CORTEX[]: {}", text);
            debug_print!("CORTEX[]: {}", text);
        });

        self.engine.on_debug(|text, source, pos| {
            if let Some(source) = source {
                println!("{} @ {:?} | {}", source, pos, text);
                debug_print!("{} @ {:?} | {}", source, pos, text);
            } else if pos.is_none() {
                println!("{}", text);
                debug_print!("{}", text);
            } else {
                println!("{:?} | {}", pos, text);
                debug_print!("{:?} | {}", pos, text);
            }
        });
    }

    pub fn register_functions(&mut self) {
        self.engine.register_fn("do_store", move |key: &str, val: &str| {
            //controller.borrow_mut().deps.storage.set(key.as_bytes(), val.as_bytes());
        });
    }
}
