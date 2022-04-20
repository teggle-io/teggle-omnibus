use std::cell::RefCell;
use std::collections::BTreeMap;
use cosmwasm_std::StdError;
use rhai::{Dynamic, Map};

pub const CFG_KEY_CORTEX: &'static str = "cortex";
pub const CFG_KEY_NAME: &'static str = "name";
pub const CFG_KEY_VERSION: &'static str = "version";

pub const REQ_KEYS_CORTEX: &'static [&'static str] = &[CFG_KEY_NAME, CFG_KEY_VERSION];

pub struct CortexConfig {
    map: Map,
    cache: RefCell<BTreeMap<String, Dynamic>>,
}

impl CortexConfig {
    pub fn new(map: Map) -> Self {
        Self {
            map,
            cache: BTreeMap::new().into(),
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

    pub fn get(&self, key: &str) -> &Dynamic {
        let cache_key = key.to_string();

        if let Some(value) = self.cache.borrow().get(&cache_key) {
            return value;
        }

        let val = self._get(key);

        self.cache.borrow_mut().insert(cache_key, val);

        val
    }

    fn _get(&self, key: &str) -> &Dynamic {
        if key.is_empty() {
            return &Dynamic::UNIT;
        }

        let keys = key.split(".").collect::<Vec<_>>();

        let cur_key_opt = keys.get(0);
        if cur_key_opt.is_none() {
            return &Dynamic::UNIT;
        }

        let cur_key = cur_key_opt.unwrap();

        let mut cur = match self.map.get(*cur_key) {
            None => {
                return &Dynamic::UNIT;
            }
            Some(cur) => cur.clone()
        };

        if keys.len() >= 2 {
            for ki in 1..keys.len() {
                // Is cur a Map?
                if cur.is::<Map>() != true {
                    return &Dynamic::UNIT;
                }

                let cur_map = cur.read_lock::<Map>().unwrap();

                // Get next key
                let cur_key_opt = keys.get(ki);
                if cur_key_opt.is_none() {
                    return &Dynamic::UNIT;
                }

                // Get value
                let cur_key = cur_key_opt.unwrap();

                cur = match cur_map.get(*cur_key) {
                    None => {
                        return &Dynamic::UNIT;
                    }
                    Some(cur) => cur.clone()
                };
            }
        }

        return &cur;
    }

    pub fn cortex_name(&mut self) -> String {
        "CHANGE ME".to_string()
    }

    pub fn cortex_version(&mut self) -> String {
        "CHANGE ME".to_string()
    }
}
