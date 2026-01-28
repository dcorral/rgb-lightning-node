pub mod mnemonic;
pub mod prelude;

pub use mnemonic::{
    ActiveModel as DbMnemonicActMod, Column as MnemonicColumn, Entity as MnemonicEntity,
    Model as DbMnemonic,
};
