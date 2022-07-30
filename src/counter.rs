extern crate redis;

use redis::{Client, Commands, IntoConnectionInfo, RedisError};
use std::{
    collections::HashMap,
    fmt::Display,
    sync::{Arc, RwLock},
};

pub enum StateError {
    Redis(RedisError),
    Mem(String),
}

impl From<RedisError> for StateError {
    fn from(err: RedisError) -> StateError {
        StateError::Redis(err)
    }
}

impl From<String> for StateError {
    fn from(err: String) -> StateError {
        StateError::Mem(err)
    }
}

impl Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            StateError::Redis(e) => e.fmt(f),
            StateError::Mem(e) => e.fmt(f),
        }
    }
}

#[derive(Clone, Debug)]
pub struct State {
    pub redis: Option<Client>,
    pub mem: Arc<RwLock<HashMap<String, i64>>>,
}

impl State {
    pub fn new<T: IntoConnectionInfo>(redis_params: T) -> State {
        let client = match redis::Client::open(redis_params) {
            Ok(cli) => {
                // Perform basic connectivity check to Redis.
                match cli
                    .get_connection()
                    .map(|mut con| con.set_ex::<&str, i64, i64>("init", 1, 1))
                {
                    Ok(_) => Some(cli),
                    Err(e) => {
                        println!("using memory store, error connecting to redis: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                println!("using memory store, error constructing redis client: {}", e);
                None
            }
        };
        // Build our database for holding the key/value pairs
        let state = State {
            redis: client,
            mem: Arc::new(RwLock::new(HashMap::new())),
        };
        return state;
    }

    pub fn inc(&self, key: String) -> Result<i64, StateError> {
        match &self.redis {
            Some(r) => {
                let mut con = r.get_connection()?;
                con.incr::<String, i64, i64>(key, 1i64)
                    .map_err(StateError::Redis)
            }
            None => {
                let mut m = self.mem.write().unwrap();
                // *m.get_mut(&key).unwrap() += 1;
                *m.entry(key.clone()).or_insert(0) += 1;
                match m.get(&key) {
                    Some(v) => Ok(*v),
                    None => Err(StateError::Mem(format!("value not found for key: {}", key))),
                }
            }
        }
    }

    pub fn get(&self, key: String) -> Result<i64, StateError> {
        match &self.redis {
            Some(r) => {
                let mut con = r.get_connection()?;
                con.get::<String, i64>(key).map_err(StateError::Redis)
            }
            None => self
                .mem
                .read()
                .unwrap()
                .get(&key)
                .map(|v| *v)
                .ok_or(StateError::Mem("key not found".to_string())),
        }
    }
}
