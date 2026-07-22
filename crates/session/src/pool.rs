use std::collections::{HashMap, VecDeque};

use roco_engine::EngineError;

/// A session pool that stores and retrieves opaque byte-encoded state.
pub trait SessionPool: Send {
    fn save(&mut self, id: &str, state: Vec<u8>) -> Result<(), SessionError>;
    fn load(&mut self, id: &str) -> Result<Option<Vec<u8>>, SessionError>;
    fn remove(&mut self, id: &str) -> Result<(), SessionError>;
    fn contains(&self, id: &str) -> bool;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn max_capacity(&self) -> usize;
}

/// LRU-evicting session pool with a fixed capacity.
pub struct LruSessionPool {
    pool: HashMap<String, Option<Vec<u8>>>,
    lru: VecDeque<String>,
    max_sessions: usize,
}

impl LruSessionPool {
    pub fn new(max_sessions: usize) -> Self {
        Self {
            pool: HashMap::new(),
            lru: VecDeque::new(),
            max_sessions,
        }
    }
}

impl SessionPool for LruSessionPool {
    fn save(&mut self, id: &str, state: Vec<u8>) -> Result<(), SessionError> {
        // Promote in LRU
        if let Some(pos) = self.lru.iter().position(|s| s == id) {
            self.lru.remove(pos);
        }
        self.lru.push_back(id.to_string());
        self.pool.insert(id.to_string(), Some(state));

        // Evict LRU if over capacity
        while self.pool.len() > self.max_sessions {
            if let Some(oldest) = self.lru.pop_front() {
                self.pool.remove(&oldest);
            } else {
                break;
            }
        }
        Ok(())
    }

    fn load(&mut self, id: &str) -> Result<Option<Vec<u8>>, SessionError> {
        // Promote in LRU
        if let Some(pos) = self.lru.iter().position(|s| s == id) {
            self.lru.remove(pos);
            self.lru.push_back(id.to_string());
        }
        match self.pool.get(id) {
            Some(Some(state)) => Ok(Some(state.clone())),
            Some(None) => Ok(None),
            None => Ok(None),
        }
    }

    fn remove(&mut self, id: &str) -> Result<(), SessionError> {
        self.pool.remove(id);
        if let Some(pos) = self.lru.iter().position(|s| s == id) {
            self.lru.remove(pos);
        }
        Ok(())
    }

    fn contains(&self, id: &str) -> bool {
        self.pool.contains_key(id)
    }

    fn len(&self) -> usize {
        self.pool.len()
    }

    fn max_capacity(&self) -> usize {
        self.max_sessions
    }
}

#[derive(Debug, Clone)]
pub struct SessionError(pub String);

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "session error: {}", self.0)
    }
}

impl std::error::Error for SessionError {}

impl From<SessionError> for EngineError {
    fn from(e: SessionError) -> Self {
        EngineError::Backend(e.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lru_evicts_oldest() {
        let mut pool = LruSessionPool::new(3);
        pool.save("a", vec![1]).unwrap();
        pool.save("b", vec![2]).unwrap();
        pool.save("c", vec![3]).unwrap();
        pool.save("d", vec![4]).unwrap();
        assert!(!pool.contains("a"));
        assert!(pool.contains("b"));
        assert!(pool.contains("c"));
        assert!(pool.contains("d"));
        assert_eq!(pool.len(), 3);
    }

    #[test]
    fn lru_promotes_on_access() {
        let mut pool = LruSessionPool::new(3);
        pool.save("a", vec![1]).unwrap();
        pool.save("b", vec![2]).unwrap();
        pool.save("c", vec![3]).unwrap();
        // Access "a" so it's promoted
        assert_eq!(pool.load("a").unwrap(), Some(vec![1]));
        // Now "b" should be evicted next
        pool.save("d", vec![4]).unwrap();
        assert!(pool.contains("a"));
        assert!(!pool.contains("b"));
        assert!(pool.contains("c"));
        assert!(pool.contains("d"));
    }
}
