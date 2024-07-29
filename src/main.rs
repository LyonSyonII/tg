use fake::Fake;
use rand::Rng;
use utils::Exit as _;
use yakv::storage::Select;

mod utils;

fn main() {
    let dirs = directories::ProjectDirs::from("dev", "lyonsyonii", "fg")
        .exit("error: unable to create app directory");
    let local = dirs.data_dir();
    std::fs::create_dir_all(local).unwrap();
    
    let db = yakv::storage::Storage::open(
        &local.join("db.yakv"),
        yakv::storage::StorageConfig::default(),
    )
    .unwrap();
    
    use rand::SeedableRng;
    let mut rng = rand::prelude::StdRng::seed_from_u64(56);
    let mut trans = db.start_transaction();
    for i in 0..5_000_000usize {
        let words = rng.gen_range(1..=4);
        let key: String = fake::faker::name::en::Name().fake_with_rng(&mut rng);
        let names: Vec<_> = fake::faker::lorem::en::Words(0..words).fake_with_rng(&mut rng);
        trans.put(
            &key.into_bytes(),
            &names
                .into_iter()
                .map(|n| n.into_bytes())
                .reduce(|mut acc, v| {
                    acc.extend(v);
                    acc
                })
                .unwrap_or_default(),
        ).unwrap();
        eprintln!("{i}");
    }
    trans.commit().unwrap();

    // for entry in db.iter().flatten() {
    //     println!("{entry:?}");
    // }
    // db.set("pasta", &vec!["hola"; 256]).unwrap();
    // println!("{:?}", db.iter().map(|i| (i.get_key().to_owned(), i.get_value::<Vec<String>>())).collect::<Vec<_>>())
}
