use crate::*;

pub struct Sig {
    cmd_send: tokio::sync::mpsc::UnboundedSender<SigCmd>,
}

#[derive(Debug)]
enum SigCmd {
    Shutdown,
    Demo,
}

impl Drop for Sig {
    fn drop(&mut self) {
        let _ = self.cmd_send.send(SigCmd::Shutdown);
    }
}

pub async fn load(config: Arc<config::Config>, lair: lair::Lair, core: core::Core) -> Result<()> {
    let (cmd_send, cmd_recv) = tokio::sync::mpsc::unbounded_channel();

    core.sig(Sig {
        cmd_send: cmd_send.clone(),
    });

    tokio::task::spawn(sig_task(core, config, lair, cmd_send, cmd_recv));

    Ok(())
}

async fn sig_task(
    core: core::Core,
    config: Arc<config::Config>,
    lair: lair::Lair,
    cmd_send: tokio::sync::mpsc::UnboundedSender<SigCmd>,
    mut cmd_recv: tokio::sync::mpsc::UnboundedReceiver<SigCmd>,
) {
    let demo_abort = tokio::task::spawn(async move {
        loop {
            use rand::Rng;
            let secs = rand::thread_rng().gen_range(30..60);
            tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            if cmd_send.send(SigCmd::Demo).is_err() {
                break;
            }
        }
    });

    struct KillDemo(tokio::task::JoinHandle<()>);

    impl Drop for KillDemo {
        fn drop(&mut self) {
            self.0.abort();
        }
    }

    let _kill_demo = KillDemo(demo_abort);

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

        let recv_cb = {
            let core = core.clone();
            Box::new(move |msg| {
                core.sig_msg(msg);
            })
        };

        let mut sig_cli = match hc_rtc_sig::cli::Cli::builder()
            .with_recv_cb(recv_cb)
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

        core.ice(sig_cli.ice_servers().clone());

        let signal_addr = sig_cli.local_addr().clone();
        if let Err(err) = sig_cli.demo().await {
            tracing::warn!(?err);
            continue 'sig_top;
        }

        tracing::info!(%signal_addr);
        core.addr(signal_addr);

        while let Some(cmd) = cmd_recv.recv().await {
            match cmd {
                SigCmd::Shutdown => break,
                SigCmd::Demo => {
                    if let Err(err) = sig_cli.demo().await {
                        tracing::warn!(?err);
                        continue 'sig_top;
                    }
                }
            }
        }
    }
}
