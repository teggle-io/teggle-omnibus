use cosmwasm_std::StdError;
use yaml_rust::{Yaml, YamlLoader};
use yaml_rust::yaml::Hash;

pub const CFG_KEY_CORTEX: &'static str = "cortex";
pub const CFG_KEY_NAME: &'static str = "name";
pub const CFG_KEY_VERSION: &'static str = "version";

pub const REQ_KEYS_CORTEX: &'static [&'static str] = &[CFG_KEY_NAME, CFG_KEY_VERSION];

pub struct CortexConfig {
    yaml: Option<Yaml>,
}

impl CortexConfig {
    pub fn new() -> Self {
        Self {
            yaml: None
        }
    }

    pub fn load_source(&mut self, cfg_source: &str) -> Result<(), StdError> {
        let mut yaml_vec = YamlLoader::load_from_str(cfg_source).map_err(|err| {
            StdError::GenericErr {
                msg: format!("yaml error: {err}"),
                backtrace: None,
            }
        })?;

        if yaml_vec.len() != 1 {
            return Err(StdError::GenericErr {
                msg: format!("wrong number of documents detected (got {}, expected 1)",
                             yaml_vec.len()),
                backtrace: None,
            });
        }

        self.yaml = Some(yaml_vec.swap_remove(0));
        self.validate()?;

        Ok(())
    }

    pub fn validate(&mut self) -> Result<(), StdError> {
        let cortex = self.yaml()[CFG_KEY_CORTEX].as_hash();
        if cortex.is_none() {
            return Err(StdError::GenericErr {
                msg: format!("config missing key '{}'", CFG_KEY_CORTEX),
                backtrace: None,
            });
        }

        let cortex = cortex.unwrap();
        for req in REQ_KEYS_CORTEX {
            let value = cortex.get(&Yaml::from_str(req));
            if value.is_none() {
                return Err(StdError::GenericErr {
                    msg: format!("config missing key '{}.{}'", CFG_KEY_CORTEX, req),
                    backtrace: None,
                });
            }

            let value = value.unwrap().as_str();
            if value.is_none() || value.unwrap().len() <= 0 {
                return Err(StdError::GenericErr {
                    msg: format!("config missing key '{}.{}' (or not a string)", CFG_KEY_CORTEX, req),
                    backtrace: None,
                });
            }
        }

        Ok(())
    }

    pub fn yaml(&mut self) -> &Yaml {
        self.yaml.as_ref().unwrap()
    }

    pub fn cortex(&mut self) -> &Hash {
        self.yaml()[CFG_KEY_CORTEX].as_hash().unwrap()
    }

    pub fn cortex_name(&mut self) -> String {
        unwrap_hash_str(self.cortex(), CFG_KEY_NAME).to_string()
    }

    pub fn cortex_version(&mut self) -> String {
        unwrap_hash_str(self.cortex(), CFG_KEY_VERSION).to_string()
    }
}

// Util

#[inline]
fn unwrap_hash_key<'a>(hash: &'a Hash, key: &'a str) -> &'a Yaml {
    return hash.get(&Yaml::from_str(key)).unwrap()
}

#[inline]
fn unwrap_hash_str<'a>(hash: &'a Hash, key: &'a str) -> &'a str {
    unwrap_hash_key(hash, key).as_str().unwrap()
}