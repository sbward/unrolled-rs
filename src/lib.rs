#![feature(macro_rules)]
#![feature(phase)] // For log module
#![feature(slicing_syntax)]

#[phase(plugin, link)] extern crate log;

use std::collections::DList;

// Dependencies for Slice
// use std::ops::Slice;
// use std::rc::Rc;

macro_rules! PAGE_SIZE { () => { 512u } }

/// An Unrolled Linked List.
pub struct Unrolled<'a, T: 'a> {
	dlist: DList<Page<T>>,
	len:   uint, // FIXME could be atomic for thread safety?
}

struct Page<T> {
	items: Vec<T>, // Must have capaciy of PAGE_SIZE and never shrink/grow/move
}

impl<T> Page<T> {
	fn new() -> Page<T> {
		Page{
			items: Vec::with_capacity(PAGE_SIZE!()),
		}
	}
}

impl<'a, T> Unrolled<'a, T> {
	pub fn new() -> Unrolled<'a, T> {
		Unrolled {
			dlist: DList::new(), // No pages are pre-allocated
			len:   0,
		}
	}

	/// Insert an item at the end of the list.
	pub fn push(&mut self, item: T) {
		// Make sure there's enough space for the item
		if !self.enough_pages_for(self.len) {
			self.dlist.push_back(Page::new());
			debug!("Allocating page. dlist.len() == {}", self.dlist.len());
		};

		self.len += 1;

		debug!("Pushing to list. last_page_len={}", self.dlist.back().unwrap().items.len());

		self.dlist.back_mut().unwrap().items.push(item);

		debug!("Pushed to list. last_page_len={}", self.dlist.back().unwrap().items.len());
	}

	/// Remove an item from the end of the list and return it.
	/// Returns None if the list is empty.
	pub fn pop(&mut self) -> Option<T> {
		// Cases:
		// 1) pop off last vec, 1 or 0 extra vecs, done.
		// 2) pop off last vec, 2 extra vecs, remove last vec, done.
		// 3) last vec empty, pop off next to last vec, done.
		let last_page_count = self.dlist.back().unwrap().items.len();

		debug!("Before pop. num_pages={} last_page_count={}", self.dlist.len(), self.dlist.back().unwrap().items.len());

		let item = match last_page_count == 0 {
			true => {
				debug!("Extra last page");

				self.dlist.rotate_forward();
				let item = self.dlist.back_mut().unwrap().items.pop();
				self.dlist.rotate_backward();
				item
			},
			false => self.dlist.back_mut().unwrap().items.pop()
		};

		let last_two_pages_empty = self.dlist.len() - page_of(self.len) == 3;

		if last_two_pages_empty {
			self.dlist.pop_back();
		}

		debug!("After pop. num_pages={} last_page_count={}", self.dlist.len(), self.dlist.back().unwrap().items.len());

		match item {
			Some(_) => self.len -= 1,
			None    => {},
		}

		item
	}

	pub fn len(&self) -> uint {
		self.len
	}

	// Check if enough pages exist to hold a given index
	#[inline]
	fn enough_pages_for(&self, pos: uint) -> bool {
		match self.dlist.len() {
			0 => false,
			_ => page_of(pos) <= self.dlist.len() * PAGE_SIZE!(),
		}
	}
}

// Returns the zero-indexed page that a zero-indexed item is on
#[inline]
fn page_of(pos: uint) -> uint {
	pos / PAGE_SIZE!()
}

/*
FIXME huon might implement reference counted slicing, which would make Slice much easier to implement here
impl<'c, T> Slice<uint, [&'c [T]]> for Unrolled<'c, T> {
    fn as_slice_<'a>(&'a self) -> &'a [&'c [T]] {
		let v: Vec<&'c [T]> = self.dlist
			.iter()
			.by_ref()
			.map(|&page| page.items.as_slice())
			.collect();

		Rc::new(v).as_slice()
	}

    fn slice_from_or_fail<'a>(&'a self, from: &uint) -> &'a [&'c [T]] {
		let v: Vec<&'c [T]> = self.dlist
			.iter()
			.by_ref()
			.skip(page_of(*from) - 1)
			.map(|&page| page.items.as_slice())
			.collect();

		let slice = (box v).as_slice();

		// Clip the front slice up to `*from`
		slice[0] = slice[0][page_offset(*from)..];

		slice
	}

    fn slice_to_or_fail<'a>(&'a self, to: &uint) -> &'a [&'c [T]] {
		let v: Vec<&'c [T]> = self.dlist
			.iter()
			.by_ref()
			.take(page_of(*to))
			.map(|&page| page.items.as_slice())
			.collect();

		let slice = (box v).as_slice();

		let last = slice.len() - 1;

		// Clip the back slice after `*to`
		slice[last] = slice[last][..page_offset(*to) - 1];

		slice
	}

    fn slice_or_fail<'a>(&'a self, from: &uint, to: &uint) -> &'a [&'c [T]] {
		let diff = *to - *from;

		let v: Vec<&'c [T]> = self.dlist
			.iter()
			.by_ref()
			.skip(page_of(*from) - 1)
			.take(diff)
			.map(|&page| page.items.as_slice())
			.collect();

		let slice = (box v).as_slice();

		let last = slice.len() - 1;

		// Clip the front and back slices *to fit `*from` and `*to`
		slice[0] = slice[0][page_offset(*from)..];
		slice[last] = slice[last][..page_offset(*from) - 1];

		slice
	}
}
*/

#[cfg(test)]
mod tests {
	use std::iter::range_step;
	use super::{Unrolled, page_of};

	#[test]
	fn utilities() {
		assert!(page_of(0) == 0);
		assert!(page_of(1) == 0);
		assert!(page_of(PAGE_SIZE!()) == 1);
		assert!(page_of(PAGE_SIZE!() - 1) == 0);
		assert!(page_of(PAGE_SIZE!() * 2) == 2);
		assert!(page_of(PAGE_SIZE!() * 2 + 1) == 2);
	}

	#[test]
	fn smoke_push_pop() {
		let mut list: Unrolled<int> = Unrolled::new();

		assert!(list.dlist.len() == 0);

		let psize: int = PAGE_SIZE!() as int;

		for n in 0i..(3*psize) {
			println!("Push {}. real_len={} expected_len={}", n, list.len(), n);
			assert!(list.len() == n as uint);
			list.push(n);
		}

		for n in range_step(3i*psize - 1, -1, -1) {
			let pop = list.pop().unwrap();
			println!("Pop {} received {}.  real_len={} expected_len={}", n, pop, list.len(), n);
			assert!(list.len() == n as uint);
			assert!(pop == n);
		}

		for _ in 0i..(3*psize) {
			let pop = list.pop();
			println!("Pop on empty list expects None, received {}", pop);
			assert!(pop == None);
			assert!(list.len() == 0);
		}
	}
}
