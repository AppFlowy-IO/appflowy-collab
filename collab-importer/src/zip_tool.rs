use anyhow::{Context, Result};
use async_zip::base::read::stream::{Ready, ZipFileReader};
use async_zip::{StringEncoding, ZipString};
use futures::io::AsyncBufRead;
use futures::AsyncReadExt as FuturesAsyncReadExt;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::io::{AsyncWriteExt, BufReader};

use async_zip::base::read::seek::ZipFileReader as SeekZipFileReader;
use tokio::fs::{create_dir_all, OpenOptions};
use tokio_util::compat::TokioAsyncReadCompatExt;
use tokio_util::compat::TokioAsyncWriteCompatExt;

use tracing::error;

pub struct UnzipFile {
  pub file_name: String,
  pub unzip_dir_path: PathBuf,
}

pub async fn unzip_async<R: AsyncBufRead + Unpin>(
  mut zip_reader: ZipFileReader<Ready<R>>,
  out_dir: PathBuf,
) -> Result<UnzipFile, anyhow::Error> {
  let mut unzip_root_folder_name = None;

  #[allow(irrefutable_let_patterns)]
  while let result = zip_reader.next_with_entry().await {
    match result {
      Ok(Some(mut next_reader)) => {
        let entry_reader = next_reader.reader_mut();
        let filename = get_filename(entry_reader.entry().filename())
          .with_context(|| "Failed to extract filename from entry".to_string())?;

        if unzip_root_folder_name.is_none() && filename.ends_with('/') {
          unzip_root_folder_name =
            Some(filename.split('/').next().unwrap_or(&filename).to_string());
        }

        let output_path = out_dir.join(&filename);
        if filename.ends_with('/') {
          fs::create_dir_all(&output_path)
            .await
            .with_context(|| format!("Failed to create directory: {}", output_path.display()))?;
        } else {
          // Ensure parent directories exist
          if let Some(parent) = output_path.parent() {
            if !parent.exists() {
              fs::create_dir_all(parent).await.with_context(|| {
                format!("Failed to create parent directory: {}", parent.display())
              })?;
            }
          }

          // Write file contents
          if let Ok(mut outfile) = File::create(&output_path).await {
            let mut buffer = vec![];
            match entry_reader.read_to_end(&mut buffer).await {
              Ok(_) => {
                outfile.write_all(&buffer).await.with_context(|| {
                  format!("Failed to write data to file: {}", output_path.display())
                })?;
              },
              Err(err) => {
                error!(
                  "Failed to read entry: {:?}. Error: {:?}",
                  entry_reader.entry(),
                  err,
                );
                return Err(anyhow::anyhow!(
                  "Unexpected EOF while reading: {}",
                  filename
                ));
              },
            }
          }
        }

        // Move to the next file in the zip
        zip_reader = next_reader
          .done()
          .await
          .with_context(|| "Failed to move to the next entry")?;
      },
      Ok(None) => break,
      Err(zip_error) => {
        error!("Error reading zip file: {:?}", zip_error);
        break;
      },
    }
  }

  match unzip_root_folder_name {
    None => Err(anyhow::anyhow!("No files found in the zip archive")),
    Some(file_name) => Ok(UnzipFile {
      file_name: file_name.clone(),
      unzip_dir_path: out_dir.join(file_name),
    }),
  }
}

pub fn get_filename(zip_string: &ZipString) -> Result<String, anyhow::Error> {
  match zip_string.encoding() {
    StringEncoding::Utf8 => match zip_string.as_str() {
      Ok(valid_str) => Ok(valid_str.to_string()),
      Err(err) => Err(err.into()),
    },

    StringEncoding::Raw => {
      let raw_bytes = zip_string.as_bytes();
      let os_string = OsString::from_vec(raw_bytes.to_vec());
      Ok(os_string.to_string_lossy().into_owned())
    },
  }
}

/// Check if the first 4 bytes of the buffer match known multi-part zip signatures.
fn is_multi_part_zip_signature(buffer: &[u8; 4]) -> bool {
  const MULTI_PART_SIGNATURES: [[u8; 4]; 2] = [
    [0x50, 0x4b, 0x07, 0x08], // Spanned zip signature
    [0x50, 0x4b, 0x03, 0x04], // Regular zip signature
  ];

  MULTI_PART_SIGNATURES.contains(buffer)
}

/// Async function to check if a file is a multi-part zip by reading the first 4 bytes.
pub async fn is_multi_part_zip(file_path: &Path) -> Result<bool> {
  let mut file = File::open(file_path).await?;
  let mut buffer = [0; 4]; // Read only the first 4 bytes
  file.read_exact(&mut buffer).await?;
  Ok(is_multi_part_zip_signature(&buffer))
}

/// Check if a buffer contains the multi-part zip signature.
pub fn is_multi_part_zip_file(buffer: &[u8; 4]) -> bool {
  is_multi_part_zip_signature(buffer)
}

fn sanitize_file_path(path: &str) -> PathBuf {
  // Replaces backwards slashes
  path.replace('\\', "/")
      // Sanitizes each component
      .split('/')
      .map(sanitize_filename::sanitize)
      .collect()
}

/// Extracts everything from the ZIP archive to the output directory
pub async fn unzip_file(archive: File, out_dir: &Path) -> Result<UnzipFile, anyhow::Error> {
  let mut unzip_root_folder_name = None;
  let archive = BufReader::new(archive).compat();
  let mut reader = SeekZipFileReader::new(archive)
    .await
    .expect("Failed to read zip file");

  for index in 0..reader.file().entries().len() {
    let entry = reader.file().entries().get(index).unwrap();
    let file_name = entry.filename().as_str().unwrap();
    if unzip_root_folder_name.is_none() && file_name.ends_with('/') {
      unzip_root_folder_name = Some(file_name.split('/').next().unwrap_or(file_name).to_string());
    }

    let path = out_dir.join(sanitize_file_path(entry.filename().as_str().unwrap()));
    // If the filename of the entry ends with '/', it is treated as a directory.
    // This is implemented by previous versions of this crate and the Python Standard Library.
    // https://docs.rs/async_zip/0.0.8/src/async_zip/read/mod.rs.html#63-65
    // https://github.com/python/cpython/blob/820ef62833bd2d84a141adedd9a05998595d6b6d/Lib/zipfile.py#L528
    let entry_is_dir = entry.dir().unwrap();
    let mut entry_reader = reader
      .reader_without_entry(index)
      .await
      .expect("Failed to read ZipEntry");

    if entry_is_dir {
      // The directory may have been created if iteration is out of order.
      if !path.exists() {
        create_dir_all(&path)
          .await
          .expect("Failed to create extracted directory");
      }
    } else {
      // Creates parent directories. They may not exist if iteration is out of order
      // or the archive does not contain directory entries.
      let parent = path
        .parent()
        .expect("A file entry should have parent directories");
      if !parent.is_dir() {
        create_dir_all(parent)
          .await
          .expect("Failed to create parent directories");
      }
      let writer = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
        .expect("Failed to create extracted file");
      futures_lite::io::copy(&mut entry_reader, &mut writer.compat_write())
        .await
        .expect("Failed to copy to extracted file");
    }
  }
  match unzip_root_folder_name {
    None => Err(anyhow::anyhow!("No files found in the zip archive")),
    Some(file_name) => Ok(UnzipFile {
      file_name: file_name.clone(),
      unzip_dir_path: out_dir.join(file_name),
    }),
  }
}
