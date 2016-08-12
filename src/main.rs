extern crate sub_finder;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, BufReader};
use std::{env, mem, thread, fs};
use std::sync::{Arc, Mutex, mpsc};
use std::path::{Path, PathBuf};
use std::result::Result;
use std::collections::HashSet;

use sub_finder::error::SubError;
use sub_finder::client;
use sub_finder::file_utils;

/// Opensubtitle hashing algorithm [hash file size]+[first 64KB]+[last64KB]
const HASH_BLK_SIZE: u64 = 65536;

// Number of threads hashing and retrieving subtitles. Don't flood server pls.
const THREAD_COUNT: u64 = 4;

/// Represent a single movie file in need of subtitles
struct Show {
    full_path: String,
    file_name: String,
    file_size: u64,
    hash: String,
}

impl Show {
    /// Create hash value string from a file, to be used for subtitle lookup
    fn create_hash(&mut self) -> Result<(), std::io::Error> {

        let mut buf = [0u8; 8];
        let mut word: u64 = 0;
        let mut hash_val: u64 = self.file_size;  // seed hash with file size

        let iterations = HASH_BLK_SIZE / mem::size_of::<u64>() as u64;

        let f = try!(File::open(&self.full_path));

        // BufReader is fast
        let mut reader = BufReader::with_capacity(HASH_BLK_SIZE as usize, f);

        {
            // Create a closure for hashing a 64KB section  and run it
            // over start and end of file. Little Endian.
            let mut hash_section = |seek_to_end_block| -> Result<(), std::io::Error> {
                if seek_to_end_block {
                    try!(reader.seek(SeekFrom::Start(self.file_size - HASH_BLK_SIZE)));
                }
                for _ in 0..iterations {
                    try!(reader.read(&mut buf));
                    unsafe { word = mem::transmute(buf); };
                    hash_val = hash_val.wrapping_add(word);
                }
                Ok(())
            };
            try!(hash_section(false));
            try!(hash_section(true));
        }
        self.hash = format!("{:01$x}", hash_val, 16);
        Ok(())
    }

    // Look for and retrieve subtitle ZIP from opensubtitles.org and unpack to .SRT file.
    fn get_subtitles(&self, language: &String) -> Result<(), SubError> {
        let client = try!(client::OpenSubtitlesClient::create_client("", "", "en", "RustSubFinder 0.1.0"));
        let subs = try!(client.search_subtitles(&self.hash, self.file_size, language));

        if subs.is_empty() {
            return Err(SubError::SvrNoSubtitlesFound);
        } else {
            let mut path = PathBuf::from(&self.file_name);
            path.set_extension("srt");
            let srt_path = path.to_str().unwrap();
            try!(file_utils::download_to_srt(&subs[0].ZipDownloadLink, srt_path));
        }
        Ok(())
    }
}

/// Start a number of worker threads to consume the list of files,
/// creates hashes and get subtitle files from server.
/// Every worker get access to the shared Vec through a critical section.
fn run_workers(shows: Vec<Show>, language: String) {
    let data = Arc::new(Mutex::new(shows));

    let (tx, rx) = mpsc::channel();
    let lang_arc = Arc::new(language); // using Arc because closure is 'move ||''

    for i in 0..THREAD_COUNT {
        let (data, tx) = (data.clone(), tx.clone());
        let language = lang_arc.clone();
        thread::spawn(move || {

            // now consume all data, competing with other threads for critical section
            loop {

                // access mutex semaphore and safely take ownership of next work item
                let option: Option<Show> = {
                    let mut data = data.lock().unwrap();
                    data.pop()
                }; // mutex unlocked by scope

                match option {
                    Some(mut show) => {
                        match show.create_hash() {
                            Ok(_) => {
                                println!("[{}] Found show {}.", i, show.file_name);
                                match show.get_subtitles(&language) {
                                    Ok(_) => println!("[{}]     Downloaded subtitles for {}.",
                                                      i, show.file_name),
                                    Err(e) => println!("[{}]     {:?}.", i, e),
                                }
                            }
                            Err(e) => {
                                println!("[{}] Found show {}. ERROR {}: unable to read file, skipping.",
                                         i, show.file_name, e)
                            }
                        }
                    }
                    None => break, // list empty, leave loop
                }
            }

            tx.send(()).unwrap();  // signal thread done
        });
    }

    // wait for workers to complete (blocks if mutex is poisoned)
    for _ in 0..THREAD_COUNT {
        rx.recv().unwrap();
    }
}

/// Traverse directory for valid movies
fn get_show_list(path: String, valid_extensions: &HashSet<&str>) -> Result<Vec<Show>, std::io::Error> {
    let files = try!(fs::read_dir(path));
    let mut show_list: Vec<Show> = Vec::new();

    // Process entries. If we fail with an entry
    // we want to drop and continue with the rest.
    for file in files {
        if let Ok(file) = file {

            // Only accept files big enough for hashing (error discerning file size
            // interpreted as 0 size file for discarding entry)
            let fsize = fs::metadata(file.path()).map(|i| i.len()).unwrap_or(0);
            let ext = file.path().extension().unwrap_or_default().to_string_lossy().into_owned();
            if valid_extensions.contains(ext.as_str()) && fsize >= HASH_BLK_SIZE {
                if let Ok(unicode_name) = file.file_name().into_string() {
                    show_list.push(Show {
                        full_path: file.path().to_string_lossy().into_owned(),
                        file_name: unicode_name,
                        file_size: fsize,
                        hash: String::new(),
                    });
                }
            }
        }
    }
    Ok(show_list)
}

fn main() {
    let language;
    let mut dir = "./".to_string();

    // A bit ugly, but if first argument is three characters, assumed to be language selection
    // don't put a three character filename/directory!
    let arg1 = env::args().nth(1).unwrap_or("./".to_string());
    if arg1.len()==3 {
        language = arg1;
    } else {
        dir = arg1;
        language = env::args().nth(2).unwrap_or("eng".to_string());
    }

    println!("SubFinder 0.1.0");
    println!("Usage: SubFinder <dir/filename> <lang>. Default is ./ eng.");
    println!("Finding subtitles for {}  Language: {}.\n", dir, language);

    // Common file extensions for movies. Put into HashSet for O(1) lookup.
    let extensions = vec!("avi", "mp4", "m4v", "mpg", "mkv", "264", "h264", "265", "h265");
    let valid_extensions: HashSet<&str> = extensions.into_iter().collect();

    let mut shows: Vec<Show> = Vec::new();

    // If argument is a single file, do the eligibility checks and put it in list, otherwise
    // call function to scan directory for files.
    let metadata = fs::metadata(&dir);
    match metadata {
        Ok(m) => {
            let path = Path::new(&dir);
            if m.is_file() {
                let ext = path.extension().unwrap_or_default().to_string_lossy().into_owned();
                if m.len()>=HASH_BLK_SIZE && valid_extensions.contains(ext.as_str()) {
                    shows.push(Show {
                        full_path: dir.clone(),
                        file_name: path.file_name().unwrap().to_string_lossy().into_owned(),
                        file_size: m.len(),
                        hash: String::new(),
                    });
                } else {
                    println!("Error: Invalid file.");
                    return;
                }
            } else {
                match get_show_list(dir.clone(), &valid_extensions) {
                    Err(e) => {
                        println!("Error: {} reading directory!", e);
                        return;
                    },
                    Ok(vec) => shows = vec,
                }
            }
        },
        Err(e) => {
            println!("Error: {} reading file/directory!", e);
            return;
        }
    }

    // Do the actual work
    if shows.len()>0 {
        run_workers(shows, language);
    }

    println!("All done.");
}
