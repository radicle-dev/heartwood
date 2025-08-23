pub use mio::Token;

pub const WAKER: Token = Token(0);

#[derive(Clone, Debug)]
pub struct Tokens {
    initial: usize,
    current: usize,
}

impl Tokens {
    pub fn new(initial: usize) -> Self {
        Tokens {
            initial,
            current: initial,
        }
    }

    #[inline]
    pub fn advance(&mut self) -> Token {
        let current = self.current;

        self.current = {
            let candidate = current.wrapping_add(1);

            if candidate == usize::MIN {
                // If we overflowed, reset to the initial value.
                // The range of `usize` is so large that likely
                // a few years have passed since the early tokens
                // were used.
                log::info!(target = "reactor"; "Tokens wrapped.");
                self.initial
            } else {
                candidate
            }
        };

        Token(current)
    }
}

impl Default for Tokens {
    fn default() -> Self {
        Tokens::new(1)
    }
}
