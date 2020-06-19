// By wrapping a Vec and exposing only `push`, we achieve all of the invariants of a Slab with none
// of the overhead. For use cases that don't involve removing items, this is pretty nice. Unlike a
// real Slab, there is no need to keep track of vacant slots, and since a SimpleSlab cannot be
// "sparse", a large, mostly-empty SimpleSlab is not slow to iterate.

use std::ops::{Index, IndexMut};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleSlab<T>(Vec<T>);

impl<T> SimpleSlab<T> {
    #![allow(dead_code)]

    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    pub fn reserve(&mut self, additional: usize) {
        self.0.reserve(additional)
    }

    pub fn reserve_exact(&mut self, additional: usize) {
        self.0.reserve_exact(additional)
    }

    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit()
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.0.iter_mut()
    }

    pub fn into_iter(self) -> impl Iterator<Item = T> {
        self.0.into_iter()
    }

    pub fn get(&self, key: usize) -> Option<&T> {
        self.0.get(key)
    }

    pub fn get_mut(&mut self, key: usize) -> Option<&mut T> {
        self.0.get_mut(key)
    }

    pub fn insert(&mut self, val: T) -> usize {
        self.0.push(val);
        self.len() - 1
    }

    pub fn contains(&self, key: usize) -> bool {
        key < self.len()
    }

    pub fn drain<'a>(&'a mut self) -> impl Iterator<Item = T> + 'a {
        self.0.drain(..)
    }
}

impl<T> Default for SimpleSlab<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> IntoIterator for SimpleSlab<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a SimpleSlab<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut SimpleSlab<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl<T> Index<usize> for SimpleSlab<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl<T> IndexMut<usize> for SimpleSlab<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}
