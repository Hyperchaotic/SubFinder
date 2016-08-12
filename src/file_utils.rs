use std::fs::OpenOptions;
use std::path::Path;
use zip::ZipArchive;
use hyper::Client;
use error::SubError;
use std::io::{self, Cursor};

/// Download zipped data from URL to memory buffer and unzip .SRT to file
pub fn download_to_srt(url: &str, srt_path: &str) -> Result<(), SubError> {

    // Download zipped subtitles
    let client = Client::new();
    let mut res = try!(client.get(url).send());

    // Copy data into vec
    let mut data: Vec<u8> = vec![];
    try!(io::copy(&mut res, &mut data));

    // Find SRT data file in zip
    let reader = Cursor::new(data);
    let mut zip = try!(ZipArchive::new(reader));

    let mut srt_idx: Option<usize> = None;
    for i in 0..zip.len() {
        let file = try!(zip.by_index(i));
        let ext = Path::new(file.name()).extension().unwrap_or_default();
        if ext == "srt" {
            srt_idx = Some(i);
            break;
        }
    }
    // Write data to new SRT file
    if let Some(idx) = srt_idx {
        let mut srt_data = try!(zip.by_index(idx));
        let mut out = try!(OpenOptions::new().write(true).create(true).truncate(true).open(srt_path));
        try!(io::copy(&mut srt_data, &mut out));
        return Ok(());
    }

    Err(SubError::ZipEmpty)
}
