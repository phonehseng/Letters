//! the waiting line for tile requests.
//!
//! when someone scrolls fast, tiles are asked for quicker than they can
//! be drawn. serving them in arrival order would spend seconds drawing
//! pages that have already left the screen. so the line works the other
//! way round: the newest request is served first, and when the line
//! gets too long the oldest requests are given up on.

use std::collections::VecDeque;

pub struct NewestFirstQueue<T> {
    waiting: VecDeque<T>,
    capacity: usize,
}

impl<T> NewestFirstQueue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            waiting: VecDeque::new(),
            capacity,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }

    /// adds a request. if the line is now too long, returns the oldest
    /// request so the caller can tell its owner it will not be served.
    #[must_use]
    pub fn push(&mut self, request: T) -> Option<T> {
        self.waiting.push_back(request);
        if self.waiting.len() > self.capacity {
            self.waiting.pop_front()
        } else {
            None
        }
    }

    pub fn pop_newest(&mut self) -> Option<T> {
        self.waiting.pop_back()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_the_newest_request_first() {
        let mut queue = NewestFirstQueue::new(10);
        for request in [1, 2, 3] {
            assert_eq!(queue.push(request), None);
        }
        assert_eq!(queue.pop_newest(), Some(3));
        assert_eq!(queue.pop_newest(), Some(2));
        assert_eq!(queue.pop_newest(), Some(1));
        assert_eq!(queue.pop_newest(), None);
    }

    #[test]
    fn gives_up_on_the_oldest_request_when_full() {
        let mut queue = NewestFirstQueue::new(2);
        assert_eq!(queue.push(1), None);
        assert_eq!(queue.push(2), None);
        assert_eq!(queue.push(3), Some(1));
        assert_eq!(queue.pop_newest(), Some(3));
        assert_eq!(queue.pop_newest(), Some(2));
        assert!(queue.is_empty());
    }
}
