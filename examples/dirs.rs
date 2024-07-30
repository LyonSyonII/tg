fn main() {
    for i in 0..300_000 {
        std::fs::create_dir_all(format!("Tmp/Dirs/{i}")).unwrap();
    }
}