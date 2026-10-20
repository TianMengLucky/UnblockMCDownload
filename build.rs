#[cfg(feature = "gui")]
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for(
            "assets/icon.rc",
            ["unblockmcdownload"],
            embed_resource::NONE,
        )
        .manifest_required()
        .expect("Failed to embed the Windows application icon");
    }
}

#[cfg(not(feature = "gui"))]
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
}
