extern crate log;

use std::collections::DList;
use std::mem::swap;
use std::fmt::Show; // FIXME debug only

// Dependencies for Slice
// use std::ops::Slice;
// use std::rc::Rc;

macro_rules! PAGE_SIZE { () => { 512us } }

/// An Unrolled Linked List.
/// Removing an item from the middle of the list will move the last item to that position, preventing fragmentation.
pub struct Unrolled<T: Copy + Show> {
	dlist: DList<Page<T>>,
	len:   usize, // FIXME could be atomic for thread safety?
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

impl<'a, T: Copy + Show> Unrolled<T> {
	pub fn new() -> Unrolled<T> {
		Unrolled {
			dlist: DList::new(), // No pages are pre-allocated
			len:   0,
		}
	}

	/// Insert an item at the end of the list.
	pub fn push(&mut self, item: T) {
		// Make sure there's enough space for the item
		if !self.enough_pages_for(self.len + 1) {
			self.dlist.push_back(Page::new());
		};

		self.len += 1;

		self.dlist.back_mut().unwrap().items.push(item);
	}

	/// Remove an item from the end of the list and return it.
	/// Returns None if the list is empty.
	pub fn pop(&mut self) -> Option<T> {
		if self.len == 0 {
			return None;
		}

		match self.dlist.iter_mut().nth(page_of(self.len)).unwrap().items.pop() {
			Some(item) => {
				self.len -= 1;
				Some(item)
			},
			None => None,
		}
	}

	pub fn len(&self) -> usize {
		self.len
	}

	pub fn ref_mut(&mut self, pos: usize) -> &mut T {
		self.dlist.iter_mut().nth(page_of(pos)).unwrap().items.as_mut_slice().get_mut(pos % PAGE_SIZE!()).unwrap()
	}

	fn mut_slice_pages(&'a mut self) -> Vec<&'a mut[T]> {
		let mut slices: Vec<&mut[T]> = Vec::new();

		for p in self.dlist.iter_mut() {
			slices.push((*p.items).as_mut_slice());
		}

		slices
	}

	/// Removes and returns the item at a given position.
	/// Returns None if no item exists at that position.
	pub fn remove(&mut self, pos: usize) -> Option<T> {
		// What I want to write...
		let max_idx = self.len - 1;

		if pos > max_idx || self.len == 0 {
			return None;
		}

		// Swap with last, unless it's last
		if pos != max_idx {
			let mut pages = self.mut_slice_pages();
			let item_offset = pos % PAGE_SIZE!();
			let last_offset = max_idx % PAGE_SIZE!();

			// Check if it's on the last page
			if page_of(pos) != page_of(max_idx) {
				let (item_page, last_page) = pages.as_mut_slice().split_at_mut(page_of(max_idx));

				swap(
					item_page.get_mut(item_offset).unwrap(),
					last_page.get_mut(last_offset).unwrap()
				);
			} else {
				let (item, last) = pages[page_of(pos)].split_at_mut(last_offset);
				let item_len = item.len();

				swap(
					item.get_mut(item_offset).unwrap(),
					last.get_mut(last_offset - item_len).unwrap()
				);
			}
		}

		self.pop()
	}

	// Check if enough pages exist to hold a given index
	#[inline]
	fn enough_pages_for(&self, pos: usize) -> bool {
		match self.dlist.len() {
			0 => false,
			_ => page_of(pos) <= self.dlist.len() - 1,
		}
	}
}

// Returns the zero-indexed page that a zero-indexed item is on
#[inline]
fn page_of(pos: usize) -> usize {
	pos / PAGE_SIZE!()
}

/*
FIXME huon might implement reference counted slicing, which would make Slice much easier to implement here
impl<'c, T> Slice<usize, [&'c [T]]> for Unrolled<'c, T> {
    fn as_slice_<'a>(&'a self) -> &'a [&'c [T]] {
		let v: Vec<&'c [T]> = self.dlist
			.iter()
			.by_ref()
			.map(|&page| page.items.as_slice())
			.collect();

		Rc::new(v).as_slice()
	}

    fn slice_from_or_fail<'a>(&'a self, from: &usize) -> &'a [&'c [T]] {
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

    fn slice_to_or_fail<'a>(&'a self, to: &usize) -> &'a [&'c [T]] {
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

    fn slice_or_fail<'a>(&'a self, from: &usize, to: &usize) -> &'a [&'c [T]] {
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
		let mut list: Unrolled<i32> = Unrolled::new();

		assert!(list.dlist.len() == 0);

		let psize: i32 = PAGE_SIZE!() as i32;

		println!("Pushing...");

		for n in 0i32..(3*psize) {
			println!("Push {}. real_len={} expected_len={}", n, list.len(), n);
			assert!(list.len() == n as usize);
			list.push(n);
		}

		println!("Popping non-empty...");

		for n in range_step(3i32*psize - 1, -1, -1) {
			let pop = list.pop().unwrap();
			println!("Pop {} received {}.  real_len={} expected_len={}", n, pop, list.len(), n);
			assert!(list.len() == n as usize);
			assert!(pop == n);
		}

		println!("Popping empty...");

		for _ in 0i32..(3*psize) {
			let pop = list.pop();
			println!("Pop on empty list expects None, received {:?}", pop);
			assert!(pop == None);
			assert!(list.len() == 0);
		}
	}

	#[test]
	fn smoke_remove() {
		let mut list: Unrolled<i32> = Unrolled::new();
		list.push(1);
		list.push(2);
		assert!(list.remove(0) == Some(1));
		println!("Removed 1");
		assert!(list.remove(0) == Some(2));
		println!("Removed 2");
		assert!(list.remove(0) == None);
		println!("Removed None");
	}
}
