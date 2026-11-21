fn main() {
    // `include_dir!` does not notice files added to the shipped library.
    println!("cargo:rerun-if-changed=../../library");
}
