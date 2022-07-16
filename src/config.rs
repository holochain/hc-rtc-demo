use crate::*;
use lair_keystore_api::prelude::*;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub friendly_name: String,
    pub shoutout: String,
    pub signal: Vec<url::Url>,
    pub lair_tag: Arc<str>,
    pub lair_passphrase: Arc<str>,
    pub lair_config: Arc<LairServerConfigInner>,
}

impl Config {
    async fn new() -> Result<Arc<Self>> {
        tracing::info!("Generating config...");

        let mut rng = rand::thread_rng();

        let lair_tag: Arc<str> = rand_utf8::rand_utf8(&mut rng, 32).into();
        let lair_passphrase: Arc<str> = rand_utf8::rand_utf8(&mut rng, 32).into();

        let mut data_root = dirs::data_local_dir().unwrap_or_else(|| {
            let mut config = std::path::PathBuf::new();
            config.push(".");
            config
        });
        data_root.push("hc-rtc-demo");

        tokio::fs::create_dir_all(&data_root).await?;

        let passphrase = sodoken::BufRead::new_no_lock(lair_passphrase.as_bytes());

        let lair_config = PwHashLimits::Interactive.with_exec(|| {
            LairServerConfigInner::new(data_root, passphrase)
        }).await?;

        Ok(Arc::new(Self {
            friendly_name: "Holochain<3".into(),
            shoutout: "Holochain rocks!".into(),
            signal: vec![url::Url::parse("hc-rtc-sig:z9_7dB5HPV8tK6y8Q86yBtO-99Aa2QFZOZxfy0w0lDo/127.0.0.1:43489/[::1]:38559").unwrap()],
            lair_tag,
            lair_passphrase,
            lair_config: Arc::new(lair_config),
        }))
    }
}

pub async fn load_config() -> Result<Arc<Config>> {
    let config = load_config_inner().await?;

    if config.friendly_name.as_bytes().len() > 32 {
        return Err(other_err("friendlyName cannot be > 32 utf8 bytes"));
    }

    if config.shoutout.as_bytes().len() > 32 {
        return Err(other_err("shoutout cannot be > 32 utf8 bytes"));
    }

    Ok(config)
}

async fn load_config_inner() -> Result<Arc<Config>> {
    let mut config_fn = dirs::config_dir().unwrap_or_else(|| {
        let mut config = std::path::PathBuf::new();
        config.push(".");
        config
    });
    config_fn.push("hc-rtc-demo.json");
    match read_config(&config_fn).await {
        Ok(r) => Ok(r),
        Err(_) => {
            let _ = tokio::fs::remove_file(&config_fn).await;
            run_init(&config_fn).await?;
            read_config(&config_fn).await
        }
    }
}

async fn read_config(config_fn: &std::path::Path) -> Result<Arc<Config>> {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncReadExt;

    let mut file = match tokio::fs::OpenOptions::new()
        .read(true)
        .open(config_fn)
        .await
    {
        Err(err) => {
            return Err(other_err(format!(
                "Failed to open config file {:?}: {:?}",
                config_fn, err,
            )))
        }
        Ok(file) => file,
    };

    let perms = match file.metadata().await {
        Err(err) => {
            return Err(other_err(format!(
                "Failed to load config file metadata {:?}: {:?}",
                config_fn, err
            )))
        }
        Ok(perms) => perms.permissions(),
    };

    if !perms.readonly() {
        return Err(other_err(format!(
            "Refusing to run with writable config file {:?}",
            config_fn,
        )));
    }

    #[cfg(unix)]
    {
        let mode = perms.mode() & 0o777;
        if mode != 0o400 {
            return Err(other_err(format!(
                "Refusing to run with config file not set to mode 0o400 {:?} 0o{:o}",
                config_fn, mode,
            )));
        }
    }

    let mut conf = String::new();
    if let Err(err) = file.read_to_string(&mut conf).await {
        return Err(other_err(format!(
            "Failed to read config file {:?}: {:?}",
            config_fn, err,
        )));
    }

    let config: Config = match serde_json::from_str(&conf) {
        Err(err) => {
            return Err(other_err(format!(
                "Failed to parse config file {:?}: {:?}",
                config_fn, err,
            )))
        }
        Ok(res) => res,
    };

    Ok(Arc::new(config))
}

async fn run_init(config_fn: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;

    let mut file = tokio::fs::OpenOptions::new();
    file.create_new(true);
    file.write(true);
    let mut file = match file.open(config_fn).await {
        Err(err) => {
            return Err(other_err(format!(
                "Failed to create config file {:?}: {:?}",
                config_fn, err,
            )))
        }
        Ok(file) => file,
    };

    let config = Config::new().await?;
    let mut config = serde_json::to_string_pretty(&config).unwrap();
    config.push('\n');

    if let Err(err) = file.write_all(config.as_bytes()).await {
        return Err(other_err(format!(
            "Failed to initialize config file {:?}: {:?}",
            config_fn, err
        )));
    };

    let mut perms = match file.metadata().await {
        Err(err) => {
            return Err(other_err(format!(
                "Failed to load config file metadata {:?}: {:?}",
                config_fn, err,
            )))
        }
        Ok(perms) => perms.permissions(),
    };
    perms.set_readonly(true);

    #[cfg(unix)]
    perms.set_mode(0o400);

    if let Err(err) = file.set_permissions(perms).await {
        return Err(other_err(format!(
            "Failed to set config file permissions {:?}: {:?}",
            config_fn, err,
        )));
    }

    if let Err(err) = file.shutdown().await {
        return Err(other_err(format!(
            "Failed to flush/close config file: {:?}",
            err
        )));
    }

    tracing::info!("# hc-rtc-demo wrote {:?} #", config_fn);

    Ok(())
}
