fn main() {
    embed_resource::compile("res/app.rc", embed_resource::NONE).manifest_optional().unwrap();
    println!("cargo:rerun-if-changed=res");
}
