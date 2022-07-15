use crate::*;

pub struct Sig {
    cmd_send: tokio::sync::mpsc::UnboundedSender<SigCmd>,
}

#[derive(Debug)]
enum SigCmd {
    Shutdown,
}

impl Drop for Sig {
    fn drop(&mut self) {
        let _ = self.cmd_send.send(SigCmd::Shutdown);
    }
}

pub async fn load(config: Arc<config::Config>, lair: lair::Lair) -> Result<Sig> {
    let (cmd_send, cmd_recv) = tokio::sync::mpsc::unbounded_channel();

    tokio::task::spawn(sig_task(config, lair, cmd_recv));

    let sig = Sig { cmd_send };

    Ok(sig)
}

async fn sig_task(
    config: Arc<config::Config>,
    lair: lair::Lair,
    mut cmd_recv: tokio::sync::mpsc::UnboundedReceiver<SigCmd>,
) {
    let mut backoff = std::time::Duration::from_secs(1);
    let mut signal_url_iter = config.signal.iter();

    'sig_top: loop {
        let signal_url = match signal_url_iter.next() {
            Some(url) => url,
            None => {
                signal_url_iter = config.signal.iter();
                match signal_url_iter.next() {
                    Some(url) => url,
                    None => panic!("EmptySignalConfig"),
                }
            }
        };

        tracing::info!(%signal_url, "Connecting to signal server...");

        let sig_cli = match hc_rtc_sig::cli::Cli::builder()
            .with_lair_client(lair.lair_client.clone())
            .with_lair_tag(config.lair_tag.clone())
            .with_url(signal_url.clone())
            .build()
            .await
        {
            Ok(sig_cli) => sig_cli,
            Err(err) => {
                let mut secs = backoff.as_secs() * 2;
                if secs > 60 {
                    secs = 60;
                }
                backoff = std::time::Duration::from_secs(secs);
                tracing::warn!(?err, ?backoff);
                tokio::time::sleep(backoff).await;
                continue 'sig_top;
            }
        };

        backoff = std::time::Duration::from_secs(2);

        let signal_addr = sig_cli.local_addr().clone();

        tracing::info!(%signal_addr);

        while let Some(cmd) = cmd_recv.recv().await {
            tracing::info!(?cmd);
        }
    }
}
