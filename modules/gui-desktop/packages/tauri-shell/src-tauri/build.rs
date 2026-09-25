fn main() {
    println!("cargo:rerun-if-changed=../ui");
    println!("cargo:rerun-if-changed=../ui/pet.html");
    println!("cargo:rerun-if-changed=../ui/pet-mini.html");
    println!("cargo:rerun-if-changed=../ui/assets");
    // Windows icon resources are compiled into the Tauri PE. Without these
    // explicit dependencies, an incremental build can keep an old resource
    // section after the ICO/PNG changes and ship the wrong icon.
    println!("cargo:rerun-if-changed=icons/app-icon-cz-moon-gate-lantern-v1.ico");
    println!("cargo:rerun-if-changed=icons/app-icon-cz-moon-gate-lantern-v1.png");
    tauri_build::build()
}
