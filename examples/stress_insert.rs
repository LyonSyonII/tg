use anyhow::Context;
use rand::{Rng, SeedableRng};
use rusqlite::params;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let config = tg::config::Config::load()?;
    let db_path = config.db_path().to_path_buf();
    let mut conn = rusqlite::Connection::open(&db_path).context("database creation failed")?;
    conn.execute_batch(include_str!("../src/sql/migrations.sql"))?;

    // Prepare the statements for insertion
    let start = Instant::now();
    let tx = conn.transaction()?;
    {
        use std::io::Write;

        let mut insert_tag_stmt = tx.prepare("INSERT OR IGNORE INTO Tags (tag) VALUES (?1)")?;
        for i in 1..=1000 {
            insert_tag_stmt.execute([format!("t{i}")])?;
        }
        let mut insert_file_stmt = tx.prepare("INSERT OR IGNORE INTO Files (file) VALUES (?1)")?;
        let mut insert_filetag_stmt = tx.prepare(
            "INSERT INTO FileTags (fileId, tagId) VALUES (
            (SELECT id FROM Files WHERE file = ?1),
            ?2
            )",
        )?;

        let mut stdout = std::io::stdout().lock();
        let mut rng = rand::rngs::StdRng::seed_from_u64(57);
        for i in 0..5_000_000 {
            let file = format!("/f{i}");
            // Insert into Files
            insert_file_stmt.execute([&file])?;

            let num_tags = rng.gen_range(1..=4);
            for tag in rand::seq::index::sample(&mut rng, 1000, num_tags) {
                // Insert into FileTags table
                insert_filetag_stmt.execute(params![&file, tag + 1])?;
            }

            if i % 100_000 == 0 {
                writeln!(&mut stdout, "{i}")?;
            }
        }
    }
    tx.commit()?;

    let duration = start.elapsed();
    println!(
        "Time elapsed in inserting 5,000,000 entries is: {:?}",
        duration
    );

    Ok(())
}
