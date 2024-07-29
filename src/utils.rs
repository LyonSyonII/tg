pub trait Exit {
    type Return;
    type Error;
    fn exit(self, msg: &str) -> Self::Return;

    fn exit_with<S: std::fmt::Display>(self, msg: impl FnOnce(Self::Error) -> S) -> Self::Return;
}

impl<T> Exit for Option<T> {
    type Return = T;
    type Error = ();

    fn exit(self, msg: &str) -> T {
        match self {
            Some(v) => v,
            None => {
                eprintln!("{msg}");
                std::process::exit(1);
            }
        }
    }

    fn exit_with<D: std::fmt::Display>(self, msg: impl FnOnce(Self::Error) -> D) -> T {
        match self {
            Some(v) => v,
            None => {
                eprintln!("{}", msg(()));
                std::process::exit(1);
            }
        }
    }
}

impl<T, E> Exit for Result<T, E> {
    type Return = T;
    type Error = E;

    fn exit(self, msg: &str) -> Self::Return {
        match self {
            Ok(v) => v,
            Err(_) => {
                eprintln!("{msg}");
                std::process::exit(1);
            }
        }
    }

    fn exit_with<D: std::fmt::Display>(self, msg: impl FnOnce(Self::Error) -> D) -> Self::Return {
        match self {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{}", msg(e));
                std::process::exit(1);
            }
        }
    }
}
