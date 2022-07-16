use crate::*;

pub struct Core {
    core_send: CoreSend,
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.core_send.send(CoreCmd::Shutdown);
    }
}

impl Core {
    pub fn new() -> Self {
        let (core_send, core_recv) = tokio::sync::mpsc::unbounded_channel();

        tokio::task::spawn(core_task(core_recv));

        Self {
            core_send,
        }
    }

    pub fn sig(&self, sig: sig::Sig) {
        let _ = self.core_send.send(CoreCmd::Sig(sig));
    }

    pub fn ice(&self, ice: serde_json::Value) {
        let _ = self.core_send.send(CoreCmd::ICE(ice));
    }
}

enum CoreCmd {
    Shutdown,
    Sig(sig::Sig),
    ICE(serde_json::Value),
}

type CoreSend = tokio::sync::mpsc::UnboundedSender<CoreCmd>;
type CoreRecv = tokio::sync::mpsc::UnboundedReceiver<CoreCmd>;

#[allow(unused_variables, unused_assignments)]
async fn core_task(mut core_recv: CoreRecv) {
    let mut ice_servers = serde_json::json!([]);
    let mut sig = None;

    while let Some(cmd) = core_recv.recv().await {
        match cmd {
            CoreCmd::Shutdown => break,
            CoreCmd::Sig(got_sig) => {
                sig = Some(got_sig);
                tracing::error!("got sig");
            }
            CoreCmd::ICE(got_ice) => {
                ice_servers = got_ice;
                tracing::error!(%ice_servers);
            }
        }
    }
}
