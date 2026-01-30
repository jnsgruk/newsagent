use std::env;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

pub struct EnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        clear_env("NEWSAGENT_");
    }
}

pub fn with_newsagent_env<'a>(vars: impl IntoIterator<Item = (&'a str, &'a str)>) -> EnvGuard {
    let guard = ENV_LOCK.lock().expect("Failed to lock env mutex");
    clear_env("NEWSAGENT_");
    for (k, v) in vars {
        env::set_var(k, v);
    }
    EnvGuard { _lock: guard }
}

fn clear_env(prefix: &str) {
    for (key, _) in env::vars() {
        if key.starts_with(prefix) {
            env::remove_var(key);
        }
    }
}
