use cosmwasm_std::StdError;
use rhai::{Dynamic, INT};
use zip_module_resolver::Config;

pub const CFG_KEY_CORTEX_NAME: &'static str = "cortex.name";
pub const CFG_KEY_CORTEX_VERSION: &'static str = "cortex.version";

pub const REQ_STR_KEYS: &'static [&'static str] = &[CFG_KEY_CORTEX_NAME, CFG_KEY_CORTEX_VERSION];

#[derive(Debug, Clone)]
pub struct CortexConfig {
    config: Config,
}

impl CortexConfig {
    pub fn new(config: Config) -> Self {
        Self {
            config
        }
    }

    pub(crate) fn validate(&mut self) -> Result<(), StdError> {
        for vk in REQ_STR_KEYS {
            let val = self.get_str(*vk);
            if val.is_none() {
                return Err(StdError::GenericErr {
                    msg: format!("cortex config missing key '{}'", *vk),
                    backtrace: None,
                });
            }
        }

        Ok(())
    }

    pub fn get(&self, key: &str) -> Dynamic {
        return self.config.get(key);
    }

    pub fn get_str(&self, key: &str) -> Option<String> {
        return self.config.get_str(key);
    }

    pub fn get_int(&self, key: &str) -> Option<INT> {
        return self.config.get_int(key);
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        return self.config.get_bool(key);
    }

    pub fn cortex_name(&self) -> String {
        return self.get_str(CFG_KEY_CORTEX_NAME).unwrap();
    }

    pub fn cortex_version(&self) -> String {
        return self.get_str(CFG_KEY_CORTEX_VERSION).unwrap();
    }
}
