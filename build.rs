fn main() {
    pyo3_build_config::use_pyo3_cfgs();
    println!("cargo:rustc-link-search=native=/usr/lib/x86_64-linux-gnu");
    println!("cargo:rustc-link-lib=python3.10");
}
