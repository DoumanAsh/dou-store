use arg::Args;

#[derive(Args, Debug)]
///Find files utility
pub struct Cli {
    #[arg(short, default_value = "6666")]
    ///Port to use in case of transport that allows it. Default is 6666
    pub port: u16,

    #[arg(long, default_value = "Default::default()")]
    ///Path on filesystem to store database. Default: dou_store_db
    pub db: crate::db::Db,
}

impl Cli {
    #[inline]
    pub fn new<'a, T: IntoIterator<Item = &'a str>>(args: T) -> Result<Self, bool> {
        let args = args.into_iter();

        Cli::from_args(args).map_err(|err| match err.is_help() {
            true => {
                println!("{}", Cli::HELP);
                false
            },
            false => {
                eprintln!("{}", err);
                true
            },
        })
    }
}
