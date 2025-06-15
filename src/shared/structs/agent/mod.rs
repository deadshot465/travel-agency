pub mod food;
pub mod history;
pub mod modern;
pub mod nature;
pub mod transport;

pub trait Task {
    fn execute(self);
}

#[derive(Debug, Clone, PartialEq)]
pub enum Agent {
    Food,
    Transport,
    History,
    Modern,
    Nature,
}
