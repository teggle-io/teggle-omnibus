use cosmwasm_std::StdError;
use rhai::{Dynamic, Map};

pub const CFG_KEY_CORTEX: &'static str = "cortex";
pub const CFG_KEY_NAME: &'static str = "name";
pub const CFG_KEY_VERSION: &'static str = "version";

pub const REQ_KEYS_CORTEX: &'static [&'static str] = &[CFG_KEY_NAME, CFG_KEY_VERSION];

pub struct CortexConfig {
    map: Map,
}

impl CortexConfig {
    pub fn new(map: Map) -> Self {
        Self {
            map
        }
    }

    pub fn init(&mut self) -> Result<(), StdError> {
        let name = self.get("cortex.name");
        if name.type_name() == "()" {
            return Err(StdError::GenericErr {
                msg: format!("key not found."),
                backtrace: None,
            })
        }

        let name_str: String = name.into_string().map_err(|err| {
            return StdError::GenericErr {
                msg: format!("failed to convert blah: {err}"),
                backtrace: None,
            }
        })?;

        println!("name: {}", name_str);

        Ok(())
    }

    pub fn get(&self, key: &str) -> Dynamic {
        if key.len() == 0 {
            println!("here 1");
            return Dynamic::from(());
        }

        let split = key.split(".");
        let keys = split.collect::<Vec<&str>>();

        let cur_key_opt = keys.get(0);
        if cur_key_opt.is_none() {
            println!("here 2");
            return Dynamic::UNIT;
        }

        let mut cur = match self.map.get(cur_key_opt.as_ref().unwrap()) {
            None => {
                println!("here 3");
                return Dynamic::UNIT;
            }
            Some(cur) => cur.clone()
        };

        if keys.len() >= 2 {
            for ki in 1..keys.len() {
                // Is cur a Map?
                if cur.is::<Map>() != true {
                    println!("here 4.1");
                    return Dynamic::UNIT;
                }

                let cur_map = cur.read_lock::<Map>().unwrap();

                // Get next key
                let cur_key_opt = keys.get(ki);
                if cur_key_opt.is_none() {
                    println!("here 5");
                    return Dynamic::UNIT;
                }

                // Get value
                cur = match cur_map.get(cur_key_opt.as_ref().unwrap()) {
                    None => {
                        println!("here 7");
                        return Dynamic::UNIT;
                    }
                    Some(cur) => cur.clone()
                };
            }
        }

        return cur;
    }

    pub fn cortex_name(&mut self) -> String {
        "CHANGE ME".to_string()
    }

    pub fn cortex_version(&mut self) -> String {
        "CHANGE ME".to_string()
    }
}
