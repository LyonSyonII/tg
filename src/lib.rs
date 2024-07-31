pub mod utils;

pub fn list_to_values(key: impl std::fmt::Debug, list: impl AsRef<[String]>) -> String {
    let list = list.as_ref();

    let mut acc = format!("({key:?},\"");
    let separator = format!("\"),({key:?},\"");
    if let Some(f) = list.first() {
        acc += f.as_ref();
    }
    for s in list.get(1..).unwrap_or_default() {
        acc = acc + &separator + s.as_ref();
    }
    acc += "\")";

    acc
}

pub fn list_to_sql<S: std::fmt::Debug>(list: impl IntoIterator<Item = S>) -> (usize, String) {
    let mut list = list.into_iter();

    let mut len = 0;
    let mut acc = String::from("(");
    if let Some(f) = list.next() {
        acc = format!("{acc}{f:?}");
        len += 1;
    }
    for s in list {
        acc = format!("{acc},{s:?}");
        len += 1;
    }
    acc += ")";

    (len, acc)
}
