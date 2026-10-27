fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("netbsd") {
        cc::Build::new()
            .file("src/provider/battery/netbsd_envsys.c")
            .compile("rubar_netbsd_envsys");
        println!("cargo:rustc-link-lib=prop");
    }
}
