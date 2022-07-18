use crate::*;

pub struct Con {
    con_send: ConSend,
}

impl Drop for Con {
    fn drop(&mut self) {
        let _ = self.con_send.send(ConCmd::Shutdown);
    }
}

impl Con {
    pub fn new(
        core: core::Core,
        id: state::PeerId,
        ice_servers: serde_json::Value,
        is_out: bool,
    ) -> Self {
        let (con_send, con_recv) = tokio::sync::mpsc::unbounded_channel();

        tokio::task::spawn(con_task(core, id, ice_servers, is_out, con_recv));

        Self {
            con_send,
        }
    }
}

enum ConCmd {
    Shutdown,
}

type ConSend = tokio::sync::mpsc::UnboundedSender<ConCmd>;
type ConRecv = tokio::sync::mpsc::UnboundedReceiver<ConCmd>;

#[allow(unused_variables, unused_assignments)]
async fn con_task(
    core: core::Core,
    id: state::PeerId,
    ice_servers: serde_json::Value,
    is_out: bool,
    mut con_recv: ConRecv,
) -> Result<()> {
    tracing::debug!(?id, "open con");

    struct DoneDrop {
        core: core::Core,
        id: state::PeerId,
    }

    impl Drop for DoneDrop {
        fn drop(&mut self) {
            self.core.drop_con(self.id.clone());
        }
    }

    let _done_drop = DoneDrop {
        core: core.clone(),
        id: id.clone(),
    };

    let conf = serde_json::to_string(&serde_json::json!({
        "iceServers": ice_servers,
    }))?;

    let mut peer_con = go_pion_webrtc::PeerConnection::new(&conf, move |evt| {
        tracing::warn!(?evt);
    }).map_err(other_err)?;

    let mut data_chan = None;

    if is_out {
        data_chan = Some(peer_con.create_data_channel("{ \"label\": \"data\" }").map_err(other_err)?);
        let offer = peer_con.create_offer(None).map_err(other_err)?;
        peer_con.set_local_description(&offer).map_err(other_err)?;
        tracing::error!(%offer);
    }

    while let Some(cmd) = con_recv.recv().await {
        match cmd {
            ConCmd::Shutdown => break,
        }
    }

    Ok(())
}
