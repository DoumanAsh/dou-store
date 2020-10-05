#![no_main]

mod protocol;
mod cli;
mod db;
mod server;

c_ffi::c_main!(rust_main);

fn rust_main(args: c_ffi::Args) -> bool {
    let args = match cli::Cli::new(args.into_iter().skip(1)) {
        Ok(args) => args,
        Err(code) => return code,
    };

    rogu::set_level(rogu::Level::INFO);

    let tcp = server::tcp::Tcp::new(args.port, args.db.view());

    let mut rt = match tokio::runtime::Builder::new().core_threads(1).max_threads(8).enable_io().basic_scheduler().build() {
        Ok(rt) => rt,
        Err(error) => {
            eprintln!("Unable to start IO loop: {}", error);
            return true;
        }
    };

    loop {
        if !rt.block_on(tcp.start()) {
            //We only exit with false when server unable to start.
            std::thread::sleep(core::time::Duration::from_secs(1));
        }
    }
}
