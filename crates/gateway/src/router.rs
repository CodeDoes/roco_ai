/// Request router (stub).
pub struct Router;

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

impl Router {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_new() {
        let router = Router::new();
        // Router is a zero-sized marker type; just verify it constructs
        let _ = router;
    }

    #[test]
    fn test_router_default() {
        let router: Router = Default::default();
        let _ = router;
    }

    #[test]
    fn test_router_zst() {
        assert_eq!(std::mem::size_of::<Router>(), 0, "Router should be a ZST");
    }
}
