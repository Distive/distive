use std::ops::{Deref, DerefMut};

use indexmap::IndexMap;
use crate::Comment;
// pub type Thread = IndexMap<String, Comment>;
#[derive(Clone)]
pub struct Thread(IndexMap<String,Comment>);

impl Thread {
    pub fn new() -> Self {
        Thread(IndexMap::new())
    }
}

//Implement deref trait
impl Deref for Thread {
    type Target = IndexMap<String, Comment>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

//Implement derefmut trait
impl DerefMut for Thread {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
