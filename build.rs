// include_dir! does not tell cargo to rebuild when embedded ports change.
fn main() {
    println!("cargo:rerun-if-changed=ports");
}
