extern crate sub_finder;
extern crate glob;
extern crate config;
extern crate crossbeam;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, BufReader};
use std::{env, mem, fs};
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::result::Result;
use std::collections::HashSet;
use crossbeam::scope;

use sub_finder::error::SubError;
use sub_finder::client;
use sub_finder::file_utils;

use glob::glob;

/// Opensubtitle hashing algorithm [hash file size]+[first 64KB]+[last64KB]
const HASH_BLK_SIZE: u64 = 65536;

// Number of threads hashing and retrieving subtitles. Don't flood server pls.
const THREAD_COUNT: u64 = 4;

const CONFIG_DIR: &'static str = ".subfinder";
const CONFIG_FILENAME: &'static str = "subfinder.conf";
const CONFIG_STR_USER: &'static str = "username";
const CONFIG_STR_PASS: &'static str = "password";

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
                    unsafe {
                        word = mem::transmute(buf);
                    };
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
    fn get_subtitles(&self, params: & UserParams) -> Result<(), SubError> {
        let client =
            try!(client::OpenSubtitlesClient::create_client(&params.username,
                        //&params.password, "en", "OSTestUserAgent"));
                        &params.password, "en", "RustSubFinder 0.1.0"));



        let subs = try!(client.search_subtitles(&self.hash, self.file_size, &params.language));

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
fn run_workers(shows: Vec<Show>, params: UserParams) {

    // Using scoped threading makes everything easier wrt lifetimes and waiting for completion.
    crossbeam::scope(|scope| {

        let params_arc = Arc::new(params); // using Arc because closure is 'move ||''
        let data = Arc::new(Mutex::new(shows));

        for i in 0..THREAD_COUNT {

            // Increment refcounts for each thread
            let (data, params) = ( data.clone(), params_arc.clone());
            scope.spawn(move || {

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
                                    match show.get_subtitles(&params) {
                                        Ok(_) => {
                                            println!("[{}]     Downloaded subtitles for {}.",
                                                     i, show.file_name)
                                        }
                                        Err(e) => println!("[{}]     {:?}.", i, e),
                                    }
                                }
                                Err(e) => {
                                    println!("[{}] Found show {}. ERROR {}: unable to read file, \
                                              skipping.",
                                             i, show.file_name, e)
                                }
                            }
                        }
                        None => break, // list empty, leave loop
                    }
                }
            });
        }
    });
}

/// Traverse directory for valid movies
fn get_show_list(path: String, valid_extensions: &HashSet<&str>) -> Result<Vec<Show>, SubError> {

    let mut show_list: Vec<Show> = Vec::new();
    for entry in try!(glob(&path)) {
        if let Ok(path) = entry {

            // Only accept files big enough for hashing (error discerning file size
            // interpreted as 0 size file for discarding entry)
            let fsize = fs::metadata(&path).map(|i| i.len()).unwrap_or(0);

            let ext = path.extension().unwrap_or_default().to_string_lossy().into_owned();
            if valid_extensions.contains(ext.as_str()) && fsize >= HASH_BLK_SIZE {
                if let Some(unicode_name) = path.file_name() {
                    show_list.push(Show {
                        full_path: path.to_string_lossy().into_owned(),
                        file_name: unicode_name.to_string_lossy().into_owned(),
                        file_size: fsize,
                        hash: String::new(),
                    });
                }
            }
        }
    }

    Ok(show_list)
}

// Username, password and search language for opensubtitles.
struct UserParams<'a> {
        username: &'a str,
        password: &'a str,
        language: &'a str,
}

fn main() {

    println!("\nSubFinder 0.1.0. Subtitle search for opensubtitles.org.\n");

    let language;
    let mut dir = "*".to_string();
    // Decode command line parameters
    let arg1 = env::args().nth(1).unwrap_or("*".to_string());

    if env::args().len() == 1 || arg1 == "-h" {

        let mut p = PathBuf::new();
        if let Some(config_path) = env::home_dir() {
            p = config_path;
        }
        p.push(CONFIG_DIR);
        p.push(CONFIG_FILENAME);

        println!("Usage: SubFinder <dir/filename> <lang>. Defaulting to \"SubFinder * eng\".\n");
        println!("Examples: ");
        println!("    subfinder * eng");
        println!("    subfinder *.avi eng");
        println!("    subfinder breakdance.avi\n");
        println!("For opensubtitles user name and password, create a text file in {} containing:", p.display());
        println!("    username = \"auser\";");
        println!("    password = \"apass\";");
        return;
    } else {
        println!("Use -h for help/usage.\n");
    }

    if arg1.len() == 3 && !arg1.contains("*") && !arg1.contains(".") {
        language = arg1;
    } else {
        dir = arg1;
        language = env::args().nth(2).unwrap_or("eng".to_string());
    }

    // get username/password from config file, or use defaults
    let mut username = String::new();
    let mut password = String::new();

    if let Some(mut config_path) = env::home_dir() {
        config_path.push(CONFIG_DIR);
        config_path.push(CONFIG_FILENAME);

        use config::reader::from_file;

        if config_path.as_path().exists() {
            match from_file(config_path.as_path()) {
                Ok(parser) => {
                    println!("Found configuration file at {}.", config_path.display());

                    if let Some(un) = parser.lookup_str(CONFIG_STR_USER) {
                        username = un.to_string();
                    }

                    if let Some(pw) = parser.lookup_str(CONFIG_STR_PASS) {
                        password = pw.to_string();
                    }
                },
                Err(e) => {
                        println!("Error reading config file {:?}\n", e);
                        println!("subfinder.conf example:");
                        println!("    username = \"auser\";");
                        println!("    password = \"apass\";");
                        return;
                    },
            }
        } else {
            println!("No configuration file at {}. Using empty username/password.", config_path.display());
        }
    }

    println!("Finding subtitles for {}  Language: {}.\n", dir, language);

    // Common file extensions for movies. Put into HashSet for O(1) lookup.
    let extensions = vec!["avi", "mp4", "m4v", "mpg", "mkv", "264", "h264", "265", "h265"];
    let valid_extensions: HashSet<&str> = extensions.into_iter().collect();

    let params = UserParams { username: &username, password: &password, language: &language};

    match get_show_list(dir, &valid_extensions) {
        Err(e) => {
            println!("Error: {} reading directory!", e);
            return;
        }
        Ok(vec) => {
            if vec.len() > 0 {
                run_workers(vec, params);
            }
        }
    }

    println!("All done.");
}
