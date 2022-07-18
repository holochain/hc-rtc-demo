use crate::*;
use std::collections::HashMap;

pub struct Core {
    is_main: bool,
    core_send: CoreSend,
}

impl Clone for Core {
    fn clone(&self) -> Self {
        Self {
            is_main: false,
            core_send: self.core_send.clone(),
        }
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        if self.is_main {
            let _ = self.core_send.send(CoreCmd::Shutdown);
        }
    }
}

impl Core {
    pub fn new() -> Self {
        let (core_send, core_recv) = tokio::sync::mpsc::unbounded_channel();

        let main_core = Self { is_main: true, core_send: core_send.clone() };

        tokio::task::spawn(core_task(main_core.clone(), core_send, core_recv));

        main_core
    }

    pub fn addr(&self, addr: url::Url) {
        let _ = self.core_send.send(CoreCmd::Addr(addr));
    }

    pub fn sig(&self, sig: sig::Sig) {
        let _ = self.core_send.send(CoreCmd::Sig(sig));
    }

    pub fn ice(&self, ice: serde_json::Value) {
        let _ = self.core_send.send(CoreCmd::ICE(ice));
    }

    pub fn sig_msg(&self, msg: hc_rtc_sig::cli::SigMessage) {
        let _ = self.core_send.send(CoreCmd::SigMsg(msg));
    }

    pub fn drop_con(&self, id: state::PeerId) {
        let _ = self.core_send.send(CoreCmd::DropCon(id));
    }
}

enum CoreCmd {
    Shutdown,
    Tick,
    Addr(url::Url),
    ICE(serde_json::Value),
    Sig(sig::Sig),
    SigMsg(hc_rtc_sig::cli::SigMessage),
    DropCon(state::PeerId),
}

type CoreSend = tokio::sync::mpsc::UnboundedSender<CoreCmd>;
type CoreRecv = tokio::sync::mpsc::UnboundedReceiver<CoreCmd>;

#[allow(unused_variables, unused_assignments)]
async fn core_task(
    core: Core,
    core_send: CoreSend,
    mut core_recv: CoreRecv,
) {
    let tick_abort = tokio::task::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            if core_send.send(CoreCmd::Tick).is_err() {
                break;
            }
        }
    });

    struct KillTick(tokio::task::JoinHandle<()>);

    impl Drop for KillTick {
        fn drop(&mut self) {
            tracing::warn!("DemoShuttingDown");
            self.0.abort();
        }
    }

    let _kill_tick = KillTick(tick_abort);

    use hc_rtc_sig::cli::SigMessage;

    let state = state::State::new();

    let mut ice_servers = serde_json::json!([]);
    let mut sig = None;
    let mut loc_id = None;
    let mut loc_pk = None;
    let mut con_map = HashMap::new();

    while let Some(cmd) = core_recv.recv().await {
        match cmd {
            CoreCmd::Shutdown => break,
            CoreCmd::Tick => {
                tracing::trace!("tick");
                for id in state.check_want_outgoing() {
                    // this is an "outgoing" connection,
                    // that is, the one that will make the webrtc "offer".
                    let is_out = true;
                    let con = con::Con::new(
                        core.clone(),
                        id.clone(),
                        ice_servers.clone(),
                        is_out,
                    );
                    con_map.insert(id, con);
                }
            }
            CoreCmd::DropCon(id) => {
                con_map.remove(&id);
            }
            CoreCmd::Addr(addr) => {
                loc_id = Some(hc_rtc_sig::signal_id_from_addr(&addr).unwrap());
                loc_pk = Some(hc_rtc_sig::pk_from_addr(&addr).unwrap());
                tracing::info!(?loc_id, ?loc_pk, "recv local id");
            }
            CoreCmd::ICE(got_ice) => {
                ice_servers = got_ice;
                tracing::info!(%ice_servers);
            }
            CoreCmd::Sig(got_sig) => {
                sig = Some(got_sig);
            }
            CoreCmd::SigMsg(msg) => match msg {
                SigMessage::Offer {
                    rem_id,
                    rem_pk,
                    offer,
                } => {
                    tracing::error!(?rem_id, ?rem_pk, ?offer, "recv offer");
                }
                SigMessage::Answer {
                    rem_id,
                    rem_pk,
                    answer,
                } => {
                    tracing::error!(?rem_id, ?rem_pk, ?answer, "recv answer");
                }
                SigMessage::ICE {
                    rem_id,
                    rem_pk,
                    ice,
                } => {
                    tracing::error!(?rem_id, ?rem_pk, ?ice, "recv ice");
                }
                SigMessage::Demo { rem_id, rem_pk } => {
                    if &rem_id != loc_id.as_ref().unwrap() && &rem_pk != loc_pk.as_ref().unwrap() {
                        state.discover(rem_id, rem_pk);
                    }
                }
            },
        }
    }
}
