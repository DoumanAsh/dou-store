use core::{ptr, mem};

static mut DB: mem::MaybeUninit<sled::Db> = mem::MaybeUninit::uninit();
static mut CONFIG: mem::MaybeUninit<sled::Tree> = mem::MaybeUninit::uninit();
static mut CHECKSUM: mem::MaybeUninit<sled::Tree> = mem::MaybeUninit::uninit();

fn init(path: &str) -> Result<(), sled::Error> {
    let db = sled::Config::new().path(path)
                                .cache_capacity(128_000)
                                .mode(sled::Mode::LowSpace)
                                .use_compression(true)
                                .flush_every_ms(Some(60_000))
                                .open()?;

    unsafe {
        ptr::write(CONFIG.as_mut_ptr(), db.open_tree("config")?);
        ptr::write(CHECKSUM.as_mut_ptr(), db.open_tree("checksum")?);
        ptr::write(DB.as_mut_ptr(), db);
    }

    Ok(())
}

#[inline]
fn open(path: &str) {
    match init(path) {
        Ok(_) => (),
        Err(error) => {
            eprintln!("Unable to initialize db at the default path. Error: {}", error);
            std::process::exit(1);
        }
    }
}

#[derive(Debug)]
///Initializer to be run as part of argument parsing.
pub struct ArgInit;

impl core::str::FromStr for ArgInit {
    type Err = ();

    fn from_str(path: &str) -> Result<Self, Self::Err> {
        open(path);
        Ok(Self)
    }
}

impl Default for ArgInit {
    fn default() -> Self {
        open("dou_store_db");
        Self
    }
}

pub fn config() -> &'static sled::Tree {
    unsafe {
        &*CONFIG.as_ptr()
    }
}

pub fn checksum() -> &'static sled::Tree {
    unsafe {
        &*CHECKSUM.as_ptr()
    }
}
