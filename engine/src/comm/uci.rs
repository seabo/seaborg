use std::io::{self, Stdin};

pub struct UciSess {
    handle: Stdin,
}

impl UciSess {
    pub fn new() -> Self {
        Self {
            handle: io::stdin(),
        }
    }

    pub fn run(&mut self) {
        loop {
            let mut buffer = String::new();
            self.handle.read_line(&mut buffer);
            println!("{:?}", UciSess::parse_input(&buffer));
        }
    }

    fn parse_input(buf: &String) -> Vec<&str> {
        let toks = Self::tokenize(buf);
        toks
    }

    fn tokenize(buf: &String) -> Vec<&str> {
        buf.split_whitespace().collect()
    }
}
