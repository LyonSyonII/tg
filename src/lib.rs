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

pub fn list_to_sql<S: AsRef<str>>(list: impl AsRef<[S]>) -> String {
    let list = list.as_ref();

    let mut acc = String::from("(\"");
    if let Some(f) = list.first() {
        acc += f.as_ref();
    }
    for s in list.get(1..).unwrap_or_default() {
        acc = acc + "\",\"" + s.as_ref();
    }
    acc += "\")";

    acc
}
