use crate::*;
use hc_rtc_sig::Id;
use parking_lot::Mutex;
use std::collections::hash_map;
use std::collections::HashMap;

const MAX_CON: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeerId {
    pub rem_id: Arc<Id>,
    pub rem_pk: Arc<Id>,
}

#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PeerState {
    Unknown,
    Discovered,
    ConnectingOut,
    ConnectingIn,
    Connected,
    BlockUntil(std::time::Instant),
    Block(String),
}

#[derive(Debug)]
pub struct PeerInfo {
    pub id: PeerId,
    pub state: PeerState,
    pub last_contact: Option<std::time::Instant>,
    pub friendly_name: Option<String>,
    pub shoutout: Option<String>,
}

#[derive(Clone)]
pub struct State(Arc<Mutex<StateInner>>);

impl State {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(StateInner::new())))
    }

    pub fn discover(&self, rem_id: Arc<Id>, rem_pk: Arc<Id>) {
        let mut inner = self.0.lock();
        if let hash_map::Entry::Vacant(e) = inner.map.entry(PeerId {
            rem_id: rem_id.clone(),
            rem_pk: rem_pk.clone(),
        }) {
            let id = PeerId { rem_id, rem_pk };
            let info = PeerInfo {
                id,
                state: PeerState::Discovered,
                last_contact: None,
                friendly_name: None,
                shoutout: None,
            };
            tracing::debug!(?info, "Discover");
            e.insert(info);
        }
    }

    pub fn check_want_outgoing(&self) -> Vec<PeerId> {
        let mut out = Vec::new();

        let mut inner = self.0.lock();

        let mut con_count = inner
            .map
            .iter()
            .filter(|(_, i)| {
                i.state == PeerState::ConnectingOut
                    || i.state == PeerState::ConnectingIn
                    || i.state == PeerState::Connected
            })
            .count();

        for (_, i) in inner.map.iter_mut() {
            if con_count >= MAX_CON {
                return out;
            }

            if i.state == PeerState::Discovered {
                i.state = PeerState::ConnectingOut;
                out.push(i.id.clone());
                con_count += 1;
            }
        }

        out
    }
}

struct StateInner {
    map: HashMap<PeerId, PeerInfo>,
}

impl StateInner {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
}
