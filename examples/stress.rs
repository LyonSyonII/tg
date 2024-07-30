use anyhow::{anyhow, Context as _, Result};
use fake::Fake as _;
use rand::{Rng as _, SeedableRng as _};
use rusqlite::Connection;
use tg::list_to_sql;

fn main() -> Result<()> {
    let dirs = directories::ProjectDirs::from("dev", "lyonsyonii", "fg")
        .ok_or(anyhow!("Unable to create the application's data directory"))?;
    let local = dirs.data_dir();
    std::fs::create_dir_all(local)?;
    let mut db = rusqlite::Connection::open(local.join("db.sqlite")).context("Database Creation")?;
    
    // drop(&db)?;
    // insert(&mut db)?;
    select(&db)?;
    Ok(())
}

fn insert(db: &mut Connection) -> Result<()> {
    let mut rng = rand::prelude::StdRng::seed_from_u64(57);
    
    let mut inserted = 0;
    let tx = db.transaction()?;
    for i in 0..5_000_000usize {
        let words = rng.gen_range(1..=4);
        let key: String = fake::faker::name::en::Name().fake_with_rng(&mut rng);
        let names: Vec<_> = fake::faker::lorem::en::Words(1..words+1)
            .fake_with_rng(&mut rng);
        let values = tg::list_to_values(&key, names);
    
        let stmt = format!("insert or ignore into FileTags values {values}");
        eprintln!("[{i}] {stmt}");
        inserted += tx.execute(&stmt, [])?;
    }
    tx.commit()?;
    println!("inserted {} entries", inserted);

    Ok(())
}

fn select(db: &Connection) -> Result<()> {
    let tags = ["voluptatem", "eius", "est", "dolores", "harum", "et", "ex"];
    let list = list_to_sql(tags);
/*     let stmt = format!(r#"
        select tag from FileTags where file in (
            select file from FileTags
            where tag in {list}
            group by file 
            having count(*) = {}
        ) and tag not in {list};"#, tags.len()
    ); */
     let stmt = format!(r#"
        select file from FileTags
        where tag in {list}
        group by file 
        having count(*) = {}"#, tags.len());
    let mut stmt = db.prepare(&stmt)?;
    let mut rows = stmt.query([])?;
    
    while let Ok(Some(r)) = rows.next() {
        let file = r.get_ref(0)?.as_str()?;
        let tags = {
            let mut stmt = db.prepare("select tag from FileTags where file = ?")?;
            let rows = stmt.query_map([file], |r| r.get::<_, String>(0))?;
            rows.flatten().collect::<Vec<_>>()
        };
        // let tag = r.get_ref(1)?.as_str()?;
        eprintln!("{file}: {tags:?}");
    }

    Ok(())
}

fn drop(db: &Connection) -> Result<()> {
    let dropped = db.execute("delete from FileTags", [])?;
    eprintln!("Dropped {dropped} rows");
    Ok(())
}