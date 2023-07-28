pub use kaspa_wallet_core::imports::DashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use workflow_log::log_info;
// use workflow_core::enums::{Describe,EnumTrait};

pub fn toggle(flag: &Arc<AtomicBool>) -> &'static str {
    let v = !flag.load(Ordering::SeqCst);
    flag.store(v, Ordering::SeqCst);
    if v {
        "on"
    } else {
        "off"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Track {
    Daa = 0,
    Balance,
    Pending,
    Tx,
}

impl FromStr for Track {
    type Err = String;
    fn from_str(s: &str) -> Result<Track, String> {
        match s {
            "daa" => Ok(Track::Daa),
            "balance" => Ok(Track::Balance),
            "pending" => Ok(Track::Pending),
            "tx" => Ok(Track::Tx),
            _ => Err(format!("unknown attribute '{}'", s)),
        }
    }
}

impl ToString for Track {
    fn to_string(&self) -> String {
        match self {
            Track::Daa => "daa".to_string(),
            Track::Balance => "balance".to_string(),
            Track::Pending => "pending".to_string(),
            Track::Tx => "tx".to_string(),
        }
    }
}

pub struct Flags(DashMap<Track, Arc<AtomicBool>>);

impl Default for Flags {
    fn default() -> Self {
        let mut map = DashMap::new();
        let iter = [(Track::Daa, false), (Track::Balance, false), (Track::Pending, true), (Track::Tx, true)]
            .into_iter()
            .map(|(flag, default)| (flag, Arc::new(AtomicBool::new(default))));
        map.extend(iter);
        Flags(map)
    }
}

impl Flags {
    pub fn map(&self) -> &DashMap<Track, Arc<AtomicBool>> {
        &self.0
    }

    pub fn toggle(&self, track: Track) {
        let flag = self.0.get(&track).unwrap();
        let v = !flag.load(Ordering::SeqCst);
        flag.store(v, Ordering::SeqCst);
        let s = if v { "on" } else { "off" };
        log_info!("{} is {s}", track.to_string());
    }

    pub fn get(&self, track: Track) -> bool {
        self.0.get(&track).unwrap().load(Ordering::SeqCst)
    }
}
