use core::{fmt};

#[derive(Clone)]
//Namespaces that we use.
//
//Generally `sled::Db` is light-weight, but we do not really need it
//to write into namespaces.
pub struct DbView {
    pub config: sled::Tree,
    pub checksum: sled::Tree,
}

pub struct Db {
    #[allow(unused)]
    db: sled::Db,
    view: DbView,
}

impl Db {
    pub fn open(path: &str) -> Result<Self, sled::Error> {
        let db = sled::Config::new().path(path)
                                    .cache_capacity(128_000)
                                    .mode(sled::Mode::LowSpace)
                                    .use_compression(true)
                                    .flush_every_ms(Some(60_000))
                                    .open()?;

        let config = db.open_tree("config")?;
        let checksum = db.open_tree("cheksum")?;

        Ok(Self {
            db,
            view: DbView {
                config,
                checksum
            },
        })
    }

    fn init(path: &str) -> Self {
        match Self::open(path) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("Unable to initialize db at the default path. Error: {}", error);
                std::process::exit(1);
            }
        }
    }

    #[inline]
    pub fn view(&self) -> DbView {
        self.view.clone()
    }
}

impl fmt::Debug for Db {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Db")
    }
}

impl core::str::FromStr for Db {
    type Err = ();

    #[inline]
    fn from_str(path: &str) -> Result<Self, Self::Err> {
        Ok(Self::init(path))
    }
}

impl Default for Db {
    #[inline]
    fn default() -> Self {
        Self::init("dou_store_db")
    }
}
