use dashmap::DashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use workflow_log::log_info;

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
    Utxo,
}

impl FromStr for Track {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Track, String> {
        match s {
            "daa" => Ok(Track::Daa),
            "balance" => Ok(Track::Balance),
            "pending" => Ok(Track::Pending),
            "tx" => Ok(Track::Tx),
            "utxo" => Ok(Track::Utxo),
            _ => Err(format!("unknown attribute '{}'", s)),
        }
    }
}

impl fmt::Display for Track {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Track::Daa => write!(f, "daa"),
            Track::Balance => write!(f, "balance"),
            Track::Pending => write!(f, "pending"),
            Track::Tx => write!(f, "tx"),
            Track::Utxo => write!(f, "utxo"),
        }
    }
}

pub struct Flags(DashMap<Track, Arc<AtomicBool>>);

impl Default for Flags {
    fn default() -> Self {
        let mut map = DashMap::new();
        let iter = [(Track::Daa, false), (Track::Balance, false), (Track::Pending, false), (Track::Tx, false), (Track::Utxo, false)]
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
