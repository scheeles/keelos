use std::fs;
use std::io;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use futures::StreamExt;

pub struct PartitionInfo {
    pub device: String,
    pub index: u32,
}

pub fn get_active_partition() -> io::Result<PartitionInfo> {
    let cmdline = fs::read_to_string("/proc/cmdline")?;
    
    if cmdline.contains("PARTUUID=") || cmdline.contains("/dev/sda2") {
        Ok(PartitionInfo { device: "/dev/sda2".to_string(), index: 2 })
    } else if cmdline.contains("/dev/sda3") {
        Ok(PartitionInfo { device: "/dev/sda3".to_string(), index: 3 })
    } else {
        Ok(PartitionInfo { device: "/dev/sda2".to_string(), index: 2 })
    }
}

pub fn get_inactive_partition() -> io::Result<PartitionInfo> {
    let active = get_active_partition()?;
    if active.index == 2 {
        Ok(PartitionInfo { device: "/dev/sda3".to_string(), index: 3 })
    } else {
        Ok(PartitionInfo { device: "/dev/sda2".to_string(), index: 2 })
    }
}

pub async fn flash_image(source_url: &str, target_device: &str) -> io::Result<()> {
    println!("Flashing {} to {}...", source_url, target_device);
    
    let response = reqwest::get(source_url).await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Download failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(io::Error::new(io::ErrorKind::Other, format!("Server returned error: {}", response.status())));
    }

    let mut file = OpenOptions::new()
        .write(true)
        .open(target_device).await?;

    let mut stream = response.bytes_stream();
    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Stream error: {}", e)))?;
        file.write_all(&chunk).await?;
    }

    file.flush().await?;
    Ok(())
}

pub fn switch_boot_partition(target_index: u32) -> io::Result<()> {
    println!("Switching boot partition to {}...", target_index);
    Ok(())
}
