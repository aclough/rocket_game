/// Generic design lineage: a mutable head design plus frozen revisions.
/// Used for engines now; will be reused for rockets, spacecraft, etc.

/// A frozen snapshot of a design at a point in time.
#[derive(Debug, Clone)]
pub struct Revision<T: Clone> {
    pub revision_number: u32,
    pub snapshot: T,
    pub label: String,
}

/// A design lineage tracks the current (mutable) head design
/// plus a list of frozen revisions that can be used for manufacturing.
#[derive(Debug, Clone)]
pub struct DesignLineage<T: Clone> {
    pub name: String,
    pub head: T,
    pub revisions: Vec<Revision<T>>,
    next_revision: u32,
}

impl<T: Clone> DesignLineage<T> {
    /// Create a new lineage with the given name and initial head design.
    pub fn new(name: &str, head: T) -> Self {
        Self {
            name: name.to_string(),
            head,
            revisions: Vec::new(),
            next_revision: 1,
        }
    }

    /// Freeze the current head as a new revision.
    /// Returns the revision number.
    pub fn cut_revision(&mut self, label: &str) -> u32 {
        let rev_num = self.next_revision;
        self.revisions.push(Revision {
            revision_number: rev_num,
            snapshot: self.head.clone(),
            label: label.to_string(),
        });
        self.next_revision += 1;
        rev_num
    }

    /// Get a revision by its number.
    pub fn get_revision(&self, revision_number: u32) -> Option<&Revision<T>> {
        self.revisions.iter().find(|r| r.revision_number == revision_number)
    }

    /// Get the most recent revision, if any.
    pub fn latest_revision(&self) -> Option<&Revision<T>> {
        self.revisions.last()
    }

    /// Get a reference to the current head design.
    pub fn head(&self) -> &T {
        &self.head
    }

    /// Get a mutable reference to the current head design.
    pub fn head_mut(&mut self) -> &mut T {
        &mut self.head
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_lineage() {
        let lineage = DesignLineage::new("Test", 42);
        assert_eq!(lineage.name, "Test");
        assert_eq!(*lineage.head(), 42);
        assert!(lineage.revisions.is_empty());
    }

    #[test]
    fn test_cut_revision() {
        let mut lineage = DesignLineage::new("Test", 10);

        let rev1 = lineage.cut_revision("First");
        assert_eq!(rev1, 1);
        assert_eq!(lineage.revisions.len(), 1);

        let rev2 = lineage.cut_revision("Second");
        assert_eq!(rev2, 2);
        assert_eq!(lineage.revisions.len(), 2);
    }

    #[test]
    fn test_get_revision() {
        let mut lineage = DesignLineage::new("Test", 10);
        lineage.cut_revision("First");

        let rev = lineage.get_revision(1).unwrap();
        assert_eq!(rev.snapshot, 10);
        assert_eq!(rev.label, "First");

        assert!(lineage.get_revision(99).is_none());
    }

    #[test]
    fn test_latest_revision() {
        let mut lineage = DesignLineage::new("Test", 10);
        assert!(lineage.latest_revision().is_none());

        lineage.cut_revision("First");
        assert_eq!(lineage.latest_revision().unwrap().revision_number, 1);

        lineage.cut_revision("Second");
        assert_eq!(lineage.latest_revision().unwrap().revision_number, 2);
    }

    #[test]
    fn test_revision_immutability() {
        let mut lineage = DesignLineage::new("Test", 10);
        lineage.cut_revision("Before mutation");

        // Mutate the head
        *lineage.head_mut() = 99;

        // Revision should still have the old value
        let rev = lineage.get_revision(1).unwrap();
        assert_eq!(rev.snapshot, 10);
        assert_eq!(*lineage.head(), 99);
    }

    #[test]
    fn test_head_mutation_independence() {
        let mut lineage = DesignLineage::new("Test", vec![1, 2, 3]);
        lineage.cut_revision("Snapshot");

        // Mutate head
        lineage.head_mut().push(4);

        // Revision unaffected
        assert_eq!(lineage.get_revision(1).unwrap().snapshot, vec![1, 2, 3]);
        assert_eq!(*lineage.head(), vec![1, 2, 3, 4]);
    }
}
