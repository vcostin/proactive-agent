fn main() {
    // lance-* crates use prost-build which requires protoc; vendor it so no system install is needed
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc not found");
    std::env::set_var("PROTOC", protoc);

    tauri_build::build()
}
