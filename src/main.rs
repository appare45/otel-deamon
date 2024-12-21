use std::{
    env::{self},
    fs::{File, Permissions},
    io::Write,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
    str::FromStr,
};
use tokio::signal;

use base64::prelude::*;
use reqwest::{redirect::Policy, Client, IntoUrl, Url};
use tokio::fs::{self, create_dir};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pwd = env::current_dir().unwrap();
    let binary_path = find_binary_root(&pwd);
    if !binary_path.exists() {
        create_dir(&binary_path).await?;
    }
    let binary_row_name = "my-otel-collector-macos-14";
    let latest_url: Url =
        Url::parse("https://github.com/appare45/my-otel-collector/releases/latest")?;
    let latest_release_url = get_latest_version_url(latest_url).await?;
    let download_url = format!("{}/{}", latest_release_url, binary_row_name);
    let download_url =
        Url::from_str(str::replace(download_url.as_str(), "tag", "download").as_str())?;
    let binary_id = BASE64_URL_SAFE.encode(md5::compute(download_url.as_str()).as_ref());
    let binary_path = binary_path.join(binary_id);
    if !binary_path.exists() {
        let bytes = reqwest::get(download_url).await?.bytes().await?;
        println!("{:?}", binary_path.as_path().to_str());
        let mut bin = File::create(&binary_path)?;
        bin.write(bytes.as_ref())?;
    }
    debug_assert!(binary_path.exists());
    fs::set_permissions(&binary_path, Permissions::from_mode(0o700)).await?;
    let (_, mut shutdown_recv) = mpsc::unbounded_channel::<i32>();
    let row_path = binary_path.to_str().unwrap_or("");
    let mut command = Command::new(row_path);
    command
        .env("OTEL_RESOURCE_ATTRIBUTES", "service.name=macosprivate")
        .args(["--config", "./config.yaml"]);
    if let Ok(mut child) = command.spawn() {
        println!("started {}", child.id());
        tokio::select! {
            _ = signal::ctrl_c() => {
                child.kill().unwrap();
            },
            _ = shutdown_recv.recv() => {
                child.kill().unwrap();
            },
        }
    }
    println!("{}", row_path);
    println!("{:?}", command.output());
    Ok(())
}

fn find_binary_root(base: &PathBuf) -> PathBuf {
    let mut binary_root = base.clone();
    binary_root.push("bin");
    return binary_root;
}

async fn get_latest_version_url<T: IntoUrl>(url: T) -> Result<Url, Box<dyn std::error::Error>> {
    let client = Client::builder().redirect(Policy::none()).build()?;
    let response = client.get(url).send().await?;
    if response.status().is_redirection() {
        if let Some(location) = response.headers().get("Location") {
            return Ok(Url::from_str(location.to_str()?)?);
        }
    }
    return Ok(response.url().clone());
}
